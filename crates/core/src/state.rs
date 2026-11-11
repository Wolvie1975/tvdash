//! Small app-written state (not user config): `~/.local/share/tvdash/state.toml`.
//!
//! Records when each streaming site was last opened, so the UI can tell a first
//! visit (show the sign-in hint) from a returning one, and Settings › Accounts can
//! say "opened N days ago".

use std::{collections::BTreeMap, fs, io, path::Path, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    /// Site name → last time it was opened on the TV.
    pub streaming_opened: BTreeMap<String, DateTime<Utc>>,
}

/// `~/.local/share/tvdash/state.toml` (respects `XDG_DATA_HOME`).
pub fn default_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", crate::APP_NAME).map(|d| d.data_dir().join("state.toml"))
}

impl AppState {
    /// Missing or unreadable state is not an error; it just starts empty.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!(path = %path.display(), error = %e, "ignoring unreadable state file");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = toml::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, body)?;
        fs::rename(tmp, path)
    }

    /// Record an opening; returns true if this was the first time.
    pub fn mark_opened(&mut self, site: &str, at: DateTime<Utc>) -> bool {
        self.streaming_opened.insert(site.to_owned(), at).is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_open_then_round_trip() {
        let dir = std::env::temp_dir().join(format!("tvdash-state-{}", uuid::Uuid::new_v4()));
        let path = dir.join("state.toml");
        let mut s = AppState::load(&path);
        assert!(s.mark_opened("Netflix", Utc::now()));
        assert!(!s.mark_opened("Netflix", Utc::now()));
        s.save(&path).unwrap();
        assert_eq!(AppState::load(&path), s);
        fs::remove_dir_all(dir).ok();
    }
}
