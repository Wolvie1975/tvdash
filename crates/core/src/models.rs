//! Data the UI renders. Media models are source-agnostic; `source` drives the badge.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    #[default]
    Plex,
}

impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Plex => "Plex",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Movie,
    Series,
    Episode,
    #[default]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayState {
    Playing,
    Paused,
    Stopped,
}

/// One playable (or browsable) item from a media server.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub source: SourceKind,
    /// Server-side ID (Plex `ratingKey`).
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
    /// Season and episode numbers (episodes).
    #[serde(default)]
    pub season: Option<u32>,
    #[serde(default)]
    pub episode: Option<u32>,
    /// The show an episode belongs to (Plex `grandparentRatingKey`).
    #[serde(default)]
    pub show_id: Option<String>,
    #[serde(default)]
    pub year: Option<u32>,
    /// "TV-14", "PG-13".
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    /// Description from the server.
    #[serde(default)]
    pub summary: Option<String>,
    /// Times played all the way through (Plex `viewCount`).
    #[serde(default)]
    pub view_count: u32,
    /// Shows and seasons: episodes in total and watched.
    #[serde(default)]
    pub leaf_count: Option<u32>,
    #[serde(default)]
    pub viewed_leaf_count: Option<u32>,
    /// When it was added to the library (Unix seconds).
    #[serde(default)]
    pub added_at: Option<i64>,
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

/// Position and size of an output, in desktop (logical) pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Decoded RGBA8 image (posters). Plain bytes so it can cross threads; the UI
/// turns it into a `slint::Image`.
#[derive(Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RgbaImage({}x{})", self.width, self.height)
    }
}

/// Progress of an interactive sign-in (Plex PIN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInState {
    /// Contacting the server for a code.
    Starting,
    /// Show this code to the user and wait.
    Code(String),
    Done,
    Failed(String),
    Cancelled,
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
    /// WMO weather code (Open-Meteo `weather_code`).
    pub code: u8,
    pub condition: String,
    pub high: Option<f64>,
    pub low: Option<f64>,
}

/// A place found by name (Settings › Display › Location).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// "Chicago, Illinois, United States"
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
    /// IANA zone, e.g. "America/Chicago" (may be empty).
    pub timezone: String,
}

/// Where to watch groups, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WatchTier {
    /// In Plex.
    Library,
    /// A flat-rate, free or ad-supported offer on one of your streaming sites.
    Included,
    /// Offers exist, but only rent/buy or on services you don't have.
    CostsExtra,
    /// No US offers.
    NotStreaming,
    /// Availability not fetched yet.
    Checking,
}

/// One Where to watch result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchHit {
    pub tmdb_id: u32,
    /// TV show (else movie). TMDB IDs are per kind.
    pub is_tv: bool,
    pub title: String,
    pub year: Option<i32>,
    pub overview: String,
    pub tier: WatchTier,
    /// Your sites that include it: (config index, site name).
    pub services: Vec<(usize, String)>,
    /// Other offers, e.g. "Rent · Apple TV", "Max (not one of yours)".
    pub elsewhere: Vec<String>,
    /// The matching Plex item.
    pub library: Option<MediaItem>,
    /// TMDB's watch page for the title.
    pub link: Option<String>,
}
