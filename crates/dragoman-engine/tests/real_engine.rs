// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests against the real Bergamot engine and real model files.
//!
//! They run only when `DRAGOMAN_TEST_MODELS` points at a directory laid
//! out like scripts/fetch-test-models.py produces (test-models/), and are
//! skipped silently otherwise.

#![cfg(feature = "bergamot")]

use std::path::PathBuf;

use dragoman_engine::bergamot::BergamotBackend;
use dragoman_engine::{ModelFiles, ModelSpec, TranslateOptions, Worker, marian_config};

fn model_spec(root: &std::path::Path, pair: &str, tag: &str) -> ModelSpec {
    let dir = root.join(pair).join("3.0");
    ModelSpec {
        files: ModelFiles {
            model: dir.join(format!("model.{tag}.intgemm.alphas.bin")),
            vocab: Some(dir.join(format!("vocab.{tag}.spm"))),
            ..ModelFiles::default()
        },
        config_yaml: marian_config(&format!("model.{tag}.intgemm.alphas.bin")),
    }
}

fn test_models() -> Option<PathBuf> {
    match std::env::var_os("DRAGOMAN_TEST_MODELS") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => {
            eprintln!("DRAGOMAN_TEST_MODELS not set; skipping real-engine test");
            None
        }
    }
}

#[tokio::test]
async fn translates_bulgarian_to_english() {
    let Some(root) = test_models() else { return };
    let backend = BergamotBackend::new(0).unwrap();
    let (worker, ready) = Worker::spawn(backend, model_spec(&root, "bg-en", "bgen"), None);
    ready.await.unwrap().unwrap();

    let out = worker
        .translate(
            vec!["Добро утро.".into(), "Котката спи.".into()],
            TranslateOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(out.len(), 2);
    assert!(
        out[0].to_lowercase().contains("good morning"),
        "got: {out:?}"
    );
    assert!(out[1].to_lowercase().contains("cat"), "got: {out:?}");
}

#[tokio::test]
async fn pivots_through_english() {
    let Some(root) = test_models() else { return };
    let backend = BergamotBackend::new(0).unwrap();
    let (worker, ready) = Worker::spawn(
        backend,
        model_spec(&root, "bg-en", "bgen"),
        Some(model_spec(&root, "en-bg", "enbg")),
    );
    ready.await.unwrap().unwrap();

    let out = worker
        .translate(vec!["Добро утро.".into()], TranslateOptions::default())
        .await
        .unwrap();
    assert!(
        out[0].contains("утро"),
        "round trip lost the morning: {out:?}"
    );
}

#[tokio::test]
async fn html_markup_is_preserved() {
    let Some(root) = test_models() else { return };
    let backend = BergamotBackend::new(0).unwrap();
    let (worker, ready) = Worker::spawn(backend, model_spec(&root, "bg-en", "bgen"), None);
    ready.await.unwrap().unwrap();

    let out = worker
        .translate(
            vec!["<b>Добро утро.</b>".into()],
            TranslateOptions { html: true },
        )
        .await
        .unwrap();
    assert!(out[0].contains("<b>"), "markup dropped: {out:?}");
}

#[tokio::test]
async fn missing_model_file_is_an_error_not_a_crash() {
    let Some(root) = test_models() else { return };
    let backend = BergamotBackend::new(0).unwrap();
    let spec = ModelSpec {
        files: ModelFiles {
            model: root.join("no-such-model.bin"),
            vocab: Some(root.join("no-such-vocab.spm")),
            ..ModelFiles::default()
        },
        config_yaml: marian_config("no-such-model.bin"),
    };
    let (_worker, ready) = Worker::spawn(backend, spec, None);
    ready.await.unwrap().unwrap_err();
}
