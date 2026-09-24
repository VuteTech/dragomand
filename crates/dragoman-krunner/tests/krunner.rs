// SPDX-License-Identifier: GPL-3.0-or-later

//! KRunner service test: private bus, fake-backend daemon in-process, the
//! real dragoman-krunner binary, driven exactly as KRunner would.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use dragoman_engine::fake::FakeConfig;
use dragoman_models::Stores;
use dragoman_models::manifest::{Manifest, ManifestFile, SCHEMA_VERSION, now_rfc3339};
use dragomand::backend::BackendKind;
use dragomand::config::Config;
use dragomand::{DaemonOptions, launch};
use zbus::zvariant::OwnedValue;

type KMatch = (
    String,
    String,
    String,
    i32,
    f64,
    HashMap<String, OwnedValue>,
);

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

#[tokio::test(flavor = "multi_thread")]
async fn krunner_matches_and_translates() {
    let Some(bus) = PrivateBus::start() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let user_store = tmp.path().join("user");
    install_fake(&user_store, "bg-en");
    let mut provider = dragoman_models::RemoteSettingsConfig::from_env();
    provider.server = "https://dragomand.invalid/v1".into();
    provider.cache_dir = tmp.path().join("cache");
    let _daemon = launch(DaemonOptions {
        config: Config::default(),
        backend: BackendKind::Fake(FakeConfig::default()),
        stores: Stores {
            system: vec![],
            user: user_store,
        },
        provider,
        bus_address: Some(bus.address.clone()),
    })
    .await
    .unwrap();

    let mut runner = Command::new(env!("CARGO_BIN_EXE_dragoman-krunner"))
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .spawn()
        .unwrap();

    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();

    // Wait for the runner to claim its name.
    let call_match = |query: &'static str| {
        let client = client.clone();
        async move {
            client
                .call_method(
                    Some("dev.l10n_bg.dragomand.KRunner1"),
                    "/dev/l10n_bg/dragomand/krunner",
                    Some("org.kde.krunner1"),
                    "Match",
                    &(query,),
                )
                .await
        }
    };
    let mut reply = None;
    for _ in 0..100 {
        match call_match("tr bg en добро утро").await {
            Ok(r) => {
                reply = Some(r);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    let matches: Vec<KMatch> = reply.expect("runner came up").body().deserialize().unwrap();
    assert_eq!(matches.len(), 1, "{matches:?}");
    assert_eq!(matches[0].1, "[model.bgen] добро утро");
    assert!(matches[0].4 > 0.5, "relevance");

    // Default pair without explicit languages.
    let reply = call_match("tr здравей свят").await.unwrap();
    let matches: Vec<KMatch> = reply.body().deserialize().unwrap();
    assert_eq!(matches[0].1, "[model.bgen] здравей свят");

    // Unavailable pair: an informational entry, not silence or a crash.
    let reply = call_match("tr de fr bonjour monde").await.unwrap();
    let matches: Vec<KMatch> = reply.body().deserialize().unwrap();
    assert_eq!(matches[0].0, "error", "{matches:?}");

    // Run on a hint id must not crash the service.
    client
        .call_method(
            Some("dev.l10n_bg.dragomand.KRunner1"),
            "/dev/l10n_bg/dragomand/krunner",
            Some("org.kde.krunner1"),
            "Run",
            &("hint", ""),
        )
        .await
        .unwrap();

    runner.kill().unwrap();
    let _ = runner.wait();
}
