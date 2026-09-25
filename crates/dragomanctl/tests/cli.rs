// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! CLI tests against the fake backend: a private dbus-daemon, the daemon
//! in-process, and the actual dragomanctl binary driven like a user
//! would.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use dragoman_engine::fake::FakeConfig;
use dragoman_models::Stores;
use dragoman_models::manifest::{Manifest, ManifestFile, SCHEMA_VERSION, now_rfc3339};
use dragomand::backend::BackendKind;
use dragomand::config::Config;
use dragomand::{Daemon, DaemonOptions, launch};

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

struct Setup {
    bus: PrivateBus,
    _daemon: Daemon,
    tmp: tempfile::TempDir,
}

impl Setup {
    fn user_store(&self) -> std::path::PathBuf {
        self.tmp.path().join("xdg-data/dragomand/models")
    }

    /// Runs the dragomanctl binary wired to the private bus and stores.
    fn ctl(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_dragomanctl"))
            .args(args)
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus.address)
            .env("XDG_DATA_HOME", self.tmp.path().join("xdg-data"))
            .env("XDG_DATA_DIRS", self.tmp.path().join("nonexistent"))
            .env("XDG_CACHE_HOME", self.tmp.path().join("xdg-cache"))
            .stdin(Stdio::null())
            .output()
            .expect("dragomanctl runs")
    }
}

async fn setup(pairs: &[&str]) -> Option<Setup> {
    let bus = PrivateBus::start()?;
    let tmp = tempfile::tempdir().unwrap();
    let user_store = tmp.path().join("xdg-data/dragomand/models");
    for pair in pairs {
        install_fake(&user_store, pair);
    }
    let mut provider = dragoman_models::RemoteSettingsConfig::from_env();
    provider.server = "https://dragomand.invalid/v1".into();
    provider.cache_dir = tmp.path().join("xdg-cache/dragomand/remote-settings");

    let daemon = launch(DaemonOptions {
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
    Some(Setup {
        bus,
        _daemon: daemon,
        tmp,
    })
}

fn stdout_of(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn translate_pairs_status_remove() {
    let Some(setup) = setup(&["bg-en"]).await else {
        return;
    };

    // translate, plain and JSON
    let out = setup.ctl(&["translate", "-f", "bg", "-t", "en", "добър ден"]);
    assert_eq!(stdout_of(&out), "[model.bgen] добър ден\n");
    let out = setup.ctl(&["--json", "translate", "-f", "bg", "-t", "en", "x"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    assert_eq!(parsed["translations"][0], "[model.bgen] x");

    // pairs
    let out = setup.ctl(&["pairs", "--installed"]);
    let text = stdout_of(&out);
    assert!(text.contains("bg") && text.contains("3.0"), "{text}");
    let out = setup.ctl(&["--json", "pairs"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    assert_eq!(parsed[0]["installed_version"], "3.0");

    // status
    let out = setup.ctl(&["--json", "status"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    assert_eq!(parsed["loaded"][0], "bg-en");

    // store list and verify agree with the daemon's store
    let out = setup.ctl(&["store", "list"]);
    assert!(stdout_of(&out).contains("bg"), "store list");
    let out = setup.ctl(&["store", "verify"]);
    assert!(stdout_of(&out).contains("1 model(s), 0 failure(s)"));

    // remove via the daemon, then the store is empty
    let out = setup.ctl(&["remove", "bg-en"]);
    stdout_of(&out);
    let out = setup.ctl(&["--json", "pairs"]);
    assert_eq!(stdout_of(&out).trim(), "[]");
}

#[tokio::test(flavor = "multi_thread")]
async fn translate_reads_stdin_and_pivots() {
    let Some(setup) = setup(&["bg-en", "en-de"]).await else {
        return;
    };

    let mut child = Command::new(env!("CARGO_BIN_EXE_dragomanctl"))
        .args(["translate", "-f", "bg", "-t", "de"])
        .env("DBUS_SESSION_BUS_ADDRESS", &setup.bus.address)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all("ред едно\nред две\n".as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout,
        "[model.bgen+model.ende] ред едно\n[model.bgen+model.ende] ред две\n"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("via en"));
}

#[tokio::test(flavor = "multi_thread")]
async fn store_verify_catches_corruption_with_exit_1() {
    let Some(setup) = setup(&["bg-en"]).await else {
        return;
    };
    // Same size, different bytes.
    std::fs::write(
        setup
            .user_store()
            .join("mozilla-remote-settings/bg-en/3.0/model.bgen.bin"),
        b"EVIL MODEL",
    )
    .unwrap();
    let out = setup.ctl(&["store", "verify"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("sha256"));
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_errors_exit_2() {
    let Some(setup) = setup(&[]).await else {
        return;
    };
    let out = setup.ctl(&["translate", "-f", "bg"]); // missing -t
    assert_eq!(out.status.code(), Some(2));
    let out = setup.ctl(&["install", "notapair"]);
    assert_eq!(out.status.code(), Some(1)); // runtime: bad pair message
    assert!(String::from_utf8_lossy(&out.stderr).contains("expected SRC-TRG"));
}

#[tokio::test(flavor = "multi_thread")]
async fn completions_and_manpage_render() {
    let Some(setup) = setup(&[]).await else {
        return;
    };
    let out = setup.ctl(&["completions", "bash"]);
    assert!(stdout_of(&out).contains("dragomanctl"));
    let out = setup.ctl(&["manpage"]);
    assert!(stdout_of(&out).contains(".TH"));
}
