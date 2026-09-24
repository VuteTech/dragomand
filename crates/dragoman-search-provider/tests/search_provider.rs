// SPDX-License-Identifier: GPL-3.0-or-later

//! Search-provider test: private bus, fake-backend daemon in-process, the
//! real dragoman-search-provider binary, driven as GNOME Shell would.

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
async fn search_provider_translates_and_describes() {
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

    let mut service = Command::new(env!("CARGO_BIN_EXE_dragoman-search-provider"))
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .spawn()
        .unwrap();

    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let call = |method: &'static str, body: zbus::zvariant::Structure<'static>| {
        let client = client.clone();
        async move {
            client
                .call_method(
                    Some("dev.l10n_bg.dragomand.SearchProvider"),
                    "/dev/l10n_bg/dragomand/SearchProvider",
                    Some("org.gnome.Shell.SearchProvider2"),
                    method,
                    &body,
                )
                .await
        }
    };

    // Wait for the service to claim its name, then search.
    let terms = vec![
        "tr".to_owned(),
        "bg".to_owned(),
        "en".to_owned(),
        "добро".to_owned(),
    ];
    let mut reply = None;
    for _ in 0..100 {
        match call("GetInitialResultSet", (terms.clone(),).into()).await {
            Ok(r) => {
                reply = Some(r);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    let ids: Vec<String> = reply
        .expect("provider came up")
        .body()
        .deserialize()
        .unwrap();
    assert_eq!(ids.len(), 1);
    assert!(ids[0].ends_with("[model.bgen] добро"), "{ids:?}");

    // Non-keyword queries never reach the daemon and return nothing.
    let reply = call("GetInitialResultSet", (vec!["firefox".to_owned()],).into())
        .await
        .unwrap();
    let none: Vec<String> = reply.body().deserialize().unwrap();
    assert!(none.is_empty());

    // Metas carry the translation as the display name.
    let reply = call("GetResultMetas", (ids.clone(),).into()).await.unwrap();
    let metas: Vec<HashMap<String, OwnedValue>> = reply.body().deserialize().unwrap();
    assert_eq!(
        metas[0]["name"].downcast_ref::<&str>().unwrap(),
        "[model.bgen] добро"
    );
    assert!(
        metas[0]["description"]
            .downcast_ref::<&str>()
            .unwrap()
            .contains("bg → en")
    );

    // Activating must not crash even without a clipboard tool.
    call("ActivateResult", (ids[0].clone(), terms, 0u32).into())
        .await
        .unwrap();

    service.kill().unwrap();
    let _ = service.wait();
}
