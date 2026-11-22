//! Secrets in GNOME Keyring via the Secret Service API (service name `tvdash`).
//!
//! All calls are blocking D-Bus round-trips: call them from
//! `tokio::task::spawn_blocking` or a background thread, never the UI thread.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("keyring error for {key}: {source}")]
    Keyring {
        key: &'static str,
        #[source]
        source: keyring::Error,
    },
    #[error("keyring self-test read back a different value")]
    Mismatch,
}

pub type Result<T> = std::result::Result<T, SecretError>;

/// Every secret tvdash stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKey {
    /// Plex auth token from the PIN flow.
    PlexToken,
    /// Jellyfin access token.
    JellyfinToken,
    /// TMDB v4 read access token (Where to watch, service logos).
    TmdbToken,
    /// Tautulli API key (Popular on Republic City).
    TautulliApiKey,
    /// SQL Server read-only password for Sports / Competitions.
    EventsSqlPassword,
}

impl SecretKey {
    pub const ALL: [SecretKey; 5] = [
        Self::PlexToken,
        Self::JellyfinToken,
        Self::TmdbToken,
        Self::TautulliApiKey,
        Self::EventsSqlPassword,
    ];

    /// Inverse of [`Self::as_str`].
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlexToken => "plex_token",
            Self::JellyfinToken => "jellyfin_token",
            Self::TmdbToken => "tmdb_api_key",
            Self::TautulliApiKey => "tautulli_api_key",
            Self::EventsSqlPassword => "events_sql_password",
        }
    }
}

fn entry(key: &'static str) -> Result<keyring::Entry> {
    keyring::Entry::new(crate::APP_NAME, key).map_err(|source| SecretError::Keyring { key, source })
}

fn get_raw(key: &'static str) -> Result<Option<String>> {
    match entry(key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(source) => Err(SecretError::Keyring { key, source }),
    }
}

fn set_raw(key: &'static str, value: &str) -> Result<()> {
    entry(key)?.set_password(value).map_err(|source| SecretError::Keyring { key, source })
}

fn delete_raw(key: &'static str) -> Result<()> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(source) => Err(SecretError::Keyring { key, source }),
    }
}

/// Read a secret. `Ok(None)` means it was never stored (or was deleted).
pub fn get(key: SecretKey) -> Result<Option<String>> {
    get_raw(key.as_str())
}

/// Store or replace a secret.
pub fn set(key: SecretKey, value: &str) -> Result<()> {
    set_raw(key.as_str(), value)
}

/// Remove a secret. Removing a missing secret is not an error.
pub fn delete(key: SecretKey) -> Result<()> {
    delete_raw(key.as_str())
}

/// Write, read back and delete a throwaway entry to prove the keyring works.
pub fn self_test() -> Result<()> {
    const KEY: &str = "selftest";
    let value = format!("tvdash-selftest-{}", uuid::Uuid::new_v4());
    set_raw(KEY, &value)?;
    let back = get_raw(KEY)?;
    delete_raw(KEY)?;
    if back.as_deref() == Some(value.as_str()) { Ok(()) } else { Err(SecretError::Mismatch) }
}

/// Which secrets are currently present (for Settings › Accounts and logging).
/// Never returns the values themselves.
pub fn presence() -> Result<Vec<(SecretKey, bool)>> {
    SecretKey::ALL.into_iter().map(|k| get(k).map(|v| (k, v.is_some()))).collect()
}

/// Entries from features that were removed (calendar, homelab).
const RETIRED: [&str; 3] = ["calendar_ics_url", "portainer_api_key", "uptime_kuma_api_key"];

/// Delete entries left behind by removed features. Missing ones are skipped.
pub fn purge_retired() -> Result<()> {
    RETIRED.into_iter().try_for_each(delete_raw)
}
