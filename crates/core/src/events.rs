//! Messages between background tasks and the UI thread.
//!
//! ```text
//! pollers (tokio tasks) ──► AppEvent channel ──► UI thread (invoke_from_event_loop)
//! UI callbacks          ──► Command channel  ──► player / media tasks
//! ```

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::models::{
    AccountStatus, Geometry, MediaItem, PlayState, RgbaImage, Session, SignInState, SourceKind, Weather,
};

/// What tvdash itself is playing on the TV through mpv.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerStatus {
    pub item: MediaItem,
    pub state: PlayState,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
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
    /// Show the first-run setup (nothing is connected yet).
    ShowSetup,
    /// The streaming sites changed (all of them, hidden ones too, in tile order).
    Sites(Vec<crate::config::StreamingSite>),
}

/// Requests from the UI, handled by background tasks.
#[derive(Debug, Clone)]
pub enum Command {
    Play(MediaItem),
    Pause,
    Resume,
    /// Relative seek in seconds (negative = back).
    Seek(i64),
    Stop,
    /// Open `config.streaming[index]` on the TV.
    OpenSite(usize),
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
    /// Open an address on the TV in the streaming profile (the editor's Test on TV).
    TestSite(String),
    CloseBrowser,
    Search(String),
    /// Start the interactive sign-in for a media server.
    SignIn(SourceKind),
    CancelSignIn(SourceKind),
    /// Re-fetch rows now instead of waiting for the next poll.
    Refresh,
    /// Where the TV output is, once the windows know (used to place the browser).
    TvGeometry(Geometry),
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
    /// Remember the laptop theme picked in the header.
    SetTheme(crate::config::ThemeMode),
}
