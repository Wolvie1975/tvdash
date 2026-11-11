//! mpv control over its JSON IPC socket — milestone M2.
//!
//! Launch:
//! ```text
//! mpv --fullscreen --input-ipc-server=$XDG_RUNTIME_DIR/tvdash-mpv.sock \
//!     --fs-screen-name=HDMI-A-1 --force-window=yes --keep-open=no <url>
//! ```
//! Then send one JSON object per line (`set_property pause`, `seek N relative`,
//! `quit`, `observe_property time-pos/pause`) and forward observed events to
//! `MediaSource::report_progress` every ~10 s and on pause/stop.

use std::path::PathBuf;

/// Path of the IPC socket: `$XDG_RUNTIME_DIR/tvdash-mpv.sock`.
pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("tvdash-mpv.sock")
}
