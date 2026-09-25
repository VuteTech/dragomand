// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Local model stores.
//!
//! Read-only system stores (`$XDG_DATA_DIRS/dragomand/models`, filled by
//! distribution packages) plus one writable user store
//! (`$XDG_DATA_HOME/dragomand/models`, where downloads land). Layout:
//! `<store>/<provider>/<src>-<trg>/<version>/` with a `manifest.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::manifest::Manifest;
use crate::records::{MODEL_MAJOR_MAX, MODEL_MAJOR_MIN};
use crate::version::MozVersion;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Verify(String),
    #[error("model not installed: {0}")]
    NotInstalled(String),
}

type Result<T> = std::result::Result<T, StoreError>;

fn io_err(path: &Path, source: std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Where a model copy lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    System,
    User,
}

/// One installed model version found during a scan.
#[derive(Debug, Clone)]
pub struct InstalledModel {
    pub origin: Origin,
    pub directory: PathBuf,
    pub version: MozVersion,
    pub manifest: Manifest,
}

impl InstalledModel {
    pub fn pair(&self) -> String {
        format!("{}-{}", self.manifest.source, self.manifest.target)
    }

    pub fn file_path(&self, role: &str) -> Option<PathBuf> {
        self.manifest
            .file(role)
            .map(|f| self.directory.join(&f.name))
    }
}

/// The set of stores. Build with [`Stores::from_env`] normally, or with
/// explicit paths in tests and `dragomanctl store --root`.
#[derive(Debug, Clone)]
pub struct Stores {
    pub system: Vec<PathBuf>,
    pub user: PathBuf,
}

