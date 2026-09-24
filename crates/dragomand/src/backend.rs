// SPDX-License-Identifier: GPL-3.0-or-later

//! Backend selection: the real Bergamot engine (feature `bergamot`) or the
//! fake backend, chosen at daemon start. Worker spawning is funneled
//! through here so the rest of the daemon never names a concrete backend.

use dragoman_engine::fake::{FakeBackend, FakeConfig};
use dragoman_engine::{ModelSpec, Worker, marian_config};
use dragoman_models::InstalledModel;
use tokio::sync::oneshot;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum BackendKind {
    #[cfg(feature = "bergamot")]
    Bergamot,
    Fake(FakeConfig),
}

impl BackendKind {
    /// The daemon's default: the real engine when compiled in.
    pub fn default_for_build() -> Self {
        #[cfg(feature = "bergamot")]
        {
            BackendKind::Bergamot
        }
        #[cfg(not(feature = "bergamot"))]
        {
            BackendKind::Fake(FakeConfig::default())
        }
    }

    /// Spawns a worker thread holding the given model (and pivot leg).
    pub fn spawn_worker(
        &self,
        first: ModelSpec,
        second: Option<ModelSpec>,
    ) -> Result<(Worker, oneshot::Receiver<dragoman_engine::Result<()>>)> {
        match self {
            #[cfg(feature = "bergamot")]
            BackendKind::Bergamot => {
                let backend = dragoman_engine::bergamot::BergamotBackend::new(0)?;
                Ok(Worker::spawn(backend, first, second))
            }
            BackendKind::Fake(config) => {
                let backend = FakeBackend::new(config.clone());
                Ok(Worker::spawn(backend, first, second))
            }
        }
    }
}

/// Builds the engine [`ModelSpec`] for an installed model.
pub fn model_spec(installed: &InstalledModel) -> Result<ModelSpec> {
    let model = installed.file_path("model").ok_or_else(|| {
        Error::EngineFailure(format!(
            "{}: manifest has no model file",
            installed.directory.display()
        ))
    })?;
    let file_name = model
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(ModelSpec {
        files: dragoman_engine::ModelFiles {
            model,
            vocab: installed.file_path("vocab"),
            src_vocab: installed.file_path("srcvocab"),
            trg_vocab: installed.file_path("trgvocab"),
            // The lex shortlist is not used by default, matching Firefox.
            shortlist: None,
        },
        config_yaml: marian_config(&file_name),
    })
}
