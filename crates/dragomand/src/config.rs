// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon configuration, `$XDG_CONFIG_HOME/dragomand/config.toml`, read
//! at startup and changed at run time through `SetConfig`, which writes
//! the file back (keeping its comments and layout). Everything has a
//! sensible default; the file may be absent. Defaults follow the
//! measurements in docs/benchmarks.md.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use zbus::zvariant::{OwnedValue, Value};

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

    pub fn path() -> PathBuf {
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

/// A settable key, its D-Bus type, and the accepted range for numbers.
struct Key {
    name: &'static str,
    kind: Kind,
}

enum Kind {
    Number { min: u64, max: u64 },
    Bool,
}

const KEYS: &[Key] = &[
    Key {
        name: "memory_budget_mb",
        kind: Kind::Number {
            min: 64,
            max: 1 << 20,
        },
    },
    Key {
        name: "keep_warm",
        kind: Kind::Number { min: 0, max: 16 },
    },
    Key {
        name: "keep_warm_seconds",
        kind: Kind::Number {
            min: 0,
            max: 86_400,
        },
    },
    Key {
        name: "idle_exit_seconds",
        kind: Kind::Number {
            min: 5,
            max: 86_400,
        },
    },
    Key {
        name: "network",
        kind: Kind::Bool,
    },
    Key {
        name: "allow_prerelease",
        kind: Kind::Bool,
    },
];

/// A value checked against its key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    Number(u64),
    Bool(bool),
}

/// Any unsigned or non-negative integer D-Bus type.
fn as_number(value: &OwnedValue) -> Option<u64> {
    match &**value {
        Value::U8(n) => Some(u64::from(*n)),
        Value::U16(n) => Some(u64::from(*n)),
        Value::U32(n) => Some(u64::from(*n)),
        Value::U64(n) => Some(*n),
        Value::I16(n) => u64::try_from(*n).ok(),
        Value::I32(n) => u64::try_from(*n).ok(),
        Value::I64(n) => u64::try_from(*n).ok(),
        _ => None,
    }
}

impl Config {
    /// Every key with its current value: numbers as `t` (`u` for
    /// keep_warm), switches as `b`.
    pub fn to_values(&self) -> HashMap<String, OwnedValue> {
        let owned = |v: Value<'static>| OwnedValue::try_from(v).expect("no fds in config");
        HashMap::from([
            (
                "memory_budget_mb".to_owned(),
                owned(self.memory_budget_mb.into()),
            ),
            (
                "keep_warm".to_owned(),
                owned((self.keep_warm as u32).into()),
            ),
            (
                "keep_warm_seconds".to_owned(),
                owned(self.keep_warm_seconds.into()),
            ),
            (
                "idle_exit_seconds".to_owned(),
                owned(self.idle_exit_seconds.into()),
            ),
            ("network".to_owned(), owned(self.network.into())),
            (
                "allow_prerelease".to_owned(),
                owned(self.allow_prerelease.into()),
            ),
        ])
    }

    /// Validates every change first, so a bad key or value changes nothing.
    pub fn check(changes: &HashMap<String, OwnedValue>) -> Result<Vec<(String, Setting)>, String> {
        let mut checked = Vec::new();
        for (name, value) in changes {
            let key = KEYS
                .iter()
                .find(|k| k.name == name)
                .ok_or_else(|| format!("unknown setting {name:?}"))?;
            let setting = match key.kind {
                Kind::Bool => Setting::Bool(
                    value
                        .downcast_ref::<bool>()
                        .map_err(|_| format!("{name} must be a boolean"))?,
                ),
                Kind::Number { min, max } => {
                    let n = as_number(value).ok_or_else(|| format!("{name} must be an integer"))?;
                    if !(min..=max).contains(&n) {
                        return Err(format!("{name} must be between {min} and {max}"));
                    }
                    Setting::Number(n)
                }
            };
            checked.push((name.clone(), setting));
        }
        Ok(checked)
    }

    /// Applies changes from [`Config::check`].
    pub fn apply(&mut self, changes: &[(String, Setting)]) {
        for (name, setting) in changes {
            match (name.as_str(), *setting) {
                ("memory_budget_mb", Setting::Number(n)) => self.memory_budget_mb = n,
                ("keep_warm", Setting::Number(n)) => self.keep_warm = n as usize,
                ("keep_warm_seconds", Setting::Number(n)) => self.keep_warm_seconds = n,
                ("idle_exit_seconds", Setting::Number(n)) => self.idle_exit_seconds = n,
                ("network", Setting::Bool(b)) => self.network = b,
                ("allow_prerelease", Setting::Bool(b)) => self.allow_prerelease = b,
                _ => unreachable!("checked by Config::check"),
            }
        }
    }

    /// Writes the changed keys into the file at `path`, keeping everything
    /// else in it (comments, order, formatting) and replacing it atomically.
    pub fn save(path: &Path, changes: &[(String, Setting)]) -> Result<(), String> {
        let existing = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut document: toml_edit::DocumentMut = existing
            .parse()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        for (name, setting) in changes {
            document[name.as_str()] = match *setting {
                Setting::Number(n) => toml_edit::value(i64::try_from(n).unwrap_or(i64::MAX)),
                Setting::Bool(b) => toml_edit::value(b),
            };
        }
        let directory = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
        let temporary = directory.join(format!(".config.toml.{}", std::process::id()));
        std::fs::write(&temporary, document.to_string())
            .and_then(|()| std::fs::rename(&temporary, path))
            .map_err(|e| {
                let _ = std::fs::remove_file(&temporary);
                format!("{}: {e}", path.display())
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zbus::zvariant::{OwnedValue, Value};

    use super::{Config, Setting};

    fn owned(v: Value<'static>) -> OwnedValue {
        OwnedValue::try_from(v).unwrap()
    }

    #[test]
    fn checks_types_and_ranges() {
        let good = HashMap::from([
            ("memory_budget_mb".to_owned(), owned(256u32.into())),
            ("network".to_owned(), owned(false.into())),
        ]);
        let mut checked = Config::check(&good).unwrap();
        checked.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            checked,
            vec![
                ("memory_budget_mb".to_owned(), Setting::Number(256)),
                ("network".to_owned(), Setting::Bool(false)),
            ]
        );
        let mut config = Config::default();
        config.apply(&checked);
        assert_eq!(config.memory_budget_mb, 256);
        assert!(!config.network);

        for bad in [
            ("memory_budget_mb", owned(1u64.into())),
            ("memory_budget_mb", owned((-5i64).into())),
            ("network", owned(1u32.into())),
            ("no_such_key", owned(true.into())),
        ] {
            let changes = HashMap::from([(bad.0.to_owned(), bad.1)]);
            assert!(Config::check(&changes).is_err(), "{:?} accepted", bad.0);
        }
    }

    #[test]
    fn save_keeps_comments_and_other_keys() {
        let dir = std::env::temp_dir().join(format!("dragomand-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "# my budget\nmemory_budget_mb = 512\nkeep_warm = 3\n",
        )
        .unwrap();
        Config::save(
            &path,
            &[("memory_budget_mb".to_owned(), Setting::Number(300))],
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my budget"), "{text}");
        assert!(text.contains("memory_budget_mb = 300"), "{text}");
        assert!(text.contains("keep_warm = 3"), "{text}");
        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.memory_budget_mb, 300);
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
