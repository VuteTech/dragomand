// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A fake [`Backend`] for tests: deterministic output, configurable delays
//! and failure injection, no C++ and no model files.

use std::time::Duration;

use crate::backend::{Backend, Error, ModelFiles, Result, TranslateOptions};

/// Configuration for [`FakeBackend`]. The defaults succeed instantly.
#[derive(Debug, Clone, Default)]
pub struct FakeConfig {
    pub load_delay: Duration,
    pub translate_delay: Duration,
    /// When set, every load fails with this message.
    pub fail_load: Option<String>,
    /// When set, every translation fails with this message.
    pub fail_translate: Option<String>,
}

pub struct FakeBackend {
    config: FakeConfig,
}

/// The fake's "model": tagged with the model file's stem, so outputs show
/// which model (or pivot chain) produced them.
pub struct FakeModel {
    tag: String,
}

impl FakeBackend {
    pub fn new(config: FakeConfig) -> Self {
        FakeBackend { config }
    }
}

impl Backend for FakeBackend {
    type Model = FakeModel;

    fn load(&mut self, files: &ModelFiles, _config_yaml: &str) -> Result<FakeModel> {
        std::thread::sleep(self.config.load_delay);
        if let Some(message) = &self.config.fail_load {
            return Err(Error::Load(message.clone()));
        }
        let tag = files
            .model
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "model".to_owned());
        Ok(FakeModel { tag })
    }

    fn translate(
        &mut self,
        first: &FakeModel,
        second: Option<&FakeModel>,
        segments: Vec<String>,
        options: TranslateOptions,
    ) -> Result<Vec<String>> {
        std::thread::sleep(self.config.translate_delay);
        if let Some(message) = &self.config.fail_translate {
            return Err(Error::Engine(message.clone()));
        }
        let tag = match second {
            Some(second) => format!("{}+{}", first.tag, second.tag),
            None => first.tag.clone(),
        };
        let html = if options.html { ",html" } else { "" };
        Ok(segments
            .into_iter()
            .map(|s| format!("[{tag}{html}] {s}"))
            .collect())
    }
}
