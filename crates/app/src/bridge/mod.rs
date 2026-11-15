//! Moves `AppEvent`s from background tasks onto the UI thread and keeps the
//! UI-side state (rows, posters, accounts, library) that models are built from.
//!
//! Both layouts read the same globals (the TV-only shell is a view inside the
//! main window), so this one module feeds both. By concern:
//!
//! - [`home`]: Continue Watching cards, the On the TV card, sessions, tiles, posters
//! - [`library`]: the Library page, its search and the detail panel
//! - [`accounts`]: Settings › Accounts and Streaming rows, the site editor, first-run setup
//! - [`status`]: weather, location, Settings › Diagnostics

mod accounts;
mod home;
mod library;
mod status;
#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, VecModel, Weak};
use tokio::sync::mpsc::UnboundedReceiver;
use tvdash_core::{
    AppEvent,
    config::{ClockFormat, StreamingSite, ThemeMode, Units},
    events::{PlayerStatus, Service},
    models::{AccountStatus, ArtworkChoice, MediaItem, RgbaImage, ServerSummary, SignInState, SourceKind, Weather},
};

pub use accounts::sync_setup;
pub use home::{row, set_cw_filter, up_next};
pub use library::{clear_search, close_detail, select, selected, set_filter, start_search};

use crate::{
    AccountPrompt, AccountRow, Accounts, Artwork, ArtworkRow, Dash, Diagnostics, Display, LaptopWindow, Library, Nav,
    PosterCardData, Server, ServerActivityRow, ServerLibraryRow, ServerStreamRow, SessionRow, Setup, SiteRow, Theme,
    TvWindow, UiTheme, tint::Hue,
};

/// Display order for sources everywhere (rows, prompts, sessions).
const SOURCES: [SourceKind; 1] = [SourceKind::Plex];

const TOAST_FOR: Duration = Duration::from_secs(5);
const LONG_TOAST_FOR: Duration = Duration::from_secs(30);

/// UI-thread state. Only touched from the Slint event loop.
#[derive(Default)]
pub struct UiState {
    pub tz: Option<chrono_tz::Tz>,
    pub clock: Option<ClockFormat>,
    units: Option<Units>,
    theme: ThemeMode,
    system_dark: bool,
    /// Streaming sites in tile order, hidden ones included.
    sites: Vec<StreamingSite>,
    /// What tvdash is showing on the TV: mpv playback, or the browser's site.
    player: Option<PlayerStatus>,
    browser: Option<String>,
    continue_watching: HashMap<SourceKind, Vec<MediaItem>>,
    recently_added: HashMap<SourceKind, Vec<MediaItem>>,
    /// Dashboard Continue Watching rows (filtered by `cw_filter`); index = card index.
    rows: Vec<MediaItem>,
    cw_filter: String,
    posters: HashMap<(SourceKind, String), Image>,
    /// TMDB token state.
    tmdb: Option<AccountStatus>,
    /// Last weather and when it arrived; the last fetch error, if the latest fetch failed.
    weather: Option<(Weather, DateTime<Utc>)>,
    weather_err: Option<String>,
    /// Last place search results (Settings › Display › Location).
    places: Vec<tvdash_core::models::Place>,
    /// Settings › Diagnostics: last success and last error per service.
    health: HashMap<Service, status::Health>,
    /// Service logos (from TMDB) by site name.
    logos: HashMap<String, Image>,
    /// Dominant hue of each poster, for tints.
    hues: HashMap<(SourceKind, String), Hue>,
    accounts: HashMap<SourceKind, AccountStatus>,
    servers: HashMap<SourceKind, String>,
    server_summary: Option<ServerSummary>,
    sign_in: HashMap<SourceKind, SignInState>,
    streaming: BTreeMap<String, DateTime<Utc>>,
    // Library
    search: Option<(String, Vec<MediaItem>)>,
    lib_continue: Vec<MediaItem>,
    lib_recent: Vec<MediaItem>,
    lib_results: Vec<MediaItem>,
    selected: Option<MediaItem>,
    // Models
    cards: Rc<VecModel<PosterCardData>>,
    prompts: Rc<VecModel<AccountPrompt>>,
    tv_prompts: Rc<VecModel<AccountPrompt>>,
    sessions: Rc<VecModel<SessionRow>>,
    lib_continue_cards: Rc<VecModel<PosterCardData>>,
    lib_recent_cards: Rc<VecModel<PosterCardData>>,
    lib_result_cards: Rc<VecModel<PosterCardData>>,
    account_rows: Rc<VecModel<AccountRow>>,
    site_rows: Rc<VecModel<SiteRow>>,
    toast_timer: Option<Timer>,
}

