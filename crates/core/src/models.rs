//! Data the UI renders. Media models are source-agnostic; `source` drives the badge.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    Plex,
    Jellyfin,
}

impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Plex => "Plex",
            Self::Jellyfin => "Jellyfin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Movie,
    Series,
    Episode,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

/// One playable (or browsable) item from a media server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub source: SourceKind,
    /// Server-side ID (Plex `ratingKey`, Jellyfin `Id`).
    pub id: String,
    pub kind: MediaKind,
    pub title: String,
    /// e.g. series name for an episode.
    pub subtitle: Option<String>,
    /// e.g. "S2 · E5".
    pub episode_label: Option<String>,
    pub duration_ms: Option<u64>,
    pub resume_ms: Option<u64>,
    /// Server-relative poster path / tag; resolved by the source.
    pub poster: Option<String>,
}

impl MediaItem {
    /// Watched fraction 0.0–1.0 when both resume point and duration are known.
    pub fn progress(&self) -> Option<f32> {
        match (self.resume_ms, self.duration_ms) {
            (Some(pos), Some(dur)) if dur > 0 => Some((pos as f32 / dur as f32).clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// An active playback session on one of the media servers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub source: SourceKind,
    pub item: MediaItem,
    /// Device/player name reported by the server.
    pub device: String,
    pub state: PlayState,
    pub position_ms: u64,
}

/// Connection state of an account, shown in Settings › Accounts and on cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    NotSetUp,
    Connected,
    /// Token rejected; the UI shows a Reconnect action.
    Expired,
    Unreachable(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    pub temperature: f64,
    pub condition: String,
    pub high: Option<f64>,
    pub low: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub title: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub all_day: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomelabStatus {
    pub monitors_up: u32,
    pub monitors_total: u32,
    pub containers_running: u32,
    pub containers_total: u32,
}
