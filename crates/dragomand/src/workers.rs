// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loaded language pairs: scheduling and lifecycle.
//!
//! Each loaded route (a direct pair, or two legs pivoting through English)
//! gets one engine worker thread plus one scheduler task with its own
//! queue. Interactive jobs go before batch jobs, and batch jobs are
//! processed in chunks so interactive requests can slip in between.
//!
//! Lifecycle policy (defaults, all configurable): routes in use are
//! pinned; recently used routes stay warm for a window; eviction is LRU;
//! loading keeps an approximate memory budget; under memory pressure
//! everything unpinned is dropped.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dragoman_engine::{TranslateOptions, Worker};
use dragoman_models::Stores;
use tokio::sync::{Notify, oneshot};

use crate::backend::{BackendKind, model_spec};
use crate::error::{Error, Result};

pub const PIVOT_LANGUAGE: &str = "en";
/// Batch jobs are translated at most this many segments at a time.
const BATCH_CHUNK: usize = 16;
/// Assumed resident cost per loaded model when the measured RSS delta is
/// smaller (allocator reuse makes deltas unreliable); docs/benchmarks.md.
const ESTIMATED_MODEL_MB: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Interactive,
    Batch,
}

/// Why a queued job ended without a translation.
#[derive(Debug)]
pub enum JobError {
    Cancelled,
    Engine(String),
}

struct QueuedJob {
    remaining: VecDeque<String>,
    done: Vec<String>,
    html: bool,
    priority: Priority,
    cancelled: Arc<AtomicBool>,
    reply: oneshot::Sender<std::result::Result<Vec<String>, JobError>>,
}

#[derive(Default)]
struct QueueInner {
    interactive: VecDeque<QueuedJob>,
    batch: VecDeque<QueuedJob>,
    closed: bool,
}

struct PairQueue {
    inner: Mutex<QueueInner>,
    notify: Notify,
    /// Jobs accepted and not yet finally answered; > 0 pins the route.
    active: AtomicUsize,
}

impl PairQueue {
    fn len(&self) -> usize {
        let inner = self.inner.lock().expect("queue lock");
        inner.interactive.len() + inner.batch.len()
    }
}

/// One loaded route.
pub struct PairEntry {
    pub source: String,
    pub target: String,
    pub pivot: Option<String>,
    /// Approximate resident cost of this route's models.
    pub cost_mb: u64,
    queue: Arc<PairQueue>,
    last_used: Mutex<Instant>,
}

impl PairEntry {
    /// Queues a job and returns a receiver for its outcome, or `Err(())`
    /// when the route was unloaded under the caller (reload and retry).
    #[allow(clippy::result_unit_err)]
    pub fn submit(
        &self,
        segments: Vec<String>,
        html: bool,
        priority: Priority,
        cancelled: Arc<AtomicBool>,
    ) -> std::result::Result<oneshot::Receiver<std::result::Result<Vec<String>, JobError>>, ()>
    {
        let (reply, receiver) = oneshot::channel();
        let job = QueuedJob {
            remaining: segments.into(),
            done: Vec::new(),
            html,
            priority,
            cancelled,
            reply,
        };
        {
            let mut inner = self.queue.inner.lock().expect("queue lock");
            if inner.closed {
                return Err(());
            }
            self.queue.active.fetch_add(1, Ordering::SeqCst);
            match job.priority {
                Priority::Interactive => inner.interactive.push_back(job),
                Priority::Batch => inner.batch.push_back(job),
            }
        }
        *self.last_used.lock().expect("last_used lock") = Instant::now();
        self.queue.notify.notify_one();
        Ok(receiver)
    }

    fn is_pinned(&self) -> bool {
        self.queue.active.load(Ordering::SeqCst) > 0
    }

    fn idle_for(&self) -> Duration {
        self.last_used.lock().expect("last_used lock").elapsed()
    }
}

impl Drop for PairEntry {
    fn drop(&mut self) {
        let mut inner = self.queue.inner.lock().expect("queue lock");
        inner.closed = true;
        drop(inner);
        self.queue.notify.notify_one();
    }
}

/// The registry of loaded routes.
pub struct PairWorkers {
    backend: BackendKind,
    entries: tokio::sync::Mutex<HashMap<(String, String), Arc<PairEntry>>>,
}

