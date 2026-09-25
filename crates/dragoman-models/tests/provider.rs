// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider behavior against a fake HTTP server: install, verification,
//! offline reuse, ETag revalidation, update checks.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest, Sha256};

use dragoman_models::http::{FetchResult, Http, HttpError};
use dragoman_models::remote_settings::{RemoteSettingsConfig, RemoteSettingsProvider, UpdateInfo};
use dragoman_models::{Origin, Stores};

const SERVER: &str = "https://rs.test/v1";
const ATTACHMENTS: &str = "https://cdn.test/";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One attachment: raw content plus its zstd-compressed transport form.
struct Blob {
    raw: Vec<u8>,
    compressed: Vec<u8>,
}

fn blob(content: &[u8]) -> Blob {
    Blob {
        raw: content.to_vec(),
        compressed: zstd::stream::encode_all(content, 3).unwrap(),
    }
}

fn record(
    src: &str,
    trg: &str,
    version: &str,
    file_type: &str,
    name: &str,
    blob: &Blob,
) -> serde_json::Value {
    serde_json::json!({
        "id": format!("{src}{trg}-{version}-{file_type}"),
        "name": name,
        "sourceLanguage": src,
        "targetLanguage": trg,
        "version": version,
        "fileType": file_type,
        "architecture": "base-memory",
        "filter_expression": "",
        "attachment": {
            "hash": sha(&blob.compressed),
            "size": blob.compressed.len(),
            "location": format!("loc/{name}.zst"),
            "mimetype": "application/zstd",
        },
        "decompressedHash": sha(&blob.raw),
        "decompressedSize": blob.raw.len(),
    })
}

/// url -> (body, optional etag)
type Responses = Mutex<HashMap<String, (Vec<u8>, Option<String>)>>;

struct FakeHttp {
    responses: Responses,
    requests: AtomicUsize,
    not_modified_served: AtomicUsize,
}

impl FakeHttp {
    fn new() -> Self {
        FakeHttp {
            responses: Mutex::new(HashMap::new()),
            requests: AtomicUsize::new(0),
            not_modified_served: AtomicUsize::new(0),
        }
    }

    fn put(&self, url: &str, bytes: Vec<u8>, etag: Option<&str>) {
        self.responses
            .lock()
            .unwrap()
            .insert(url.to_owned(), (bytes, etag.map(str::to_owned)));
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Http for FakeHttp {
    async fn get(&self, url: &str, if_none_match: Option<&str>) -> Result<FetchResult, HttpError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let responses = self.responses.lock().unwrap();
        let Some((bytes, etag)) = responses.get(url) else {
            return Err(HttpError::Status {
                url: url.to_owned(),
                status: 404,
            });
        };
        if if_none_match.is_some() && if_none_match == etag.as_deref() {
            self.not_modified_served.fetch_add(1, Ordering::SeqCst);
            return Ok(FetchResult::NotModified);
        }
        Ok(FetchResult::Fetched {
            bytes: bytes.clone(),
            etag: etag.clone(),
        })
    }
}

struct Fixture {
    http: FakeHttp,
    model: Blob,
    vocab: Blob,
}

fn fixture() -> Fixture {
    let model = blob(b"model-bytes-bg-en");
    let vocab = blob(b"vocab-bytes-bg-en");
    let lex = blob(b"lex-bytes-bg-en");
    let http = FakeHttp::new();
    http.put(
        &format!("{SERVER}/"),
        serde_json::to_vec(&serde_json::json!({
            "capabilities": {"attachments": {"base_url": ATTACHMENTS}}
        }))
        .unwrap(),
        None,
    );
    http.put(
        &format!("{SERVER}/buckets/main/collections/translations-models-v2/records"),
        serde_json::to_vec(&serde_json::json!({"data": [
            record("bg", "en", "3.0", "model", "model.bgen.bin", &model),
            record("bg", "en", "3.0", "vocab", "vocab.bgen.spm", &vocab),
            record("bg", "en", "3.0", "lex", "lex.bgen.bin", &lex),
        ]}))
        .unwrap(),
        Some("\"etag-1\""),
    );
    http.put(
        &format!("{ATTACHMENTS}loc/model.bgen.bin.zst"),
        model.compressed.clone(),
        None,
    );
    http.put(
        &format!("{ATTACHMENTS}loc/vocab.bgen.spm.zst"),
        vocab.compressed.clone(),
        None,
    );
    http.put(
        &format!("{ATTACHMENTS}loc/lex.bgen.bin.zst"),
        lex.compressed.clone(),
        None,
    );
    Fixture { http, model, vocab }
}

fn test_setup(root: &Path) -> (Stores, RemoteSettingsConfig) {
    let stores = Stores {
        system: vec![root.join("system")],
        user: root.join("user"),
    };
    let config = RemoteSettingsConfig {
        server: SERVER.to_owned(),
        allow_prerelease: false,
        download_lex: false,
        cache_dir: root.join("cache"),
    };
    (stores, config)
}

#[tokio::test]
async fn fresh_install_downloads_verifies_and_reuses_offline() {
    let dir = tempfile::tempdir().unwrap();
    let (stores, config) = test_setup(dir.path());
    let fx = fixture();
    let provider = RemoteSettingsProvider::new(fx.http, config);

    let installed = provider.install_pair(&stores, "bg", "en").await.unwrap();
    assert_eq!(installed.version.as_str(), "3.0");
    assert_eq!(installed.origin, Origin::User);
    // Files stored decompressed; lex skipped by default.
    let model_path = installed.file_path("model").unwrap();
    assert_eq!(std::fs::read(model_path).unwrap(), fx.model.raw);
    assert_eq!(
        std::fs::read(installed.file_path("vocab").unwrap()).unwrap(),
        fx.vocab.raw
    );
    assert!(installed.file_path("lex").is_none());
    assert_eq!(
        installed.manifest.architecture.as_deref(),
        Some("base-memory")
    );

    // The store now resolves the pair without any network.
    let resolved = stores.resolve("bg", "en").unwrap();
    assert_eq!(resolved.version.as_str(), "3.0");
    Stores::verify(&resolved).unwrap();
}

#[tokio::test]
async fn second_install_needs_no_downloads() {
    let dir = tempfile::tempdir().unwrap();
    let (stores, config) = test_setup(dir.path());
    let fx = fixture();
    let provider = RemoteSettingsProvider::new(fx.http, config);

    provider.install_pair(&stores, "bg", "en").await.unwrap();
    let after_first = provider.http_ref().request_count();
    provider.install_pair(&stores, "bg", "en").await.unwrap();
    // Only records + server info again (revalidated), no attachments.
    assert_eq!(provider.http_ref().request_count(), after_first + 2);
}

#[tokio::test]
async fn corrupted_download_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (stores, config) = test_setup(dir.path());
    let fx = fixture();
    // Tamper with the model attachment.
    fx.http.put(
        &format!("{ATTACHMENTS}loc/model.bgen.bin.zst"),
        zstd::stream::encode_all(&b"evil-bytes"[..], 3).unwrap(),
        None,
    );
    let provider = RemoteSettingsProvider::new(fx.http, config);

