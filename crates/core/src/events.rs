//! Messages between background tasks and the UI thread.
//!
//! ```text
//! pollers (tokio tasks) ──► AppEvent channel ──► UI thread (invoke_from_event_loop)
//! UI callbacks          ──► Command channel  ──► player / media tasks
//! ```

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{
    AccountStatus, Geometry, MediaItem, PlayState, RgbaImage, Session, SignInState, SourceKind, WatchHit, Weather,
};

/// What tvdash itself is playing on the TV through mpv.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerStatus {
    pub item: MediaItem,
    pub state: PlayState,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
}

/// Things Settings › Diagnostics reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Service {
    Jellyfin,
    Plex,
    Tmdb,
    Weather,
    Keyring,
    Player,
    Browser,
    Events,
}

impl Service {
    pub const ALL: [Service; 8] = [
        Self::Jellyfin,
        Self::Plex,
        Self::Tmdb,
        Self::Weather,
        Self::Keyring,
        Self::Player,
        Self::Browser,
        Self::Events,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Jellyfin => "Jellyfin",
            Self::Plex => "Plex",
            Self::Tmdb => "TMDB",
            Self::Weather => "Weather (Open-Meteo)",
            Self::Keyring => "GNOME Keyring",
            Self::Player => "mpv",
            Self::Browser => "Streaming browser",
            Self::Events => "Sports (SQL Server)",
        }
    }
}

impl From<crate::models::SourceKind> for Service {
    fn from(k: crate::models::SourceKind) -> Self {
        match k {
            crate::models::SourceKind::Jellyfin => Self::Jellyfin,
            crate::models::SourceKind::Plex => Self::Plex,
        }
    }
}

/// Finished data for the UI. Produced off the UI thread.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Result of the startup keyring self-test (M1).
    KeyringStatus(Result<(), String>),
    Account(SourceKind, AccountStatus),
    SignIn(SourceKind, SignInState),
    NowPlaying(Vec<Session>),
    ContinueWatching(SourceKind, Vec<MediaItem>),
    RecentlyAdded(SourceKind, Vec<MediaItem>),
    /// Poster for the item with this source + id.
    Poster {
        source: SourceKind,
        item_id: String,
        image: RgbaImage,
    },
    /// 16:9 backdrop (or blurred poster fallback) for Library Option C.
    Backdrop {
        source: SourceKind,
        item_id: String,
        image: RgbaImage,
    },
    /// `None`: no location configured.
    Weather(Option<Weather>),
    /// Local playback changed; `None` means the TV is back to idle.
    Player(Option<PlayerStatus>),
    /// The streaming browser opened (`Some(site name)`) or closed (`None`).
    Browser(Option<String>),
    Toast(String),
    /// A toast that stays up while the user does something (e.g. signs in on the TV).
    LongToast(String),
    /// Library search finished (all connected sources).
    SearchResults {
        query: String,
        items: Vec<MediaItem>,
    },
    /// Server address in use, for Settings › Accounts (`None` after sign-out).
    Server(SourceKind, Option<String>),
    /// Site name → last opened, for the setup and Accounts screens.
    Streaming(BTreeMap<String, DateTime<Utc>>),
    /// Last 3 recently opened streaming services (TV-only v2).
    RecentServices(Vec<crate::config::RecentService>),
    /// Show the first-run setup (nothing is connected yet).
    ShowSetup,
    /// Places matching a search (Settings › Display › Location).
    Places {
        query: String,
        result: Result<Vec<crate::models::Place>, String>,
    },
    /// The weather location changed (name, and the zone the clocks should use).
    Location {
        name: String,
        timezone: String,
    },
    /// A service was polled (Settings › Diagnostics): `Ok(note)` or `Err(reason)`.
    Health {
        service: Service,
        result: Result<String, String>,
    },
    /// The streaming sites changed (all of them, hidden ones too, in tile order).
    Sites(Vec<crate::config::StreamingSite>),
    /// TMDB token state (Where to watch).
    Tmdb(AccountStatus),
    /// Where to watch results so far (re-sent as availability arrives).
    WatchResults {
        query: String,
        hits: Vec<WatchHit>,
        /// Why the search failed, if it did.
        error: Option<String>,
    },
    /// TMDB poster for a Where to watch result.
    WatchPoster {
        tmdb_id: u32,
        is_tv: bool,
        image: RgbaImage,
    },
    /// Service logos for the dashboard tiles: site name → logo.
    SiteLogos(Vec<(String, RgbaImage)>),
    /// TMDB providers whose name matches a site name (the editor's Find button).
    ProviderMatches {
        name: String,
        ids: Vec<u32>,
        names: Vec<String>,
    },
    /// Server summary and activity for the Settings › Server page.
    ServerSummary(crate::models::ServerSummary),
    ServerActivities(Vec<crate::models::ServerActivity>),
    ServerError(String),
    ArtworkChoices {
        item_id: String,
        backdrop: bool,
        result: Result<Vec<crate::models::ArtworkChoice>, String>,
    },
    MetadataRefreshFinished(String),
    /// Whether a TV display output is currently active/connected.
    TvConnected(bool),
    /// Display configuration changed (after debounce); UI thread should re-check monitors.
    DisplaysChanged,
    /// Desktop portal `org.freedesktop.appearance color-scheme` (`true` = dark).
    SystemColorScheme(bool),
    /// Tautulli account status (URL + whether an API key is stored).
    Tautulli {
        configured: bool,
        url: String,
    },
    /// Library C smart shelves payload.
    LibraryShelves(crate::models::LibraryShelves),
    /// Show page data.
    ShowDetail(Result<crate::models::ShowDetail, String>),
    /// Episodes for one season on the Show page.
    SeasonEpisodes {
        season_key: String,
        result: Result<Vec<crate::models::EpisodeInfo>, String>,
    },
    /// Prompt to continue to the next episode after an episode finishes.
    NextEpisodePrompt(Option<MediaItem>),
    /// Collection page data.
    CollectionDetail(Result<crate::models::CollectionDetail, String>),
    /// Sports / Competitions page data.
    SportsEvents(Result<crate::models::SportsEventsPage, String>),
    /// Downloaded team logo for the Sports tab.
    SportsLogo {
        url: String,
        image: RgbaImage,
    },
    /// Whether the SQL Server password is stored in the keyring.
    EventsPasswordConfigured(bool),
    /// Sports → Scoreboard page data.
    Scoreboard(Result<crate::models::ScoreboardPage, String>),
    /// Current sleep timer state for UI and status endpoints.
    Sleep(SleepSnapshot),
}

