// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! dragomand binary: run the daemon on the session bus until it goes
//! idle (models unloaded, no requests, quiet) or receives SIGTERM/SIGINT.
//! It is D-Bus activated and not meant to be started at login.
//! SIGUSR2 simulates memory pressure (unloads idle models).

use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("DRAGOMAND_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let options = dragomand::DaemonOptions::from_env()?;
    let daemon = dragomand::launch(options).await?;

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigusr2 = signal(SignalKind::user_defined2())?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = sigterm.recv() => break,
            _ = sigusr2.recv() => {
                tracing::info!("SIGUSR2: simulating memory pressure");
                daemon.simulate_memory_pressure().await;
            }
            () = daemon.wait_until_idle() => {
                tracing::info!("idle; releasing the bus name and exiting");
                let _ = daemon.release_name().await;
                break;
            }
        }
    }
    tracing::info!("shutting down");
    Ok(())
}
