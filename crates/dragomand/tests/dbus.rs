// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon integration tests: a private dbus-daemon per test, the daemon
//! running in-process with the fake backend, driven through
//! dragoman-client.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use dragoman_client::{
    Request1Proxy, Translator1Proxy, call_with_request, names, new_handle_token, request_path,
    response_code, result_key,
};
use dragoman_engine::fake::FakeConfig;
use dragoman_models::Stores;
use dragoman_models::manifest::{Manifest, ManifestFile, SCHEMA_VERSION, now_rfc3339};
use dragomand::backend::BackendKind;
use dragomand::{Daemon, DaemonOptions, launch};
use futures_util::StreamExt;
use zbus::zvariant::Value;

struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn start() -> Option<PrivateBus> {
        let mut child = match Command::new("dbus-daemon")
            .args(["--session", "--print-address", "--nofork"])
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!("dbus-daemon unavailable, skipping test: {e}");
                return None;
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let mut address = String::new();
        BufReader::new(stdout)
            .read_line(&mut address)
            .expect("read bus address");
        Some(PrivateBus {
            child,
            address: address.trim().to_owned(),
        })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Puts a fake-installed model into a store (mirrors the real layout).
fn install_fake(store: &Path, pair: &str, version: &str) {
    let (source, target) = pair.split_once('-').unwrap();
    let dir = store
        .join("mozilla-remote-settings")
        .join(pair)
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();
    let name = format!("model.{}{}.bin", source, target);
    let content = b"fake model";
    std::fs::write(dir.join(&name), content).unwrap();
    use sha2::Digest;
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
            name,
            role: "model".into(),
            size: content.len() as u64,
            sha256: format!("{:x}", sha2::Sha256::digest(content)),
        }],
    }
    .store(&dir)
    .unwrap();
}

struct TestSetup {
    _bus: PrivateBus,
    _daemon: Daemon,
    client: zbus::Connection,
    _tmp: tempfile::TempDir,
}

async fn setup(pairs: &[&str], fake: FakeConfig) -> Option<TestSetup> {
    let bus = PrivateBus::start()?;
    let tmp = tempfile::tempdir().unwrap();
    let stores = Stores {
        system: vec![tmp.path().join("system")],
        user: tmp.path().join("user"),
    };
    for pair in pairs {
        install_fake(&stores.user, pair, "3.0");
    }
    let mut provider = dragoman_models::RemoteSettingsConfig::from_env();
    provider.server = "https://dragomand.invalid/v1".into();
    provider.cache_dir = tmp.path().join("cache");

    let daemon = launch(DaemonOptions {
        config: dragomand::config::Config::default(),
        backend: BackendKind::Fake(fake),
        stores,
        provider,
        bus_address: Some(bus.address.clone()),
    })
    .await
    .expect("daemon launches");

    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .expect("client connects");
    Some(TestSetup {
        _bus: bus,
        _daemon: daemon,
        client,
        _tmp: tmp,
    })
}

fn translations_of(results: &HashMap<String, zbus::zvariant::OwnedValue>) -> Vec<String> {
    let value = results
        .get(result_key::TRANSLATIONS)
        .expect("translations present");
    Vec::<String>::try_from(value.clone()).expect("as type")
}

#[tokio::test]
async fn translate_direct() {
    let Some(setup) = setup(&["bg-en"], FakeConfig::default()).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    let outcome = call_with_request(&setup.client, |token| {
        let proxy = proxy.clone();
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy
                .translate("bg", "en", vec!["добро утро".into()], options)
                .await
        }
    })
    .await
    .unwrap();

    assert_eq!(outcome.code, response_code::SUCCESS);
    assert_eq!(
        translations_of(&outcome.results),
        vec!["[model.bgen] добро утро"]
    );
    assert!(!outcome.results.contains_key(result_key::PIVOT));
}

#[tokio::test]
async fn translate_pivots_through_english() {
    let Some(setup) = setup(&["bg-en", "en-de"], FakeConfig::default()).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    let outcome = call_with_request(&setup.client, |token| {
        let proxy = proxy.clone();
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy
                .translate("bg", "de", vec!["здравей".into()], options)
                .await
        }
    })
    .await
    .unwrap();

    assert_eq!(outcome.code, response_code::SUCCESS);
    assert_eq!(
        translations_of(&outcome.results),
        vec!["[model.bgen+model.ende] здравей"]
    );
    let pivot = outcome.results.get(result_key::PIVOT).unwrap();
    assert_eq!(pivot.downcast_ref::<&str>().unwrap(), "en");
}

#[tokio::test]
async fn not_installed_and_invalid_arguments() {
    let Some(setup) = setup(&["bg-en"], FakeConfig::default()).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    let error = proxy
        .translate("xx", "yy", vec!["a".into()], HashMap::new())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        error.to_string() // keep the readable form in failures
    );
    let zbus::Error::MethodError(name, _, _) = &error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(name.as_str(), "dev.l10n_bg.dragomand.Error.NotInstalled");

    let error = proxy
        .translate("bg", "bg", vec!["a".into()], HashMap::new())
        .await
        .unwrap_err();
    let zbus::Error::MethodError(name, _, _) = &error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(name.as_str(), "dev.l10n_bg.dragomand.Error.InvalidArgument");
}

