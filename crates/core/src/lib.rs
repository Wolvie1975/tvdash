//! Shared foundation for tvdash: configuration, keyring-backed secrets,
//! the data models the UI renders, and the event/command types that flow
//! between background tasks and the UI thread.

pub mod config;
pub mod events;
pub mod layout;
pub mod models;
pub mod secrets;
pub mod state;

pub use config::{Config, LoadedConfig};
pub use events::{AppEvent, Command};
pub use layout::{Decision, Layout, LayoutSetting};
pub use secrets::SecretKey;

/// Application name, used for the config dir, keyring service and HTTP headers.
pub const APP_NAME: &str = "tvdash";
/// Application version, sent to Plex/Jellyfin as the client version.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
