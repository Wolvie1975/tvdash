//! Cached watched counts for Orbit rings (§1.6).

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchedCount {
    pub total: usize,
    pub watched: usize,
    pub fetched_at: Instant,
}

impl WatchedCount {
    pub fn progress(self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.watched.min(self.total) as f32) / (self.total as f32)
        }
    }
}

#[derive(Debug, Default)]
pub struct OrbitCountCache {
    entries: HashMap<String, WatchedCount>,
}

impl OrbitCountCache {
    pub const TTL: Duration = Duration::from_secs(30 * 60);

    pub fn get(&self, key: &str) -> Option<WatchedCount> {
        self.entries.get(key).copied()
    }

    pub fn is_fresh(&self, key: &str, now: Instant) -> bool {
        self.entries
            .get(key)
            .is_some_and(|e| now.duration_since(e.fetched_at) < Self::TTL)
    }

    pub fn insert(&mut self, key: impl Into<String>, total: usize, watched: usize, now: Instant) {
        self.entries.insert(
            key.into(),
            WatchedCount { total, watched: watched.min(total), fetched_at: now },
        );
    }

    pub fn invalidate(&mut self, key: &str) {
        self.entries.remove(key);
    }
}
