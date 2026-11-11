//! Messages between background tasks and the UI thread.
//!
//! ```text
//! pollers (tokio tasks) ──► AppEvent channel ──► UI thread (invoke_from_event_loop)
//! UI callbacks          ──► Command channel  ──► player / media tasks
//! ```

use crate::models::{
    AccountStatus, CalendarState, Geometry, HomelabStatus, MediaItem, PlayState, RgbaImage, Session, SignInState,
    SourceKind, Weather,
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
    Calendar(CalendarState),
    Homelab(HomelabStatus),
    /// Local playback changed; `None` means the TV is back to idle.
    Player(Option<PlayerStatus>),
    /// The streaming browser opened (`Some(site name)`) or closed (`None`).
    Browser(Option<String>),
    Toast(String),
    /// A toast that stays up while the user does something (e.g. signs in on the TV).
    LongToast(String),
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
    CloseBrowser,
    Search(String),
    /// Start the interactive sign-in for a media server.
    SignIn(SourceKind),
    CancelSignIn(SourceKind),
    /// Re-fetch rows now instead of waiting for the next poll.
    Refresh,
    /// Where the TV output is, once the windows know (used to place the browser).
    TvGeometry(Geometry),
}
