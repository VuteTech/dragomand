// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The lifecycle "scripted session", as a deterministic test:
//! models load, stay warm, get evicted LRU, get dropped under (simulated)
//! memory pressure, and the daemon goes idle and comes back on the next
//! call. Runs on a private bus with the fake backend and tiny timings.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use dragoman_client::{Translator1Proxy, call_with_request, response_code};
use dragoman_engine::fake::FakeConfig;
use dragoman_models::Stores;
use dragoman_models::manifest::{Manifest, ManifestFile, SCHEMA_VERSION, now_rfc3339};
use dragomand::backend::BackendKind;
use dragomand::config::Config;
use dragomand::{Daemon, DaemonOptions, launch};
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

fn install_fake(store: &Path, pair: &str) {
    let (source, target) = pair.split_once('-').unwrap();
    let dir = store.join("mozilla-remote-settings").join(pair).join("3.0");
    std::fs::create_dir_all(&dir).unwrap();
    let name = format!("model.{source}{target}.bin");
    let content = b"fake model";
    std::fs::write(dir.join(&name), content).unwrap();
    use sha2::Digest;
    Manifest {
        schema: SCHEMA_VERSION,
        provider: "mozilla-remote-settings".into(),
        source: source.into(),
        target: target.into(),
        version: "3.0".into(),
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

async fn translate(client: &zbus::Connection, proxy: &Translator1Proxy<'_>, src: &str, trg: &str) {
    let outcome = call_with_request(client, |token| {
        let proxy = proxy.clone();
        let (src, trg) = (src.to_owned(), trg.to_owned());
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy.translate(&src, &trg, vec!["x".into()], options).await
        }
    })
    .await
    .unwrap();
    assert_eq!(outcome.code, response_code::SUCCESS);
}

async fn loaded_routes(proxy: &Translator1Proxy<'_>) -> Vec<String> {
    let status = proxy.get_status().await.unwrap();
    Vec::<String>::try_from(status.get("loaded").unwrap().clone()).unwrap()
}

struct Session {
    _bus: PrivateBus,
    daemon: Daemon,
    client: zbus::Connection,
    _tmp: tempfile::TempDir,
}

async fn session(config: Config, pairs: &[&str]) -> Option<Session> {
    let bus = PrivateBus::start()?;
    let tmp = tempfile::tempdir().unwrap();
    let stores = Stores {
        system: vec![],
        user: tmp.path().join("user"),
    };
    for pair in pairs {
        install_fake(&stores.user, pair);
    }
    let mut provider = dragoman_models::RemoteSettingsConfig::from_env();
    provider.server = "https://dragomand.invalid/v1".into();
    provider.cache_dir = tmp.path().join("cache");

    let daemon = launch(DaemonOptions {
        config,
        backend: BackendKind::Fake(FakeConfig::default()),
        stores,
        provider,
        bus_address: Some(bus.address.clone()),
    })
    .await
    .unwrap();
    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    Some(Session {
        _bus: bus,
        daemon,
        client,
        _tmp: tmp,
    })
}

#[tokio::test]
async fn keep_warm_and_lru_capacity() {
    let config = Config {
        keep_warm: 1,
        keep_warm_seconds: 2,
        ..Config::default()
    };
    let Some(s) = session(config, &["bg-en", "en-de", "en-fr"]).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&s.client).await.unwrap();

    // Load three routes; capacity 1 keeps only the most recent warm.
    translate(&s.client, &proxy, "bg", "en").await;
    translate(&s.client, &proxy, "en", "de").await;
    translate(&s.client, &proxy, "en", "fr").await;
    assert_eq!(loaded_routes(&proxy).await.len(), 3);

    // The sweeper (interval = window/4 = 500ms) trims to capacity in LRU
    // order: bg-en and en-de go, en-fr stays warm.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let loaded = loaded_routes(&proxy).await;
        if loaded == vec!["en-fr".to_owned()] {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "LRU trim did not happen: {loaded:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // After the keep-warm window everything is gone.
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let loaded = loaded_routes(&proxy).await;
        if loaded.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "warm window did not expire: {loaded:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // And the next call loads again.
    translate(&s.client, &proxy, "bg", "en").await;
    assert_eq!(loaded_routes(&proxy).await, vec!["bg-en".to_owned()]);
}

#[tokio::test]
async fn memory_budget_evicts_lru() {
    // Fake models cost the 64 MiB estimate each; a 128 MiB budget holds
    // two direct routes.
    let config = Config {
        memory_budget_mb: 128,
        keep_warm: 10,
        keep_warm_seconds: 3600,
        ..Config::default()
    };
    let Some(s) = session(config, &["bg-en", "en-de", "en-fr"]).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&s.client).await.unwrap();

    translate(&s.client, &proxy, "bg", "en").await;
    tokio::time::sleep(Duration::from_millis(20)).await; // distinct LRU stamps
    translate(&s.client, &proxy, "en", "de").await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    translate(&s.client, &proxy, "en", "fr").await;

    let loaded = loaded_routes(&proxy).await;
    assert_eq!(
        loaded,
        vec!["en-de".to_owned(), "en-fr".to_owned()],
        "the least recently used route (bg-en) should have been evicted"
    );
}

#[tokio::test]
async fn pressure_drops_idle_models_and_daemon_recovers() {
    let config = Config {
        keep_warm: 4,
        keep_warm_seconds: 3600,
        ..Config::default()
    };
    let Some(s) = session(config, &["bg-en", "en-de"]).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&s.client).await.unwrap();

    translate(&s.client, &proxy, "bg", "en").await;
    translate(&s.client, &proxy, "en", "de").await;
    assert_eq!(loaded_routes(&proxy).await.len(), 2);

    s.daemon.simulate_memory_pressure().await;
    assert!(loaded_routes(&proxy).await.is_empty());

    // Still alive and able to reload.
    translate(&s.client, &proxy, "bg", "en").await;
    assert_eq!(loaded_routes(&proxy).await, vec!["bg-en".to_owned()]);
}

#[tokio::test]
async fn idle_exit_after_models_unload() {
    let config = Config {
        keep_warm: 1,
        keep_warm_seconds: 1,
        idle_exit_seconds: 1,
        ..Config::default()
    };
    let Some(s) = session(config, &["bg-en"]).await else {
        return;
    };
    let proxy = Translator1Proxy::new(&s.client).await.unwrap();

    translate(&s.client, &proxy, "bg", "en").await;

    // wait_until_idle resolves once the model was swept and things stayed
    // quiet; the binary would then release the name and exit.
    tokio::time::timeout(Duration::from_secs(10), s.daemon.wait_until_idle())
        .await
        .expect("daemon should become idle");
    assert!(loaded_routes(&proxy).await.is_empty());
    assert!(s.daemon.release_name().await.unwrap());

    // "The next call re-activates the daemon": on a real bus that is
    // D-Bus activation starting a new process; here a fresh launch on the
    // same bus picks the name right back up and serves.
    let stores = Stores {
        system: vec![],
        user: s._tmp.path().join("user"),
    };
    let mut provider = dragoman_models::RemoteSettingsConfig::from_env();
    provider.server = "https://dragomand.invalid/v1".into();
    provider.cache_dir = s._tmp.path().join("cache");
    let second = launch(DaemonOptions {
        config: Config::default(),
        backend: BackendKind::Fake(FakeConfig::default()),
        stores,
        provider,
        bus_address: Some(s._bus.address.clone()),
    })
    .await
    .expect("second daemon takes over the released name");
    translate(&s.client, &proxy, "bg", "en").await;
    drop(second);
}
