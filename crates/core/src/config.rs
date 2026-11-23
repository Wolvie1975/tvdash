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
    pub tautulli: TautulliConfig,
    pub events: EventsConfig,
    pub browser: BrowserConfig,
    pub player: PlayerConfig,
    pub tv: TvConfig,
    pub sleep: SleepConfig,
    pub sports: SportsConfig,
    pub startup: StartupConfig,
    /// Kept last: TOML arrays of tables must follow plain tables.
    pub streaming: Vec<StreamingSite>,
}

pub use crate::layout::{Layout, LayoutSetting};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Connector name of the TV output, e.g. `HDMI-A-1`.
    pub tv_output: String,
    /// Connector name of the laptop panel, e.g. `eDP-1`.
    pub laptop_output: String,
    /// Automatic (based on active screens), two screens, or TV only.
    pub layout: LayoutSetting,
    /// Switch layout automatically while running when screens turn on/off (only used when `layout = "auto"`).
    pub switch_while_running: bool,
    /// Force Dark theme on the TV during night-mode hours.
    #[serde(default)]
    pub night_mode: bool,
    /// Start of night-mode hours in `HH:MM` 24-hour format (default `"22:00"`).
    #[serde(default = "default_night_start")]
    pub night_start: String,
    /// End of night-mode hours in `HH:MM` 24-hour format (default `"07:00"`).
    #[serde(default = "default_night_end")]
    pub night_end: String,
    /// Dashboard UI skin (`"classic"` or `"blades"`).
    #[serde(default)]
    pub dashboard_ui: DashboardUi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardUi {
    #[default]
    Classic,
    Blades,
}

impl DashboardUi {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Blades => "blades",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "blades" => Self::Blades,
            _ => Self::Classic,
        }
    }
}

fn default_night_start() -> String {
    "22:00".into()
}

fn default_night_end() -> String {
    "07:00".into()
}

impl DisplayConfig {
    /// True when `night_mode` is enabled and `now` falls inside `[night_start, night_end)`.
    pub fn is_night_now(&self, now: chrono::NaiveTime) -> bool {
        if !self.night_mode {
            return false;
        }
        let Ok(start) = chrono::NaiveTime::parse_from_str(&self.night_start, "%H:%M") else {
            return false;
        };
        let Ok(end) = chrono::NaiveTime::parse_from_str(&self.night_end, "%H:%M") else {
            return false;
        };
        if start <= end {
            now >= start && now < end
        } else {
            now >= start || now < end
        }
    }
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
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[serde(alias = "system")]
    FollowSystem,
    #[default]
    Dark,
    /// Deep plum dark theme.
    Aubergine,
    /// University of Kansas (Crimson & Blue) dark theme.
    #[serde(alias = "ku-light")]
    Ku,
    /// Warm grey light theme.
    Stone,
    /// Cool blue-grey light theme.
    Mist,
}

impl ThemeMode {
    pub fn is_light(self) -> bool {
        matches!(self, Self::Stone | Self::Mist)
    }

    /// Resolve `FollowSystem` into a concrete theme given the desktop preference.
    pub fn resolve(self, system_dark: bool) -> Self {
        match self {
            Self::FollowSystem => {
                if system_dark {
                    Self::Dark
                } else {
                    Self::Stone
                }
            }
            other => other,
        }
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
    /// Name shown for the location, e.g. "Chicago, Illinois, United States".
    pub location: String,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TautulliConfig {
    /// Tautulli base URL (used for Popular on Republic City).
    pub url: String,
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

/// TV-only hub state persisted in `config.toml` (`[tv]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TvConfig {
    /// Last 3 distinct streaming services opened, newest first.
    pub recent: Vec<RecentService>,
}

/// Sleep timer settings (`[sleep]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SleepConfig {
    /// Preset durations in minutes for picker tiles 1–6.
    pub presets: Vec<i64>,
    /// Default preset (minutes) focused when the timer is off and there is no last-used value.
    pub default: i64,
    /// Focus the last-used preset next time the picker opens.
    pub remember_last: bool,
    /// Warning duration before sleep (`"30s"`, `"60s"`, `"2m"`).
    pub warn_before: String,
    /// Extension added by "Keep watching" (`"30m"`).
    pub extend_by: String,
    /// Audio fade-out duration before stopping mpv (`"20s"`).
    pub fade: String,
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self {
            presets: vec![15, 30, 45, 60, 90, 120],
            default: 60,
            remember_last: true,
            warn_before: "60s".into(),
            extend_by: "30m".into(),
            fade: "20s".into(),
        }
    }
}

impl SleepConfig {
    /// Parse a short human duration like `"30s"`, `"60s"`, `"2m"`, `"30m"`, `"1h"` into seconds.
    pub fn parse_duration_secs(raw: &str) -> Option<i64> {
        let s = raw.trim();
        if s.is_empty() {
            return None;
        }
        if let Some(n) = s.strip_suffix('s') {
            n.trim().parse::<i64>().ok().filter(|&v| v > 0)
        } else if let Some(n) = s.strip_suffix('m') {
            n.trim().parse::<i64>().ok().filter(|&v| v > 0).map(|v| v * 60)
        } else if let Some(n) = s.strip_suffix('h') {
            n.trim().parse::<i64>().ok().filter(|&v| v > 0).map(|v| v * 3600)
        } else {
            s.parse::<i64>().ok().filter(|&v| v > 0)
        }
    }

