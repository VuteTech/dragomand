// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! dragoman-krunner: a KRunner D-Bus plugin (`org.kde.krunner1`) in its
//! own small process, so KDE-specific interfaces stay out of the daemon.
//!
//! Query syntax: `tr SRC TRG text …` (or `tr text …` for the default
//! bg → en). The translation appears as the match; activating it copies
//! the text to the clipboard. The service is D-Bus activated by KRunner
//! and exits again after a few minutes of silence.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dragoman_client::{Translator1Proxy, call_with_request, response_code, result_key};
use zbus::zvariant::{OwnedValue, Value};

pub const BUS_NAME: &str = "dev.l10n_bg.dragomand.KRunner1";
pub const OBJECT_PATH: &str = "/dev/l10n_bg/dragomand/krunner";
const IDLE_EXIT: Duration = Duration::from_secs(300);

/// KRunner match categories (Plasma's QueryMatch types).
const MATCH_INFORMATIONAL: i32 = 50;
const MATCH_HELPER: i32 = 70;

type KMatch = (
    String,
    String,
    String,
    i32,
    f64,
    HashMap<String, OwnedValue>,
);

struct Runner {
    connection: zbus::Connection,
    last_used: Arc<Mutex<Instant>>,
}

fn simple_match(id: &str, text: String, kind: i32, relevance: f64, subtext: &str) -> KMatch {
    let mut properties = HashMap::new();
    if !subtext.is_empty() {
        if let Ok(value) = OwnedValue::try_from(Value::from(subtext)) {
            properties.insert("subtext".to_owned(), value);
        }
    }
    (
        id.to_owned(),
        text,
        "applications-education-language".to_owned(),
        kind,
        relevance,
        properties,
    )
}

fn parse_query(query: &str) -> Option<(String, String, String)> {
    let rest = query.strip_prefix("tr")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let looks_like_lang = |s: &str| {
        (2..=8).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    match tokens.as_slice() {
        [] => None,
        [src, trg, text @ ..]
            if !text.is_empty() && looks_like_lang(src) && looks_like_lang(trg) =>
        {
            Some(((*src).to_owned(), (*trg).to_owned(), text.join(" ")))
        }
        text => Some(("bg".to_owned(), "en".to_owned(), text.join(" "))),
    }
}

#[zbus::interface(name = "org.kde.krunner1")]
impl Runner {
    /// KRunner calls this on every keystroke (gated by the desktop file's
    /// match regex, so only `tr …` queries arrive).
    #[zbus(name = "Match")]
    async fn match_query(&self, query: String) -> Vec<KMatch> {
        *self.last_used.lock().expect("time lock") = Instant::now();
        let Some((source, target, text)) = parse_query(&query) else {
            return Vec::new();
        };
        if text.split_whitespace().count() < 1 || text.len() < 2 {
            return vec![simple_match(
                "hint",
                format!("tr {source} {target} …"),
                MATCH_INFORMATIONAL,
                0.1,
                "keep typing to translate",
            )];
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
        // KRunner expects answers quickly; the daemon may still be
        // installing a model. Give up politely rather than blocking.
        match tokio::time::timeout(Duration::from_secs(15), translate).await {
            Ok(Some(translation)) => vec![simple_match(
                &translation,
                translation.clone(),
                MATCH_HELPER,
                0.9,
                &format!("{source} → {target} · Enter copies to the clipboard"),
            )],
            Ok(None) => vec![simple_match(
                "error",
                format!("cannot translate {source} → {target}"),
                MATCH_INFORMATIONAL,
                0.1,
                "pair not available?",
            )],
            Err(_) => Vec::new(),
        }
    }

    async fn actions(&self) -> Vec<(String, String, String)> {
        Vec::new()
    }

    async fn run(&self, match_id: String, _action_id: String) {
        *self.last_used.lock().expect("time lock") = Instant::now();
        if match_id == "hint" || match_id == "error" {
            return;
        }
        copy_to_clipboard(&self.connection, &match_id).await;
    }
}

/// Klipper when present, wl-copy/xclip otherwise.
async fn copy_to_clipboard(connection: &zbus::Connection, text: &str) {
    let klipper: zbus::Result<()> = async {
        let reply = connection
            .call_method(
                Some("org.kde.klipper"),
                "/klipper",
                Some("org.kde.klipper.klipper"),
                "setClipboardContents",
                &(text,),
            )
            .await?;
        drop(reply);
        Ok(())
    }
    .await;
    if klipper.is_ok() {
        return;
    }
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
    eprintln!("dragoman-krunner: no clipboard tool found");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::Connection::session().await?;
    let last_used = Arc::new(Mutex::new(Instant::now()));
    let runner = Runner {
        connection: connection.clone(),
        last_used: Arc::clone(&last_used),
    };
    connection.object_server().at(OBJECT_PATH, runner).await?;
    connection.request_name(BUS_NAME).await?;

    // KRunner activates this service on demand; go away when unused.
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let idle = last_used.lock().expect("time lock").elapsed();
        if idle >= IDLE_EXIT {
            break;
        }
    }
    Ok(())
}
