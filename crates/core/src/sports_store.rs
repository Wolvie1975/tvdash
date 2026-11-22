//! Local persistence for Sports Alerts:
//! - `~/.local/share/tvdash/sports-starred.json` (starred Scoreboard EventIds)
//! - `~/.local/state/tvdash/sports-alerts.json` (fired alert keys, pruned after 3 days)
//! - `~/.cache/tvdash/sports-follows.json` (offline fallback cache of MyNewsFeed follows)

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarredGame {
    pub event_id: String,
    pub starred_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedTeam {
    pub league: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FollowCache {
    pub updated_at: i64,
    pub teams: Vec<CachedTeam>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FiredAlerts {
    /// Alert key -> unix timestamp when shown.
    pub keys: HashMap<String, i64>,
}

pub fn default_starred_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", crate::APP_NAME)
        .map(|d| d.data_dir().join("sports-starred.json"))
}

pub fn default_fired_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().join(".local/state")))?;
    Some(base.join(crate::APP_NAME).join("sports-alerts.json"))
}

pub fn default_follow_cache_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", crate::APP_NAME)
        .map(|d| d.cache_dir().join("sports-follows.json"))
}

fn load_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn save_json<T: serde::Serialize + ?Sized>(path: &Path, val: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_string_pretty(val).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)?;
    fs::rename(tmp, path)
}

pub fn load_starred(path: &Path) -> Vec<StarredGame> {
    load_json(path)
}

pub fn save_starred(path: &Path, games: &[StarredGame]) -> io::Result<()> {
    save_json(path, games)
}

pub fn load_fired(path: &Path, now: i64) -> FiredAlerts {
    let mut f: FiredAlerts = load_json(path);
    // Prune keys older than 3 days.
    let cutoff = now - 3 * 86_400;
    f.keys.retain(|_, &mut ts| ts >= cutoff);
    f
}

pub fn save_fired(path: &Path, fired: &FiredAlerts) -> io::Result<()> {
    save_json(path, fired)
}

pub fn load_follow_cache(path: &Path) -> FollowCache {
    load_json(path)
}

pub fn save_follow_cache(path: &Path, cache: &FollowCache) -> io::Result<()> {
    save_json(path, cache)
}
