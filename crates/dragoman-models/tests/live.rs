// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end against the real Remote Settings service. Opt-in:
//! `DRAGOMAN_TEST_NETWORK=1 cargo test -p dragoman-models --test live`

use dragoman_models::Stores;
use dragoman_models::http::ReqwestHttp;
use dragoman_models::remote_settings::{RemoteSettingsConfig, RemoteSettingsProvider};

fn enabled() -> bool {
    if std::env::var_os("DRAGOMAN_TEST_NETWORK").is_none() {
        eprintln!("DRAGOMAN_TEST_NETWORK not set; skipping live network test");
        return false;
    }
    true
}

#[tokio::test]
async fn live_install_bg_en_and_reuse_offline() {
    if !enabled() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores {
        system: vec![],
        user: dir.path().join("user"),
    };
    let config = RemoteSettingsConfig {
        cache_dir: dir.path().join("cache"),
        ..RemoteSettingsConfig::from_env()
    };
    let provider = RemoteSettingsProvider::new(ReqwestHttp::new(), config);

    let installed = provider.install_pair(&stores, "bg", "en").await.unwrap();
    assert_eq!(installed.version.major(), 3, "expected a 3.x record set");
    dragoman_models::Stores::verify(&installed).unwrap();

    // Everything the engine needs is resolvable offline now.
    let resolved = stores.resolve("bg", "en").unwrap();
    assert!(resolved.file_path("model").unwrap().exists());
    let vocab_ok = resolved.file_path("vocab").is_some()
        || (resolved.file_path("srcvocab").is_some() && resolved.file_path("trgvocab").is_some());
    assert!(
        vocab_ok,
        "vocab files missing: {:?}",
        resolved.manifest.files
    );
}