impl UiState {
    pub(super) fn effective_theme(&self) -> ThemeMode {
        self.theme.resolve(self.system_dark)
    }
}

thread_local! {
    pub static STATE: RefCell<UiState> = RefCell::new(UiState::default());
}

/// Settings the UI needs from the config.
pub struct UiConfig {
    pub tz: Option<chrono_tz::Tz>,
    /// Weather location name ("" = not set).
    pub location: String,
    pub clock: ClockFormat,
    pub seconds: bool,
    /// "Log file: …" lines for Settings › Diagnostics.
    pub paths: Vec<String>,
    pub units: Units,
    pub theme: ThemeMode,
    pub layout_setting: tvdash_core::LayoutSetting,
    pub tv_connected: bool,
    pub sites: Vec<StreamingSite>,
}

/// Call once on the UI thread before the event loop starts.
pub fn init(laptop: &LaptopWindow, c: UiConfig) {
    STATE.with_borrow_mut(|s| {
        s.tz = c.tz;
        s.clock = Some(c.clock);
        s.units = Some(c.units);
        s.system_dark = true;
        s.cw_filter = "tv".into();
        s.sites = c.sites;
        let dash = laptop.global::<Dash>();
        dash.set_tv_connected(c.tv_connected);
        dash.set_continue_watching(ModelRc::from(s.cards.clone()));
        dash.set_prompts(ModelRc::from(s.prompts.clone()));
        dash.set_tv_prompts(ModelRc::from(s.tv_prompts.clone()));
        dash.set_sessions(ModelRc::from(s.sessions.clone()));
        let lib = laptop.global::<Library>();
        lib.set_continue_watching(ModelRc::from(s.lib_continue_cards.clone()));
        lib.set_recently_added(ModelRc::from(s.lib_recent_cards.clone()));
        lib.set_results(ModelRc::from(s.lib_result_cards.clone()));
        let acc = laptop.global::<Accounts>();
        acc.set_accounts(ModelRc::from(s.account_rows.clone()));
        acc.set_sites(ModelRc::from(s.site_rows.clone()));
        let d = laptop.global::<Display>();
        d.set_layout_setting(c.layout_setting.as_str().into());
        d.set_celsius(c.units == Units::Celsius);
        d.set_h12(c.clock == ClockFormat::H12);
        d.set_seconds(c.seconds);
        d.set_location_name(c.location.as_str().into());
        let paths: Vec<slint::SharedString> = c.paths.iter().map(|p| p.as_str().into()).collect();
        laptop.global::<Diagnostics>().set_paths(ModelRc::new(VecModel::from(paths)));
        status::refresh_diagnostics(s, laptop);
        apply_theme(s, laptop, c.theme);
        accounts::refresh(s, laptop);
    });
}

/// The place picked from the search results (for SetLocation).
pub fn place(index: usize) -> Option<tvdash_core::models::Place> {
    STATE.with_borrow(|s| s.places.get(index).cloned())
}

/// The sites as the UI knows them (for the editor).
pub fn sites() -> Vec<StreamingSite> {
    STATE.with_borrow(|s| s.sites.clone())
}

