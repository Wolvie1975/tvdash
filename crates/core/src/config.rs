//! `~/.config/tvdash/config.toml`: loading, first-run creation and saving.
//!
//! Secrets (Plex and Jellyfin tokens) never live here; see
//! [`crate::secrets`].

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine the user's config directory")]
    NoConfigDir,
    #[error("failed to read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid config in {path}: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub display: DisplayConfig,
    pub general: GeneralConfig,
    pub plex: PlexConfig,
    pub jellyfin: JellyfinConfig,
    pub browser: BrowserConfig,
    pub player: PlayerConfig,
    /// Kept last: TOML arrays of tables must follow plain tables.
    pub streaming: Vec<StreamingSite>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Connector name of the TV output, e.g. `HDMI-A-1`.
    pub tv_output: String,
    /// Connector name of the laptop panel, e.g. `eDP-1`.
    pub laptop_output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    Fahrenheit,
    Celsius,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockFormat {
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "12h")]
    H12,
}

/// Laptop window colours. The TV always stays dark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    /// Warm grey light theme.
    Stone,
    /// Cool blue-grey light theme.
    Mist,
}

impl ThemeMode {
    pub fn is_light(self) -> bool {
        self != Self::Dark
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub units: Units,
    pub clock: ClockFormat,
    /// Show seconds on the clocks.
    pub seconds: bool,
    /// Laptop theme, switched from the dashboard header.
    pub theme: ThemeMode,
    pub latitude: f64,
    pub longitude: f64,
    /// IANA zone name, e.g. `America/Chicago`. Empty or invalid → system local time.
    pub timezone: String,
}

impl GeneralConfig {
    /// Parsed timezone, or `None` to fall back to the system's local time.
    pub fn tz(&self) -> Option<chrono_tz::Tz> {
        if self.timezone.is_empty() {
            return None;
        }
        match self.timezone.parse() {
            Ok(tz) => Some(tz),
            Err(_) => {
                tracing::warn!(timezone = %self.timezone, "unknown timezone; using system local time");
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlexConfig {
    pub enabled: bool,
    /// Filled after PIN sign-in and server discovery.
    pub server_url: String,
    /// Generated UUID, sent as `X-Plex-Client-Identifier`.
    pub client_id: String,
    /// plex.tv base URL. Only written to the file when overridden (tests).
    #[serde(skip_serializing_if = "is_default_plex_account_url")]
    pub account_url: String,
}

pub const DEFAULT_PLEX_ACCOUNT_URL: &str = "https://plex.tv";

fn is_default_plex_account_url(url: &String) -> bool {
    url == DEFAULT_PLEX_ACCOUNT_URL
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JellyfinConfig {
    pub enabled: bool,
    pub server_url: String,
    /// Filled after sign-in.
    pub user_id: String,
    /// Generated UUID, sent as `DeviceId` in the Authorization header.
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    pub binary: String,
    /// Dedicated profile directory; `~` is expanded with [`expand_home`].
    pub profile_dir: String,
    /// Run Chrome through XWayland so it can be placed on the TV. Native Wayland
    /// windows can't choose their output, so GNOME opens them on the focused display.
    pub use_xwayland: bool,
    /// Override where the browser window opens, as "X,Y" in desktop coordinates.
    /// Empty = the TV output's position, as reported at startup.
    pub window_position: String,
}

impl BrowserConfig {
    /// Parsed `window_position` override.
    pub fn position_override(&self) -> Option<(i32, i32)> {
        let (x, y) = self.window_position.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub mpv: String,
}

/// A Watch on TV tile. Array order in the config = tile order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StreamingSite {
    pub name: String,
    /// Start page, opened full screen on the TV.
    pub url: String,
    /// Optional search page; `{title}` is replaced with the URL-encoded title
    /// (used by Where to watch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_url: Option<String>,
    /// TMDB watch-provider IDs this site counts as (Where to watch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tmdb_providers: Vec<u32>,
    /// Hidden from the dashboard; still counts as one of your services.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

impl StreamingSite {
    /// Check a site before saving it. `others` are the other sites (for unique names).
    pub fn validate(&self, others: &[&StreamingSite]) -> std::result::Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Give the tile a name.".into());
        }
        if others.iter().any(|o| o.name.trim().eq_ignore_ascii_case(name)) {
            return Err(format!("There's already a site called {name}."));
        }
        let web = |u: &str| u.starts_with("https://") || u.starts_with("http://");
        if !web(self.url.trim()) {
            return Err("The start page must be a web address (https://…).".into());
        }
        if let Some(search) = self.search_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if !web(search) {
                return Err("The search page must be a web address (https://…).".into());
            }
            if !search.contains("{title}") {
                return Err("Put {title} in the search page where the title goes.".into());
            }
        }
        Ok(())
    }

    /// Where to open a title: the search page with `{title}` filled in, else the start page.
    pub fn open_url(&self, title: &str) -> String {
        match self.search_url.as_deref() {
            Some(tpl) if tpl.contains("{title}") => tpl.replace("{title}", &encode_component(title)),
            _ => self.url.clone(),
        }
    }

    /// Host of the start page, e.g. `www.netflix.com`.
    pub fn domain(&self) -> Option<&str> {
        let rest = self.url.split_once("://")?.1;
        let host = rest.split(['/', '?', '#']).next()?;
        (!host.is_empty()).then_some(host)
    }
}

/// Percent-encode everything but unreserved characters (RFC 3986).
pub fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Default for Config {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            general: GeneralConfig::default(),
            plex: PlexConfig::default(),
            jellyfin: JellyfinConfig::default(),
            browser: BrowserConfig::default(),
            player: PlayerConfig::default(),
            streaming: default_streaming_sites(),
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { tv_output: "HDMI-A-1".into(), laptop_output: "eDP-1".into() }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            units: Units::Fahrenheit,
            clock: ClockFormat::H24,
            seconds: false,
            theme: ThemeMode::Dark,
            latitude: 0.0,
            longitude: 0.0,
            timezone: "America/Chicago".into(),
        }
    }
}

impl Default for PlexConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server_url: String::new(),
            client_id: String::new(),
            account_url: DEFAULT_PLEX_ACCOUNT_URL.into(),
        }
    }
}

