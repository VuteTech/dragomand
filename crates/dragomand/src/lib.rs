// SPDX-License-Identifier: GPL-3.0-or-later

//! dragomand: per-user daemon exposing offline machine translation over
//! D-Bus as `dev.l10n_bg.dragomand.Translator1`.
//!
//! This is a library so the integration tests can run the daemon
//! in-process on a private bus; the binary in `main.rs` is a thin wrapper.

pub mod backend;
pub mod config;
pub mod error;
pub mod lifecycle;
pub mod requests;
pub mod service;
pub mod workers;

use std::sync::Arc;

use dragoman_client::names;
use dragoman_models::Stores;
use dragoman_models::http::ReqwestHttp;
use dragoman_models::remote_settings::{RemoteSettingsConfig, RemoteSettingsProvider};

use backend::BackendKind;
use service::{DaemonState, Translator};

pub struct DaemonOptions {
    pub config: config::Config,
    pub backend: BackendKind,
    pub stores: Stores,
    pub provider: RemoteSettingsConfig,
    /// D-Bus address to connect to; `None` uses the session bus.
    pub bus_address: Option<String>,
}

impl DaemonOptions {
    pub fn from_env() -> Result<Self, String> {
        let config = config::Config::load()?;
        let mut provider = RemoteSettingsConfig::from_env();
        provider.allow_prerelease = config.allow_prerelease;
        Ok(DaemonOptions {
            config,
            backend: BackendKind::default_for_build(),
            stores: Stores::from_env(),
            provider,
            bus_address: None,
        })
    }
}

/// A running daemon; dropping it closes the connection.
pub struct Daemon {
    pub connection: zbus::Connection,
    state: Arc<DaemonState>,
}

impl Daemon {
    /// Resolves when the daemon has nothing loaded, nothing in flight and
    /// has been idle beyond the configured window. The caller then
    /// releases the name and exits; the next call re-activates it.
    pub async fn wait_until_idle(&self) {
        lifecycle::wait_until_idle(Arc::clone(&self.state)).await;
    }

    /// Gives up the bus name (new calls re-activate a fresh daemon).
    pub async fn release_name(&self) -> zbus::Result<bool> {
        self.connection.release_name(names::BUS_NAME).await
    }

    /// Acts as if memory pressure fired (SIGUSR2 in the binary, tests).
    pub async fn simulate_memory_pressure(&self) {
        self.state.workers.evict_unpinned().await;
    }
}

/// Connects, serves the interface, then claims the bus name (in that
/// order, so activation never sees the name without the object).
pub async fn launch(options: DaemonOptions) -> zbus::Result<Daemon> {
    let mut provider_config = options.provider;
    provider_config.allow_prerelease = options.config.allow_prerelease;
    let state = Arc::new(DaemonState::new(
        options.config,
        options.backend,
        options.stores,
        RemoteSettingsProvider::new(ReqwestHttp::new(), provider_config),
    ));

    let builder = match &options.bus_address {
        Some(address) => zbus::connection::Builder::address(address.as_str())?,
        None => zbus::connection::Builder::session()?,
    };
    let connection = builder
        .serve_at(
            names::OBJECT_PATH,
            Translator {
                state: Arc::clone(&state),
            },
        )?
        .name(names::BUS_NAME)?
        .build()
        .await?;

    tokio::spawn(requests::watch_disconnects(
        connection.clone(),
        Arc::clone(&state.requests),
    ));
    tokio::spawn(lifecycle::run_sweeper(Arc::clone(&state)));
    tokio::spawn(lifecycle::watch_memory_pressure(Arc::clone(&state)));

    tracing::info!(name = names::BUS_NAME, "listening");
    Ok(Daemon { connection, state })
}
