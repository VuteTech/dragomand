// SPDX-License-Identifier: GPL-3.0-or-later

//! The `dev.l10n_bg.dragomand.Translator1` interface.
//!
//! Anything slow returns a request object immediately; results and errors
//! arrive as `Response` signals on it. D-Bus is the control plane: inputs
//! are size-limited, clients are untrusted.

use std::collections::HashMap;
use std::sync::Arc;

use dragoman_client::result_key;
use dragoman_models::http::ReqwestHttp;
use dragoman_models::remote_settings::RemoteSettingsProvider;
use dragoman_models::{Origin, Stores};
use zbus::message::Header;
use zbus::names::OwnedUniqueName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::backend::BackendKind;
use crate::error::{Error, Result};
use crate::requests::{self, RequestManager, spawn_request};
use crate::workers::{JobError, PairWorkers, Priority};

/// Per-request input limits, enforced before anything is queued.
pub const MAX_SEGMENTS: usize = 256;
pub const MAX_TOTAL_BYTES: usize = 1 << 20; // 1 MiB

pub struct DaemonState {
    pub config: crate::config::Config,
    pub stores: Stores,
    pub provider: RemoteSettingsProvider<ReqwestHttp>,
    pub workers: PairWorkers,
    pub requests: Arc<RequestManager>,
    pub activity: crate::lifecycle::Activity,
}

impl DaemonState {
    pub fn new(
        config: crate::config::Config,
        backend: BackendKind,
        stores: Stores,
        provider: RemoteSettingsProvider<ReqwestHttp>,
    ) -> Self {
        DaemonState {
            config,
            stores,
            provider,
            workers: PairWorkers::new(backend),
            requests: Arc::new(RequestManager::default()),
            activity: crate::lifecycle::Activity::default(),
        }
    }

    fn require_network(&self) -> Result<()> {
        if self.config.network {
            Ok(())
        } else {
            Err(Error::NetworkDisabled(
                "downloads and update checks are disabled by configuration".into(),
            ))
        }
    }
}

pub struct Translator {
    pub state: Arc<DaemonState>,
}

fn valid_lang(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 16
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !tag.starts_with('-')
        && !tag.ends_with('-')
}

fn check_langs(source: &str, target: &str) -> Result<()> {
    if !valid_lang(source) || !valid_lang(target) || source == target {
        return Err(Error::InvalidArgument(format!(
            "bad language pair {source:?} -> {target:?}"
        )));
    }
    Ok(())
}

fn sender_of(header: &Header<'_>) -> Result<OwnedUniqueName> {
    header
        .sender()
        .map(|s| OwnedUniqueName::from(s.to_owned()))
        .ok_or_else(|| Error::InvalidArgument("message has no sender".into()))
}

fn opt_str(options: &HashMap<String, OwnedValue>, key: &str) -> Result<Option<String>> {
    match options.get(key) {
        None => Ok(None),
        Some(value) => match value.downcast_ref::<&str>() {
            Ok(s) => Ok(Some(s.to_owned())),
            Err(_) => Err(Error::InvalidArgument(format!(
                "option {key} must be a string"
            ))),
        },
    }
}

fn opt_bool(options: &HashMap<String, OwnedValue>, key: &str, default: bool) -> Result<bool> {
    match options.get(key) {
        None => Ok(default),
        Some(value) => value
            .downcast_ref::<bool>()
            .map_err(|_| Error::InvalidArgument(format!("option {key} must be a boolean"))),
    }
}

fn token_from(options: &HashMap<String, OwnedValue>) -> Result<String> {
    Ok(opt_str(options, "handle_token")?.unwrap_or_else(dragoman_client::new_handle_token))
}

fn value(v: impl Into<Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(v.into()).expect("no fds in our values")
}

#[zbus::interface(name = "dev.l10n_bg.dragomand.Translator1")]
impl Translator {
    /// Installed and cached-available pairs with their state.
    async fn list_language_pairs(&self) -> Result<Vec<HashMap<String, OwnedValue>>> {
        self.state.activity.touch();
        let state = &self.state;
        let mut problems = Vec::new();
        let installed = state.stores.installed_by_pair(&mut problems);
        for problem in &problems {
            tracing::warn!("store scan: {problem}");
        }
        let available = state.provider.available_cached();

        let mut rows: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        for (pair, versions) in &installed {
            let best = &versions[0];
            let mut row = HashMap::new();
            row.insert("source".into(), value(best.manifest.source.clone()));
            row.insert("target".into(), value(best.manifest.target.clone()));
            row.insert(
                "installed_version".into(),
                value(best.version.as_str().to_owned()),
            );
            row.insert(
                "origin".into(),
                value(match best.origin {
                    Origin::System => "system",
                    Origin::User => "user",
                }),
            );
            if let Some(architecture) = &best.manifest.architecture {
                row.insert("architecture".into(), value(architecture.clone()));
            }
            let size: u64 = best.manifest.files.iter().map(|f| f.size).sum();
            row.insert("size".into(), value(size));
            rows.insert(pair.clone(), row);
        }
        if let Some(available) = available {
            for set in available.sets {
                let row = rows.entry(set.pair()).or_insert_with(|| {
                    let mut row = HashMap::new();
                    row.insert("source".into(), value(set.source.clone()));
                    row.insert("target".into(), value(set.target.clone()));
                    row
                });
                row.insert(
                    "available_version".into(),
                    value(set.version.as_str().to_owned()),
                );
                if !row.contains_key("architecture")
                    && let Some(architecture) = &set.architecture
                {
                    row.insert("architecture".into(), value(architecture.clone()));
                }
            }
        }
        let mut rows: Vec<_> = rows.into_iter().collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }

    /// Install if missing, then load, so the first Translate is fast.
    async fn prepare_pair(
        &self,
        source: String,
        target: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<OwnedObjectPath> {
        self.state.activity.touch();
        check_langs(&source, &target)?;
        let sender = sender_of(&header)?;
        let token = token_from(&options)?;
        let allow_pivot = opt_bool(&options, "allow_pivot", true)?;

        let state = Arc::clone(&self.state);
        let connection2 = connection.clone();
        let path_for_progress = dragoman_client::request_path(sender.as_str(), &token)
            .map_err(|e| Error::InvalidArgument(e.to_string()))?;

        spawn_request(
            connection,
            &state.requests.clone(),
            sender,
            &token,
            async move {
                let progress = |fraction: f64, stage: &'static str| {
                    let connection = connection2.clone();
                    let path = path_for_progress.clone();
                    async move {
                        requests::emit_progress(
                            &connection,
                            &ObjectPath::from(&path),
                            fraction,
                            stage,
                        )
                        .await;
                    }
                };

                // Install whatever the route is missing (this is the one
                // translate-path operation allowed to touch the network).
                if PairWorkers::plan_route(&state.stores, &source, &target, allow_pivot).is_err() {
                    state.require_network()?;
                    progress(0.1, "downloading").await;
                    let mut legs: Vec<(String, String)> = Vec::new();
                    if source == crate::workers::PIVOT_LANGUAGE
                        || target == crate::workers::PIVOT_LANGUAGE
                        || !allow_pivot
                    {
                        legs.push((source.clone(), target.clone()));
                    } else {
                        legs.push((source.clone(), crate::workers::PIVOT_LANGUAGE.into()));
                        legs.push((crate::workers::PIVOT_LANGUAGE.into(), target.clone()));
                    }
                    for (src, trg) in legs {
                        if state.stores.resolve(&src, &trg).is_none() {
                            state
                                .provider
                                .install_pair(&state.stores, &src, &trg)
                                .await?;
                        }
                    }
                }

                progress(0.7, "loading").await;
                let entry = state
                    .workers
                    .ensure_loaded(
                        &state.stores,
                        &source,
                        &target,
                        allow_pivot,
                        state.config.memory_budget_mb,
                    )
                    .await?;

                let mut results = HashMap::new();
                if let Some(pivot) = &entry.pivot {
                    results.insert(result_key::PIVOT.to_owned(), value(pivot.clone()));
                }
                if let Some(installed) = state.stores.resolve(&source, &target) {
                    results.insert(
                        "version".to_owned(),
                        value(installed.version.as_str().to_owned()),
                    );
                }
                Ok(results)
            },
        )
        .await
    }

