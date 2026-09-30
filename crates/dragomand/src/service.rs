// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `dev.l10n_bg.dragomand.Translator1` interface.
//!
//! Anything slow returns a request object immediately; results and errors
//! arrive as `Response` signals on it. D-Bus is the control plane: inputs
//! are size-limited, clients are untrusted.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use dragoman_client::result_key;
use dragoman_engine::Translation;
use dragoman_models::http::ReqwestHttp;
use dragoman_models::registry::{Registry, RegistryProvider};
use dragoman_models::remote_settings::RemoteSettingsProvider;
use dragoman_models::{FileType, Origin, Stores};
use zbus::message::Header;
use zbus::names::OwnedUniqueName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedFd, OwnedObjectPath, OwnedValue, Value};

use crate::backend::BackendKind;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::requests::{self, RequestManager, spawn_request};
use crate::workers::{JobError, PairWorkers, Priority};

/// Per-request input limits, enforced before anything is queued.
pub const MAX_SEGMENTS: usize = 256;
pub const MAX_TOTAL_BYTES: usize = 1 << 20; // 1 MiB
/// TranslateFd reads at most this much.
pub const MAX_DOCUMENT_BYTES: usize = 64 << 20; // 64 MiB
/// TranslateFd translates this many lines (and at most MAX_TOTAL_BYTES)
/// per step, reporting progress in between.
const DOCUMENT_CHUNK_LINES: usize = 64;
/// DetectLanguage looks at no more than this much text.
pub const MAX_DETECT_BYTES: usize = 64 << 10; // 64 KiB

pub struct DaemonState {
    config: RwLock<Config>,
    /// Where SetConfig persists changes; `None` keeps them in memory.
    config_path: Option<PathBuf>,
    pub stores: Stores,
    pub provider: RemoteSettingsProvider<ReqwestHttp>,
    pub registry: RegistryProvider<ReqwestHttp>,
    pub workers: PairWorkers,
    pub requests: Arc<RequestManager>,
    pub activity: crate::lifecycle::Activity,
}

impl DaemonState {
    pub fn new(
        config: Config,
        config_path: Option<PathBuf>,
        backend: BackendKind,
        stores: Stores,
        provider: RemoteSettingsProvider<ReqwestHttp>,
        registry: RegistryProvider<ReqwestHttp>,
    ) -> Self {
        DaemonState {
            config: RwLock::new(config),
            config_path,
            stores,
            provider,
            registry,
            workers: PairWorkers::new(backend),
            requests: Arc::new(RequestManager::default()),
            activity: crate::lifecycle::Activity::default(),
        }
    }

    /// Every language of an installed pair or of a cached available one.
    pub fn known_languages(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut languages: Vec<String> = self
            .stores
            .installed_by_pair(&mut problems)
            .into_values()
            .flat_map(|versions| {
                let manifest = &versions[0].manifest;
                [manifest.source.clone(), manifest.target.clone()]
            })
            .collect();
        if let Some(available) = self.provider.available_cached() {
            for set in available.sets {
                languages.push(set.source);
                languages.push(set.target);
            }
        }
        languages.sort();
        languages.dedup();
        languages
    }

    /// A snapshot of the current configuration.
    pub fn config(&self) -> Config {
        self.config.read().expect("config lock").clone()
    }

