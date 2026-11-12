//! `~/.config/tvdash/config.toml`: loading, first-run creation and saving.
//!
//! Secrets (the Plex and TMDB tokens) never live here; see
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
    pub browser: BrowserConfig,
    pub player: PlayerConfig,
    pub library: LibraryConfig,
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
    /// Two screens (laptop dashboard + TV), or everything on the TV.
    pub layout: Layout,
}

/// Screen layout. Changing it takes effect on the next start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Laptop dashboard + TV playback window.
    #[default]
    Dual,
    /// One full-screen 10-foot window on the TV, driven by remote or keyboard.
    Tv,
}

impl Layout {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "dual" => Some(Self::Dual),
            "tv" => Some(Self::Tv),
            _ => None,
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

/// How the Library's shelves and Browse all are ordered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    /// Date added, newest first.
    #[default]
    Added,
    /// Title A–Z (Plex's sort title: "The Long Drive Home" under L).
    Az,
    /// Title Z–A.
    Za,
}

impl SortOrder {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "added" => Some(Self::Added),
            "az" => Some(Self::Az),
            "za" => Some(Self::Za),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Az => "az",
            Self::Za => "za",
        }
    }
}

/// Library settings, chosen in the Library and remembered here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryConfig {
    pub sort_movies: SortOrder,
    pub sort_tv: SortOrder,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub mpv: String,
}

/// One of the fixed Watch on TV apps (see [`APPS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// File stem of its bundled logo, e.g. `disney-plus` (`assets/logos/disney-plus.svg`).
    pub id: &'static str,
    pub name: &'static str,
    /// Start page, opened full screen on the TV.
    pub url: &'static str,
    /// Search page with `{title}` where the title goes (Where to watch), if the service has one.
    pub search_url: Option<&'static str>,
    /// TMDB watch-provider IDs this app counts as.
    pub tmdb_providers: &'static [u32],
    /// TMDB provider names that also count (matched exactly, ignoring case and punctuation).
    pub tmdb_names: &'static [&'static str],
}

/// The Watch on TV apps, in tile order. Fixed: there's no add or remove; only
/// the addresses can be changed, in `[[streaming]]` in the config.
pub const APPS: [App; 10] = [
    App {
        id: "youtube-tv",
        name: "YouTube TV",
        url: "https://tv.youtube.com",
        search_url: None,
        tmdb_providers: &[],
        tmdb_names: &["YouTube TV"],
    },
    App {
        id: "youtube",
        name: "YouTube",
        url: "https://www.youtube.com",
        search_url: Some("https://www.youtube.com/results?search_query={title}"),
        tmdb_providers: &[192, 188],
        tmdb_names: &["YouTube", "YouTube Premium"],
    },
    App {
        id: "disney-plus",
        name: "Disney+",
        url: "https://www.disneyplus.com",
        search_url: None,
        tmdb_providers: &[337],
        tmdb_names: &["Disney Plus"],
    },
    App {
        id: "hulu",
        name: "Hulu",
        url: "https://www.hulu.com",
        search_url: Some("https://www.hulu.com/search?q={title}"),
        tmdb_providers: &[15],
        tmdb_names: &["Hulu"],
    },
    App {
        id: "apple-tv",
        name: "Apple TV",
        url: "https://tv.apple.com",
        search_url: Some("https://tv.apple.com/search?term={title}"),
        tmdb_providers: &[350],
        tmdb_names: &["Apple TV Plus"],
    },
    App {
        id: "netflix",
        name: "Netflix",
        url: "https://www.netflix.com",
        search_url: Some("https://www.netflix.com/search?q={title}"),
        tmdb_providers: &[8, 1796],
        tmdb_names: &["Netflix", "Netflix Standard with Ads", "Netflix basic with Ads"],
    },
    App {
        id: "fandango-at-home",
        name: "Fandango at Home",
        url: "https://athome.fandango.com",
        search_url: None,
        tmdb_providers: &[7, 332],
        tmdb_names: &["Fandango At Home", "Fandango At Home Free", "Vudu", "Vudu Free"],
    },
    App {
        id: "prime-video",
        name: "Prime Video",
        url: "https://www.primevideo.com",
        search_url: Some("https://www.primevideo.com/search/ref=atv_nb_sr?phrase={title}"),
        tmdb_providers: &[9, 2100],
        tmdb_names: &["Amazon Prime Video", "Amazon Prime Video with Ads"],
    },
    App {
        id: "crunchyroll",
        name: "Crunchyroll",
        url: "https://www.crunchyroll.com",
        search_url: Some("https://www.crunchyroll.com/search?q={title}"),
        tmdb_providers: &[283],
        tmdb_names: &["Crunchyroll"],
    },
    App {
        id: "tubi",
        name: "Tubi",
        url: "https://tubitv.com",
        search_url: Some("https://tubitv.com/search/{title}"),
        tmdb_providers: &[73],
        tmdb_names: &["Tubi TV", "Tubi"],
    },
];

/// Lower-case letters and digits only: "Disney+" and "disney plus" → "disney…".
fn name_key(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

impl App {
    /// Does a TMDB provider name count as this app?
    pub fn has_tmdb_name(&self, provider_name: &str) -> bool {
        let key = name_key(provider_name);
        self.tmdb_names.iter().any(|n| name_key(n) == key)
    }
}

/// A Watch on TV app as configured: one entry per [`APPS`] item, in the same order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StreamingSite {
    pub name: String,
    /// Start page, opened full screen on the TV.
    pub url: String,
    /// Optional search page; `{title}` is replaced with the URL-encoded title
    /// (used by Where to watch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_url: Option<String>,
    /// TMDB watch-provider IDs this site counts as (from [`APPS`], not the file).
    #[serde(skip)]
    pub tmdb_providers: Vec<u32>,
}