/// Settings › Display changed: re-render times and temperatures (the weather
/// poller fetches again in the new units by itself).
pub fn set_display(laptop: &LaptopWindow, units: Units, clock: ClockFormat, seconds: bool) {
    crate::clock::set_format(clock, seconds);
    STATE.with_borrow_mut(|s| {
        s.units = Some(units);
        s.clock = Some(clock);
        home::refresh_tv_card(s, laptop);
        status::refresh_diagnostics(s, laptop);
    });
}

/// Once a minute: ages ("updated 25 min ago"), end times.
pub fn on_minute(laptop: &LaptopWindow, _tv: &TvWindow) {
    STATE.with_borrow(|s| {
        status::refresh_weather(s, laptop);
        status::refresh_diagnostics(s, laptop);
        home::refresh_tv_card(s, laptop);
    });
    crate::tv_nav::tick(laptop);
}

// ── Theme ───────────────────────────────────────────────────────────

/// Switch the laptop's colours (the TV window keeps its own, dark, Theme).
pub fn set_theme(laptop: &LaptopWindow, mode: ThemeMode) {
    STATE.with_borrow_mut(|s| apply_theme(s, laptop, mode));
}

fn ui_theme(mode: ThemeMode) -> UiTheme {
    match mode {
        ThemeMode::Dark | ThemeMode::FollowSystem => UiTheme::Dark,
        ThemeMode::Stone => UiTheme::Stone,
        ThemeMode::Mist => UiTheme::Mist,
    }
}

pub fn theme_mode(theme: UiTheme) -> ThemeMode {
    match theme {
        UiTheme::Stone => ThemeMode::Stone,
        UiTheme::Mist => ThemeMode::Mist,
        _ => ThemeMode::Dark,
    }
}

fn apply_theme(s: &mut UiState, laptop: &LaptopWindow, mode: ThemeMode) {
    s.theme = mode;
    let resolved = s.effective_theme();
    laptop.global::<Display>().set_follow_system(mode == ThemeMode::FollowSystem);
    laptop.global::<Theme>().set_mode(ui_theme(resolved));
    home::refresh_tiles(s, laptop);
    home::rebuild_rows(s);
    library::rebuild(s, laptop);
    home::refresh_tv_card(s, laptop);
}

// ── Events ──────────────────────────────────────────────────────────

pub async fn forward(mut rx: UnboundedReceiver<AppEvent>, laptop: Weak<LaptopWindow>, tv: Weak<TvWindow>) {
    while let Some(event) = rx.recv().await {
        let (laptop, tv) = (laptop.clone(), tv.clone());
        let delivered = slint::invoke_from_event_loop(move || {
            if let (Some(laptop), Some(tv)) = (laptop.upgrade(), tv.upgrade()) {
                STATE.with_borrow_mut(|s| apply(s, &laptop, &tv, event));
            }
        });
        if delivered.is_err() {
            break; // event loop is gone; app is shutting down
        }
    }
}

#[cfg(test)]
pub fn apply_for_test(laptop: &LaptopWindow, tv: &TvWindow, event: AppEvent) {
    STATE.with_borrow_mut(|s| apply(s, laptop, tv, event));
}

