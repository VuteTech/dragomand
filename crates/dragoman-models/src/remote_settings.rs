// SPDX-License-Identifier: GPL-3.0-or-later

//! The Firefox Remote Settings model provider: exactly the files Firefox
//! installs, from the `translations-models-v2` collection. See
//! docs/model-compatibility.md for the acceptance rule and the zstd/hash
//! scheme.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::http::{FetchResult, Http, HttpError};
use crate::manifest::{Manifest, ManifestFile, now_rfc3339};
use crate::records::{FileType, FilterEnv, ModelRecord, ModelSet, accepted_sets};
use crate::store::{InstalledModel, Origin, Stores};
use crate::version::MozVersion;

pub const PROVIDER_ID: &str = "mozilla-remote-settings";
pub const DEFAULT_SERVER: &str = "https://firefox.settings.services.mozilla.com/v1";
const COLLECTION: &str = "translations-models-v2";

#[derive(Debug, Clone)]
pub struct RemoteSettingsConfig {
    /// Remote Settings server root, [`DEFAULT_SERVER`] normally.
    pub server: String,
    /// Opt into pre-release (nightly-gated) models.
    pub allow_prerelease: bool,
    /// Also download the optional `lex` shortlist files (Firefox does not
    /// use them by default).
    pub download_lex: bool,
    /// Cache directory for records and ETags
    /// (`$XDG_CACHE_HOME/dragomand/remote-settings`).
    pub cache_dir: PathBuf,
}

impl RemoteSettingsConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let cache_home = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".cache"));
        RemoteSettingsConfig {
            server: DEFAULT_SERVER.to_owned(),
            allow_prerelease: false,
            download_lex: false,
            cache_dir: cache_home.join("dragomand/remote-settings"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("unexpected server response from {url}: {message}")]
    BadResponse { url: String, message: String },
    #[error("verification failed for {what}: {message}")]
    Verify { what: String, message: String },
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("no accepted records for pair {0}")]
    NoSuchPair(String),
}

type Result<T> = std::result::Result<T, ProviderError>;