impl Default for JellyfinConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            server_url: "http://republic-city:8096".into(),
            user_id: String::new(),
            device_id: String::new(),
        }
    }
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            // Any Chromium-based browser with Widevine works (Chrome, Edge, Vivaldi).
            binary: "brave-browser".into(),
            profile_dir: "~/.local/share/tvdash/browser".into(),
            use_xwayland: true,
            window_position: String::new(),
        }
    }
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self { mpv: "mpv".into() }
    }
}

fn default_streaming_sites() -> Vec<StreamingSite> {
    [
        ("Netflix", "https://www.netflix.com"),
        ("Disney+", "https://www.disneyplus.com"),
        ("Hulu", "https://www.hulu.com"),
        ("Prime Video", "https://www.primevideo.com"),
        ("Apple TV+", "https://tv.apple.com"),
        ("YouTube TV", "https://tv.youtube.com"),
    ]
    .into_iter()
    .map(|(name, url)| StreamingSite { name: name.into(), url: url.into(), ..Default::default() })
    .collect()
}

const HEADER: &str = "\
# tvdash configuration.
# Plex and Jellyfin tokens are kept in GNOME Keyring, not here.
# Output names: run tvdash once and check the log for \"available monitor\" lines.

";

/// A config plus where it came from.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    /// True when no config existed and a default one was written.
    pub created: bool,
}

/// `~/.config/tvdash/config.toml` (respects `XDG_CONFIG_HOME`).
pub fn default_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", crate::APP_NAME).ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.toml"))
}

impl Config {
    /// Load the config from the default path, creating it on first run.
    pub fn load_or_create() -> Result<LoadedConfig> {
        Self::load_or_create_at(&default_path()?)
    }

    /// Load the config at `path`, creating it with defaults if missing, and
    /// filling in generated client/device IDs that are still empty.
    pub fn load_or_create_at(path: &Path) -> Result<LoadedConfig> {
        let (mut config, created) = match fs::read_to_string(path) {
            Ok(text) => {
                let config =
                    toml::from_str(&text).map_err(|source| ConfigError::Parse { path: path.to_owned(), source })?;
                (config, false)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Config::default(), true),
            Err(source) => {
                return Err(ConfigError::Read { path: path.to_owned(), source });
            }
        };

        let ids_added = config.ensure_ids();
        if created || ids_added {
            config.save_to(path)?;
        }

        Ok(LoadedConfig { config, path: path.to_owned(), created })
    }

    /// Generate the Plex client ID and Jellyfin device ID if missing.
    /// Returns true if anything changed.
    pub fn ensure_ids(&mut self) -> bool {
        let mut changed = false;
        for id in [&mut self.plex.client_id, &mut self.jellyfin.device_id] {
            if id.trim().is_empty() {
                *id = uuid::Uuid::new_v4().to_string();
                changed = true;
            }
        }
        changed
    }

    /// Re-read the config at `path`, apply `change`, and save it. Re-reading
    /// first keeps hand edits made while the app was running.
    pub fn update_at(path: &Path, change: impl FnOnce(&mut Config)) -> Result<Config> {
        let mut config = Self::load_or_create_at(path)?.config;
        change(&mut config);
        config.save_to(path)?;
        Ok(config)
    }

