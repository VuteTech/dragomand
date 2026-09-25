// SPDX-License-Identifier: GPL-3.0-or-later

//! dragomanctl: CLI and reference client for dragomand.
//! Exit codes: 0 success, 1 runtime failure, 2 usage error (from clap).

mod args;
mod bus;
mod store_cmd;

use std::io::Read;

use clap::{CommandFactory, Parser};

use args::{Cli, Command, StoreCommand, parse_pair};

fn parse_pairs(pairs: &[String]) -> Result<Vec<(String, String)>, String> {
    pairs.iter().map(|p| parse_pair(p)).collect()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("dragomanctl: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let json = cli.json;
    match cli.command {
        Command::Translate {
            source,
            target,
            html,
            no_pivot,
            batch,
            text,
        } => {
            let segments = if text.is_empty() {
                let mut input = String::new();
                std::io::stdin()
                    .read_to_string(&mut input)
                    .map_err(|e| e.to_string())?;
                input.lines().map(str::to_owned).collect()
            } else {
                text
            };
            if segments.is_empty() {
                return Err("nothing to translate".into());
            }
            bus::translate(&source, &target, segments, html, no_pivot, batch, json).await
        }
        Command::Pairs {
            installed,
            available,
        } => bus::pairs(installed, available, json).await,
        Command::Install { pairs } => bus::install(&parse_pairs(&pairs)?, json).await,
        Command::Remove { pairs } => bus::remove(&parse_pairs(&pairs)?, json).await,
        Command::Update { check } => bus::update(check, json).await,
        Command::Status => bus::status(json).await,
        Command::Store(store) => match store {
            StoreCommand::List => store_cmd::list(json),
            StoreCommand::Available => store_cmd::available(json).await,
            StoreCommand::Verify { pairs } => store_cmd::verify(&pairs, json),
            StoreCommand::Install { root, pairs } => {
                store_cmd::install(root, &parse_pairs(&pairs)?, json).await
            }
            StoreCommand::Remove { pairs } => store_cmd::remove(&parse_pairs(&pairs)?, json),
        },
        Command::Completions { shell } => {
            let mut command = Cli::command();
            let name = command.get_name().to_owned();
            clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
            Ok(())
        }
        Command::Manpage => {
            let man = clap_mangen::Man::new(Cli::command());
            man.render(&mut std::io::stdout())
                .map_err(|e| e.to_string())
        }
    }
}
