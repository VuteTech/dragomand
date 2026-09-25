// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! dragoman-search-provider: `org.gnome.Shell.SearchProvider2` in its
//! own small process, so GNOME-specific interfaces stay out of the
//! daemon. Search `tr SRC TRG text` (or `tr text` for bg → en) in the
//! GNOME overview; activating the result copies the translation.
//!
//! GNOME sends every overview keystroke to every provider, so anything
//! not starting with the `tr ` keyword is ignored without touching the
//! daemon.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dragoman_client::{Translator1Proxy, call_with_request, response_code, result_key};
use zbus::zvariant::{OwnedValue, Value};

pub const BUS_NAME: &str = "dev.l10n_bg.dragomand.SearchProvider";
pub const OBJECT_PATH: &str = "/dev/l10n_bg/dragomand/SearchProvider";
const IDLE_EXIT: Duration = Duration::from_secs(300);

struct Provider {
    connection: zbus::Connection,
    last_used: Arc<Mutex<Instant>>,
}

fn parse_terms(terms: &[String]) -> Option<(String, String, String)> {
    let (first, rest) = terms.split_first()?;
    if first != "tr" || rest.is_empty() {
        return None;
    }
    let looks_like_lang = |s: &String| {
        (2..=8).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    match rest {
        [src, trg, text @ ..]
            if !text.is_empty() && looks_like_lang(src) && looks_like_lang(trg) =>
        {
            Some((src.clone(), trg.clone(), text.join(" ")))
        }
        text => Some(("bg".to_owned(), "en".to_owned(), text.join(" "))),
    }
}

impl Provider {
    async fn translate_terms(&self, terms: Vec<String>) -> Vec<String> {
        *self.last_used.lock().expect("time lock") = Instant::now();
        let Some((source, target, text)) = parse_terms(&terms) else {
            return Vec::new();
        };
        if text.len() < 2 {
            return Vec::new();
        }
        let translate = async {
            let proxy = Translator1Proxy::new(&self.connection).await.ok()?;
            let outcome = call_with_request(&self.connection, |token| {
                let proxy = proxy.clone();
                let (source, target, text) = (source.clone(), target.clone(), text.clone());
                async move {
                    let mut options = HashMap::new();
                    options.insert("handle_token", Value::from(token));
                    proxy.translate(&source, &target, vec![text], options).await
                }
            })
            .await
            .ok()?;
            if outcome.code != response_code::SUCCESS {
                return None;
            }
            outcome
                .results
                .get(result_key::TRANSLATIONS)
                .cloned()
                .and_then(|v| Vec::<String>::try_from(v).ok())
                .and_then(|mut v| v.pop())
        };
        match tokio::time::timeout(Duration::from_secs(15), translate).await {
            // The result id carries the translation; the provider stays
            // stateless.
            Ok(Some(translation)) => vec![format!("{source}\u{1f}{target}\u{1f}{translation}")],
            _ => Vec::new(),
        }
    }
}

fn split_id(id: &str) -> (String, String, String) {
    let mut parts = id.splitn(3, '\u{1f}');
    let source = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let translation = parts.next().unwrap_or(id).to_owned();
    (source, target, translation)
}

#[zbus::interface(name = "org.gnome.Shell.SearchProvider2")]
impl Provider {
    #[zbus(name = "GetInitialResultSet")]
    async fn get_initial_result_set(&self, terms: Vec<String>) -> Vec<String> {
        self.translate_terms(terms).await
    }

    #[zbus(name = "GetSubsearchResultSet")]
    async fn get_subsearch_result_set(
        &self,
        _previous_results: Vec<String>,
        terms: Vec<String>,
    ) -> Vec<String> {
        self.translate_terms(terms).await
    }

    #[zbus(name = "GetResultMetas")]
    async fn get_result_metas(&self, identifiers: Vec<String>) -> Vec<HashMap<String, OwnedValue>> {
        identifiers
            .iter()
            .map(|id| {
                let (source, target, translation) = split_id(id);
                let mut meta = HashMap::new();
                let mut put = |key: &str, value: &str| {
                    if let Ok(value) = OwnedValue::try_from(Value::from(value)) {
                        meta.insert(key.to_owned(), value);
                    }
                };
                put("id", id);
                put("name", &translation);
                put(
                    "description",
                    &format!("{source} → {target} · activating copies to the clipboard"),
                );
                put("gicon", "applications-education-language");
                meta
            })
            .collect()
    }

    #[zbus(name = "ActivateResult")]
    async fn activate_result(&self, identifier: String, _terms: Vec<String>, _timestamp: u32) {
        *self.last_used.lock().expect("time lock") = Instant::now();
        let (_, _, translation) = split_id(&identifier);
        copy_to_clipboard(&translation).await;
    }

    #[zbus(name = "LaunchSearch")]
    async fn launch_search(&self, _terms: Vec<String>, _timestamp: u32) {}
}

/// wl-copy on Wayland sessions (the GNOME default), xclip as fallback.
async fn copy_to_clipboard(text: &str) {
    for (cmd, args) in [
        ("wl-copy", vec![]),
        ("xclip", vec!["-i", "-selection", "clipboard"]),
    ] {
        let mut child = match tokio::process::Command::new(cmd)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => continue,
        };
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(text.as_bytes()).await;
        }
        let _ = child.wait().await;
        return;
    }
    eprintln!("dragoman-search-provider: no clipboard tool found");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::Connection::session().await?;
    let last_used = Arc::new(Mutex::new(Instant::now()));
    let provider = Provider {
        connection: connection.clone(),
        last_used: Arc::clone(&last_used),
    };
    connection.object_server().at(OBJECT_PATH, provider).await?;
    connection.request_name(BUS_NAME).await?;

    // GNOME activates this service on demand; go away when unused.
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let idle = last_used.lock().expect("time lock").elapsed();
        if idle >= IDLE_EXIT {
            break;
        }
    }
    Ok(())
}