    fn require_network(&self) -> Result<()> {
        if self.config().network {
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

fn opt_strings(options: &HashMap<String, OwnedValue>, key: &str) -> Result<Vec<String>> {
    match options.get(key) {
        None => Ok(Vec::new()),
        Some(v) => Vec::<String>::try_from(
            v.try_clone()
                .map_err(|e| Error::InvalidArgument(e.to_string()))?,
        )
        .map_err(|_| Error::InvalidArgument(format!("option {key} must be an array of strings"))),
    }
}

fn priority_from(options: &HashMap<String, OwnedValue>, default: Priority) -> Result<Priority> {
    match opt_str(options, "priority")?.as_deref() {
        None => Ok(default),
        Some("interactive") => Ok(Priority::Interactive),
        Some("batch") => Ok(Priority::Batch),
        Some(other) => Err(Error::InvalidArgument(format!(
            "unknown priority {other:?}"
        ))),
    }
}

/// Unicode code point offset of byte offset `byte` in `text`.
fn code_points(text: &str, byte: usize) -> u32 {
    let byte = byte.min(text.len());
    let mut boundary = byte;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text[..boundary].chars().count() as u32
}

/// The `sentences` result for one segment: (source begin, source end,
/// translation begin, translation end) per sentence, in code points.
fn sentence_spans(source: &str, translation: &Translation) -> Vec<(u32, u32, u32, u32)> {
    translation
        .sentences
        .iter()
        .map(|pair| {
            (
                code_points(source, pair.source.start),
                code_points(source, pair.source.end),
                code_points(&translation.text, pair.target.start),
                code_points(&translation.text, pair.target.end),
            )
        })
        .collect()
}

/// What to translate with: the pair, and how.
struct Route<'a> {
    source: &'a str,
    target: &'a str,
    allow_pivot: bool,
    html: bool,
    priority: Priority,
}

/// Translates `segments` on the route, loading it when needed. A route can
/// be evicted between lookup and submit; that is retried once.
async fn run_translation(
    state: &DaemonState,
    route: &Route<'_>,
    segments: Vec<String>,
    cancelled: Arc<AtomicBool>,
) -> Result<(Vec<Translation>, Option<String>)> {
    for _ in 0..2 {
        let entry = state
            .workers
            .ensure_loaded(
                &state.stores,
                route.source,
                route.target,
                route.allow_pivot,
                state.config().memory_budget_mb,
            )
            .await?;
        let Ok(receiver) = entry.submit(
            segments.clone(),
            route.html,
            route.priority,
            Arc::clone(&cancelled),
        ) else {
            continue;
        };
        let outcome = receiver
            .await
            .map_err(|_| Error::EngineFailure("worker vanished".into()))?;
        return match outcome {
            Ok(translations) => Ok((translations, entry.pivot.clone())),
            // The request task turns this into a CANCELLED response.
            Err(JobError::Cancelled) => Err(Error::EngineFailure("cancelled".into())),
            Err(JobError::Engine(message)) => Err(Error::EngineFailure(message)),
        };
    }
    Err(Error::EngineFailure("route kept unloading".into()))
}

/// Reads all of `fd` (at most `limit` bytes) as UTF-8.
fn read_document(fd: std::os::fd::OwnedFd, limit: usize) -> std::result::Result<String, String> {
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("reading the input failed: {e}"))?;
    if bytes.len() > limit {
        return Err(format!("the input is larger than {limit} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| "the input is not valid UTF-8".to_owned())
}

fn write_document(fd: std::os::fd::OwnedFd, text: &str) -> std::result::Result<(), String> {
    let mut file = std::fs::File::from(fd);
    file.write_all(text.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|e| format!("writing the output failed: {e}"))
}

/// Quality metadata for a model file (by its sha256) into a pair row.
fn annotate(
    row: &mut HashMap<String, OwnedValue>,
    registry: Option<&Registry>,
    model_sha256: Option<&str>,
) {
    let Some(quality) = registry.zip(model_sha256).and_then(|(r, h)| r.quality(h)) else {
        return;
    };
    if let Some(status) = &quality.release_status {
        row.insert("release_status".into(), value(status.clone()));
    }
    if let Some(comet) = quality.comet {
        row.insert("quality".into(), value(comet));
    }
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
        let registry = state.registry.cached();

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
            let model_hash = best
                .manifest
                .files
                .iter()
                .find(|f| f.role == "model")
                .map(|f| f.sha256.as_str());
            annotate(&mut row, registry.as_ref(), model_hash);
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
                if !row.contains_key("architecture") {
                    if let Some(architecture) = &set.architecture {
                        row.insert("architecture".into(), value(architecture.clone()));
                    }
                }
                // Not installed: describe the model an install would fetch.
                if !row.contains_key("installed_version") {
                    let model_hash = set
                        .files
                        .get(&FileType::Model)
                        .map(|r| r.decompressed_hash.as_str());
                    annotate(row, registry.as_ref(), model_hash);
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
                        state.config().memory_budget_mb,
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
        let want_sentences = opt_bool(&options, "sentences", false)?;
        let priority = priority_from(&options, Priority::Interactive)?;

        // Fail fast before creating a request object.
        PairWorkers::plan_route(&self.state.stores, &source, &target, allow_pivot)?;

        let state = Arc::clone(&self.state);
        let requests = Arc::clone(&self.state.requests);
        let cancel_flag = Arc::new(AtomicBool::new(false));

        spawn_request(connection, &requests, sender, &token, async move {
            let originals = want_sentences.then(|| segments.clone());
            let route = Route {
                source: &source,
                target: &target,
                allow_pivot,
                html,
                priority,
            };
            let (translations, pivot) =
                run_translation(&state, &route, segments, cancel_flag).await?;
            let mut results = HashMap::new();
            if let Some(originals) = originals {
                let spans: Vec<Vec<(u32, u32, u32, u32)>> = originals
                    .iter()
                    .zip(&translations)
                    .map(|(source, translation)| sentence_spans(source, translation))
                    .collect();
                results.insert("sentences".to_owned(), value(spans));
            }
            let texts: Vec<String> = translations.into_iter().map(|t| t.text).collect();
            results.insert(result_key::TRANSLATIONS.to_owned(), value(texts));
            if let Some(pivot) = pivot {
                results.insert(result_key::PIVOT.to_owned(), value(pivot));
            }
            Ok(results)
        })
        .await
    }

    /// Translates a document passed as a file descriptor, line by line:
    /// every line with a letter or digit is a segment, the others are
    /// copied. The translation goes to `output`.
    #[allow(clippy::too_many_arguments)]
    async fn translate_fd(
        &self,
        source: String,
        target: String,
        input: OwnedFd,
        output: OwnedFd,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<OwnedObjectPath> {
        self.state.activity.touch();
        check_langs(&source, &target)?;
        let sender = sender_of(&header)?;
        let token = token_from(&options)?;
        let html = opt_bool(&options, "html", false)?;
        let allow_pivot = opt_bool(&options, "allow_pivot", true)?;
        let priority = priority_from(&options, Priority::Batch)?;
        PairWorkers::plan_route(&self.state.stores, &source, &target, allow_pivot)?;

        let input: std::os::fd::OwnedFd = input.into();
        let output: std::os::fd::OwnedFd = output.into();
        let state = Arc::clone(&self.state);
        let requests = Arc::clone(&self.state.requests);
        let progress_path = dragoman_client::request_path(sender.as_str(), &token)
            .map_err(|e| Error::InvalidArgument(e.to_string()))?;
        let progress_connection = connection.clone();
        let cancel_flag = Arc::new(AtomicBool::new(false));

        spawn_request(connection, &requests, sender, &token, async move {
            let text =
                tokio::task::spawn_blocking(move || read_document(input, MAX_DOCUMENT_BYTES))
                    .await
                    .map_err(|e| Error::EngineFailure(e.to_string()))?
                    .map_err(Error::InvalidArgument)?;
            let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
            let wanted: Vec<usize> = (0..lines.len())
                .filter(|&i| lines[i].chars().any(char::is_alphanumeric))
                .collect();

            let mut pivot = None;
            let mut done = 0;
            while done < wanted.len() {
                let mut chunk = Vec::new();
                let mut bytes = 0;
                for &line in &wanted[done..] {
                    let len = lines[line].len();
                    if chunk.len() == DOCUMENT_CHUNK_LINES
                        || (!chunk.is_empty() && bytes + len > MAX_TOTAL_BYTES)
                    {
                        break;
                    }
                    chunk.push(line);
                    bytes += len;
                }
                let segments: Vec<String> = chunk.iter().map(|&i| lines[i].clone()).collect();
                let route = Route {
                    source: &source,
                    target: &target,
                    allow_pivot,
                    html,
                    priority,
                };
                let (translations, route_pivot) =
                    run_translation(&state, &route, segments, Arc::clone(&cancel_flag)).await?;
                if translations.len() != chunk.len() {
                    return Err(Error::EngineFailure("the engine lost lines".into()));
                }
                for (&line, translation) in chunk.iter().zip(translations) {
                    lines[line] = translation.text;
                }
                pivot = route_pivot;
                done += chunk.len();
                requests::emit_progress(
                    &progress_connection,
                    &ObjectPath::from(&progress_path),
                    done as f64 / wanted.len() as f64,
                    "translating",
                )
                .await;
            }

            let translated = lines.join("\n");
            tokio::task::spawn_blocking(move || write_document(output, &translated))
                .await
                .map_err(|e| Error::EngineFailure(e.to_string()))?
                .map_err(Error::EngineFailure)?;

            let mut results = HashMap::new();
            results.insert("lines".to_owned(), value(wanted.len() as u32));
            if let Some(pivot) = pivot {
                results.insert(result_key::PIVOT.to_owned(), value(pivot));
            }
            Ok(results)
        })
        .await
    }

    /// Identifies the language of `text`. Options: "candidates" (as) limits
    /// the answer to these languages. Results: "language" (s, absent when
    /// nothing was detected), "confidence" (d, 0 to 1), "reliable" (b).
    async fn detect_language(
        &self,
        text: String,
        options: HashMap<String, OwnedValue>,
    ) -> Result<HashMap<String, OwnedValue>> {
        self.state.activity.touch();
        let mut candidates = opt_strings(&options, "candidates")?;
        if candidates.is_empty() {
            // Only languages some model covers are useful answers, and the
            // restriction keeps look-alikes apart (Bulgarian, Macedonian).
            candidates = self.state.known_languages();
        }
        let mut end = text.len().min(MAX_DETECT_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let detection = crate::detect::detect(&text[..end], &candidates);
        let mut results = HashMap::new();
        if let Some(language) = detection.language {
            results.insert("language".to_owned(), value(language));
        }
        results.insert("confidence".to_owned(), value(detection.confidence));
        results.insert("reliable".to_owned(), value(detection.reliable));
        Ok(results)
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
                // Quality metadata is a bonus: its failures fail nothing.
                if let Err(error) = state.registry.refresh().await {
                    tracing::warn!(%error, "model registry refresh failed");
                }
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

    /// The configuration: memory_budget_mb (t), keep_warm (u),
    /// keep_warm_seconds (t), idle_exit_seconds (t), network (b),
    /// allow_prerelease (b).
    async fn get_config(&self) -> HashMap<String, OwnedValue> {
        self.state.activity.touch();
        self.state.config().to_values()
    }

    /// Changes any of GetConfig's keys (integers of any width), persists
    /// them and applies them at once. All or nothing: one bad key or value
    /// changes nothing. Emits ConfigChanged.
    async fn set_config(
        &self,
        changes: HashMap<String, OwnedValue>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<()> {
        self.state.activity.touch();
        let checked = Config::check(&changes).map_err(Error::InvalidArgument)?;
        if let Some(path) = &self.state.config_path {
            Config::save(path, &checked).map_err(Error::EngineFailure)?;
        }
        let config = {
            let mut config = self.state.config.write().expect("config lock");
            config.apply(&checked);
            config.clone()
        };
        self.state
            .provider
            .set_allow_prerelease(config.allow_prerelease);
        self.state
            .workers
            .enforce_budget(config.memory_budget_mb)
            .await;
        let _ = Self::config_changed(&emitter, config.to_values()).await;
        Ok(())
    }

    /// The new configuration after a SetConfig.
    #[zbus(signal)]
    async fn config_changed(
        emitter: &SignalEmitter<'_>,
        config: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

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