fn io_err(path: &Path, source: std::io::Error) -> ProviderError {
    ProviderError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// What the provider knows about the remote state.
#[derive(Debug)]
pub struct Available {
    pub sets: Vec<ModelSet>,
    /// Attachment download base URL from the server capabilities.
    pub attachments_base: String,
    /// `filter_expression`s we did not understand (their records skipped).
    pub skipped_filters: Vec<String>,
}

#[derive(Debug)]
pub struct UpdateInfo {
    pub source: String,
    pub target: String,
    pub installed: MozVersion,
    pub available: MozVersion,
}

pub struct RemoteSettingsProvider<H> {
    http: H,
    config: RemoteSettingsConfig,
}

#[derive(Deserialize)]
struct RecordsResponse {
    data: Vec<ModelRecord>,
}

#[derive(Deserialize)]
struct ServerInfo {
    capabilities: Capabilities,
}
#[derive(Deserialize)]
struct Capabilities {
    attachments: AttachmentsCapability,
}
#[derive(Deserialize)]
struct AttachmentsCapability {
    base_url: String,
}

impl<H: Http> RemoteSettingsProvider<H> {
    pub fn new(http: H, config: RemoteSettingsConfig) -> Self {
        RemoteSettingsProvider { http, config }
    }

    /// The underlying HTTP implementation (tests observe traffic through
    /// this).
    pub fn http_ref(&self) -> &H {
        &self.http
    }

    /// Fetches (or revalidates) the records and returns the accepted model
    /// sets. Falls back to the on-disk cache when the network is
    /// unreachable.
    pub async fn available(&self) -> Result<Available> {
        let records_url = format!(
            "{}/buckets/main/collections/{COLLECTION}/records",
            self.config.server
        );
        let records_bytes = self.cached_get(&records_url, "records").await?;
        let records: RecordsResponse =
            serde_json::from_slice(&records_bytes).map_err(|e| ProviderError::BadResponse {
                url: records_url.clone(),
                message: e.to_string(),
            })?;

        let server_url = format!("{}/", self.config.server);
        let info_bytes = self.cached_get(&server_url, "server-info").await?;
        let info: ServerInfo =
            serde_json::from_slice(&info_bytes).map_err(|e| ProviderError::BadResponse {
                url: server_url,
                message: e.to_string(),
            })?;

        let env = FilterEnv::prerelease(self.config.allow_prerelease);
        let mut skipped_filters = Vec::new();
        let sets = accepted_sets(&records.data, &env, &mut skipped_filters);
        Ok(Available {
            sets,
            attachments_base: info.capabilities.attachments.base_url,
            skipped_filters,
        })
    }

    /// Like [`Self::available`], but strictly offline: reads only the
    /// cached records (written by an earlier `available` call). `None`
    /// when nothing is cached yet. Listing must never touch the network;
    /// only installs and update checks may.
    pub fn available_cached(&self) -> Option<Available> {
        let records = std::fs::read(self.config.cache_dir.join("records.json")).ok()?;
        let records: RecordsResponse = serde_json::from_slice(&records).ok()?;
        let info = std::fs::read(self.config.cache_dir.join("server-info.json")).ok()?;
        let info: ServerInfo = serde_json::from_slice(&info).ok()?;
        let env = FilterEnv::prerelease(self.config.allow_prerelease);
        let mut skipped_filters = Vec::new();
        let sets = accepted_sets(&records.data, &env, &mut skipped_filters);
        Some(Available {
            sets,
            attachments_base: info.capabilities.attachments.base_url,
            skipped_filters,
        })
    }

    /// GET with ETag revalidation backed by files in the cache directory.
    async fn cached_get(&self, url: &str, cache_name: &str) -> Result<Vec<u8>> {
        let body_path = self.config.cache_dir.join(format!("{cache_name}.json"));
        let etag_path = self.config.cache_dir.join(format!("{cache_name}.etag"));
        let cached_body = std::fs::read(&body_path).ok();
        let cached_etag = std::fs::read_to_string(&etag_path).ok();

        let etag = cached_body
            .is_some()
            .then_some(cached_etag.as_deref())
            .flatten();
        match self.http.get(url, etag).await {
            Ok(FetchResult::NotModified) => {
                Ok(cached_body.expect("etag was only sent with a cached body"))
            }
            Ok(FetchResult::Fetched { bytes, etag }) => {
                let _ = std::fs::create_dir_all(&self.config.cache_dir);
                let _ = std::fs::write(&body_path, &bytes);
                match etag {
                    Some(etag) => {
                        let _ = std::fs::write(&etag_path, etag);
                    }
                    None => {
                        let _ = std::fs::remove_file(&etag_path);
                    }
                }
                Ok(bytes)
            }
            // Network trouble: stale cache beats nothing at all.
            Err(error) => match cached_body {
                Some(bytes) => Ok(bytes),
                None => Err(error.into()),
            },
        }
    }

    /// Downloads, verifies and installs one model set into the user store.
    /// Returns the existing copy when this exact version is already there.
    pub async fn install(
        &self,
        stores: &Stores,
        set: &ModelSet,
        attachments_base: &str,
    ) -> Result<InstalledModel> {
        let _lock = stores.lock_user()?;

        let pair_dir = stores
            .user
            .join(PROVIDER_ID)
            .join(format!("{}-{}", set.source, set.target));
        let final_dir = pair_dir.join(set.version.as_str());
        if final_dir.join(crate::manifest::MANIFEST_FILE).exists() {
            let mut problems = Vec::new();
            let mut found = Vec::new();
            crate::store::scan_model_dir(&final_dir, Origin::User, &mut found, &mut problems);
            if let Some(model) = found.pop() {
                return Ok(model);
            }
            // Damaged leftover: replace it.
            std::fs::remove_dir_all(&final_dir).map_err(|e| io_err(&final_dir, e))?;
        }

        let tmp_dir = pair_dir.join(format!(
            ".tmp-{}-{}",
            set.version.as_str(),
            std::process::id()
        ));
        if tmp_dir.exists() {
            std::fs::remove_dir_all(&tmp_dir).map_err(|e| io_err(&tmp_dir, e))?;
        }
        std::fs::create_dir_all(&tmp_dir).map_err(|e| io_err(&tmp_dir, e))?;

        let mut manifest_files = Vec::new();
        let mut source_urls = Vec::new();
        for (file_type, record) in &set.files {
            let role = match file_type {
                FileType::Model => "model",
                FileType::Vocab => "vocab",
                FileType::SrcVocab => "srcvocab",
                FileType::TrgVocab => "trgvocab",
                FileType::Lex => {
                    if !self.config.download_lex {
                        continue;
                    }
                    "lex"
                }
                FileType::Unknown => continue,
            };
            let url = format!("{attachments_base}{}", record.attachment.location);
            let bytes = match self.http.get(&url, None).await? {
                FetchResult::Fetched { bytes, .. } => bytes,
                FetchResult::NotModified => {
                    return Err(ProviderError::BadResponse {
                        url,
                        message: "unexpected 304 without validator".into(),
                    });
                }
            };
            verify(
                &record.name,
                "download",
                bytes.len() as u64,
                record.attachment.size,
                &sha256_hex(&bytes),
                &record.attachment.hash,
            )?;
            let decompressed =
                zstd::stream::decode_all(bytes.as_slice()).map_err(|e| ProviderError::Verify {
                    what: record.name.clone(),
                    message: format!("zstd decompression failed: {e}"),
                })?;
            verify(
                &record.name,
                "decompressed file",
                decompressed.len() as u64,
                record.decompressed_size,
                &sha256_hex(&decompressed),
                &record.decompressed_hash,
            )?;

            let path = tmp_dir.join(&record.name);
            write_fsync(&path, &decompressed)?;
            manifest_files.push(ManifestFile {
                name: record.name.clone(),
                role: role.to_owned(),
                size: record.decompressed_size,
                sha256: record.decompressed_hash.clone(),
            });
            source_urls.push(url);
        }

        let manifest = Manifest {
            schema: crate::manifest::SCHEMA_VERSION,
            provider: PROVIDER_ID.to_owned(),
            source: set.source.clone(),
            target: set.target.clone(),
            version: set.version.as_str().to_owned(),
            architecture: set.architecture.clone(),
            // The records themselves carry no license field, but the
            // mozilla/translations README states the model files are
            // MPL-2.0; see docs/packaging.md "Model packages" for the provenance chain.
            license: Some("MPL-2.0".to_owned()),
            source_urls,
            installed_at: now_rfc3339(),
            files: manifest_files,
        };
        manifest.store(&tmp_dir).map_err(|e| io_err(&tmp_dir, e))?;
        fsync_dir(&tmp_dir)?;
        std::fs::rename(&tmp_dir, &final_dir).map_err(|e| io_err(&final_dir, e))?;
        fsync_dir(&pair_dir)?;

        Ok(InstalledModel {
            origin: Origin::User,
            directory: final_dir,
            version: set.version.clone(),
            manifest,
        })
    }

    /// Installs the accepted set for one pair (used by install-on-demand).
    pub async fn install_pair(
        &self,
        stores: &Stores,
        source: &str,
        target: &str,
    ) -> Result<InstalledModel> {
        let available = self.available().await?;
        let set = available
            .sets
            .iter()
            .find(|s| s.source == source && s.target == target && s.variant.is_none())
            .ok_or_else(|| ProviderError::NoSuchPair(format!("{source}-{target}")))?;
        self.install(stores, set, &available.attachments_base).await
    }

    /// Compares installed pairs with the accepted remote sets.
    pub async fn check_for_updates(&self, stores: &Stores) -> Result<Vec<UpdateInfo>> {
        let available = self.available().await?;
        let mut problems = Vec::new();
        let installed = stores.installed_by_pair(&mut problems);
        let mut updates = Vec::new();
        for set in &available.sets {
            let Some(best) = installed.get(&set.pair()).and_then(|v| v.first()) else {
                continue;
            };
            if set.version > best.version {
                updates.push(UpdateInfo {
                    source: set.source.clone(),
                    target: set.target.clone(),
                    installed: best.version.clone(),
                    available: set.version.clone(),
                });
            }
        }
        Ok(updates)
    }
}

fn verify(
    name: &str,
    stage: &str,
    size: u64,
    expected_size: u64,
    hash: &str,
    expected_hash: &str,
) -> Result<()> {
    if size != expected_size {
        return Err(ProviderError::Verify {
            what: name.to_owned(),
            message: format!("{stage} size {size} != expected {expected_size}"),
        });
    }
    if !hash.eq_ignore_ascii_case(expected_hash) {
        return Err(ProviderError::Verify {
            what: name.to_owned(),
            message: format!("{stage} sha256 mismatch"),
        });
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_fsync(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path).map_err(|e| io_err(path, e))?;
    file.write_all(bytes).map_err(|e| io_err(path, e))?;
    file.sync_all().map_err(|e| io_err(path, e))?;
    Ok(())
}

fn fsync_dir(path: &Path) -> Result<()> {
    let dir = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
    dir.sync_all().map_err(|e| io_err(path, e))?;
    Ok(())
}
