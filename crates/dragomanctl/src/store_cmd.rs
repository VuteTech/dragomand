// SPDX-License-Identifier: GPL-3.0-or-later

//! `dragomanctl store …`: direct store access without a session bus,
//! for packagers (chroots have no bus) and offline maintenance. Takes the
//! same lock as the daemon, so the two never write concurrently.

use std::path::PathBuf;

use dragoman_models::http::ReqwestHttp;
use dragoman_models::remote_settings::{RemoteSettingsConfig, RemoteSettingsProvider};
use dragoman_models::{Origin, Stores};

type Fail = String;

fn origin_str(origin: Origin) -> &'static str {
    match origin {
        Origin::System => "system",
        Origin::User => "user",
    }
}

pub fn list(json: bool) -> Result<(), Fail> {
    let stores = Stores::from_env();
    let mut problems = Vec::new();
    let mut models = stores.scan(&mut problems);
    models.sort_by(|a, b| (a.pair(), b.version.clone()).cmp(&(b.pair(), a.version.clone())));
    for problem in &problems {
        eprintln!("warning: {problem}");
    }

    if json {
        let rows: Vec<_> = models
            .iter()
            .map(|m| {
                serde_json::json!({
                    "source": m.manifest.source,
                    "target": m.manifest.target,
                    "version": m.version.as_str(),
                    "origin": origin_str(m.origin),
                    "architecture": m.manifest.architecture,
                    "directory": m.directory.display().to_string(),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(rows));
        return Ok(());
    }
    for model in &models {
        println!(
            "{:>7} -> {:<7} {:<7} {:<6} {}",
            model.manifest.source,
            model.manifest.target,
            model.version.as_str(),
            origin_str(model.origin),
            model.directory.display(),
        );
    }
    if models.is_empty() {
        eprintln!("no models installed");
    }
    Ok(())
}

pub fn verify(pairs: &[String], json: bool) -> Result<(), Fail> {
    let stores = Stores::from_env();
    let mut problems = Vec::new();
    let models = stores.scan(&mut problems);
    let mut checked = 0usize;
    let mut failures: Vec<String> = problems;

    for model in &models {
        if !pairs.is_empty() && !pairs.contains(&model.pair()) {
            continue;
        }
        checked += 1;
        if let Err(error) = Stores::verify(model) {
            failures.push(error.to_string());
        }
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"checked": checked, "failures": failures})
        );
    } else {
        for failure in &failures {
            eprintln!("FAILED: {failure}");
        }
        println!("verified {checked} model(s), {} failure(s)", failures.len());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err("verification failed".into())
    }
}

pub async fn available(json: bool) -> Result<(), Fail> {
    let provider =
        RemoteSettingsProvider::new(ReqwestHttp::new(), RemoteSettingsConfig::from_env());
    let available = provider.available().await.map_err(|e| e.to_string())?;
    for filter in &available.skipped_filters {
        eprintln!("warning: skipped records with an unknown filter: {filter}");
    }
    let mut sets = available.sets;
    sets.sort_by_key(|set| set.pair());
    if json {
        let rows: Vec<_> = sets
            .iter()
            .map(|set| {
                serde_json::json!({
                    "source": set.source,
                    "target": set.target,
                    "variant": set.variant,
                    "version": set.version.as_str(),
                    "architecture": set.architecture,
                    "last_modified": set.files.values().filter_map(|r| r.last_modified).max(),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(rows));
    } else {
        for set in &sets {
            println!(
                "{:<12} {:<8} {}",
                set.pair(),
                set.version.as_str(),
                set.architecture.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

pub async fn install(
    root: Option<PathBuf>,
    pairs: &[(String, String)],
    json: bool,
) -> Result<(), Fail> {
    let stores = match root {
        // A package build fills an isolated store root.
        Some(root) => Stores {
            system: vec![],
            user: root,
        },
        None => Stores::from_env(),
    };
    let provider =
        RemoteSettingsProvider::new(ReqwestHttp::new(), RemoteSettingsConfig::from_env());
    let mut report = Vec::new();
    for (source, target) in pairs {
        let installed = provider
            .install_pair(&stores, source, target)
            .await
            .map_err(|e| e.to_string())?;
        if json {
            report.push(serde_json::json!({
                "source": source,
                "target": target,
                "version": installed.version.as_str(),
                "directory": installed.directory.display().to_string(),
            }));
        } else {
            println!(
                "installed {source}-{target} {} into {}",
                installed.version.as_str(),
                installed.directory.display()
            );
        }
    }
    if json {
        println!("{}", serde_json::Value::Array(report));
    }
    Ok(())
}

pub fn remove(pairs: &[(String, String)], json: bool) -> Result<(), Fail> {
    let stores = Stores::from_env();
    let _lock = stores.lock_user().map_err(|e| e.to_string())?;
    let mut problems = Vec::new();
    let models = stores.scan(&mut problems);
    let mut removed = 0usize;
    for (source, target) in pairs {
        let pair = format!("{source}-{target}");
        let mine: Vec<_> = models
            .iter()
            .filter(|m| m.pair() == pair && m.origin == Origin::User)
            .collect();
        if mine.is_empty() {
            return Err(format!("{pair} is not installed in the user store"));
        }
        for model in mine {
            stores.remove(model).map_err(|e| e.to_string())?;
            removed += 1;
            if !json {
                println!("removed {} {}", pair, model.version.as_str());
            }
        }
    }
    if json {
        println!("{}", serde_json::json!({"removed": removed}));
    }
    Ok(())
}