    async fn translate(
        &self,
        source: String,
        target: String,
        segments: Vec<String>,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<OwnedObjectPath> {
        self.state.activity.touch();
        check_langs(&source, &target)?;
        if segments.is_empty() {
            return Err(Error::InvalidArgument("segments is empty".into()));
        }
        if segments.len() > MAX_SEGMENTS {
            return Err(Error::LimitExceeded(format!(
                "at most {MAX_SEGMENTS} segments per request"
            )));
        }
        let total: usize = segments.iter().map(String::len).sum();
        if total > MAX_TOTAL_BYTES {
            return Err(Error::LimitExceeded(format!(
                "at most {MAX_TOTAL_BYTES} bytes per request; send documents through TranslateFd"
            )));
        }
        let sender = sender_of(&header)?;
        let token = token_from(&options)?;
        let html = opt_bool(&options, "html", false)?;
        let allow_pivot = opt_bool(&options, "allow_pivot", true)?;
        let priority = match opt_str(&options, "priority")?.as_deref() {
            None | Some("interactive") => Priority::Interactive,
            Some("batch") => Priority::Batch,
            Some(other) => {
                return Err(Error::InvalidArgument(format!(
                    "unknown priority {other:?}"
                )));
            }
        };

        // Fail fast before creating a request object.
        PairWorkers::plan_route(&self.state.stores, &source, &target, allow_pivot)?;

        let state = Arc::clone(&self.state);
        let requests = Arc::clone(&self.state.requests);
        let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_for_work = Arc::clone(&cancel_flag);

        spawn_request(connection, &requests, sender, &token, async move {
            // A route can be evicted between lookup and submit; retry once.
            let mut receiver = None;
            let mut entry = None;
            for _ in 0..2 {
                let candidate = state
                    .workers
                    .ensure_loaded(
                        &state.stores,
                        &source,
                        &target,
                        allow_pivot,
                        state.config.memory_budget_mb,
                    )
                    .await?;
                match candidate.submit(segments.clone(), html, priority, Arc::clone(&flag_for_work))
                {
                    Ok(r) => {
                        receiver = Some(r);
                        entry = Some(candidate);
                        break;
                    }
                    Err(()) => continue,
                }
            }
            let (receiver, entry) = receiver
                .zip(entry)
                .ok_or_else(|| Error::EngineFailure("route kept unloading".into()))?;
            let outcome = receiver
                .await
                .map_err(|_| Error::EngineFailure("worker vanished".into()))?;
            match outcome {
                Ok(translations) => {
                    let mut results = HashMap::new();
                    results.insert(result_key::TRANSLATIONS.to_owned(), value(translations));
                    if let Some(pivot) = &entry.pivot {
                        results.insert(result_key::PIVOT.to_owned(), value(pivot.clone()));
                    }
                    Ok(results)
                }
                Err(JobError::Cancelled) => {
                    // The request task turns this into a CANCELLED response.
                    Err(Error::EngineFailure("cancelled".into()))
                }
                Err(JobError::Engine(message)) => Err(Error::EngineFailure(message)),
            }
        })
        .await
    }

    /// Install or upgrade one pair (network).
    async fn install_pair(
        &self,
        source: String,
        target: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<OwnedObjectPath> {
        self.state.activity.touch();
        check_langs(&source, &target)?;
        self.state.activity.touch();
        self.state.require_network()?;
        let sender = sender_of(&header)?;
        let token = token_from(&options)?;
        let state = Arc::clone(&self.state);

        spawn_request(
            connection,
            &state.requests.clone(),
            sender,
            &token,
            async move {
                let installed = state
                    .provider
                    .install_pair(&state.stores, &source, &target)
                    .await?;
                let mut results = HashMap::new();
                results.insert(
                    "version".to_owned(),
                    value(installed.version.as_str().to_owned()),
                );
                if let Some(architecture) = &installed.manifest.architecture {
                    results.insert("architecture".to_owned(), value(architecture.clone()));
                }
                Ok(results)
            },
        )
        .await
    }

    /// Remove every user-store copy of a pair. System copies stay.
    async fn remove_pair(&self, source: String, target: String) -> Result<()> {
        self.state.activity.touch();
        check_langs(&source, &target)?;
        let state = &self.state;
        state.workers.unload_pair(&source, &target).await;

        let _lock = state
            .stores
            .lock_user()
            .map_err(|e| Error::EngineFailure(e.to_string()))?;
        let mut problems = Vec::new();
        let pair = format!("{source}-{target}");
        let removed: Vec<_> = state
            .stores
            .scan(&mut problems)
            .into_iter()
            .filter(|m| m.pair() == pair && m.origin == Origin::User)
            .collect();
        if removed.is_empty() {
            return Err(Error::NotInstalled(pair));
        }
        for model in removed {
            state
                .stores
                .remove(&model)
                .map_err(|e| Error::EngineFailure(e.to_string()))?;
        }
        Ok(())
    }

    /// Refresh the record cache and report available updates.
    async fn check_for_updates(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<OwnedObjectPath> {
        self.state.require_network()?;
        let sender = sender_of(&header)?;
        let token = token_from(&options)?;
        let state = Arc::clone(&self.state);

        spawn_request(
            connection,
            &state.requests.clone(),
            sender,
            &token,
            async move {
                let updates = state.provider.check_for_updates(&state.stores).await?;
                let rows: Vec<String> = updates
                    .iter()
                    .map(|u| {
                        format!(
                            "{}-{} {} -> {}",
                            u.source, u.target, u.installed, u.available
                        )
                    })
                    .collect();
                let mut results = HashMap::new();
                results.insert("updates".to_owned(), value(rows));
                Ok(results)
            },
        )
        .await
    }

    /// Liveness data: loaded routes, queue length, memory use.
    async fn get_status(&self) -> HashMap<String, OwnedValue> {
        self.state.activity.touch();
        let (loaded, queued, model_cost_mb) = self.state.workers.status().await;
        let mut status = HashMap::new();
        status.insert("version".to_owned(), value(env!("CARGO_PKG_VERSION")));
        status.insert("loaded".to_owned(), value(loaded));
        status.insert("queued".to_owned(), value(queued as u32));
        status.insert("model_cost_mb".to_owned(), value(model_cost_mb));
        status.insert("rss_mb".to_owned(), value(crate::workers::resident_mb()));
        status
    }

    #[zbus(property)]
    async fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }
}
