// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Model management for dragomand.
//!
//! Read-only system stores and the writable user store ([`store`]),
//! per-model manifests ([`manifest`]), Mozilla toolkit version ordering
//! ([`version`]), the Remote Settings record acceptance rule ([`records`]),
//! the download provider ([`remote_settings`]) and the model registry
//! metadata provider ([`registry`]). Everything works fully
//! offline once models are installed; the network is only touched to fetch
//! records, download models and check for updates.

pub mod http;
pub mod manifest;
pub mod records;
pub mod registry;
pub mod remote_settings;
pub mod store;
pub mod version;

pub use manifest::Manifest;
pub use records::{FileType, FilterEnv, ModelSet};
pub use registry::{Registry, RegistryConfig, RegistryProvider};
pub use remote_settings::{RemoteSettingsConfig, RemoteSettingsProvider};
pub use store::{InstalledModel, Origin, Stores};
pub use version::MozVersion;