    let error = provider
        .install_pair(&stores, "bg", "en")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("verification failed"), "{error}");
    assert!(
        stores.resolve("bg", "en").is_none(),
        "nothing may be installed"
    );
}

#[tokio::test]
async fn unknown_pair_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (stores, config) = test_setup(dir.path());
    let provider = RemoteSettingsProvider::new(fixture().http, config);
    let error = provider
        .install_pair(&stores, "xx", "yy")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("xx-yy"), "{error}");
}

#[tokio::test]
async fn update_check_reports_newer_remote_version() {
    let dir = tempfile::tempdir().unwrap();
    let (stores, config) = test_setup(dir.path());
    let fx = fixture();
    let provider = RemoteSettingsProvider::new(fx.http, config.clone());
    provider.install_pair(&stores, "bg", "en").await.unwrap();

    // Publish 3.1 and check again.
    let model = blob(b"model-bytes-bg-en-31");
    let vocab = blob(b"vocab-bytes-bg-en-31");
    let fx2 = FakeHttp::new();
    fx2.put(
        &format!("{SERVER}/"),
        serde_json::to_vec(&serde_json::json!({
            "capabilities": {"attachments": {"base_url": ATTACHMENTS}}
        }))
        .unwrap(),
        None,
    );
    fx2.put(
        &format!("{SERVER}/buckets/main/collections/translations-models-v2/records"),
        serde_json::to_vec(&serde_json::json!({"data": [
            record("bg", "en", "3.1", "model", "model.bgen.bin", &model),
            record("bg", "en", "3.1", "vocab", "vocab.bgen.spm", &vocab),
        ]}))
        .unwrap(),
        Some("\"etag-2\""),
    );
    let provider = RemoteSettingsProvider::new(fx2, config);
    let updates = provider.check_for_updates(&stores).await.unwrap();
    let update: &UpdateInfo = &updates[0];
    assert_eq!(update.installed.as_str(), "3.0");
    assert_eq!(update.available.as_str(), "3.1");
}

#[tokio::test]
async fn etag_revalidation_serves_from_cache() {
    let dir = tempfile::tempdir().unwrap();
    let (_stores, config) = test_setup(dir.path());
    let fx = fixture();
    let provider = RemoteSettingsProvider::new(fx.http, config);

    let first = provider.available().await.unwrap();
    assert_eq!(first.sets.len(), 1);
    let second = provider.available().await.unwrap();
    assert_eq!(second.sets.len(), 1);
    assert!(
        provider
            .http_ref()
            .not_modified_served
            .load(Ordering::SeqCst)
            >= 1,
        "records should have been revalidated via ETag"
    );
}
