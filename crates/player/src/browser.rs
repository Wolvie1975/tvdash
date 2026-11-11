//! Streaming sites in a dedicated, full-screen browser profile — milestone M4.
//!
//! ```text
//! google-chrome --user-data-dir=$HOME/.local/share/tvdash/browser \
//!   --app=<url> --start-fullscreen --no-first-run
//! ```
//! The child process is tracked so Stop/Close kills it and the TV returns to idle.
//! Note: Fedora's `chromium` lacks Widevine; use Google Chrome (or Firefox with DRM).
