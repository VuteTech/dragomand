// SPDX-License-Identifier: GPL-3.0-or-later

//! Command-line definition.
//!
//! Exit codes: 0 success, 1 runtime failure (daemon error, verification
//! failure, network trouble), 2 usage error.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "dragomanctl",
    version,
    about = "Offline machine translation (client for dragomand)",
    long_about = "Client for the dragomand translation daemon.\n\
                  Exit codes: 0 success, 1 runtime failure, 2 usage error."
)]
pub struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Translate text (reads stdin when no TEXT is given, one segment per
    /// line). Installs the language pair on first use.
    Translate {
        /// Source language (BCP-47, e.g. bg).
        #[arg(short = 'f', long = "from")]
        source: String,
        /// Target language.
        #[arg(short = 't', long = "to")]
        target: String,
        /// Treat the input as HTML and preserve markup.
        #[arg(long)]
        html: bool,
        /// Refuse to pivot through English.
        #[arg(long)]
        no_pivot: bool,
        /// Queue behind interactive requests.
        #[arg(long)]
        batch: bool,
        /// Text segments; stdin when empty.
        text: Vec<String>,
    },
    /// List language pairs.
    Pairs {
        /// Only pairs installed locally.
        #[arg(long, conflicts_with = "available")]
        installed: bool,
        /// Only pairs known from the provider (cached records).
        #[arg(long)]
        available: bool,
    },
    /// Install (or upgrade) language pairs, e.g. bg-en.
    Install {
        /// Pairs as SRC-TRG.
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Remove language pairs from the user store.
    Remove {
        /// Pairs as SRC-TRG.
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Check for model updates, and install them unless --check.
    Update {
        /// Only report available updates.
        #[arg(long)]
        check: bool,
    },
    /// Show daemon status (loaded models, memory, queue).
    Status,
    /// Direct model-store access without the daemon (packaging, offline
    /// maintenance, debugging).
    #[command(subcommand)]
    Store(StoreCommand),
    /// Print shell completions to stdout.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print the man page (roff) to stdout.
    #[command(hide = true)]
    Manpage,
}

#[derive(Subcommand)]
pub enum StoreCommand {
    /// List every installed model in every store.
    List,
    /// Re-verify installed models (sizes and sha256) against their
    /// manifests. Exits 1 when anything fails.
    Verify {
        /// Pairs to verify; everything when empty.
        pairs: Vec<String>,
    },
    /// List the pairs the provider offers and the version each would
    /// install. Contacts the provider, not the daemon.
    Available,
    /// Download and install pairs into a store directory (defaults to the
    /// user store). --root fills a system store for model packages.
    Install {
        /// Target store directory (e.g. pkg/usr/share/dragomand/models).
        #[arg(long)]
        root: Option<std::path::PathBuf>,
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Remove pairs from the user store.
    Remove {
        #[arg(required = true)]
        pairs: Vec<String>,
    },
}

/// Splits "bg-en" into ("bg", "en"). Language codes may carry a script or
/// region subtag, as in "zh-Hans-en": a four-letter script ("Hans") or a
/// two-letter uppercase or three-digit region ("BR", "419") belongs to the
/// code before it. Scripts are normalized to title case, so "zh-hans-en"
/// works too.
pub fn parse_pair(pair: &str) -> Result<(String, String), String> {
    let bad = || format!("bad pair {pair:?}: expected SRC-TRG, e.g. bg-en or zh-Hans-en");
    let mut codes: Vec<String> = Vec::new();
    for part in pair.split('-') {
        if part.is_empty() {
            return Err(bad());
        }
        let is_script = part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic());
        let is_region = (part.len() == 2 && part.chars().all(|c| c.is_ascii_uppercase()))
            || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()));
        match codes.last_mut() {
            Some(code) if is_script => {
                let (first, rest) = part.split_at(1);
                code.push('-');
                code.push_str(&first.to_ascii_uppercase());
                code.push_str(&rest.to_ascii_lowercase());
            }
            Some(code) if is_region => {
                code.push('-');
                code.push_str(part);
            }
            _ => codes.push(part.to_owned()),
        }
    }
    match <[String; 2]>::try_from(codes) {
        Ok([source, target]) => Ok((source, target)),
        Err(_) => Err(bad()),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_pair;

    fn ok(pair: &str) -> (String, String) {
        parse_pair(pair).unwrap()
    }

    #[test]
    fn plain_pairs() {
        assert_eq!(ok("bg-en"), ("bg".into(), "en".into()));
    }

    #[test]
    fn script_and_region_subtags_stay_with_their_code() {
        assert_eq!(ok("zh-Hans-en"), ("zh-Hans".into(), "en".into()));
        assert_eq!(ok("en-zh-Hant"), ("en".into(), "zh-Hant".into()));
        assert_eq!(ok("zh-hans-en"), ("zh-Hans".into(), "en".into()));
        assert_eq!(ok("pt-BR-en"), ("pt-BR".into(), "en".into()));
    }

    #[test]
    fn malformed_pairs_are_rejected() {
        for pair in ["bg", "bg-", "-en", "bg--en", "bg-en-de", ""] {
            assert!(parse_pair(pair).is_err(), "{pair:?} should be rejected");
        }
    }
}
