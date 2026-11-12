//! Messages between background tasks and the UI thread.
//!
//! ```text
//! pollers (tokio tasks) ──► AppEvent channel ──► UI thread (invoke_from_event_loop)
//! UI callbacks          ──► Command channel  ──► player / media tasks
//! ```

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::models::{
    AccountStatus, Geometry, LibraryData, MediaItem, PlayState, RgbaImage, Session, SignInState, SourceKind, WatchHit,
    Weather,
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
    Plex,
    Tmdb,
    Weather,
    Keyring,
    Player,
    Browser,
}

impl Service {
    pub const ALL: [Service; 6] = [Self::Plex, Self::Tmdb, Self::Weather, Self::Keyring, Self::Player, Self::Browser];

    pub fn label(self) -> &'static str {
        match self {
            Self::Plex => "Plex",
            Self::Tmdb => "TMDB",
            Self::Weather => "Weather (Open-Meteo)",
            Self::Keyring => "GNOME Keyring",
            Self::Player => "mpv",
            Self::Browser => "Streaming browser",
        }
    }
}

impl From<crate::models::SourceKind> for Service {
    fn from(k: crate::models::SourceKind) -> Self {
        match k {
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
    /// The whole library (Library shelves). Empty after sign-out.
    Library(SourceKind, LibraryData),
    RecentlyAdded(SourceKind, Vec<MediaItem>),
    /// Everything the server knows about an item (summary, genres, rating) for
    /// the On the TV box: what's playing, or the top Continue Watching item.
    Details(MediaItem),
    /// Poster for the item with this source + id.
    Poster {
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
    /// Server address in use, for Settings › Accounts (`None` after sign-out).
    Server(SourceKind, Option<String>),
    /// Site name → last opened, for the setup and Accounts screens.
    Streaming(BTreeMap<String, DateTime<Utc>>),
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
}

/// Requests from the UI, handled by background tasks.
#[derive(Debug, Clone)]
pub enum Command {
    Play(MediaItem),
    /// Play a show's next episode (in progress, else the first unwatched).
    PlayShow(MediaItem),
    Pause,
    Resume,
    /// Relative seek in seconds (negative = back).
    Seek(i64),
    Stop,
    /// Open app `index` (`config.streaming[index]`, same order as `APPS`) on the TV.
    OpenSite(usize),
    /// Open an address on the TV in the streaming profile (TMDB's page for a title).
    OpenUrl {
        label: String,
        url: String,
    },
    /// Search TMDB and rank by availability.
    WhereToWatch(String),
    /// Open a title on one of your sites (its search page, else start page).
    OpenOnService {
        site: usize,
        title: String,
    },
    /// Check and store the TMDB read access token; empty removes it.
    SetTmdbToken(String),
    CloseBrowser,
    /// Start the interactive sign-in for a media server.
    SignIn(SourceKind),
    CancelSignIn(SourceKind),
    /// Re-fetch rows now instead of waiting for the next poll.
    Refresh,
    /// Re-read the library (the Library page opened); skipped if it's a minute old or less.
    RefreshLibrary,
    /// Fetch posters for these items (shelves, the Browse grid). Already-fetched ones are skipped.
    Posters(Vec<MediaItem>),
    /// Where the TV output is, once the windows know (used to place the browser).
    TvGeometry(Geometry),
    /// The TV output's real name, as picked by the window placement (mpv goes full
    /// screen there, even when `[display] tv_output` doesn't match any output).
    TvOutput(String),
    /// Forget a media server's sign-in (keyring token + config).
    SignOut(SourceKind),
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
    /// Two screens or TV only (saved; used on the next start).
    SetLayout(crate::config::Layout),
    /// Remember the laptop theme picked in the header.
    SetTheme(crate::config::ThemeMode),
    /// Settings › Display: temperature units, clock format, seconds.
    SetDisplay {
        units: crate::config::Units,
        clock: crate::config::ClockFormat,
        seconds: bool,
    },
}
