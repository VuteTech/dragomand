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

/// Splits "bg-en" into ("bg", "en").
pub fn parse_pair(pair: &str) -> Result<(String, String), String> {
    match pair.split_once('-') {
        Some((source, target)) if !source.is_empty() && !target.is_empty() => {
            Ok((source.to_owned(), target.to_owned()))
        }
        _ => Err(format!("bad pair {pair:?}: expected SRC-TRG, e.g. bg-en")),
    }
}