impl Stores {
    /// Resolves store directories from the XDG environment.
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|s| !s.is_empty());
        let data_dirs = data_dirs
            .as_deref()
            .unwrap_or("/usr/local/share:/usr/share");
        let system = data_dirs
            .split(':')
            .filter(|p| !p.is_empty())
            .map(|p| Path::new(p).join("dragomand/models"))
            .collect();
        Stores {
            system,
            user: data_home.join("dragomand/models"),
        }
    }

    /// Scans every store. Returns all readable installed models; models with
    /// unreadable or wrong-schema manifests, and files whose sizes do not
    /// match the manifest, are skipped (with the reason collected in
    /// `problems`).
    pub fn scan(&self, problems: &mut Vec<String>) -> Vec<InstalledModel> {
        let mut found = Vec::new();
        for (origin, root) in self.roots() {
            scan_store(root, origin, &mut found, problems);
        }
        found
    }

    fn roots(&self) -> impl Iterator<Item = (Origin, &Path)> {
        std::iter::once((Origin::User, self.user.as_path()))
            .chain(self.system.iter().map(|p| (Origin::System, p.as_path())))
    }

    /// The best installed copy of a pair: the newest version inside the
    /// supported major window; on a version tie the system copy wins.
    pub fn resolve(&self, source: &str, target: &str) -> Option<InstalledModel> {
        let mut problems = Vec::new();
        let pair = format!("{source}-{target}");
        self.scan(&mut problems)
            .into_iter()
            .filter(|m| m.pair() == pair)
            .filter(|m| (MODEL_MAJOR_MIN..=MODEL_MAJOR_MAX).contains(&m.version.major()))
            .max_by(|a, b| {
                (a.version.clone(), a.origin == Origin::System)
                    .cmp(&(b.version.clone(), b.origin == Origin::System))
            })
    }

    /// Groups every installed model by pair, newest first.
    pub fn installed_by_pair(
        &self,
        problems: &mut Vec<String>,
    ) -> BTreeMap<String, Vec<InstalledModel>> {
        let mut map: BTreeMap<String, Vec<InstalledModel>> = BTreeMap::new();
        for model in self.scan(problems) {
            map.entry(model.pair()).or_default().push(model);
        }
        for models in map.values_mut() {
            models.sort_by(|a, b| b.version.cmp(&a.version));
        }
        map
    }

    /// Takes the user-store lock (blocking), creating the store if needed.
    /// The daemon and `dragomanctl store` hold it around installs and
    /// removals. Released when the returned guard drops.
    pub fn lock_user(&self) -> Result<StoreLock> {
        std::fs::create_dir_all(&self.user).map_err(|e| io_err(&self.user, e))?;
        let path = self.user.join(".lock");
        let file = std::fs::File::create(&path).map_err(|e| io_err(&path, e))?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
            .map_err(|e| io_err(&path, e.into()))?;
        Ok(StoreLock { _file: file })
    }

    /// Fully re-verifies one installed model (sizes and sha256 of every
    /// file against its manifest).
    pub fn verify(model: &InstalledModel) -> Result<()> {
        for file in &model.manifest.files {
            let path = model.directory.join(&file.name);
            let metadata = std::fs::metadata(&path).map_err(|e| io_err(&path, e))?;
            if metadata.len() != file.size {
                return Err(StoreError::Verify(format!(
                    "{}: size {} != manifest {}",
                    path.display(),
                    metadata.len(),
                    file.size
                )));
            }
            let digest = sha256_file(&path)?;
            if digest != file.sha256 {
                return Err(StoreError::Verify(format!(
                    "{}: sha256 mismatch",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    /// Removes one installed version from the user store.
    pub fn remove(&self, model: &InstalledModel) -> Result<()> {
        if model.origin != Origin::User {
            return Err(StoreError::Verify(format!(
                "{} is in a read-only system store",
                model.directory.display()
            )));
        }
        std::fs::remove_dir_all(&model.directory).map_err(|e| io_err(&model.directory, e))?;
        // Prune the now-possibly-empty pair directory; ignore failures.
        if let Some(parent) = model.directory.parent() {
            let _ = std::fs::remove_dir(parent);
        }
        Ok(())
    }
}

/// Held while writing to the user store; the `flock` releases when the
/// file descriptor closes on drop.
pub struct StoreLock {
    _file: std::fs::File,
}

fn scan_store(
    root: &Path,
    origin: Origin,
    found: &mut Vec<InstalledModel>,
    problems: &mut Vec<String>,
) {
    let providers = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return, // Missing store directories are normal.
    };
    for provider in providers.flatten().filter(is_dir) {
        let pairs = match std::fs::read_dir(provider.path()) {
            Ok(entries) => entries,
            Err(e) => {
                problems.push(format!("{}: {e}", provider.path().display()));
                continue;
            }
        };
        for pair in pairs.flatten().filter(is_dir) {
            let versions = match std::fs::read_dir(pair.path()) {
                Ok(entries) => entries,
                Err(e) => {
                    problems.push(format!("{}: {e}", pair.path().display()));
                    continue;
                }
            };
            for version_dir in versions.flatten().filter(is_dir) {
                if version_dir
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".tmp-")
                {
                    continue; // Unfinished install.
                }
                scan_model_dir(&version_dir.path(), origin, found, problems);
            }
        }
    }
}

pub(crate) fn scan_model_dir(
    dir: &Path,
    origin: Origin,
    found: &mut Vec<InstalledModel>,
    problems: &mut Vec<String>,
) {
    let manifest = match Manifest::load(dir) {
        Ok(manifest) => manifest,
        Err(e) => {
            problems.push(e.to_string());
            return;
        }
    };
    let version = match MozVersion::parse(&manifest.version) {
        Ok(version) => version,
        Err(e) => {
            problems.push(format!("{}: {e}", dir.display()));
            return;
        }
    };
    // Cheap integrity check on every scan: file sizes only.
    for file in &manifest.files {
        let path = dir.join(&file.name);
        match std::fs::metadata(&path) {
            Ok(m) if m.len() == file.size => {}
            Ok(m) => {
                problems.push(format!(
                    "{}: size {} != manifest {}",
                    path.display(),
                    m.len(),
                    file.size
                ));
                return;
            }
            Err(e) => {
                problems.push(format!("{}: {e}", path.display()));
                return;
            }
        }
    }
    found.push(InstalledModel {
        origin,
        directory: dir.to_path_buf(),
        version,
        manifest,
    });
}

fn is_dir(entry: &std::fs::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| io_err(path, e))?;
    Ok(format!("{:x}", hasher.finalize()))
}