    pub fn warn_secs(&self) -> i64 {
        Self::parse_duration_secs(&self.warn_before).unwrap_or(60)
    }

    pub fn extend_minutes(&self) -> i64 {
        (Self::parse_duration_secs(&self.extend_by).unwrap_or(30 * 60) / 60).max(1)
    }

    pub fn fade_duration(&self) -> std::time::Duration {
        let secs = Self::parse_duration_secs(&self.fade).unwrap_or(20).max(0) as u64;
        std::time::Duration::from_secs(secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupConfig {
    /// `"on"` | `"off"`
    pub intro: String,
    /// Longest the logo waits for first Home data (seconds).
    pub max_hold_secs: u64,
}

impl StartupConfig {
    pub fn intro_on(&self) -> bool {
        !self.intro.trim().eq_ignore_ascii_case("off")
    }
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            intro: "on".into(),
            max_hold_secs: 6,
        }
    }
}

/// SQL Server connection for the Sports / Competitions tab. Password lives in the keyring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventsConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub trust_cert: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SportsConfig {
    pub alerts: SportsAlertsConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuringVideo {
    #[default]
    Show,
    Hold,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SportsAlertsConfig {
    pub enabled: bool,
    /// Minutes before start to show "Starting soon" (`0` = off).
    pub soon_minutes: u32,
    #[serde(rename = "final")]
    pub final_on: bool,
    pub during_video: DuringVideo,
}

impl Default for SportsAlertsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            soon_minutes: 15,
            final_on: true,
            during_video: DuringVideo::Show,
        }
    }
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            host: "192.168.1.100".into(),
            port: 1433,
            database: "WebScraper".into(),
            user: "mynewspaper_web".into(),
            trust_cert: true,
        }
    }
}

/// A recently opened streaming service entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentService {
    pub id: String,
    pub at: i64,
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
    /// Tile logo to use instead of TMDB's: a PNG, JPEG or WebP file (`~/` allowed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// Logo box sizing (`"wide"` or `"glyph"`, default `"glyph"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_box: Option<String>,
    /// Hold the sleep timer while this site is open (e.g. Google Meet).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold_sleep_timer: bool,
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
            tautulli: TautulliConfig::default(),
            events: EventsConfig::default(),
            browser: BrowserConfig::default(),
            player: PlayerConfig::default(),
            tv: TvConfig::default(),
            sleep: SleepConfig::default(),
            sports: SportsConfig::default(),
            startup: StartupConfig::default(),
            streaming: default_streaming_sites(),
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            tv_output: "HDMI-A-1".into(),
            laptop_output: "eDP-1".into(),
            layout: LayoutSetting::Tv,
            switch_while_running: true,
            night_mode: false,
            night_start: default_night_start(),
            night_end: default_night_end(),
            dashboard_ui: DashboardUi::Classic,
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            units: Units::Fahrenheit,
            clock: ClockFormat::H24,
            seconds: false,
            theme: ThemeMode::Dark,
            location: String::new(),
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
        ("YouTube TV", "https://tv.youtube.com", false),
        ("YouTube", "https://www.youtube.com", false),
        ("Disney+", "https://www.disneyplus.com", false),
        ("Hulu", "https://www.hulu.com", false),
        ("Apple TV", "https://tv.apple.com", false),
        ("Netflix", "https://www.netflix.com", false),
        ("Fandango at Home", "https://athome.fandango.com", false),
        ("Prime Video", "https://www.primevideo.com", false),
        ("YouTube Music", "https://music.youtube.com", false),
        ("Tubi", "https://tubitv.com", false),
        ("Crunchyroll", "https://www.crunchyroll.com", false),
        ("Google Meet", "https://meet.google.com", true),
    ]
    .into_iter()
    .map(|(name, url, hold)| StreamingSite {
        name: name.into(),
        url: url.into(),
        hold_sleep_timer: hold,
        ..Default::default()
    })
    .collect()
}

const HEADER: &str = "\
# tvdash configuration.
# Plex, TMDB and Tautulli tokens are kept in GNOME Keyring, not here.
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
        assert_eq!(loaded.config.streaming.len(), 12);

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

    #[test]
    fn night_mode_window() {
        let mut d = DisplayConfig::default();
        let t = |h, m| chrono::NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert!(!d.is_night_now(t(23, 0)), "disabled by default");
        d.night_mode = true;
        // Overnight window: 22:00..07:00
        assert!(d.is_night_now(t(22, 0)));
        assert!(d.is_night_now(t(2, 30)));
        assert!(!d.is_night_now(t(7, 0)));
        assert!(!d.is_night_now(t(15, 0)));
        // Same-day window: 01:00..05:00
        d.night_start = "01:00".into();
        d.night_end = "05:00".into();
        assert!(d.is_night_now(t(3, 0)));
        assert!(!d.is_night_now(t(23, 0)));
    }
}