#[tokio::test]
async fn limits_are_enforced_before_queueing() {
    let Some(setup) = setup(&["bg-en"], FakeConfig::default()).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    let many: Vec<String> = (0..dragomand::service::MAX_SEGMENTS + 1)
        .map(|i| format!("s{i}"))
        .collect();
    let error = proxy
        .translate("bg", "en", many, HashMap::new())
        .await
        .unwrap_err();
    let zbus::Error::MethodError(name, _, _) = &error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(name.as_str(), "dev.l10n_bg.dragomand.Error.LimitExceeded");
}

#[tokio::test]
async fn cancel_answers_with_cancelled_code() {
    let fake = FakeConfig {
        translate_delay: Duration::from_millis(200),
        ..FakeConfig::default()
    };
    let Some(setup) = setup(&["bg-en"], fake).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    // First job occupies the worker …
    let first_client = setup.client.clone();
    let first_proxy = proxy.clone();
    let first = tokio::spawn(async move {
        call_with_request(&first_client, |token| {
            let proxy = first_proxy.clone();
            async move {
                let mut options = HashMap::new();
                options.insert("handle_token", Value::from(token));
                proxy.translate("bg", "en", vec!["a".into()], options).await
            }
        })
        .await
    });

    // … the second gets cancelled while queued.
    let token = new_handle_token();
    let sender = setup.client.unique_name().unwrap();
    let path = request_path(sender.as_str(), &token).unwrap();
    let request = Request1Proxy::builder(&setup.client)
        .path(path.clone())
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut responses = request.receive_response().await.unwrap();

    let mut options = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let returned = proxy
        .translate("bg", "en", vec!["b".into()], options)
        .await
        .unwrap();
    assert_eq!(returned, path);
    request.cancel().await.unwrap();

    let signal = responses.next().await.expect("response arrives");
    assert_eq!(signal.args().unwrap().code, response_code::CANCELLED);

    let first = first.await.unwrap().unwrap();
    assert_eq!(first.code, response_code::SUCCESS);
}

#[tokio::test]
async fn interactive_overtakes_batch() {
    let fake = FakeConfig {
        translate_delay: Duration::from_millis(30),
        ..FakeConfig::default()
    };
    let Some(setup) = setup(&["bg-en"], fake).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    let batch_segments: Vec<String> = (0..64).map(|i| format!("b{i}")).collect();
    let batch_client = setup.client.clone();
    let batch_proxy = proxy.clone();
    let batch = tokio::spawn(async move {
        let outcome = call_with_request(&batch_client, |token| {
            let proxy = batch_proxy.clone();
            let segments = batch_segments.clone();
            async move {
                let mut options = HashMap::new();
                options.insert("handle_token", Value::from(token));
                options.insert("priority", Value::from("batch"));
                proxy.translate("bg", "en", segments, options).await
            }
        })
        .await
        .unwrap();
        (Instant::now(), outcome)
    });

    // Give the batch a head start, then race an interactive request.
    tokio::time::sleep(Duration::from_millis(40)).await;
    let outcome = call_with_request(&setup.client, |token| {
        let proxy = proxy.clone();
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy
                .translate("bg", "en", vec!["quick".into()], options)
                .await
        }
    })
    .await
    .unwrap();
    let interactive_done = Instant::now();
    assert_eq!(outcome.code, response_code::SUCCESS);

    let (batch_done, batch_outcome) = batch.await.unwrap();
    assert_eq!(batch_outcome.code, response_code::SUCCESS);
    assert_eq!(translations_of(&batch_outcome.results).len(), 64);
    assert!(
        interactive_done < batch_done,
        "interactive request should finish before the batch"
    );
}

