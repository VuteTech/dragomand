// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Store scanning, version resolution and precedence rules.

use std::path::Path;

use sha2::{Digest, Sha256};

use dragoman_models::manifest::{Manifest, ManifestFile, SCHEMA_VERSION, now_rfc3339};
use dragoman_models::{Origin, Stores};

/// Creates `<store>/<provider>/<pair>/<version>/` with one model file and a
/// valid manifest.
fn install_fake(store: &Path, pair: &str, version: &str, content: &[u8]) {
    let (source, target) = pair.split_once('-').unwrap();
    let dir = store
        .join("mozilla-remote-settings")
        .join(pair)
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("model.bin"), content).unwrap();
    Manifest {
        schema: SCHEMA_VERSION,
        provider: "mozilla-remote-settings".into(),
        source: source.into(),
        target: target.into(),
        version: version.into(),
        architecture: Some("tiny".into()),
        license: None,
        source_urls: vec![],
        installed_at: now_rfc3339(),
        files: vec![ManifestFile {
            name: "model.bin".into(),
            role: "model".into(),
            size: content.len() as u64,
            sha256: format!("{:x}", Sha256::digest(content)),
        }],
    }
    .store(&dir)
    .unwrap();
}

fn stores(root: &Path) -> Stores {
    Stores {
        system: vec![root.join("system-a"), root.join("system-b")],
        user: root.join("user"),
    }
}

#[test]
fn newest_supported_version_wins() {
    let dir = tempfile::tempdir().unwrap();
    let stores = stores(dir.path());
    install_fake(&stores.user, "bg-en", "3.0", b"old");
    install_fake(&stores.user, "bg-en", "3.1", b"new");
    // Outside the supported major window: never resolved.
    install_fake(&stores.user, "bg-en", "4.0", b"future");

    let best = stores.resolve("bg", "en").unwrap();
    assert_eq!(best.version.as_str(), "3.1");
}

#[test]
fn system_store_wins_a_version_tie() {
    let dir = tempfile::tempdir().unwrap();
    let stores = stores(dir.path());
    install_fake(&stores.user, "bg-en", "3.0", b"user copy");
    install_fake(&stores.system[1], "bg-en", "3.0", b"system copy");

    let best = stores.resolve("bg", "en").unwrap();
    assert_eq!(best.origin, Origin::System);

    // But a newer user version beats an older system one.
    install_fake(&stores.user, "bg-en", "3.1", b"newer user copy");
    let best = stores.resolve("bg", "en").unwrap();
    assert_eq!(best.origin, Origin::User);
    assert_eq!(best.version.as_str(), "3.1");
}

#[test]
fn size_mismatch_is_skipped_on_scan_and_hash_caught_on_verify() {
    let dir = tempfile::tempdir().unwrap();
    let stores = stores(dir.path());
    install_fake(&stores.user, "bg-en", "3.0", b"content");

    // Same size, different content: the cheap scan keeps it…
    let model = stores.resolve("bg", "en").unwrap();
    std::fs::write(model.directory.join("model.bin"), b"CONTENT").unwrap();
    let model = stores.resolve("bg", "en").unwrap();
    // …but full verification catches it.
    assert!(
        Stores::verify(&model)
            .unwrap_err()
            .to_string()
            .contains("sha256")
    );

    // Truncated file: skipped at scan time already.
    std::fs::write(model.directory.join("model.bin"), b"short").unwrap();
    assert!(stores.resolve("bg", "en").is_none());
    let mut problems = Vec::new();
    stores.scan(&mut problems);
    assert!(problems.iter().any(|p| p.contains("size")), "{problems:?}");
}

#[test]
fn unfinished_tmp_dirs_and_foreign_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let stores = stores(dir.path());
    install_fake(&stores.user, "bg-en", "3.0", b"ok");
    std::fs::create_dir_all(
        stores
            .user
            .join("mozilla-remote-settings/bg-en/.tmp-3.1-1234"),
    )
    .unwrap();
    std::fs::write(stores.user.join(".lock"), b"").unwrap();

    let mut problems = Vec::new();
    let found = stores.scan(&mut problems);
    assert_eq!(found.len(), 1);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn user_store_lock_is_exclusive_between_processes() {
    let dir = tempfile::tempdir().unwrap();
    let stores = stores(dir.path());
    let _lock = stores.lock_user().unwrap();
    // A second flock from the same process would succeed (flock is
    // per-open-file), so exercise the cross-process behavior with flock(1).
    let status = std::process::Command::new("flock")
        .arg("--nonblock")
        .arg(stores.user.join(".lock"))
        .args(["-c", "true"])
        .status();
    if let Ok(status) = status {
        assert!(!status.success(), "lock should be held");
    }
}
