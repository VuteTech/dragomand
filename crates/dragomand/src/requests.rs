// SPDX-License-Identifier: GPL-3.0-or-later

//! Request objects (`dev.l10n_bg.dragomand.Request1`) and per-client
//! accounting.
//!
//! Follows the xdg-desktop-portal Request pattern: the client passes a
//! `handle_token`, can compute the request path up front and subscribe to
//! `Response` before the method returns. Every slow method registers a
//! request object, does its work in a task, emits `Response` exactly once
//! and removes the object again. Clients are untrusted: per-client
//! concurrency is capped, and a client's requests die with it
//! (`NameOwnerChanged`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;
use zbus::names::OwnedUniqueName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

use crate::error::{Error, Result};

/// Per-client concurrent request cap (checked before any work is queued).
pub const MAX_REQUESTS_PER_CLIENT: usize = 8;

/// Cancellation handle shared between a request object and its work task.
#[derive(Clone, Default)]
pub struct CancelHandle {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelHandle {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

/// The D-Bus request object.
pub struct RequestObject {
    cancel: CancelHandle,
}

#[zbus::interface(name = "dev.l10n_bg.dragomand.Request1")]
impl RequestObject {
    async fn cancel(&self) {
        self.cancel.cancel();
    }

    #[zbus(signal)]
    pub async fn progress(
        emitter: &SignalEmitter<'_>,
        fraction: f64,
        stage: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn response(
        emitter: &SignalEmitter<'_>,
        code: u32,
        results: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

/// Tracks live requests per client.
#[derive(Default)]
pub struct RequestManager {
    clients: Mutex<HashMap<OwnedUniqueName, Vec<(OwnedObjectPath, CancelHandle)>>>,
}

impl RequestManager {
    /// Reserves a request slot. Fails when the client is over its cap.
    fn register(
        self: &Arc<Self>,
        sender: OwnedUniqueName,
        path: OwnedObjectPath,
        cancel: CancelHandle,
    ) -> Result<()> {
        let mut clients = self.clients.lock().expect("request manager lock");
        let requests = clients.entry(sender).or_default();
        if requests.len() >= MAX_REQUESTS_PER_CLIENT {
            return Err(Error::LimitExceeded(format!(
                "at most {MAX_REQUESTS_PER_CLIENT} concurrent requests per client"
            )));
        }
        requests.push((path, cancel));
        Ok(())
    }

    fn release(&self, sender: &OwnedUniqueName, path: &OwnedObjectPath) {
        let mut clients = self.clients.lock().expect("request manager lock");
        if let Some(requests) = clients.get_mut(sender) {
            requests.retain(|(p, _)| p != path);
            if requests.is_empty() {
                clients.remove(sender);
            }
        }
    }

    /// Requests currently in flight, across all clients.
    pub fn active_count(&self) -> usize {
        let clients = self.clients.lock().expect("request manager lock");
        clients.values().map(Vec::len).sum()
    }

    /// Cancels everything a vanished client had in flight.
    pub fn cancel_client(&self, sender: &OwnedUniqueName) {
        let requests = {
            let mut clients = self.clients.lock().expect("request manager lock");
            clients.remove(sender)
        };
        for (_, cancel) in requests.into_iter().flatten() {
            cancel.cancel();
        }
    }
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Registers a request object for `(sender, token)`, runs `work` in a
/// task, emits `Response` once (result, error, or cancellation) and cleans
/// up. Returns the request object path for the method reply.
pub async fn spawn_request<F>(
    connection: &zbus::Connection,
    manager: &Arc<RequestManager>,
    sender: OwnedUniqueName,
    token: &str,
    work: F,
) -> Result<OwnedObjectPath>
where
    F: Future<Output = Result<HashMap<String, OwnedValue>>> + Send + 'static,
{
    if !valid_token(token) {
        return Err(Error::InvalidArgument(
            "handle_token must be 1..=128 chars of [A-Za-z0-9_]".into(),
        ));
    }
    let path = dragoman_client::request_path(sender.as_str(), token)
        .map_err(|e| Error::InvalidArgument(format!("bad request path: {e}")))?;

    let cancel = CancelHandle::default();
    manager.register(sender.clone(), path.clone(), cancel.clone())?;

    let object = RequestObject {
        cancel: cancel.clone(),
    };
    let added = connection
        .object_server()
        .at(path.clone(), object)
        .await
        .map_err(Error::ZBus)?;
    if !added {
        manager.release(&sender, &path);
        return Err(Error::InvalidArgument(format!(
            "a request with token {token:?} already exists"
        )));
    }

    let connection = connection.clone();
    let manager = Arc::clone(manager);
    let task_path = path.clone();
    tokio::spawn(async move {
        let path = task_path;
        let outcome = {
            let work = std::pin::pin!(work);
            tokio::select! {
                result = work => result,
                () = cancel.cancelled() => Err(Error::EngineFailure("cancelled".into())),
            }
        };
        let (code, results) = if cancel.is_cancelled() {
            (dragoman_client::response_code::CANCELLED, HashMap::new())
        } else {
            match outcome {
                Ok(results) => (dragoman_client::response_code::SUCCESS, results),
                Err(error) => {
                    let mut results = HashMap::new();
                    if let Ok(value) =
                        OwnedValue::try_from(zbus::zvariant::Value::from(error.to_string()))
                    {
                        results
                            .insert(dragoman_client::result_key::ERROR_MESSAGE.to_owned(), value);
                    }
                    (dragoman_client::response_code::ERROR, results)
                }
            }
        };

        emit_response(&connection, &path, code, results).await;
        let _ = connection
            .object_server()
            .remove::<RequestObject, _>(&path)
            .await;
        manager.release(&sender, &path);
    });

    Ok(path)
}

async fn emit_response(
    connection: &zbus::Connection,
    path: &OwnedObjectPath,
    code: u32,
    results: HashMap<String, OwnedValue>,
) {
    let Ok(iface) = connection
        .object_server()
        .interface::<_, RequestObject>(path)
        .await
    else {
        return;
    };
    if let Err(error) = RequestObject::response(iface.signal_emitter(), code, results).await {
        tracing::warn!(%path, %error, "failed to emit Response");
    }
}

/// Emits `Progress` on a request object, if it still exists.
pub async fn emit_progress(
    connection: &zbus::Connection,
    path: &ObjectPath<'_>,
    fraction: f64,
    stage: &str,
) {
    let Ok(iface) = connection
        .object_server()
        .interface::<_, RequestObject>(path)
        .await
    else {
        return;
    };
    let _ = RequestObject::progress(iface.signal_emitter(), fraction, stage).await;
}

/// Watches `NameOwnerChanged` and cancels requests of vanished clients.
pub async fn watch_disconnects(
    connection: zbus::Connection,
    manager: Arc<RequestManager>,
) -> zbus::Result<()> {
    use futures_util::StreamExt;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut stream = dbus.receive_name_owner_changed().await?;
    while let Some(signal) = stream.next().await {
        let Ok(args) = signal.args() else { continue };
        if args.new_owner.is_none()
            && let zbus::names::BusName::Unique(old) = &args.name
        {
            manager.cancel_client(&OwnedUniqueName::from(old.clone()));
        }
    }
    Ok(())
}
