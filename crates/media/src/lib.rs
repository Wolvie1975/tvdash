//! The Plex client behind the [`MediaSource`] trait.
//!
//! The UI only ever sees [`MediaItem`] / [`Session`]; `source` drives the badge.

pub mod plex;

use thiserror::Error;
pub use tvdash_core::models::{MediaItem, PlayState, Session, SourceKind};

#[derive(Debug, Error)]
pub enum MediaError {
    /// Token missing or rejected: the UI shows a Reconnect state.
    #[error("not signed in or sign-in expired")]
    Unauthorized,
    #[error("server not configured")]
    NotConfigured,
    #[error("not found on the server")]
    NotFound,
    #[error("server unreachable: {0}")]
    Unreachable(String),
    #[error("{0}")]
    Protocol(String),
    #[error("not implemented yet")]
    NotImplemented,
}

impl MediaError {
    /// Map an HTTP status to an error (only called for non-success statuses).
    pub fn from_status(status: reqwest::StatusCode, context: &str) -> Self {
        match status.as_u16() {
            401 | 403 => Self::Unauthorized,
            404 => Self::NotFound,
            _ => Self::Protocol(format!("{context}: HTTP {status}")),
        }
    }
}

impl From<reqwest::Error> for MediaError {
    fn from(e: reqwest::Error) -> Self {
        if let Some(status) = e.status() {
            Self::from_status(status, "request failed")
        } else if e.is_decode() {
            Self::Protocol(format!("unexpected response: {e}"))
        } else {
            Self::Unreachable(e.to_string())
        }
    }
}

pub type Result<T> = std::result::Result<T, MediaError>;

#[async_trait::async_trait]
pub trait MediaSource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn now_playing(&self) -> Result<Vec<Session>>;
    async fn continue_watching(&self) -> Result<Vec<MediaItem>>;
    async fn recently_added(&self, limit: u32) -> Result<Vec<MediaItem>>;
    async fn search(&self, query: &str) -> Result<Vec<MediaItem>>;
    async fn poster(&self, item: &MediaItem, max_h: u32) -> Result<Vec<u8>>;
    async fn stream_url(&self, item: &MediaItem) -> Result<String>;
    async fn report_progress(&self, item: &MediaItem, pos_ms: u64, state: PlayState) -> Result<()>;
    /// Every movie and show with its TMDB ID (for Where to watch's library matches).
    async fn tmdb_library(&self) -> Result<Vec<(u32, MediaItem)>>;
}

/// Shared HTTP client settings for media servers.
pub(crate) fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("tvdash/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| MediaError::Protocol(format!("building HTTP client: {e}")))
}