impl StreamingSite {
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

/// The ten apps with any addresses the config overrides. Entries are matched by
/// name ("Apple TV+" counts as Apple TV); anything else in the file is dropped.
pub fn resolve_apps(configured: &[StreamingSite]) -> Vec<StreamingSite> {
    let web = |u: &str| u.starts_with("https://") || u.starts_with("http://");
    APPS.iter()
        .map(|app| {
            let entry = configured.iter().find(|s| name_key(&s.name) == name_key(app.name));
            let url = entry.map(|e| e.url.trim()).filter(|u| web(u)).unwrap_or(app.url);
            let search_url = match entry.and_then(|e| e.search_url.as_deref()).map(str::trim) {
                Some(u) if web(u) && u.contains("{title}") => Some(u.to_owned()),
                _ => app.search_url.map(str::to_owned),
            };
            StreamingSite {
                name: app.name.into(),
                url: url.into(),
                search_url,
                tmdb_providers: app.tmdb_providers.to_vec(),
            }
        })
        .collect()
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
            browser: BrowserConfig::default(),
            player: PlayerConfig::default(),
            library: LibraryConfig::default(),
            streaming: resolve_apps(&[]),
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { tv_output: "HDMI-A-1".into(), laptop_output: "eDP-1".into(), layout: Layout::Dual }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            units: Units::Fahrenheit,
            clock: ClockFormat::H12,
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

const HEADER: &str = "\
# tvdash configuration.
# The Plex and TMDB tokens are kept in GNOME Keyring, not here.
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
        let apps = resolve_apps(&config.streaming);
        let apps_changed = apps != config.streaming;
        config.streaming = apps;
        if created || ids_added || apps_changed {
            config.save_to(path)?;
        }

        Ok(LoadedConfig { config, path: path.to_owned(), created })
    }

    /// Generate the Plex client ID if missing. Returns true if anything changed.
    pub fn ensure_ids(&mut self) -> bool {
        if !self.plex.client_id.trim().is_empty() {
            return false;
        }
        self.plex.client_id = uuid::Uuid::new_v4().to_string();
        true
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
        assert_eq!(loaded.config.streaming.len(), 10);

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
        assert_eq!(loaded.config.general.clock, ClockFormat::H12);
        assert!(!loaded.config.plex.client_id.is_empty());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn old_sections_are_ignored() {
        let path = temp_path("old");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "[calendar]\nenabled = true\n\n[homelab]\nportainer_url = \"x\"\n\n[jellyfin]\nenabled = true\n",
        )
        .unwrap();
        assert!(Config::load_or_create_at(&path).is_ok());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn apps_are_fixed_and_only_addresses_change() {
        // An old file: renamed Apple TV+, a custom Netflix address, a removed site,
        // and fields that no longer exist.
        let path = temp_path("apps");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "[[streaming]]\nname = \"Netflix\"\nurl = \"https://www.netflix.com/browse\"\nhidden = true\n\
             tmdb_providers = [1]\n\n[[streaming]]\nname = \"Max\"\nurl = \"https://www.max.com\"\n\n\
             [[streaming]]\nname = \"Apple TV+\"\nurl = \"not a url\"\nlogo = \"~/x.png\"\n",
        )
        .unwrap();
        let c = Config::load_or_create_at(&path).unwrap().config;
        let names: Vec<&str> = c.streaming.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, APPS.map(|a| a.name));
        let netflix = &c.streaming[5];
        assert_eq!(netflix.url, "https://www.netflix.com/browse", "address kept");
        assert_eq!(netflix.tmdb_providers, [8, 1796], "providers come from the code");
        assert_eq!(c.streaming[4].url, "https://tv.apple.com", "bad address → default");
        // Saved back normalised: ten entries, Max gone, no provider IDs in the file.
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("[[streaming]]").count(), 10);
        assert!(!text.contains("Max") && !text.contains("tmdb_providers") && !text.contains("hidden"));
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn open_url_and_names() {
        let hulu = &resolve_apps(&[])[3];
        assert_eq!(hulu.open_url("Rick & Morty"), "https://www.hulu.com/search?q=Rick%20%26%20Morty");
        assert_eq!(hulu.domain(), Some("www.hulu.com"));
        let yt_tv = &resolve_apps(&[])[0];
        assert_eq!(yt_tv.open_url("x"), "https://tv.youtube.com", "no search page: start page");
        assert!(APPS[2].has_tmdb_name("Disney Plus"));
        assert!(APPS[0].has_tmdb_name("YouTube TV") && !APPS[0].has_tmdb_name("YouTube"));
        assert!(!APPS[4].has_tmdb_name("Apple TV"), "the store isn't the subscription");
    }

    #[test]
    fn library_sort_is_saved_per_tab() {
        let c: Config = toml::from_str("[library]\nsort_tv = \"za\"\n").unwrap();
        assert_eq!((c.library.sort_movies, c.library.sort_tv), (SortOrder::Added, SortOrder::Za));
        assert!(toml::to_string(&c).unwrap().contains("sort_tv = \"za\""));
        assert_eq!(SortOrder::parse("az"), Some(SortOrder::Az));
        assert_eq!(SortOrder::Za.id(), "za");
    }

    #[test]
    fn round_trips_through_toml() {
        let mut c = Config::default();
        c.ensure_ids();
        let text = toml::to_string_pretty(&c).unwrap();
        let mut back: Config = toml::from_str(&text).unwrap();
        back.streaming = resolve_apps(&back.streaming);
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