impl PairWorkers {
    pub fn new(backend: BackendKind) -> Self {
        PairWorkers {
            backend,
            entries: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// The route that would serve `source -> target`, without loading it:
    /// `Ok(None)` for a direct model, `Ok(Some("en"))` for a pivot.
    pub fn plan_route(
        stores: &Stores,
        source: &str,
        target: &str,
        allow_pivot: bool,
    ) -> Result<Option<String>> {
        if stores.resolve(source, target).is_some() {
            return Ok(None);
        }
        if !allow_pivot || source == PIVOT_LANGUAGE || target == PIVOT_LANGUAGE {
            return Err(Error::NotInstalled(format!("{source}-{target}")));
        }
        let mut missing = Vec::new();
        if stores.resolve(source, PIVOT_LANGUAGE).is_none() {
            missing.push(format!("{source}-{PIVOT_LANGUAGE}"));
        }
        if stores.resolve(PIVOT_LANGUAGE, target).is_none() {
            missing.push(format!("{PIVOT_LANGUAGE}-{target}"));
        }
        if missing.is_empty() {
            Ok(Some(PIVOT_LANGUAGE.to_owned()))
        } else {
            Err(Error::NotInstalled(missing.join(", ")))
        }
    }

    /// Returns the loaded entry for the route, loading it first when
    /// needed and keeping the memory budget by evicting LRU idle routes.
    pub async fn ensure_loaded(
        &self,
        stores: &Stores,
        source: &str,
        target: &str,
        allow_pivot: bool,
        budget_mb: u64,
    ) -> Result<Arc<PairEntry>> {
        let key = (source.to_owned(), target.to_owned());
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.get(&key) {
            return Ok(Arc::clone(entry));
        }

        let pivot = Self::plan_route(stores, source, target, allow_pivot)?;
        let (first, second) = match &pivot {
            None => {
                let installed = stores
                    .resolve(source, target)
                    .ok_or_else(|| Error::NotInstalled(format!("{source}-{target}")))?;
                (model_spec(&installed)?, None)
            }
            Some(pivot_language) => {
                let first = stores
                    .resolve(source, pivot_language)
                    .ok_or_else(|| Error::NotInstalled(format!("{source}-{pivot_language}")))?;
                let second = stores
                    .resolve(pivot_language, target)
                    .ok_or_else(|| Error::NotInstalled(format!("{pivot_language}-{target}")))?;
                (model_spec(&first)?, Some(model_spec(&second)?))
            }
        };
        let models = 1 + second.is_some() as u64;

        let rss_before = resident_mb();
        let (worker, ready) = self.backend.spawn_worker(first, second)?;
        ready
            .await
            .map_err(|_| Error::EngineFailure("worker exited during load".into()))??;
        let rss_after = resident_mb();
        let estimated = models * ESTIMATED_MODEL_MB;
        let measured = rss_after.saturating_sub(rss_before);
        let cost_mb = measured.max(estimated);
        tracing::info!(
            route = format!("{source}-{target}"),
            measured_rss_delta_mb = measured,
            cost_mb,
            "route loaded"
        );

        let queue = Arc::new(PairQueue {
            inner: Mutex::new(QueueInner::default()),
            notify: Notify::new(),
            active: AtomicUsize::new(0),
        });
        tokio::spawn(run_scheduler(worker, Arc::clone(&queue)));

        let entry = Arc::new(PairEntry {
            source: source.to_owned(),
            target: target.to_owned(),
            pivot,
            cost_mb,
            queue,
            last_used: Mutex::new(Instant::now()),
        });
        entries.insert(key, Arc::clone(&entry));

        // Enforce the budget now that the new route is in; the newest
        // entries have fresh last_used stamps, so LRU spares them.
        Self::evict_over_budget(&mut entries, budget_mb);
        Ok(entry)
    }

    fn evict_over_budget(entries: &mut HashMap<(String, String), Arc<PairEntry>>, budget_mb: u64) {
        loop {
            let total: u64 = entries.values().map(|e| e.cost_mb).sum();
            if total <= budget_mb {
                return;
            }
            let victim = entries
                .iter()
                .filter(|(_, e)| !e.is_pinned())
                .max_by_key(|(_, e)| e.idle_for())
                .map(|(k, _)| k.clone());
            match victim {
                Some(key) => {
                    tracing::info!(route = format!("{}-{}", key.0, key.1), "evicting (budget)");
                    entries.remove(&key);
                }
                None => return, // Everything is pinned; active work wins.
            }
        }
    }

    /// Periodic policy sweep: drop routes idle beyond the keep-warm
    /// window, and keep at most `keep_warm` unpinned routes (LRU order).
    pub async fn sweep(&self, keep_warm: usize, window: Duration) {
        let mut entries = self.entries.lock().await;
        let expired: Vec<_> = entries
            .iter()
            .filter(|(_, e)| !e.is_pinned() && e.idle_for() >= window)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired {
            tracing::info!(route = format!("{}-{}", key.0, key.1), "evicting (idle)");
            entries.remove(&key);
        }
        let mut unpinned: Vec<_> = entries
            .iter()
            .filter(|(_, e)| !e.is_pinned())
            .map(|(k, e)| (k.clone(), e.idle_for()))
            .collect();
        if unpinned.len() > keep_warm {
            let excess = unpinned.len() - keep_warm;
            unpinned.sort_by_key(|(_, idle)| std::cmp::Reverse(*idle));
            for (key, _) in unpinned.into_iter().take(excess) {
                tracing::info!(
                    route = format!("{}-{}", key.0, key.1),
                    "evicting (keep-warm capacity)"
                );
                entries.remove(&key);
            }
        }
    }

    /// Memory pressure: unload everything that is not actively working.
    pub async fn evict_unpinned(&self) {
        let mut entries = self.entries.lock().await;
        let before = entries.len();
        entries.retain(|_, e| e.is_pinned());
        tracing::info!(evicted = before - entries.len(), "memory pressure eviction");
    }

    /// Unloads every route touching this pair (used before removal).
    pub async fn unload_pair(&self, source: &str, target: &str) {
        let mut entries = self.entries.lock().await;
        entries.retain(|_, entry| {
            !(entry.source == source && entry.target == target
                || entry.pivot.is_some() && (entry.source == source || entry.target == target))
        });
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.lock().await.is_empty()
    }

    /// (loaded routes, total queued jobs, model cost MiB) for GetStatus.
    pub async fn status(&self) -> (Vec<String>, usize, u64) {
        let entries = self.entries.lock().await;
        let mut routes: Vec<String> = entries
            .values()
            .map(|e| match &e.pivot {
                Some(p) => format!("{}-{} (via {p})", e.source, e.target),
                None => format!("{}-{}", e.source, e.target),
            })
            .collect();
        routes.sort();
        let queued = entries.values().map(|e| e.queue.len()).sum();
        let cost = entries.values().map(|e| e.cost_mb).sum();
        (routes, queued, cost)
    }
}

/// Resident set size of this process in MiB (0 when unreadable).
pub fn resident_mb() -> u64 {
    let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    let pages: u64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|f| f.parse().ok())
        .unwrap_or(0);
    pages * 4096 / (1024 * 1024)
}

/// The per-route scheduler: pops interactive jobs first, translates batch
/// jobs one chunk at a time, and ends when the queue closes.
async fn run_scheduler(worker: Worker, queue: Arc<PairQueue>) {
    // Sends the final answer for an accepted job.
    let finish = |job_reply: oneshot::Sender<std::result::Result<Vec<String>, JobError>>,
                  result: std::result::Result<Vec<String>, JobError>| {
        queue.active.fetch_sub(1, Ordering::SeqCst);
        let _ = job_reply.send(result);
    };

    loop {
        let job = {
            let mut inner = queue.inner.lock().expect("queue lock");
            match inner
                .interactive
                .pop_front()
                .or_else(|| inner.batch.pop_front())
            {
                Some(job) => Some(job),
                None if inner.closed => break,
                None => None,
            }
        };
        let Some(mut job) = job else {
            queue.notify.notified().await;
            continue;
        };

        if job.cancelled.load(Ordering::SeqCst) {
            finish(job.reply, Err(JobError::Cancelled));
            continue;
        }

        let chunk: Vec<String> = match job.priority {
            Priority::Interactive => job.remaining.drain(..).collect(),
            Priority::Batch => {
                let n = job.remaining.len().min(BATCH_CHUNK);
                job.remaining.drain(..n).collect()
            }
        };
        let options = TranslateOptions { html: job.html };
        match worker.translate(chunk, options).await {
            Ok(translations) => {
                job.done.extend(translations);
                if job.remaining.is_empty() {
                    let done = std::mem::take(&mut job.done);
                    finish(job.reply, Ok(done));
                } else {
                    // Back to the front, behind any interactive arrivals.
                    let mut inner = queue.inner.lock().expect("queue lock");
                    inner.batch.push_front(job);
                }
            }
            Err(error) => {
                finish(job.reply, Err(JobError::Engine(error.to_string())));
            }
        }
    }
    // Queue closed: answer whatever is left, then drop the worker (which
    // joins its thread and unloads the models).
    let mut inner = queue.inner.lock().expect("queue lock");
    let mut leftovers: Vec<QueuedJob> = std::mem::take(&mut inner.interactive).into();
    leftovers.extend(std::mem::take(&mut inner.batch));
    drop(inner);
    for job in leftovers {
        finish(job.reply, Err(JobError::Cancelled));
    }
    // Worker::drop joins the engine thread; keep that off the async pool.
    // The JoinHandle is intentionally detached.
    drop(tokio::task::spawn_blocking(move || drop(worker)));
}
