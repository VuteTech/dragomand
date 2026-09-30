// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Mozilla model registry, a metadata provider only: release status and
//! evaluation scores for every model Mozilla trained, released or not.
//!
//! Models are downloaded through Remote Settings; the registry annotates
//! them. The two sources are joined on the sha256 of the decompressed model
//! file, which both publish, so an annotation always describes exactly the
//! file that is (or would be) installed.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::http::{Http, HttpError, cached_get};

pub const DEFAULT_URL: &str = "https://storage.googleapis.com/moz-fx-translations-data--303e-prod-translations-data/db/models.json";

#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Where models.json lives, [`DEFAULT_URL`] normally.
    pub url: String,
    /// `$XDG_CACHE_HOME/dragomand/registry`.
    pub cache_dir: PathBuf,
}

impl RegistryConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let cache_home = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".cache"));
        RegistryConfig {
            url: DEFAULT_URL.to_owned(),
            cache_dir: cache_home.join("dragomand/registry"),
        }
    }
}

/// What the registry says about one model file.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuality {
    /// "Release", "Release Desktop", "Nightly", ... or `None` when the model
    /// was never released.
    pub release_status: Option<String>,
    /// COMET-22 on the flores200-plus test set, 0 to 1 (higher is better).
    pub comet: Option<f64>,
}

/// The parsed registry, keyed by the sha256 of the decompressed model file.
#[derive(Debug, Default)]
pub struct Registry {
    by_model_hash: HashMap<String, ModelQuality>,
}

impl Registry {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let file: RegistryFile = serde_json::from_slice(bytes)?;
        let mut by_model_hash = HashMap::new();
        for entry in file.models.into_values().flatten() {
            let Some(hash) = entry.files.model.and_then(|m| m.uncompressed_hash) else {
                continue;
            };
            let comet = entry
                .metrics
                .get("flores200-plus")
                .and_then(|m| m.get("comet22"))
                .copied()
                .filter(|c| c.is_finite());
            by_model_hash.insert(
                hash.to_ascii_lowercase(),
                ModelQuality {
                    release_status: entry.release_status,
                    comet,
                },
            );
        }
        Ok(Registry { by_model_hash })
    }

    /// The annotation for a model file, by its sha256.
    pub fn quality(&self, model_sha256: &str) -> Option<&ModelQuality> {
        self.by_model_hash.get(&model_sha256.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.by_model_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_model_hash.is_empty()
    }
}

#[derive(Deserialize)]
struct RegistryFile {
    models: HashMap<String, Vec<RegistryEntry>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    release_status: Option<String>,
    files: RegistryFiles,
    #[serde(default)]
    metrics: HashMap<String, HashMap<String, f64>>,
}

#[derive(Deserialize)]
struct RegistryFiles {
    model: Option<RegistryModelFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryModelFile {
    uncompressed_hash: Option<String>,
}

pub struct RegistryProvider<H> {
    http: H,
    config: RegistryConfig,
}

impl<H: Http> RegistryProvider<H> {
    pub fn new(http: H, config: RegistryConfig) -> Self {
        RegistryProvider { http, config }
    }

    /// Fetches models.json (ETag-revalidated into the cache directory).
    pub async fn refresh(&self) -> Result<Registry, RegistryError> {
        let bytes = cached_get(
            &self.http,
            &self.config.url,
            &self.config.cache_dir,
            "models",
        )
        .await?;
        Ok(Registry::parse(&bytes)?)
    }

    /// The cached registry, without touching the network; `None` before the
    /// first successful refresh.
    pub fn cached(&self) -> Option<Registry> {
        let bytes = std::fs::read(self.config.cache_dir.join("models.json")).ok()?;
        Registry::parse(&bytes).ok()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("malformed model registry: {0}")]
    Parse(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::Registry;

    const SAMPLE: &str = r#"{
        "generated": "2026-09-30T00:44:58Z",
        "models": {
            "bg-en": [{
                "architecture": "base-memory",
                "releaseStatus": "Release",
                "files": {"model": {"path": "x", "uncompressedHash": "ABC123"}},
                "metrics": {"flores200-plus": {"bleu": 38.8, "comet22": 0.8719}}
            }, {
                "releaseStatus": null,
                "files": {"model": {"path": "y", "uncompressedHash": "def456"}}
            }],
            "xx-en": [{"releaseStatus": "Release", "files": {}}]
        }
    }"#;

    #[test]
    fn indexes_models_by_hash() {
        let registry = Registry::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(registry.len(), 2);
        let released = registry.quality("abc123").unwrap();
        assert_eq!(released.release_status.as_deref(), Some("Release"));
        assert_eq!(released.comet, Some(0.8719));
        let unreleased = registry.quality("DEF456").unwrap();
        assert_eq!(unreleased.release_status, None);
        assert_eq!(unreleased.comet, None);
        assert!(registry.quality("nope").is_none());
    }
}
