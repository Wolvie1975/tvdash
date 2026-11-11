//! Header widgets, each a tokio poller emitting `AppEvent`s on its own interval.

pub mod calendar; // M5, every 5 min
pub mod homelab; // M5
pub mod weather; // M5, every 10 min
