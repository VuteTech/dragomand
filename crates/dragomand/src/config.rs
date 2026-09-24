// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon configuration, `$XDG_CONFIG_HOME/dragomand/config.toml`, read
//! once at startup. Everything has a sensible default; the file may be
//! absent. Defaults follow the
//! measurements in docs/benchmarks.md.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Rough memory budget for loaded models, in MiB. One active
    /// base-memory pair costs ~260 MiB (docs/benchmarks.md); 512 fits two.
    pub memory_budget_mb: u64,
    /// How many recently used routes stay warm when idle.
    pub keep_warm: usize,
    /// How long an unused route stays warm, in seconds.
    pub keep_warm_seconds: u64,
    /// The daemon exits this long after the last activity once nothing is
    /// loaded any more.
    pub idle_exit_seconds: u64,
    /// Master network switch: false disables downloads and update checks
    /// entirely (dev.l10n_bg.dragomand.Error.NetworkDisabled).
    pub network: bool,
    /// Opt into pre-release (nightly-gated) models.
    pub allow_prerelease: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            memory_budget_mb: 512,
            keep_warm: 2,
            keep_warm_seconds: 600,
            idle_exit_seconds: 60,
            network: true,
            allow_prerelease: false,
        }
    }
}

impl Config {
    pub fn keep_warm_window(&self) -> Duration {
        Duration::from_secs(self.keep_warm_seconds)
    }

    pub fn idle_exit(&self) -> Duration {
        Duration::from_secs(self.idle_exit_seconds)
    }

    fn path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        config_home.join("dragomand/config.toml")
    }

    /// Loads the config file, or the defaults when it does not exist. A
    /// malformed file is an error: silently ignoring it would mask typos.
    pub fn load() -> Result<Self, String> {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn defaults_are_sane() {
        let config = Config::default();
        assert!(config.memory_budget_mb >= 256);
        assert!(config.keep_warm >= 1);
        assert!(config.network);
    }

    #[test]
    fn parses_partial_files_and_rejects_unknown_keys() {
        let config: Config = toml::from_str("memory_budget_mb = 256\nnetwork = false\n").unwrap();
        assert_eq!(config.memory_budget_mb, 256);
        assert!(!config.network);
        assert_eq!(config.keep_warm, Config::default().keep_warm);

        assert!(toml::from_str::<Config>("memory_budget = 1\n").is_err());
    }
}
