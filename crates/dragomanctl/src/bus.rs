// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon-backed commands (everything except `store`).

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::path::Path;

use dragoman_client::{
    Request1Proxy, Translator1Proxy, call_with_request, response_code, result_key,
};
use futures_util::StreamExt;
use zbus::zvariant::{OwnedValue, Value};

pub type Fail = String;

pub async fn connect() -> Result<(zbus::Connection, Translator1Proxy<'static>), Fail> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("cannot connect to the session bus: {e}"))?;
    let proxy = Translator1Proxy::new(&connection)
        .await
        .map_err(|e| e.to_string())?;
    Ok((connection, proxy))
}

fn error_name(error: &zbus::Error) -> Option<&str> {
    match error {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str()),
        _ => None,
    }
}

fn response_error(results: &HashMap<String, OwnedValue>, fallback: &str) -> Fail {
    results
        .get(result_key::ERROR_MESSAGE)
        .and_then(|v| v.downcast_ref::<&str>().ok().map(str::to_owned))
        .unwrap_or_else(|| fallback.to_owned())
}

fn as_str(value: &OwnedValue) -> Option<String> {
    value.downcast_ref::<&str>().ok().map(str::to_owned)
}

pub async fn translate(
    source: &str,
    target: &str,
    segments: Vec<String>,
    html: bool,
    no_pivot: bool,
    batch: bool,
    json: bool,
) -> Result<(), Fail> {
    let (connection, proxy) = connect().await?;

    let call = |token: String| {
        let proxy = proxy.clone();
        let (source, target, segments) = (source.to_owned(), target.to_owned(), segments.clone());
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            if html {
                options.insert("html", Value::from(true));
            }
            if no_pivot {
                options.insert("allow_pivot", Value::from(false));
            }
            if batch {
                options.insert("priority", Value::from("batch"));
            }
            proxy.translate(&source, &target, segments, options).await
        }
    };

    let outcome = match call_with_request(&connection, call).await {
        Ok(outcome) => outcome,
        Err(error) if error_name(&error) == Some("dev.l10n_bg.dragomand.Error.NotInstalled") => {
            eprintln!("dragomanctl: {source}-{target} is not installed, fetching it …");
            prepare(&connection, &proxy, source, target, no_pivot).await?;
            call_with_request(&connection, call)
                .await
                .map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    if outcome.code != response_code::SUCCESS {
        return Err(response_error(&outcome.results, "translation failed"));
    }

    let translations = outcome
        .results
        .get(result_key::TRANSLATIONS)
        .cloned()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .ok_or("malformed response")?;
    let pivot = outcome.results.get(result_key::PIVOT).and_then(as_str);
    if json {
        let mut object = serde_json::Map::new();
        object.insert("translations".into(), translations.into());
        if let Some(pivot) = &pivot {
            object.insert("pivot".into(), pivot.clone().into());
        }
        println!("{}", serde_json::Value::Object(object));
    } else {
        for line in &translations {
            println!("{line}");
        }
        if let Some(pivot) = pivot {
            eprintln!("dragomanctl: translated via {pivot}");
        }
    }
    Ok(())
}

/// Translates a document through TranslateFd: the daemon reads the file
/// and writes the translation straight into `output` (or our stdout).
#[allow(clippy::too_many_arguments)]
pub async fn translate_file(
    source: &str,
    target: &str,
    file: &Path,
    output: Option<&Path>,
    html: bool,
    no_pivot: bool,
    batch: bool,
    json: bool,
) -> Result<(), Fail> {
    let (connection, proxy) = connect().await?;
    let input = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let output_file = match output {
        Some(path) => {
            Some(std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?)
        }
        None => None,
    };
    let stdout = std::io::stdout();

    let call = |token: String| {
        let proxy = proxy.clone();
        let connection = connection.clone();
        let (source, target) = (source.to_owned(), target.to_owned());
        let input = &input;
        let output_fd = match &output_file {
            Some(f) => f.as_fd(),
            None => stdout.as_fd(),
        };
        async move {
            // Report progress on stderr while the document is translated.
            if !json {
                if let Some(unique) = connection.unique_name() {
                    if let Ok(path) = dragoman_client::request_path(unique.as_str(), &token) {
                        if let Ok(request) = Request1Proxy::builder(&connection)
                            .path(path)?
                            .build()
                            .await
                        {
                            if let Ok(mut progress) = request.receive_progress().await {
                                tokio::spawn(async move {
                                    while let Some(signal) = progress.next().await {
                                        if let Ok(args) = signal.args() {
                                            eprint!(
                                                "\rdragomanctl: {:3.0}%",
                                                args.fraction * 100.0
                                            );
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
            }
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            if html {
                options.insert("html", Value::from(true));
            }
            if no_pivot {
                options.insert("allow_pivot", Value::from(false));
            }
            if !batch {
                options.insert("priority", Value::from("interactive"));
            }
            proxy
                .translate_fd(
                    &source,
                    &target,
                    input.as_fd().into(),
                    output_fd.into(),
                    options,
                )
                .await
        }
    };

    let outcome = match call_with_request(&connection, call).await {
        Ok(outcome) => outcome,
        Err(error) if error_name(&error) == Some("dev.l10n_bg.dragomand.Error.NotInstalled") => {
            eprintln!("dragomanctl: {source}-{target} is not installed, fetching it …");
            prepare(&connection, &proxy, source, target, no_pivot).await?;
            call_with_request(&connection, call)
                .await
                .map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    if !json {
        eprintln!();
    }
    if outcome.code != response_code::SUCCESS {
        return Err(response_error(&outcome.results, "translation failed"));
    }
    let lines = outcome
        .results
        .get(result_key::LINES)
        .and_then(|v| v.downcast_ref::<u32>().ok())
        .unwrap_or(0);
    let pivot = outcome.results.get(result_key::PIVOT).and_then(as_str);
    if json {
        println!("{}", serde_json::json!({"lines": lines, "pivot": pivot}));
    } else if let Some(pivot) = pivot {
        eprintln!("dragomanctl: translated {lines} lines via {pivot}");
    } else {
        eprintln!("dragomanctl: translated {lines} lines");
    }
    Ok(())
}

pub async fn detect(text: &str, candidates: &[String], json: bool) -> Result<(), Fail> {
    let (_connection, proxy) = connect().await?;
    let mut options = HashMap::new();
    if !candidates.is_empty() {
        options.insert("candidates", Value::from(candidates.to_vec()));
    }
    let results = proxy
        .detect_language(text, options)
        .await
        .map_err(|e| e.to_string())?;
    let language = results.get("language").and_then(as_str);
    let confidence = results
        .get("confidence")
        .and_then(|v| v.downcast_ref::<f64>().ok())
        .unwrap_or(0.0);
    let reliable = results
        .get("reliable")
        .and_then(|v| v.downcast_ref::<bool>().ok())
        .unwrap_or(false);
    if json {
        println!(
            "{}",
            serde_json::json!({"language": language, "confidence": confidence, "reliable": reliable})
        );
        return Ok(());
    }
    match language {
        Some(language) => println!(
            "{language} (confidence {confidence:.2}{})",
            if reliable { "" } else { ", unreliable" }
        ),
        None => return Err("no language detected".into()),
    }
    Ok(())
}

fn config_value_json(value: &OwnedValue) -> serde_json::Value {
    if let Ok(b) = value.downcast_ref::<bool>() {
        return b.into();
    }
    if let Ok(n) = value.downcast_ref::<u64>() {
        return n.into();
    }
    if let Ok(n) = value.downcast_ref::<u32>() {
        return n.into();
    }
    serde_json::Value::Null
}

pub async fn config(key: Option<&str>, value: Option<&str>, json: bool) -> Result<(), Fail> {
    let (_connection, proxy) = connect().await?;
    if let (Some(key), Some(value)) = (key, value) {
        let parsed = match value {
            "true" | "yes" | "on" => Value::from(true),
            "false" | "no" | "off" => Value::from(false),
            number => Value::from(
                number
                    .parse::<u64>()
                    .map_err(|_| format!("{value:?} is neither true/false nor a number"))?,
            ),
        };
        let mut changes = HashMap::new();
        changes.insert(key, parsed);
        proxy.set_config(changes).await.map_err(|e| e.to_string())?;
    }
    let config = proxy.get_config().await.map_err(|e| e.to_string())?;
    let mut entries: Vec<(&String, serde_json::Value)> = config
        .iter()
        .map(|(k, v)| (k, config_value_json(v)))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    if let Some(key) = key {
        let (_, value) = entries
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .ok_or_else(|| format!("no setting {key:?}"))?;
        if json {
            println!("{}", serde_json::json!({ key: value }));
        } else {
            println!("{value}");
        }
        return Ok(());
    }
    if json {
        let object: serde_json::Map<String, serde_json::Value> =
            entries.into_iter().map(|(k, v)| (k.clone(), v)).collect();
        println!("{}", serde_json::Value::Object(object));
    } else {
        for (key, value) in entries {
            println!("{key} = {value}");
        }
    }
    Ok(())
}

async fn prepare(
    connection: &zbus::Connection,
    proxy: &Translator1Proxy<'static>,
    source: &str,
    target: &str,
    no_pivot: bool,
) -> Result<(), Fail> {
    let outcome = call_with_request(connection, |token| {
        let proxy = proxy.clone();
        let (source, target) = (source.to_owned(), target.to_owned());
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            if no_pivot {
                options.insert("allow_pivot", Value::from(false));
            }
            proxy.prepare_pair(&source, &target, options).await
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    if outcome.code != response_code::SUCCESS {
        return Err(response_error(&outcome.results, "install failed"));
    }
    Ok(())
}

pub async fn pairs(installed_only: bool, available_only: bool, json: bool) -> Result<(), Fail> {
    let (_connection, proxy) = connect().await?;
    let rows = proxy
        .list_language_pairs()
        .await
        .map_err(|e| e.to_string())?;

    let mut printed = Vec::new();
    for row in &rows {
        let get = |k: &str| row.get(k).and_then(as_str);
        let installed = get("installed_version");
        let available = get("available_version");
        if installed_only && installed.is_none() {
            continue;
        }
        if available_only && available.is_none() {
            continue;
        }
        let (Some(source), Some(target)) = (get("source"), get("target")) else {
            continue;
        };
        let size = row.get("size").and_then(|v| v.downcast_ref::<u64>().ok());
        let quality = row
            .get("quality")
            .and_then(|v| v.downcast_ref::<f64>().ok());
        printed.push(serde_json::json!({
            "source": source,
            "target": target,
            "installed_version": installed,
            "available_version": available,
            "origin": get("origin"),
            "architecture": get("architecture"),
            "size": size,
            "release_status": get("release_status"),
            "quality": quality,
        }));
    }

    if json {
        println!("{}", serde_json::Value::Array(printed));
        return Ok(());
    }
    if printed.is_empty() {
        eprintln!(
            "no pairs to show{}",
            if available_only || !installed_only {
                " (run `dragomanctl update --check` to fetch the catalog)"
            } else {
                ""
            }
        );
        return Ok(());
    }
    for row in &printed {
        let s = |k: &str| row[k].as_str().unwrap_or("-").to_owned();
        let size = row["size"]
            .as_u64()
            .map(|b| format!("{:.1} MB", b as f64 / 1e6))
            .unwrap_or_else(|| "-".into());
        let quality = row["quality"]
            .as_f64()
            .map(|q| format!("quality {q:.2}"))
            .unwrap_or_default();
        println!(
            "{:>7} -> {:<7} installed: {:<7} available: {:<7} {:<6} {:<12} {:<9} {}",
            s("source"),
            s("target"),
            s("installed_version"),
            s("available_version"),
            s("origin"),
            s("architecture"),
            size,
            quality,
        );
    }
    Ok(())
}

pub async fn install(pairs: &[(String, String)], json: bool) -> Result<(), Fail> {
    let (connection, proxy) = connect().await?;
    let mut report = Vec::new();
    for (source, target) in pairs {
        let outcome = call_with_request(&connection, |token| {
            let proxy = proxy.clone();
            let (source, target) = (source.clone(), target.clone());
            async move {
                let mut options = HashMap::new();
                options.insert("handle_token", Value::from(token));
                proxy.install_pair(&source, &target, options).await
            }
        })
        .await
        .map_err(|e| e.to_string())?;
        if outcome.code != response_code::SUCCESS {
            return Err(response_error(
                &outcome.results,
                &format!("installing {source}-{target} failed"),
            ));
        }
        let version = outcome.results.get("version").and_then(as_str);
        if json {
            report.push(serde_json::json!({
                "source": source, "target": target, "version": version,
            }));
        } else {
            println!(
                "installed {source}-{target} {}",
                version.unwrap_or_default()
            );
        }
    }
    if json {
        println!("{}", serde_json::Value::Array(report));
    }
    Ok(())
}

pub async fn remove(pairs: &[(String, String)], json: bool) -> Result<(), Fail> {
    let (_connection, proxy) = connect().await?;
    for (source, target) in pairs {
        proxy
            .remove_pair(source, target)
            .await
            .map_err(|e| e.to_string())?;
        if !json {
            println!("removed {source}-{target}");
        }
    }
    if json {
        println!("{}", serde_json::json!({"removed": pairs.len()}));
    }
    Ok(())
}

pub async fn update(check_only: bool, json: bool) -> Result<(), Fail> {
    let (connection, proxy) = connect().await?;
    let outcome = call_with_request(&connection, |token| {
        let proxy = proxy.clone();
        async move {
            let mut options = HashMap::new();
            options.insert("handle_token", Value::from(token));
            proxy.check_for_updates(options).await
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    if outcome.code != response_code::SUCCESS {
        return Err(response_error(&outcome.results, "update check failed"));
    }
    let updates = outcome
        .results
        .get("updates")
        .cloned()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .unwrap_or_default();

    if check_only {
        if json {
            println!("{}", serde_json::json!({"updates": updates}));
        } else if updates.is_empty() {
            println!("everything is up to date");
        } else {
            for update in &updates {
                println!("update available: {update}");
            }
        }
        return Ok(());
    }

    // Install every reported update ("src-trg old -> new" lines).
    let pairs: Vec<(String, String)> = updates
        .iter()
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|pair| crate::args::parse_pair(pair).ok())
        .collect();
    if pairs.is_empty() {
        if json {
            println!("{}", serde_json::json!({"updated": []}));
        } else {
            println!("everything is up to date");
        }
        return Ok(());
    }
    install(&pairs, json).await
}

pub async fn status(json: bool) -> Result<(), Fail> {
    let (_connection, proxy) = connect().await?;
    let status = proxy.get_status().await.map_err(|e| e.to_string())?;
    let loaded = status
        .get("loaded")
        .cloned()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .unwrap_or_default();
    let queued = status
        .get("queued")
        .and_then(|v| v.downcast_ref::<u32>().ok())
        .unwrap_or(0);
    let get_u64 = |k: &str| status.get(k).and_then(|v| v.downcast_ref::<u64>().ok());
    let version = status.get("version").and_then(as_str).unwrap_or_default();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "version": version,
                "loaded": loaded,
                "queued": queued,
                "model_cost_mb": get_u64("model_cost_mb"),
                "rss_mb": get_u64("rss_mb"),
            })
        );
    } else {
        println!("daemon version: {version}");
        println!(
            "loaded routes:  {}",
            if loaded.is_empty() {
                "(none)".to_owned()
            } else {
                loaded.join(", ")
            }
        );
        println!("queued jobs:    {queued}");
        if let (Some(cost), Some(rss)) = (get_u64("model_cost_mb"), get_u64("rss_mb")) {
            println!("model memory:   ~{cost} MB (process RSS {rss} MB)");
        }
    }
    Ok(())
}
