// SPDX-License-Identifier: GPL-3.0-or-later

//! The model/engine compatibility sweep: install accepted
//! record sets, load each with the real engine, translate one sentence, and
//! report per-pair status. Run it whenever the engine pin changes, and
//! weekly to catch new Mozilla releases.
//!
//! Doubly opt-in, because it downloads real models and needs the C++ engine:
//!
//! ```sh
//! DRAGOMAN_ENGINE_LIB_DIR=$PWD/engine/build \
//! DRAGOMAN_COMPAT_PAIRS=bg-en,en-bg \
//!   cargo test -p dragoman-models --features compat-test --test compat -- --nocapture
//! ```
//!
//! `DRAGOMAN_COMPAT_PAIRS=all` sweeps every accepted pair (gigabytes of
//! downloads). The test fails when any *requested* pair fails; with `all`,
//! bg-en/en-bg failures are fatal and the rest are reported.

#![cfg(feature = "compat-test")]

use dragoman_engine::bergamot::BergamotBackend;
use dragoman_engine::{ModelFiles, ModelSpec, TranslateOptions, Worker, marian_config};
use dragoman_models::http::ReqwestHttp;
use dragoman_models::remote_settings::{RemoteSettingsConfig, RemoteSettingsProvider};
use dragoman_models::{InstalledModel, Stores};

fn model_spec(installed: &InstalledModel) -> ModelSpec {
    let model = installed.file_path("model").expect("manifest has a model");
    let file_name = model.file_name().unwrap().to_string_lossy().into_owned();
    ModelSpec {
        files: ModelFiles {
            model,
            vocab: installed.file_path("vocab"),
            src_vocab: installed.file_path("srcvocab"),
            trg_vocab: installed.file_path("trgvocab"),
            shortlist: None,
        },
        config_yaml: marian_config(&file_name),
    }
}

#[tokio::test]
async fn compatibility_sweep() {
    let Ok(pairs_env) = std::env::var("DRAGOMAN_COMPAT_PAIRS") else {
        eprintln!("DRAGOMAN_COMPAT_PAIRS not set; skipping compatibility sweep");
        return;
    };

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
    let available = provider.available().await.unwrap();
    for filter in &available.skipped_filters {
        eprintln!("note: skipped records with filter_expression {filter:?}");
    }

    let requested: Vec<String> = if pairs_env == "all" {
        available.sets.iter().map(|s| s.pair()).collect()
    } else {
        pairs_env.split(',').map(|s| s.trim().to_owned()).collect()
    };

    let mut failures = Vec::new();
    for pair in &requested {
        let Some(set) = available.sets.iter().find(|s| &s.pair() == pair) else {
            eprintln!("{pair}: NOT AVAILABLE (no accepted record set)");
            failures.push(pair.clone());
            continue;
        };
        let result = check_pair(&provider, &stores, set, &available.attachments_base).await;
        match result {
            Ok(sample) => eprintln!(
                "{pair}: ok ({} {}) {:?}",
                set.version,
                set.architecture.as_deref().unwrap_or("?"),
                sample
            ),
            Err(message) => {
                eprintln!("{pair}: FAILED: {message}");
                failures.push(pair.clone());
            }
        }
        // Free the downloaded files as we go on a full sweep.
        if pairs_env == "all"
            && let Some(installed) = stores.resolve(&set.source, &set.target)
        {
            let _ = stores.remove(&installed);
        }
    }

    if pairs_env == "all" {
        let fatal: Vec<_> = failures
            .iter()
            .filter(|p| *p == "bg-en" || *p == "en-bg")
            .collect();
        eprintln!(
            "sweep finished: {} pairs, {} failures ({:?})",
            requested.len(),
            failures.len(),
            failures
        );
        assert!(fatal.is_empty(), "bg pairs failed: {fatal:?}");
    } else {
        assert!(failures.is_empty(), "failed pairs: {failures:?}");
    }
}

async fn check_pair(
    provider: &RemoteSettingsProvider<ReqwestHttp>,
    stores: &Stores,
    set: &dragoman_models::ModelSet,
    attachments_base: &str,
) -> Result<String, String> {
    let installed = provider
        .install(stores, set, attachments_base)
        .await
        .map_err(|e| format!("install: {e}"))?;
    let backend = BergamotBackend::new(0).map_err(|e| format!("engine: {e}"))?;
    let (worker, ready) = Worker::spawn(backend, model_spec(&installed), None);
    ready
        .await
        .map_err(|_| "worker gone".to_owned())?
        .map_err(|e| format!("load: {e}"))?;
    let out = worker
        .translate(vec!["Firefox 2024.".into()], TranslateOptions::default())
        .await
        .map_err(|e| format!("translate: {e}"))?;
    if out.len() != 1 || out[0].trim().is_empty() {
        return Err(format!("empty translation: {out:?}"));
    }
    Ok(out.into_iter().next().unwrap())
}
