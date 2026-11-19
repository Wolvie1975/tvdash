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
    /// Plot / episode description from the media server.
    #[serde(default)]
    pub summary: Option<String>,
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
    /// Server-side session ID for actions like stopping a Plex stream.
    pub session_id: Option<String>,
    pub transcoding: bool,
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

/// Progress of an interactive sign-in (Jellyfin Quick Connect, Plex PIN).
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

/// Server summary and activity data shown on the Settings › Server page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSummary {
    pub name: String,
    pub streams: u32,
    pub transcodes: u32,
    pub bandwidth_mbps: u32,
    pub cpu_percent: u32,
    pub ram_percent: u32,
    pub status: String,
    pub plex_pass: bool,
    pub cpu_host_path: String,
    pub cpu_process_path: String,
    pub ram_host_path: String,
    pub ram_process_path: String,
    pub bandwidth_lan_path: String,
    pub bandwidth_wan_path: String,
    pub bandwidth_scale: u32,
    pub streams_detail: Vec<ServerStream>,
    pub libraries: Vec<ServerLibrary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerStream {
    pub session_id: Option<String>,
    pub local: bool,
    pub title: String,
    pub device: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerActivity {
    pub uuid: String,
    pub title: String,
    pub subtitle: String,
    pub progress: f32,
    pub cancellable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerLibrary {
    pub title: String,
    pub item_count: u32,
    pub scanned_ago: String,
    pub refreshing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtworkChoice {
    pub key: String,
    pub provider: String,
    pub selected: bool,
    pub image: Option<RgbaImage>,
}

/// Where to watch groups, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WatchTier {
    /// In Plex or Jellyfin.
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

/// Sort order for Library C shelves and Browse all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    #[default]
    RecentlyAdded,
    TitleAsc,
    TitleDesc,
}

impl LibrarySort {
    pub fn parse(s: &str) -> Self {
        match s {
            "title_asc" | "titleSort" => Self::TitleAsc,
            "title_desc" | "titleSort:desc" => Self::TitleDesc,
            _ => Self::RecentlyAdded,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "recent",
            Self::TitleAsc => "title_asc",
            Self::TitleDesc => "title_desc",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "Recently added",
            Self::TitleAsc => "A–Z",
            Self::TitleDesc => "Z–A",
        }
    }

    pub fn plex_sort(self) -> &'static str {
        match self {
            Self::RecentlyAdded => "addedAt:desc",
            Self::TitleAsc => "titleSort",
            Self::TitleDesc => "titleSort:desc",
        }
    }
}

/// A season on a TV Show page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub rating_key: String,
    pub index: u32,
    pub title: String,
    pub leaf_count: u32,
    pub viewed_leaf_count: u32,
    pub thumb: Option<String>,
}

/// An episode row on a TV Show page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeInfo {
    pub rating_key: String,
    pub index: u32,
    pub season_index: u32,
    pub title: String,
    pub summary: Option<String>,
    pub duration_ms: Option<u64>,
    pub view_offset_ms: Option<u64>,
    pub view_count: u32,
    pub thumb: Option<String>,
}

/// Full data for the Show page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowDetail {
    pub item: MediaItem,
    pub seasons: Vec<SeasonInfo>,
    pub up_next_season_key: Option<String>,
    pub up_next_episode: Option<EpisodeInfo>,
}

/// A movie collection card or header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub rating_key: String,
    pub title: String,
    pub child_count: u32,
    pub watched_count: u32,
    pub smart: bool,
    pub thumb: Option<String>,
    pub summary: Option<String>,
    pub min_year: Option<i32>,
    pub max_year: Option<i32>,
    pub total_duration_ms: u64,
}

/// Full data for a Collection page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionDetail {
    pub collection: CollectionInfo,
    pub items: Vec<MediaItem>,
    pub watched_keys: Vec<String>,
    pub up_next_key: Option<String>,
}

/// A genre tile with item count (Library C bottom row on TV).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenreTile {
    pub key: String,
    pub title: String,
    pub count: u32,
}

/// An entry in Popular on Republic City (from Tautulli `get_home_stats`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopularItem {
    pub rank: u32,
    pub rating_key: String,
    pub title: String,
    pub plays: u32,
    pub thumb: Option<String>,
}

/// A "Finish a season" entry (TV big box in Library C).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinishSeasonItem {
    pub item: MediaItem,
    pub season_title: String,
    pub total_episodes: u32,
    pub watched_episodes: u32,
}

/// Data loaded for the Library C smart-shelves view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryShelves {
    pub is_tv: bool,
    pub finish_season: Vec<FinishSeasonItem>,
    pub continue_watching: Vec<MediaItem>,
    pub added_this_week: Vec<MediaItem>,
    pub watch_again: Vec<MediaItem>,
    pub browse_items: Vec<MediaItem>,
    pub popular: Vec<PopularItem>,
    pub genres: Vec<GenreTile>,
    pub collections: Vec<CollectionInfo>,
}
