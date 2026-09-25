// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The per-model manifest, `manifest.json` inside every installed model
//! directory. It makes the store self-describing: the daemon needs no
//! network to know what it has.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Manifest schema version, [`SCHEMA_VERSION`].
    pub schema: u32,
    /// Provider id, for example `mozilla-remote-settings`.
    pub provider: String,
    pub source: String,
    pub target: String,
    pub version: String,
    /// Quality/size tier as the provider reports it (`tiny`, `base`,
    /// `base-memory`, …).
    #[serde(default)]
    pub architecture: Option<String>,
    /// Model license, when the provider states one. Must be confirmed
    /// before distribution packages ship models (docs/packaging.md "Model packages").
    #[serde(default)]
    pub license: Option<String>,
    /// Where the files came from.
    #[serde(default)]
    pub source_urls: Vec<String>,
    /// Install time, RFC 3339 UTC.
    pub installed_at: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub name: String,
    /// `model`, `vocab`, `srcvocab`, `trgvocab` or `lex`.
    pub role: String,
    /// Size and sha256 of the file as stored (decompressed).
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid manifest {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("unsupported manifest schema {found} in {path} (supported: {SCHEMA_VERSION})")]
    Schema { path: String, found: u32 },
}

impl Manifest {
    pub fn file(&self, role: &str) -> Option<&ManifestFile> {
        self.files.iter().find(|f| f.role == role)
    }

    pub fn load(dir: &Path) -> Result<Self, ManifestError> {
        let path = dir.join(MANIFEST_FILE);
        let display = path.display().to_string();
        let bytes = std::fs::read(&path).map_err(|source| ManifestError::Io {
            path: display.clone(),
            source,
        })?;
        let manifest: Manifest =
            serde_json::from_slice(&bytes).map_err(|source| ManifestError::Parse {
                path: display.clone(),
                source,
            })?;
        if manifest.schema != SCHEMA_VERSION {
            return Err(ManifestError::Schema {
                path: display,
                found: manifest.schema,
            });
        }
        Ok(manifest)
    }

    pub fn store(&self, dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).expect("manifest serializes");
        std::fs::write(dir.join(MANIFEST_FILE), json)
    }
}

/// Current time as RFC 3339 UTC, seconds precision, without a time crate.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Civil-from-days (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest {
            schema: SCHEMA_VERSION,
            provider: "mozilla-remote-settings".into(),
            source: "bg".into(),
            target: "en".into(),
            version: "3.0".into(),
            architecture: Some("base-memory".into()),
            license: None,
            source_urls: vec!["https://example.test/x".into()],
            installed_at: now_rfc3339(),
            files: vec![ManifestFile {
                name: "model.bgen.intgemm.alphas.bin".into(),
                role: "model".into(),
                size: 3,
                sha256: "abc".into(),
            }],
        };
        manifest.store(dir.path()).unwrap();
        let loaded = Manifest::load(dir.path()).unwrap();
        assert_eq!(loaded.version, "3.0");
        assert_eq!(loaded.file("model").unwrap().size, 3);
        assert!(loaded.file("lex").is_none());
    }

    #[test]
    fn future_schema_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_FILE),
            r#"{"schema": 99, "provider": "p", "source": "a", "target": "b",
                "version": "3.0", "installed_at": "t", "files": []}"#,
        )
        .unwrap();
        assert!(matches!(
            Manifest::load(dir.path()),
            Err(ManifestError::Schema { found: 99, .. })
        ));
    }

    #[test]
    fn timestamp_shape() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.starts_with("20") && ts.ends_with('Z'));
    }
}
