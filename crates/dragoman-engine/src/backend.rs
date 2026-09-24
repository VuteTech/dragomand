// SPDX-License-Identifier: GPL-3.0-or-later

//! The backend abstraction: what the daemon needs from a translation
//! engine, implemented by the real Bergamot engine ([`crate::bergamot`])
//! and by the fake backend ([`crate::fake`]) used in tests.

use std::path::PathBuf;

/// Model files for one translation direction.
///
/// Either `vocab` is set (a vocabulary shared between source and target,
/// Mozilla's usual layout) or both `src_vocab` and `trg_vocab` are.
/// `shortlist` is optional; Firefox runs without it by default.
#[derive(Debug, Clone, Default)]
pub struct ModelFiles {
    pub model: PathBuf,
    pub vocab: Option<PathBuf>,
    pub src_vocab: Option<PathBuf>,
    pub trg_vocab: Option<PathBuf>,
    pub shortlist: Option<PathBuf>,
}

/// Per-request options.
#[derive(Debug, Clone, Copy, Default)]
pub struct TranslateOptions {
    /// The segments are HTML; markup is preserved in the output.
    pub html: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The engine rejected the input or configuration.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// A model failed to load.
    #[error("model load failed: {0}")]
    Load(String),
    /// Translation failed inside the engine.
    #[error("translation failed: {0}")]
    Engine(String),
    /// The worker thread is gone (its model was unloaded, or it panicked).
    #[error("translation worker is gone")]
    WorkerGone,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A synchronous translation backend.
///
/// One backend instance belongs to one worker thread; nothing here is
/// thread-safe and nothing may block on other backends. Unloading a model is
/// dropping its `Model` value.
pub trait Backend: Send + 'static {
    type Model: Send + 'static;

    /// Loads one direction's model. `config_yaml` holds the engine options
    /// without any file paths (see docs/model-compatibility.md).
    fn load(&mut self, files: &ModelFiles, config_yaml: &str) -> Result<Self::Model>;

    /// Translates `segments` with `first`, or pivots `first` then `second`
    /// when `second` is set. Returns exactly one output per input segment.
    fn translate(
        &mut self,
        first: &Self::Model,
        second: Option<&Self::Model>,
        segments: Vec<String>,
        options: TranslateOptions,
    ) -> Result<Vec<String>>;
}
