//! Messages between background tasks and the UI thread.
//!
//! ```text
//! pollers (tokio tasks) ──► AppEvent channel ──► UI thread (invoke_from_event_loop)
//! UI callbacks          ──► Command channel  ──► player / media tasks
//! ```

use crate::models::{
    AccountStatus, CalendarEvent, HomelabStatus, MediaItem, PlayState, Session, SourceKind, Weather,
};

/// Finished data for the UI. Produced off the UI thread.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Result of the startup keyring self-test (M1).
    KeyringStatus(Result<(), String>),
    Account(SourceKind, AccountStatus),
    NowPlaying(Vec<Session>),
    ContinueWatching(SourceKind, Vec<MediaItem>),
    RecentlyAdded(SourceKind, Vec<MediaItem>),
    Weather(Weather),
    NextEvent(Option<CalendarEvent>),
    Homelab(HomelabStatus),
    /// Local mpv state: what's on the TV right now.
    Player {
        item: Option<MediaItem>,
        state: PlayState,
        position_ms: u64,
    },
    /// The streaming browser opened (`Some(site name)`) or closed (`None`).
    Browser(Option<String>),
    Toast(String),
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
}
