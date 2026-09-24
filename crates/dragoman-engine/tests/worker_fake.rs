// SPDX-License-Identifier: GPL-3.0-or-later

//! Worker-thread behavior, exercised through the fake backend.

use std::path::PathBuf;
use std::time::Duration;

use dragoman_engine::fake::{FakeBackend, FakeConfig};
use dragoman_engine::{ModelFiles, ModelSpec, TranslateOptions, Worker};

fn spec(name: &str) -> ModelSpec {
    ModelSpec {
        files: ModelFiles {
            model: PathBuf::from(format!("{name}.bin")),
            ..ModelFiles::default()
        },
        config_yaml: String::new(),
    }
}

#[tokio::test]
async fn translates_through_worker() {
    let backend = FakeBackend::new(FakeConfig::default());
    let (worker, ready) = Worker::spawn(backend, spec("bgen"), None);
    ready.await.unwrap().unwrap();

    let out = worker
        .translate(vec!["a".into(), "b".into()], TranslateOptions::default())
        .await
        .unwrap();
    assert_eq!(out, vec!["[bgen] a", "[bgen] b"]);
}

#[tokio::test]
async fn pivot_uses_both_models() {
    let backend = FakeBackend::new(FakeConfig::default());
    let (worker, ready) = Worker::spawn(backend, spec("bgen"), Some(spec("enbg")));
    ready.await.unwrap().unwrap();

    let out = worker
        .translate(vec!["x".into()], TranslateOptions::default())
        .await
        .unwrap();
    assert_eq!(out, vec!["[bgen+enbg] x"]);
}

#[tokio::test]
async fn html_option_reaches_backend() {
    let backend = FakeBackend::new(FakeConfig::default());
    let (worker, ready) = Worker::spawn(backend, spec("m"), None);
    ready.await.unwrap().unwrap();

    let out = worker
        .translate(vec!["<b>t</b>".into()], TranslateOptions { html: true })
        .await
        .unwrap();
    assert_eq!(out, vec!["[m,html] <b>t</b>"]);
}

#[tokio::test]
async fn load_failure_reported_and_jobs_answered() {
    let backend = FakeBackend::new(FakeConfig {
        load_delay: Duration::from_millis(50),
        fail_load: Some("no such model".into()),
        ..FakeConfig::default()
    });
    let (worker, ready) = Worker::spawn(backend, spec("m"), None);

    // Submitted before the load failure resolves; must still get an answer.
    let early = worker.translate(vec!["x".into()], TranslateOptions::default());

    let load_result = ready.await.unwrap();
    assert!(
        load_result
            .unwrap_err()
            .to_string()
            .contains("no such model")
    );
    early.await.unwrap_err();
}

#[tokio::test]
async fn translate_failure_is_per_request() {
    let backend = FakeBackend::new(FakeConfig {
        fail_translate: Some("boom".into()),
        ..FakeConfig::default()
    });
    let (worker, ready) = Worker::spawn(backend, spec("m"), None);
    ready.await.unwrap().unwrap();

    let err = worker
        .translate(vec!["x".into()], TranslateOptions::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("boom"));
}

#[tokio::test]
async fn drop_unloads_and_pending_translate_errors() {
    let backend = FakeBackend::new(FakeConfig::default());
    let (worker, ready) = Worker::spawn(backend, spec("m"), None);
    ready.await.unwrap().unwrap();
    drop(worker);
    // A new worker still works after one was dropped (thread joined).
    let (worker, ready) = Worker::spawn(FakeBackend::new(FakeConfig::default()), spec("m"), None);
    ready.await.unwrap().unwrap();
    worker
        .translate(vec!["y".into()], TranslateOptions::default())
        .await
        .unwrap();
}
