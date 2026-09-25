// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Client library for the `dev.l10n_bg.dragomand.Translator1` D-Bus
//! service: zbus proxies, the shared name/path/error constants, and the
//! request-object helper implementing the portal-style `handle_token`
//! pattern (subscribe to the request's `Response` before the method call,
//! so no signal can be missed).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

pub mod names {
    pub const BUS_NAME: &str = "dev.l10n_bg.dragomand.Translator1";
    pub const OBJECT_PATH: &str = "/dev/l10n_bg/dragomand/Translator1";
    pub const TRANSLATOR_INTERFACE: &str = "dev.l10n_bg.dragomand.Translator1";
    pub const REQUEST_INTERFACE: &str = "dev.l10n_bg.dragomand.Request1";
    /// Request objects live under
    /// `REQUEST_PATH_PREFIX/<escaped sender>/<token>`.
    pub const REQUEST_PATH_PREFIX: &str = "/dev/l10n_bg/dragomand/request";
    pub const ERROR_PREFIX: &str = "dev.l10n_bg.dragomand.Error";
}

/// `Response` signal codes, following the xdg-desktop-portal convention.
pub mod response_code {
    pub const SUCCESS: u32 = 0;
    pub const CANCELLED: u32 = 1;
    pub const ERROR: u32 = 2;
}

/// Well-known keys inside a `Response` results dictionary.
pub mod result_key {
    /// `as`: one translation per input segment.
    pub const TRANSLATIONS: &str = "translations";
    /// `s`: pivot language used, present only when the route pivoted.
    pub const PIVOT: &str = "pivot";
    /// `s`: error message, present when the code is [`super::response_code::ERROR`].
    pub const ERROR_MESSAGE: &str = "error";
}

#[zbus::proxy(
    interface = "dev.l10n_bg.dragomand.Translator1",
    default_service = "dev.l10n_bg.dragomand.Translator1",
    default_path = "/dev/l10n_bg/dragomand/Translator1"
)]
pub trait Translator1 {
    /// Per pair: source, target, installed_version, available_version,
    /// origin, size, architecture (missing keys mean unknown).
    fn list_language_pairs(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;

    /// Install if missing, then load. Options: `handle_token` (s).
    fn prepare_pair(
        &self,
        source: &str,
        target: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Options: `handle_token` (s), `html` (b), `allow_pivot` (b, default
    /// true), `priority` (s: `interactive` | `batch`).
    fn translate(
        &self,
        source: &str,
        target: &str,
        segments: Vec<String>,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Install or upgrade a pair. Options: `handle_token` (s).
    fn install_pair(
        &self,
        source: &str,
        target: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    /// Remove an installed pair from the user store.
    fn remove_pair(&self, source: &str, target: &str) -> zbus::Result<()>;

    /// Refresh the record cache and report available updates.
    /// Options: `handle_token` (s).
    fn check_for_updates(&self, options: HashMap<&str, Value<'_>>)
    -> zbus::Result<OwnedObjectPath>;

    /// Loaded pairs, queue length, and similar liveness data.
    fn get_status(&self) -> zbus::Result<HashMap<String, OwnedValue>>;

    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "dev.l10n_bg.dragomand.Request1",
    default_service = "dev.l10n_bg.dragomand.Translator1"
)]
pub trait Request1 {
    fn cancel(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn progress(&self, fraction: f64, stage: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    fn response(&self, code: u32, results: HashMap<String, OwnedValue>) -> zbus::Result<()>;
}

static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A process-unique `handle_token` value.
pub fn new_handle_token() -> String {
    let n = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dragoman_{}_{n}", std::process::id())
}

/// The request object path the service will use for `(sender, token)`:
/// the sender unique name with `.` and `:` mapped to `_`, then the token.
pub fn request_path(sender: &str, token: &str) -> Result<OwnedObjectPath, zbus::zvariant::Error> {
    let escaped: String = sender
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    ObjectPath::try_from(format!("{}/{escaped}/{token}", names::REQUEST_PATH_PREFIX))
        .map(Into::into)
}

/// The result of an awaited request.
#[derive(Debug)]
pub struct RequestOutcome {
    pub code: u32,
    pub results: HashMap<String, OwnedValue>,
}

/// Subscribes to a request's `Response` *before* invoking `call`, then
/// waits for the signal. `call` receives the `handle_token` to pass in the
/// method's options.
pub async fn call_with_request<F, Fut>(
    connection: &zbus::Connection,
    call: F,
) -> zbus::Result<RequestOutcome>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = zbus::Result<OwnedObjectPath>>,
{
    let token = new_handle_token();
    let sender = connection
        .unique_name()
        .ok_or_else(|| zbus::Error::Failure("connection has no unique name".into()))?;
    let expected = request_path(sender.as_str(), &token)?;

    let request = Request1Proxy::builder(connection)
        .path(expected.clone())?
        .build()
        .await?;
    let mut responses = request.receive_response().await?;

    let path = call(token).await?;
    if path != expected {
        // The service disagreed about the path (unexpected); follow it.
        let request = Request1Proxy::builder(connection)
            .path(path)?
            .build()
            .await?;
        responses = request.receive_response().await?;
    }

    let signal = responses
        .next()
        .await
        .ok_or_else(|| zbus::Error::Failure("request stream ended without a response".into()))?;
    let args = signal.args()?;
    Ok(RequestOutcome {
        code: args.code,
        results: args.results,
    })
}

use futures_util::StreamExt;