/// Snapshot of the sleep timer sent to the UI on change / each tick.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepSnapshot {
    pub active: bool,
    /// `"off" | "counting" | "warn" | "end_of_item"`
    pub phase: String,
    /// Short label for chip/hints: `"42 min"`, `"1 h 05 min"`, `"End of episode"`, `"Paused · on a call"`, `"0:58"`, or `"Off"`.
    pub label: String,
    /// Formatted end time, e.g. `"11:42 PM"` or `""`.
    pub ends_at: String,
    /// Seconds left (for warning countdown & `/sleep` API).
    pub left: i64,
    /// Total warning duration in seconds (for countdown bar progress).
    pub warn_total: i64,
    /// True when End of episode/movie was triggered (episode finished).
    pub episode_finished: bool,
    /// True when a call (`hold_sleep_timer`) is holding the timer.
    pub held: bool,
    /// Last-used preset in minutes (or config default) for initial picker focus.
    pub default_minutes: i64,
}

/// Requests from the UI, handled by background tasks.
#[derive(Debug, Clone)]
pub enum Command {
    Play(MediaItem),
    Pause,
    Resume,
    /// Relative seek in seconds (negative = back).
    Seek(i64),
    /// Absolute seek in seconds (e.g. `0` to start from the beginning).
    SeekTo(u64),
    Stop,
    /// Cycle audio track, cycle subtitle track, or adjust volume (relative %).
    CycleAudio,
    CycleSub,
    VolumeDelta(i64),
    PreviousEpisode,
    NextEpisode,
    /// Open `config.streaming[index]` on the TV.
    OpenSite(usize),
    /// Clear the last-3 recent streaming services list.
    ClearRecentServices,
    /// Add (`index: None`) or replace a streaming site. Already validated.
    SaveSite {
        index: Option<usize>,
        site: crate::config::StreamingSite,
    },
    /// Remove a site; optionally open the browser's cookie settings for its domain.
    RemoveSite {
        index: usize,
        clear_cookies: bool,
    },
    /// Move a site to another position (tile order).
    MoveSite {
        from: usize,
        to: usize,
    },
    SetSiteHidden(usize, bool),
    /// Open an address on the TV in the streaming profile (Test on TV, TMDB page).
    OpenUrl {
        label: String,
        url: String,
    },
    /// Fetch Sports / Competitions from SQL Server.
    LoadSportsEvents {
        schedule: String,
        page: u32,
    },
    /// Save or clear the Sports SQL Server password in the keyring.
    SetEventsPassword(Option<String>),
    /// Fetch Sports → Scoreboard from SQL Server.
    LoadScoreboard {
        day: String,
        show: String,
        final_only: bool,
    },
    /// Search TMDB and rank by availability.
    WhereToWatch(String),
    /// Scan the whole Plex library or one section from the Server page.
    ServerScan {
        index: Option<usize>,
    },
    ServerRefresh {
        refresh_libraries: bool,
    },
    ServerRefreshActivities,
    RefreshSessions,
    PrefetchBackdrop(MediaItem),
    ServerCancelActivity(String),
    ServerTerminate {
        session_id: String,
        reason: String,
    },
    RefreshMetadata(MediaItem),
    FetchArtwork {
        item: MediaItem,
        backdrop: bool,
    },
    SetArtwork {
        item: MediaItem,
        key: String,
        backdrop: bool,
    },
    /// Open a title on one of your sites (its search page, else start page).
    OpenOnService {
        site: usize,
        title: String,
    },
    /// Check and store the TMDB read access token; empty removes it.
    SetTmdbToken(String),
    /// Look up TMDB provider IDs for a site name.
    FindProviders(String),
    CloseBrowser,
    Search(String),
    /// Start the interactive sign-in for a media server.
    SignIn(SourceKind),
    CancelSignIn(SourceKind),
    /// Re-fetch rows now instead of waiting for the next poll.
    Refresh,
    /// Where the TV output is, once the windows know (used to place the browser).
    TvGeometry(Geometry),
    /// Detected connector name of the TV output (e.g. `HDMI-1`, `DP-1`).
    TvOutput(String),
    /// Forget a media server's sign-in (keyring token + config).
    SignOut(SourceKind),
    /// Use this Jellyfin server address (setup lets you change it before signing in).
    JellyfinServer(String),
    /// Username/password fallback for servers without Quick Connect.
    JellyfinPassword {
        username: String,
        password: String,
    },
    /// Close the browser, delete its profile (all streaming sign-ins) and history.
    ClearBrowserProfile,
    SetupFinished,
    /// Look up places by name for the weather location.
    FindPlace(String),
    /// Use this place for the weather (and its time zone for the clocks).
    SetLocation(crate::models::Place),
    /// Back to Home: close the browser, and stop mpv when it has its own window
    /// (progress is saved). Video playing inside the tvdash window keeps playing.
    Home,
    /// Show the playback overlay (title, progress, end time) for a few seconds.
    Overlay,
    /// The window couldn't draw the video (in-window playback); play it in mpv's own window.
    VideoFailed(String),
    /// Runtime layout change (does not overwrite `[display] layout = "auto"`).
    SetLayout(crate::config::Layout),
    /// User changed Settings › Display › Layout (`auto` | `dual` | `tv`).
    SetLayoutSetting(crate::layout::LayoutSetting),
    /// User changed Settings › Display › Dashboard UI (`classic` | `blades`).
    SetDashboardUi(crate::config::DashboardUi),
    /// Remember the laptop theme picked in the header.
    SetTheme(crate::config::ThemeMode),
    /// Settings › Display: temperature units, clock format, seconds.
    SetDisplay {
        units: crate::config::Units,
        clock: crate::config::ClockFormat,
        seconds: bool,
    },
    /// Save Tautulli URL + API key.
    SetTautulli {
        url: String,
        api_key: String,
    },
    /// Remove Tautulli URL + API key.
    ClearTautulli,
    /// Load Library C smart shelves.
    LoadLibraryShelves {
        is_tv: bool,
        sort: crate::models::LibrarySort,
        popular_days: u32,
    },
    /// Open Show page for `rating_key`.
    OpenShow(String),
    /// Load episodes for `season_key`.
    LoadSeasonEpisodes(String),
    /// Open Collection page for `rating_key`.
    OpenCollection(String),
    /// Play a specific Plex item by `rating_key` (optionally resuming).
    PlayRatingKey {
        rating_key: String,
        resume: bool,
    },
    /// Set sleep timer in minutes (1–480).
    SleepSetMinutes(i64),
    /// Set sleep timer to end of the current Plex item.
    SleepSetEndOfItem,
    /// Turn off the sleep timer.
    SleepOff,
    /// Extend sleep timer ("Keep watching").
    SleepExtend,
    /// Sleep immediately ("Sleep now").
    SleepNow,
    /// Play next episode and turn off the sleep timer (when End of episode finished).
    SleepPlayNext,
    /// Change Sleep warning duration ("30s" | "60s" | "2m") in Settings › Display.
    SetSleepWarnBefore(String),
    /// Fetch posters for visible Browse-all items that aren't cached yet.
    FetchPosters(Vec<MediaItem>),
}
