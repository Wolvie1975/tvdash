//! Small JSON key/value cache in SQLite (`~/.local/share/tvdash/cache.db`).
//! Failures are logged and treated as misses: the cache is only an optimisation.

use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use tracing::warn;

pub struct Cache {
    db: Mutex<Connection>,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Cache {
    /// Open (or create) the cache file.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    /// An in-memory cache (tests).
    pub fn memory() -> Self {
        Self::init(Connection::open_in_memory().expect("in-memory SQLite")).expect("cache schema")
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL, fetched INTEGER NOT NULL)",
        )?;
        Ok(Self { db: Mutex::new(db) })
    }

    /// The value for `key` if it was stored less than `max_age` ago.
    pub fn get<T: DeserializeOwned>(&self, key: &str, max_age: Duration) -> Option<T> {
        let db = self.db.lock().ok()?;
        let row: Option<(String, i64)> = db
            .query_row("SELECT value, fetched FROM kv WHERE key = ?1", params![key], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .unwrap_or_else(|e| {
                warn!(error = %e, "cache read failed");
                None
            });
        let (value, fetched) = row?;
        if now() - fetched > max_age.as_secs() as i64 {
            return None;
        }
        serde_json::from_str(&value).ok()
    }

    pub fn put<T: Serialize>(&self, key: &str, value: &T) {
        self.put_at(key, value, now());
    }

    fn put_at<T: Serialize>(&self, key: &str, value: &T, fetched: i64) {
        let Ok(json) = serde_json::to_string(value) else { return };
        let Ok(db) = self.db.lock() else { return };
        if let Err(e) = db.execute(
            "INSERT INTO kv (key, value, fetched) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, fetched = excluded.fetched",
            params![key, json, fetched],
        ) {
            warn!(error = %e, "cache write failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_expires() {
        let c = Cache::memory();
        c.put("a", &vec![1, 2, 3]);
        assert_eq!(c.get::<Vec<i32>>("a", Duration::from_secs(60)), Some(vec![1, 2, 3]));
        assert_eq!(c.get::<Vec<i32>>("b", Duration::from_secs(60)), None);
        c.put_at("old", &1, now() - 7200);
        assert_eq!(c.get::<i32>("old", Duration::from_secs(3600)), None);
        assert_eq!(c.get::<i32>("old", Duration::from_secs(10_000)), Some(1));
        c.put("a", &vec![4]);
        assert_eq!(c.get::<Vec<i32>>("a", Duration::from_secs(60)), Some(vec![4]));
    }

    #[test]
    fn survives_reopen() {
        let path = std::env::temp_dir().join(format!("tvdash-cache-{}.db", std::process::id()));
        Cache::open(&path).unwrap().put("k", &"v");
        assert_eq!(Cache::open(&path).unwrap().get::<String>("k", Duration::from_secs(60)).as_deref(), Some("v"));
        std::fs::remove_file(path).ok();
    }
}
