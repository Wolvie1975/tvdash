//! Everything that puts pixels on the TV: mpv (local media) and the
//! dedicated full-screen browser (streaming sites).

pub mod browser; // M4
pub mod libmpv; // TV-only layout: video inside the window
pub mod mpv; // M2
