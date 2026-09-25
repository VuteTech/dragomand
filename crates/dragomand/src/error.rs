// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The D-Bus error vocabulary, `dev.l10n_bg.dragomand.Error.*`.

/// Errors returned by the `Translator1` interface.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "dev.l10n_bg.dragomand.Error")]
pub enum Error {
    /// Fallback for transport-level zbus errors.
    #[zbus(error)]
    ZBus(zbus::Error),
    /// Malformed input: bad language tag, bad options, bad token.
    InvalidArgument(String),
    /// Neither a direct model nor a pivot route can ever serve this pair.
    UnsupportedPair(String),
    /// The pair (or a pivot leg) is not installed locally.
    NotInstalled(String),
    /// A per-client limit was exceeded.
    LimitExceeded(String),
    /// The operation needs the network, which is disabled by configuration.
    NetworkDisabled(String),
    /// The translation engine failed.
    EngineFailure(String),
}

impl From<dragoman_engine::Error> for Error {
    fn from(error: dragoman_engine::Error) -> Self {
        Error::EngineFailure(error.to_string())
    }
}

impl From<dragoman_models::remote_settings::ProviderError> for Error {
    fn from(error: dragoman_models::remote_settings::ProviderError) -> Self {
        use dragoman_models::remote_settings::ProviderError;
        match &error {
            ProviderError::NoSuchPair(pair) => Error::UnsupportedPair(pair.clone()),
            _ => Error::EngineFailure(error.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
