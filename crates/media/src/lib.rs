//! Plex and Jellyfin clients behind one [`MediaSource`] trait.
//!
//! The UI only ever sees [`MediaItem`] / [`Session`]; `source` drives the badge.

pub mod jellyfin; // M2
pub mod plex; // M3

use thiserror::Error;
pub use tvdash_core::models::{MediaItem, PlayState, Session, SourceKind};

#[derive(Debug, Error)]
pub enum MediaError {
    /// Token missing or rejected: the UI shows a Reconnect state.
    #[error("not signed in or sign-in expired")]
    Unauthorized,
    #[error("server not configured")]
    NotConfigured,
    #[error("server unreachable: {0}")]
    Unreachable(String),
    #[error("unexpected response: {0}")]
    Protocol(String),
    #[error("not implemented yet")]
    NotImplemented,
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
}

/// Jellyfin positions are in ticks (1 tick = 100 ns).
pub const fn ms_to_ticks(ms: u64) -> u64 {
    ms * 10_000
}

pub const fn ticks_to_ms(ticks: u64) -> u64 {
    ticks / 10_000
}

#[cfg(test)]
mod tests {
    #[test]
    fn tick_conversion() {
        assert_eq!(super::ms_to_ticks(1_500), 15_000_000);
        assert_eq!(super::ticks_to_ms(15_000_000), 1_500);
    }
}