    /// Write the config atomically (temp file + rename).
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let body = toml::to_string_pretty(self)?;
        let write_err = |source| ConfigError::Write { path: path.to_owned(), source };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(write_err)?;
        }
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, format!("{HEADER}{body}")).map_err(write_err)?;
        fs::rename(&tmp, path).map_err(write_err)?;
        Ok(())
    }
}

/// Expand a leading `~` or `~/` to the user's home directory.
pub fn expand_home(path: &str) -> PathBuf {
    let home = || directories::BaseDirs::new().map(|b| b.home_dir().to_owned());
    let expanded =
        if path == "~" { home() } else { path.strip_prefix("~/").and_then(|rest| home().map(|h| h.join(rest))) };
    expanded.unwrap_or_else(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tvdash-test-{}-{name}", uuid::Uuid::new_v4()));
        dir.join("config.toml")
    }

    #[test]
    fn creates_default_config_with_ids() {
        let path = temp_path("create");
        let loaded = Config::load_or_create_at(&path).unwrap();
        assert!(loaded.created);
        assert!(path.exists());
        assert!(!loaded.config.plex.client_id.is_empty());
        assert!(!loaded.config.jellyfin.device_id.is_empty());
        assert_eq!(loaded.config.streaming.len(), 6);

        // Second load reads the same file and keeps the same IDs.
        let again = Config::load_or_create_at(&path).unwrap();
        assert!(!again.created);
        assert_eq!(again.config, loaded.config);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn partial_config_fills_defaults() {
        let path = temp_path("partial");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[display]\ntv_output = \"HDMI-A-2\"\n").unwrap();
        let loaded = Config::load_or_create_at(&path).unwrap();
        assert_eq!(loaded.config.display.tv_output, "HDMI-A-2");
        assert_eq!(loaded.config.display.laptop_output, "eDP-1");
        assert_eq!(loaded.config.general.clock, ClockFormat::H24);
        assert!(!loaded.config.plex.client_id.is_empty());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn old_sections_are_ignored() {
        let path = temp_path("old");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[calendar]\nenabled = true\n\n[homelab]\nportainer_url = \"x\"\n").unwrap();
        assert!(Config::load_or_create_at(&path).is_ok());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn streaming_site_fields() {
        // v1 entries load; new fields stay out of the file until used.
        let c: Config = toml::from_str("[[streaming]]\nname = \"Hulu\"\nurl = \"https://www.hulu.com\"\n").unwrap();
        assert_eq!(
            c.streaming[0],
            StreamingSite { name: "Hulu".into(), url: "https://www.hulu.com".into(), ..Default::default() }
        );
        let text = toml::to_string_pretty(&c).unwrap();
        assert!(!text.contains("hidden") && !text.contains("tmdb_providers") && !text.contains("search_url"));

        let mut s = c.streaming[0].clone();
        s.search_url = Some("https://www.hulu.com/search?q={title}".into());
        s.tmdb_providers = vec![15];
        s.hidden = true;
        assert_eq!(s.open_url("Rick & Morty"), "https://www.hulu.com/search?q=Rick%20%26%20Morty");
        assert_eq!(s.domain(), Some("www.hulu.com"));
        let back: StreamingSite = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);

        let netflix = StreamingSite { name: "netflix".into(), ..s.clone() };
        assert!(s.validate(&[]).is_ok());
        assert!(s.validate(&[&netflix]).is_ok());
        let dup = StreamingSite { name: "HULU".into(), ..s.clone() };
        assert!(s.validate(&[&dup]).unwrap_err().contains("already"));
        let bad = StreamingSite { search_url: Some("https://x.com/search".into()), ..s.clone() };
        assert!(bad.validate(&[]).unwrap_err().contains("{title}"));
        let bad = StreamingSite { url: "hulu.com".into(), ..s };
        assert!(bad.validate(&[]).is_err());
    }

    #[test]
    fn round_trips_through_toml() {
        let mut c = Config::default();
        c.ensure_ids();
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn timezone_parses() {
        assert!(GeneralConfig::default().tz().is_some());
        let bad = GeneralConfig { timezone: "Not/AZone".into(), ..Default::default() };
        assert!(bad.tz().is_none());
    }

    #[test]
    fn browser_position_override() {
        let mut b = BrowserConfig::default();
        assert_eq!(b.position_override(), None);
        b.window_position = " 1920, 0 ".into();
        assert_eq!(b.position_override(), Some((1920, 0)));
        b.window_position = "nope".into();
        assert_eq!(b.position_override(), None);
    }

    #[test]
    fn expands_home() {
        let p = expand_home("~/.local/share/tvdash/browser");
        assert!(!p.starts_with("~"));
        assert_eq!(expand_home("/abs/path"), PathBuf::from("/abs/path"));
    }
}