fn apply(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, event: AppEvent) {
    match event {
        AppEvent::Health { service, result } => status::record(s, laptop, service, result),
        AppEvent::KeyringStatus(result) => status::on_keyring(s, laptop, result),
        AppEvent::Account(kind, status) => {
            s.accounts.insert(kind, status);
            home::update_cw_state(s, laptop);
        }
        AppEvent::Server(kind, url) => {
            match url {
                Some(u) => s.servers.insert(kind, u),
                None => s.servers.remove(&kind),
            };
            accounts::refresh(s, laptop);
        }
        AppEvent::SignIn(kind, state) => accounts::on_sign_in(s, laptop, kind, state),
        AppEvent::ContinueWatching(kind, items) => {
            s.continue_watching.insert(kind, items);
            home::rebuild_rows(s);
            library::rebuild(s, laptop);
            home::update_cw_state(s, laptop);
            home::refresh_tv_card(s, laptop);
        }
        AppEvent::RecentlyAdded(kind, items) => {
            s.recently_added.insert(kind, items);
            library::rebuild(s, laptop);
        }
        AppEvent::SearchResults { query, items } => library::on_results(s, laptop, query, items),
        AppEvent::Poster { source, item_id, image } => home::on_poster(s, laptop, source, item_id, &image),
        AppEvent::Player(status) => home::on_player(s, laptop, tv, status),
        AppEvent::Browser(site) => home::on_browser(s, laptop, tv, site),
        AppEvent::NowPlaying(sessions) => home::on_sessions(s, sessions),
        AppEvent::Weather(weather) => status::on_weather(s, laptop, weather),
        AppEvent::Places { query, result } => status::on_places(s, laptop, &query, result),
        AppEvent::Location { name, timezone } => status::on_location(s, laptop, name, &timezone),
        AppEvent::Tmdb(status) => accounts::on_tmdb(s, laptop, status),
        AppEvent::WatchResults { query, hits, error } => crate::watch_ui::on_results(laptop, query, hits, error),
        AppEvent::WatchPoster { tmdb_id, is_tv, image } => crate::watch_ui::on_poster(laptop, tmdb_id, is_tv, image),
        AppEvent::SiteLogos(logos) => {
            s.logos = logos.into_iter().map(|(name, img)| (name, to_image(&img))).collect();
            home::refresh_tiles(s, laptop);
        }
        AppEvent::ProviderMatches { name, ids, names } => accounts::on_provider_matches(laptop, &name, &ids, &names),
        AppEvent::ServerSummary(summary) => {
            s.server_summary = Some(summary.clone());
            let server = laptop.global::<Server>();
            server.set_name(summary.name.as_str().into());
            server.set_streams(summary.streams as i32);
            server.set_transcodes(summary.transcodes as i32);
            server.set_bandwidth(summary.bandwidth_mbps as i32);
            server.set_cpu(summary.cpu_percent as i32);
            server.set_ram(summary.ram_percent as i32);
            server.set_cpu_host_path(summary.cpu_host_path.as_str().into());
            server.set_cpu_process_path(summary.cpu_process_path.as_str().into());
            server.set_ram_host_path(summary.ram_host_path.as_str().into());
            server.set_ram_process_path(summary.ram_process_path.as_str().into());
            server.set_bandwidth_lan_path(summary.bandwidth_lan_path.as_str().into());
            server.set_bandwidth_wan_path(summary.bandwidth_wan_path.as_str().into());
            server.set_bandwidth_scale(summary.bandwidth_scale as i32);
            server.set_status(summary.status.as_str().into());
            server.set_plex_pass(summary.plex_pass);
            let streams = summary
                .streams_detail
                .iter()
                .map(|row| ServerStreamRow {
                    session_id: if row.local { "this-tv" } else { row.session_id.as_deref().unwrap_or_default() }
                        .into(),
                    local: row.local,
                    title: row.title.as_str().into(),
                    device: row.device.as_str().into(),
                    detail: row.detail.as_str().into(),
                })
                .collect::<Vec<_>>();
            server.set_streams_detail(ModelRc::from(Rc::new(VecModel::from(streams))));
            let libraries = summary
                .libraries
                .iter()
                .map(|row| ServerLibraryRow {
                    title: row.title.as_str().into(),
                    count: row.item_count as i32,
                    scanned: row.scanned_ago.as_str().into(),
                    loading: row.refreshing,
                })
                .collect::<Vec<_>>();
            server.set_libraries(ModelRc::from(Rc::new(VecModel::from(libraries))));
        }
        AppEvent::ServerActivities(activities) => {
            let activity = activities
                .iter()
                .map(|row| ServerActivityRow {
                    uuid: row.uuid.as_str().into(),
                    title: row.title.as_str().into(),
                    subtitle: row.subtitle.as_str().into(),
                    progress: row.progress,
                    cancellable: row.cancellable,
                })
                .collect::<Vec<_>>();
            laptop.global::<Server>().set_activity(ModelRc::from(Rc::new(VecModel::from(activity))));
        }
        AppEvent::ServerError(error) => {
            laptop.global::<Server>().set_status(format!("● unavailable · {error}").into());
        }
        AppEvent::ArtworkChoices { item_id, backdrop, result } => {
            let artwork = laptop.global::<Artwork>();
            let selected_matches = s.selected.as_ref().is_some_and(|item| item.id == item_id);
            if selected_matches && artwork.get_open() && artwork.get_backdrop() == backdrop {
                artwork.set_loading(false);
                match result {
                    Ok(choices) => artwork.set_choices(ModelRc::from(Rc::new(VecModel::from(
                        choices.iter().map(artwork_row).collect::<Vec<_>>(),
                    )))),
                    Err(error) => artwork.set_error(error.into()),
                }
            }
        }
        AppEvent::MetadataRefreshFinished(item_id) => {
            if s.selected.as_ref().is_some_and(|item| item.id == item_id) {
                laptop.global::<Library>().set_metadata_refreshing(false);
            }
        }
        AppEvent::TvConnected(connected) => {
            laptop.global::<Dash>().set_tv_connected(connected);
        }
        AppEvent::DisplaysChanged => {
            crate::displays::on_displays_changed(laptop);
        }
        AppEvent::SystemColorScheme(dark) => {
            if s.system_dark != dark {
                s.system_dark = dark;
                if s.theme == ThemeMode::FollowSystem {
                    apply_theme(s, laptop, ThemeMode::FollowSystem);
                }
            }
        }
        AppEvent::Sites(sites) => {
            s.sites = sites;
            home::refresh_tiles(s, laptop);
            accounts::refresh(s, laptop);
        }
        AppEvent::Streaming(map) => {
            s.streaming = map;
            accounts::refresh(s, laptop);
        }
        AppEvent::ShowSetup => {
            laptop.global::<Setup>().set_step(0);
            laptop.global::<Nav>().set_page("setup".into());
        }
        AppEvent::Toast(msg) => show_toast(s, laptop, msg, TOAST_FOR),
        AppEvent::LongToast(msg) => show_toast(s, laptop, msg, LONG_TOAST_FOR),
    }
    crate::tv_nav::sync(laptop);
    if on_setup(laptop) {
        accounts::refresh_setup(s, laptop);
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn on_setup(laptop: &LaptopWindow) -> bool {
    laptop.global::<Nav>().get_page() == "setup"
}

fn artwork_row(choice: &ArtworkChoice) -> ArtworkRow {
    let image = choice.image.as_ref().map(to_image);
    ArtworkRow {
        key: choice.key.as_str().into(),
        provider: choice.provider.as_str().into(),
        selected: choice.selected,
        has_image: image.is_some(),
        image: image.unwrap_or_default(),
    }
}

/// Decoded pixels → a Slint image.
fn to_image(img: &RgbaImage) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&img.pixels, img.width, img.height))
}

/// Items from every source, in [`SOURCES`] order (keeps cards from jumping between refreshes).
fn in_order(map: &HashMap<SourceKind, Vec<MediaItem>>) -> Vec<MediaItem> {
    SOURCES.iter().filter_map(|k| map.get(k)).flatten().cloned().collect()
}

/// "just now", "25 min ago", "3 h ago".
fn ago(t: DateTime<Utc>) -> String {
    match (Utc::now() - t).num_minutes() {
        ..1 => "just now".into(),
        1 => "1 min ago".into(),
        m @ ..120 => format!("{m} min ago"),
        m => format!("{} h ago", m / 60),
    }
}

fn show_toast(s: &mut UiState, laptop: &LaptopWindow, msg: String, duration: Duration) {
    laptop.global::<Dash>().set_toast(msg.into());
    let weak = laptop.as_weak();
    let timer = Timer::default();
    timer.start(slint::TimerMode::SingleShot, duration, move || {
        if let Some(l) = weak.upgrade() {
            l.global::<Dash>().set_toast("".into());
        }
    });
    s.toast_timer = Some(timer); // replacing the old timer cancels it
}