#[tokio::test]
async fn client_disconnect_cancels_its_requests() {
    let fake = FakeConfig {
        translate_delay: Duration::from_millis(100),
        ..FakeConfig::default()
    };
    let Some(setup) = setup(&["bg-en"], fake).await else {
        return;
    };

    // A second client queues work, then vanishes.
    let other = zbus::connection::Builder::address(setup._bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let other_proxy = Translator1Proxy::new(&other).await.unwrap();
    for i in 0..4 {
        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(format!("gone_{i}")));
        other_proxy
            .translate("bg", "en", vec![format!("x{i}")], options)
            .await
            .unwrap();
    }
    drop(other_proxy);
    drop(other);

    // The daemon keeps serving, and the queue drains promptly because the
    // orphaned jobs were cancelled rather than translated.
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let status = proxy.get_status().await.unwrap();
        let queued: u32 = status.get("queued").unwrap().downcast_ref().unwrap();
        if queued == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "queue did not drain: {queued} left"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn status_and_version() {
    let Some(setup) = setup(&["bg-en"], FakeConfig::default()).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    assert_eq!(proxy.version().await.unwrap(), env!("CARGO_PKG_VERSION"));

    let pairs = proxy.list_language_pairs().await.unwrap();
    assert_eq!(pairs.len(), 1);
    let row = &pairs[0];
    assert_eq!(
        row.get("source").unwrap().downcast_ref::<&str>().unwrap(),
        "bg"
    );
    assert_eq!(
        row.get("installed_version")
            .unwrap()
            .downcast_ref::<&str>()
            .unwrap(),
        "3.0"
    );

    // Load the pair, then the status shows it.
    call_with_request(&setup.client, |token| {
        let proxy = proxy.clone();
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy.translate("bg", "en", vec!["x".into()], options).await
        }
    })
    .await
    .unwrap();
    let status = proxy.get_status().await.unwrap();
    let loaded = Vec::<String>::try_from(status.get("loaded").unwrap().clone()).unwrap();
    assert_eq!(loaded, vec!["bg-en"]);
}

#[tokio::test]
async fn remove_pair_unloads_and_deletes() {
    let Some(setup) = setup(&["bg-en"], FakeConfig::default()).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    call_with_request(&setup.client, |token| {
        let proxy = proxy.clone();
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy.translate("bg", "en", vec!["x".into()], options).await
        }
    })
    .await
    .unwrap();

    proxy.remove_pair("bg", "en").await.unwrap();
    let pairs = proxy.list_language_pairs().await.unwrap();
    assert!(pairs.is_empty(), "{pairs:?}");

    let error = proxy.remove_pair("bg", "en").await.unwrap_err();
    let zbus::Error::MethodError(name, _, _) = &error else {
        panic!("unexpected error: {error:?}");
    };
    assert_eq!(name.as_str(), "dev.l10n_bg.dragomand.Error.NotInstalled");
}

#[tokio::test]
async fn introspection_matches_the_contract() {
    let fake = FakeConfig {
        translate_delay: Duration::from_millis(300),
        ..FakeConfig::default()
    };
    let Some(setup) = setup(&["bg-en"], fake).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&setup.client).await.unwrap();

    // Main object.
    let introspectable = zbus::fdo::IntrospectableProxy::builder(&setup.client)
        .destination(names::BUS_NAME)
        .unwrap()
        .path(names::OBJECT_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();
    let live_main = introspectable.introspect().await.unwrap();

    // A live request object (kept alive by the slow fake translate).
    let token = new_handle_token();
    let sender = setup.client.unique_name().unwrap();
    let path = request_path(sender.as_str(), &token).unwrap();
    let mut options = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    proxy
        .translate("bg", "en", vec!["x".into()], options)
        .await
        .unwrap();
    let introspectable = zbus::fdo::IntrospectableProxy::builder(&setup.client)
        .destination(names::BUS_NAME)
        .unwrap()
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    let live_request = introspectable.introspect().await.unwrap();

    let contract = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/dbus/dev.l10n_bg.dragomand.Translator1.xml"),
    )
    .unwrap();

    compare_interface(&contract, &live_main, "dev.l10n_bg.dragomand.Translator1");
    compare_interface(&contract, &live_request, "dev.l10n_bg.dragomand.Request1");
}

/// Asserts that interface `name` is identical (members, argument types and
/// directions) in both introspection documents.
fn compare_interface(contract_xml: &str, live_xml: &str, name: &str) {
    let contract = describe(contract_xml, name);
    let live = describe(live_xml, name);
    assert_eq!(
        contract, live,
        "interface {name} drifted between data/dbus/*.xml and the implementation"
    );
}

/// A canonical, comparable description of one interface.
fn describe(xml: &str, name: &str) -> Vec<String> {
    let node = zbus_xml::Node::from_reader(xml.as_bytes()).expect("well-formed XML");
    let interface = node
        .interfaces()
        .iter()
        .find(|i| i.name() == name)
        .unwrap_or_else(|| panic!("interface {name} missing"));

    let mut lines = Vec::new();
    for method in interface.methods() {
        let args: Vec<String> = method
            .args()
            .iter()
            .map(|a| {
                format!(
                    "{}:{:?}",
                    match a.direction() {
                        Some(zbus_xml::ArgDirection::Out) => "out",
                        _ => "in",
                    },
                    a.ty()
                )
            })
            .collect();
        lines.push(format!("method {}({})", method.name(), args.join(", ")));
    }
    for signal in interface.signals() {
        let args: Vec<String> = signal
            .args()
            .iter()
            .map(|a| format!("{:?}", a.ty()))
            .collect();
        lines.push(format!("signal {}({})", signal.name(), args.join(", ")));
    }
    for property in interface.properties() {
        lines.push(format!(
            "property {} {:?} {:?}",
            property.name(),
            property.ty(),
            property.access()
        ));
    }
    lines.sort();
    lines
}
