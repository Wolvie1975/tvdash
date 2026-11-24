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
pub mod alerts;
mod ambient;
pub use ambient::start_ambient;
mod home;
mod library;
pub use library::{ensure_library_loaded, on_season_selected, start_watching};
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
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tvdash_core::{
    AppEvent, Command,
    config::{ClockFormat, StreamingSite, ThemeMode, Units},
    events::{PlayerStatus, Service},
    models::{AccountStatus, ArtworkChoice, MediaItem, RgbaImage, ServerSummary, SignInState, SourceKind, Weather},
};

use crate::image_cache::{BACKDROP_CAP, FixedImageCache, POSTER_CAP, SPORTS_LOGO_CAP};

pub use accounts::sync_setup;
pub use home::{
    HeroAction, hero_action, just_added_item, player_duration_secs, row, set_cw_filter, site_index_by_id, up_next,
    up_next_item_by_key,
};

pub fn optimistic_toggle_star(laptop: &LaptopWindow, event_id: &str) {
    STATE.with_borrow_mut(|s| {
        let list = &mut s.sports_alerts.status.starred;
        if let Some(pos) = list.iter().position(|g| g.event_id == event_id) {
            list.remove(pos);
        } else {
            list.push(tvdash_core::events::StarredGameInfo {
                event_id: event_id.to_owned(),
                matchup: String::new(),
                meta: String::new(),
            });
        }
        if s.scoreboard_page.is_some() {
            refresh_scoreboard(s, laptop);
        }
    });
}
pub use library::{
    clear_search, close_detail, missing_browse_posters, on_library_focus_changed, reorder_collection, select, selected,
    set_browse_letter, set_filter, set_media_tab, start_search, watching_now_item,
};

use crate::{
    AccountPrompt, AccountRow, Accounts, Artwork, ArtworkRow, Dash, Diagnostics, Display, LaptopWindow, Library, Nav,
    PosterCardData, Scoreboard, ScoreboardChoiceData, ScoreboardRowData, Server, ServerActivityRow, ServerLibraryRow,
    ServerStreamRow, SessionRow, Setup, SiteRow, SleepState, Sports, SportsChoiceData, SportsRowData, Theme, TvWindow, UiTheme, tint::Hue,
};

/// Display order for sources everywhere (rows, prompts, sessions).
const SOURCES: [SourceKind; 2] = [SourceKind::Plex, SourceKind::Jellyfin];

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
    tv_layout: bool,
    night_active: bool,
    night_display: tvdash_core::config::DisplayConfig,
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
    posters: FixedImageCache<(SourceKind, String), POSTER_CAP>,
    cmd_tx: Option<UnboundedSender<Command>>,
    /// TMDB token state.
    tmdb: Option<AccountStatus>,
    /// Tautulli status.
    tautulli_configured: bool,
    tautulli_url: String,
    events_password_configured: bool,
    sports_page: Option<tvdash_core::models::SportsEventsPage>,
    scoreboard_page: Option<tvdash_core::models::ScoreboardPage>,
    sports_logos: FixedImageCache<String, SPORTS_LOGO_CAP>,
    collection_detail: Option<tvdash_core::models::CollectionDetail>,
    show_detail: Option<tvdash_core::models::ShowDetail>,
    ambient_starts: usize,
    next_ep: Option<MediaItem>,
    last_played: Option<LastPlayed>,
    hero_item: Option<MediaItem>,
    up_next_items: Vec<MediaItem>,
    just_added_item: Option<MediaItem>,
    next_game: Option<tvdash_core::events::NextGameDto>,
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
    recent: Vec<tvdash_core::config::RecentService>,
    // Library
    search: Option<(String, Vec<MediaItem>)>,
    lib_continue: Vec<MediaItem>,
    lib_recent: Vec<MediaItem>,
    lib_results: Vec<MediaItem>,
    lib_added_week: Vec<MediaItem>,
    lib_watch_again: Vec<MediaItem>,
    lib_browse_all: Vec<MediaItem>,
    lib_browse: Vec<MediaItem>,
    lib_finish_season: Vec<tvdash_core::models::FinishSeasonItem>,
    lib_shelf1_items: Vec<MediaItem>,
    lib_shelf2_items: Vec<MediaItem>,
    lib_shelf3_items: Vec<MediaItem>,
    lib_recently_released: Vec<MediaItem>,
    lib_focused_key: Option<(SourceKind, String)>,
    lib_shelves_loading: bool,
    backdrops: FixedImageCache<(SourceKind, String), BACKDROP_CAP>,
    backdrop_hues: HashMap<(SourceKind, String), Hue>,
    raw_sessions: Vec<tvdash_core::models::Session>,
    selected: Option<MediaItem>,
    // Models
    cards: Rc<VecModel<PosterCardData>>,
    prompts: Rc<VecModel<AccountPrompt>>,
    tv_prompts: Rc<VecModel<AccountPrompt>>,
    sessions: Rc<VecModel<SessionRow>>,
    lib_continue_cards: Rc<VecModel<PosterCardData>>,
    lib_recent_cards: Rc<VecModel<PosterCardData>>,
    lib_result_cards: Rc<VecModel<PosterCardData>>,
    lib_added_week_cards: Rc<VecModel<PosterCardData>>,
    lib_watch_again_cards: Rc<VecModel<PosterCardData>>,
    lib_browse_cards: Rc<VecModel<PosterCardData>>,
    account_rows: Rc<VecModel<AccountRow>>,
    site_rows: Rc<VecModel<SiteRow>>,
    toast_timer: Option<Timer>,
    sports_alerts: alerts::SportsAlertsUi,
}

impl UiState {
    pub(super) fn forget_images(
        &self,
        posters: Vec<(SourceKind, String)>,
        backdrops: Vec<(SourceKind, String)>,
        logos: Vec<String>,
    ) {
        if posters.is_empty() && backdrops.is_empty() && logos.is_empty() {
            return;
        }
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(Command::ForgetImages { posters, backdrops, logos });
        }
    }

    pub(super) fn effective_theme(&self) -> ThemeMode {
        if self.tv_layout && self.night_active {
            ThemeMode::Dark
        } else {
            self.theme.resolve(self.system_dark)
        }
    }
}

fn now_time(tz: Option<chrono_tz::Tz>) -> chrono::NaiveTime {
    match tz {
        Some(tz) => chrono::Utc::now().with_timezone(&tz).time(),
        None => chrono::Local::now().time(),
    }
}

pub fn set_tv_layout(laptop: &LaptopWindow, is_tv: bool) {
    STATE.with_borrow_mut(|s| {
        s.tv_layout = is_tv;
        home::rebuild_rows(s);
        s.night_active = s.night_display.is_night_now(now_time(s.tz));
        apply_theme(s, laptop, s.theme);
    });
}

pub fn set_night_config(laptop: &LaptopWindow, display: tvdash_core::config::DisplayConfig) {
    STATE.with_borrow_mut(|s| {
        s.night_display = display;
        s.night_active = s.night_display.is_night_now(now_time(s.tz));
        apply_theme(s, laptop, s.theme);
    });
}

/// Set the initial list of last-3 recent streaming services for TV-only v2.
pub fn set_recent_services(laptop: &LaptopWindow, recent: Vec<tvdash_core::config::RecentService>) {
    STATE.with_borrow_mut(|s| {
        s.recent = recent;
        home::refresh_tiles(s, laptop);
    });
}

pub fn set_command_sender(tx: UnboundedSender<Command>) {
    STATE.with_borrow_mut(|s| s.cmd_tx = Some(tx));
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
    pub dashboard_ui: tvdash_core::config::DashboardUi,
    pub tv_connected: bool,
    pub sites: Vec<StreamingSite>,
}

/// Call once on the UI thread before the event loop starts.
#[derive(Clone)]
pub enum LastPlayed {
    Media(MediaItem),
    Site(usize),
}

pub fn set_last_played_site(laptop: &LaptopWindow, site_index: usize) {
    STATE.with_borrow_mut(|s| {
        s.last_played = Some(LastPlayed::Site(site_index));
        home::refresh_last_played(s, laptop);
    });
}

pub fn last_played() -> Option<LastPlayed> {
    STATE.with_borrow(|s| {
        s.last_played.clone().or_else(|| {
            s.rows.first().cloned().map(LastPlayed::Media).or_else(|| {
                s.recent.first().and_then(|r| {
                    s.sites
                        .iter()
                        .position(|site| !site.hidden && crate::brand::service_id(&site.name) == r.id)
                        .map(LastPlayed::Site)
                })
            })
        })
    })
}

pub(crate) fn take_next_ep() -> Option<MediaItem> {
    STATE.with_borrow_mut(|s| s.next_ep.take())
}

pub(crate) fn clear_next_ep() {
    STATE.with_borrow_mut(|s| s.next_ep = None);
}

fn parse_changelog(md: &str) -> Vec<crate::ChangelogEntry> {
    let mut entries = Vec::new();
    let mut cur: Option<(String, String, Vec<String>)> = None;
    for raw in md.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some((version, date, lines)) = cur.take() {
                entries.push(crate::ChangelogEntry {
                    version: version.into(),
                    date: date.into(),
                    notes: lines.join("\n").into(),
                });
            }
            let (version, date) = rest
                .split_once(" — ")
                .or_else(|| rest.split_once(" - "))
                .map_or((rest, ""), |(v, d)| (v.trim(), d.trim()));
            cur = Some((version.to_string(), date.to_string(), Vec::new()));
        } else if let Some((_, _, ref mut lines)) = cur
            && !line.is_empty()
        {
            let text = line.strip_prefix("- ").map_or_else(|| line.to_string(), |s| format!("• {s}"));
            lines.push(text);
        }
    }
    if let Some((version, date, lines)) = cur {
        entries.push(crate::ChangelogEntry {
            version: version.into(),
            date: date.into(),
            notes: lines.join("\n").into(),
        });
    }
    entries
}

pub fn init(laptop: &LaptopWindow, c: UiConfig) {
    STATE.with_borrow_mut(|s| {
        s.tz = c.tz;
        s.clock = Some(c.clock);
        s.units = Some(c.units);
        s.system_dark = true;
        s.cw_filter = "tv".into();
        s.sites = c.sites;
        let dash = laptop.global::<Dash>();
        dash.set_version(env!("CARGO_PKG_VERSION").into());
        dash.set_changelog(ModelRc::new(VecModel::from(parse_changelog(include_str!(
            "../../../../CHANGELOG.md"
        )))));
        dash.set_tv_connected(c.tv_connected);
        dash.set_continue_watching(ModelRc::from(s.cards.clone()));
        dash.set_prompts(ModelRc::from(s.prompts.clone()));
        dash.set_tv_prompts(ModelRc::from(s.tv_prompts.clone()));
        dash.set_sessions(ModelRc::from(s.sessions.clone()));
        let lib = laptop.global::<Library>();
        lib.set_continue_watching(ModelRc::from(s.lib_continue_cards.clone()));
        lib.set_recently_added(ModelRc::from(s.lib_recent_cards.clone()));
        lib.set_results(ModelRc::from(s.lib_result_cards.clone()));
        lib.set_added_week(ModelRc::from(s.lib_added_week_cards.clone()));
        lib.set_watch_again(ModelRc::from(s.lib_watch_again_cards.clone()));
        lib.set_browse_items(ModelRc::from(s.lib_browse_cards.clone()));
        let acc = laptop.global::<Accounts>();
        acc.set_accounts(ModelRc::from(s.account_rows.clone()));
        acc.set_sites(ModelRc::from(s.site_rows.clone()));
        let d = laptop.global::<Display>();
        d.set_layout_setting(c.layout_setting.as_str().into());
        d.set_dashboard_ui(c.dashboard_ui.as_str().into());
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

/// Once a minute: ages ("updated 25 min ago"), end times, screensaver burn-in drift.
pub fn on_minute(laptop: &LaptopWindow, _tv: &TvWindow) {
    STATE.with_borrow_mut(|s| {
        let night = s.night_display.is_night_now(now_time(s.tz));
        if night != s.night_active {
            s.night_active = night;
            apply_theme(s, laptop, s.theme);
        }
        status::refresh_weather(s, laptop);
        status::refresh_diagnostics(s, laptop);
        home::refresh_tv_card(s, laptop);
    });
    let min = chrono::Local::now().format("%M").to_string().parse::<i32>().unwrap_or(0);
    // Gentle deterministic drift (±8 px x, ±6 px y) to prevent burn-in on the screensaver.
    let dx = ((min * 7) % 17) - 8;
    let dy = ((min * 5) % 13) - 6;
    let dash = laptop.global::<Dash>();
    dash.set_screensaver_dx(dx);
    dash.set_screensaver_dy(dy);
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
        ThemeMode::Aubergine => UiTheme::Aubergine,
        ThemeMode::Ku => UiTheme::Ku,
        ThemeMode::Stone => UiTheme::Stone,
        ThemeMode::Mist => UiTheme::Mist,
    }
}

pub fn theme_mode(theme: UiTheme) -> ThemeMode {
    match theme {
        UiTheme::Aubergine => ThemeMode::Aubergine,
        UiTheme::Ku => ThemeMode::Ku,
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

pub async fn forward(
    mut rx: UnboundedReceiver<AppEvent>,
    laptop: Weak<LaptopWindow>,
    tv: Weak<TvWindow>,
    alert_http_tx: tokio::sync::watch::Sender<Option<crate::home_cmd::AlertHttpState>>,
    alert_status_tx: tokio::sync::watch::Sender<String>,
) {
    {
        let (l, t) = (laptop.clone(), tv.clone());
        let _ = slint::invoke_from_event_loop(move || {
            alerts::set_channels(alert_http_tx, alert_status_tx);
            alerts::start_timer(l, t);
        });
    }
    while let Some(event) = rx.recv().await {
        let (laptop, tv) = (laptop.clone(), tv.clone());
        let delivered = slint::invoke_from_event_loop(move || {
            if let (Some(laptop), Some(tv)) = (laptop.upgrade(), tv.upgrade()) {
                STATE.with_borrow_mut(|s| apply(s, &laptop, &tv, event));
                crate::tv_nav::sync(&laptop);
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
    crate::tv_nav::sync(laptop);
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
            if kind == SourceKind::Jellyfin
                && let Some(u) = &url
            {
                laptop.global::<Setup>().set_jellyfin_url(u.as_str().into());
            }
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
            home::refresh_last_played(s, laptop);
            crate::intro::mark_data_ready(laptop);
        }
        AppEvent::RecentlyAdded(kind, items) => {
            s.recently_added.insert(kind, items);
            library::rebuild(s, laptop);
            home::refresh_home_state(s, laptop);
        }
        AppEvent::SearchResults { query, items } => library::on_results(s, laptop, query, items),
        AppEvent::Poster { source, item_id, image } => home::on_poster(s, laptop, source, item_id, &image),
        AppEvent::Backdrop { source, item_id, image } => library::on_backdrop(s, laptop, source, item_id, &image),
        AppEvent::Player(status) => {
            let is_plex = status.as_ref().is_some_and(|p| p.item.source == SourceKind::Plex);
            let is_movie = status.as_ref().is_some_and(|p| p.item.kind == tvdash_core::models::MediaKind::Movie);
            let caption = status.as_ref().map(|p| {
                let rem_min = p
                    .duration_ms
                    .or(p.item.duration_ms)
                    .map(|d| d.saturating_sub(p.position_ms) / 60_000)
                    .unwrap_or(0);
                if let Some(sub) = p.item.subtitle.as_deref().filter(|s| !s.is_empty()) {
                    format!("{sub} · {rem_min} min left")
                } else {
                    format!("{rem_min} min left")
                }
            }).unwrap_or_default();
            for sl in [laptop.global::<SleepState>(), tv.global::<SleepState>()] {
                sl.set_end_of_item_ok(is_plex);
                sl.set_is_movie(is_movie);
                sl.set_item_caption(caption.clone().into());
            }
            home::on_player(s, laptop, tv, status);
        }
        AppEvent::Browser(site) => home::on_browser(s, laptop, tv, site),
        AppEvent::NowPlaying(sessions) => {
            library::on_sessions(s, laptop, &sessions);
            home::on_sessions(s, sessions);
        }
        AppEvent::Weather(weather) => {
            status::on_weather(s, laptop, weather);
            crate::intro::mark_data_ready(laptop);
        }
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
        AppEvent::Tautulli { configured, url } => {
            s.tautulli_configured = configured;
            s.tautulli_url = url;
            accounts::refresh(s, laptop);
        }
        AppEvent::LibraryShelves(shelves) => {
            library::on_shelves(s, laptop, shelves);
        }
        AppEvent::ShowDetail(res) => {
            library::on_show_detail(s, laptop, res);
        }
        AppEvent::SeasonEpisodes { season_key, result } => {
            library::on_season_episodes(s, laptop, &season_key, result);
        }
        AppEvent::NextEpisodePrompt(opt) => {
            s.next_ep = opt.clone();
            let dash = laptop.global::<Dash>();
            if let Some(item) = opt {
                let poster = s.posters.get(&(item.source, item.id.clone()));
                dash.set_next_ep_title(item.title.as_str().into());
                dash.set_next_ep_subtitle(crate::format::item_detail(&item).into());
                dash.set_next_ep_has_poster(poster.is_some());
                dash.set_next_ep_poster(poster.unwrap_or_default());
                dash.set_next_ep_open(true);
                laptop.global::<crate::TvUi>().set_focused_tile(0);
            } else {
                dash.set_next_ep_open(false);
            }
        }
        AppEvent::CollectionDetail(res) => {
            library::on_collection_detail(s, laptop, res);
        }
        AppEvent::SportsEvents(res) => {
            let sp = laptop.global::<Sports>();
            sp.set_loading(false);
            match res {
                Ok(page) => {
                    sp.set_error("".into());
                    s.sports_page = Some(page);
                    refresh_sports(s, laptop);
                }
                Err(msg) => {
                    sp.set_error(msg.into());
                }
            }
        }
        AppEvent::SportsLogo { url, image } => {
            let evicted = s.sports_logos.insert(url, to_image(&image));
            s.forget_images(Vec::new(), Vec::new(), evicted);
            if s.sports_page.is_some() {
                refresh_sports(s, laptop);
            }
            if s.scoreboard_page.is_some() {
                refresh_scoreboard(s, laptop);
            }
            if s.next_game.is_some() {
                home::refresh_next_game(s, laptop);
            }
        }
        AppEvent::EventsPasswordConfigured(configured) => {
            s.events_password_configured = configured;
            accounts::refresh(s, laptop);
        }
        AppEvent::Scoreboard(res) => {
            let sb = laptop.global::<Scoreboard>();
            sb.set_loading(false);
            match res {
                Ok(page) => {
                    sb.set_error("".into());
                    s.scoreboard_page = Some(page);
                    refresh_scoreboard(s, laptop);
                }
                Err(_) => {
                    sb.set_error("Scores are temporarily unavailable. Please retry.".into());
                }
            }
        }
        AppEvent::Sleep(snap) => {
            let clock_str = format!("{}:{:02}", (snap.left.max(0)) / 60, (snap.left.max(0)) % 60);
            let sel = match snap.phase.as_str() {
                "off" => 7,
                "end_of_item" => 6,
                _ => -1,
            };
            for sl in [laptop.global::<SleepState>(), tv.global::<SleepState>()] {
                sl.set_active(snap.active);
                sl.set_phase(snap.phase.clone().into());
                sl.set_label(snap.label.clone().into());
                sl.set_ends_at(snap.ends_at.clone().into());
                sl.set_left(snap.left as i32);
                sl.set_warn_total(snap.warn_total as i32);
                sl.set_warn_clock(clock_str.clone().into());
                sl.set_episode_finished(snap.episode_finished);
                sl.set_held(snap.held);
                sl.set_default_minutes(snap.default_minutes as i32);
                if sel >= 0 {
                    sl.set_selected_tile(sel);
                }
                if snap.phase == "warn" {
                    sl.set_warn_focus(0);
                }
            }
        }
        AppEvent::SportsAlerts(list) => alerts::on_alerts(s, laptop, tv, list),
        AppEvent::NextGame(dto) => home::on_next_game(s, laptop, dto),
        AppEvent::SportsAlertsStatus(st) => {
            alerts::on_status(s, laptop, st);
            if s.scoreboard_page.is_some() {
                refresh_scoreboard(s, laptop);
            }
            if s.next_game.is_some() {
                home::refresh_next_game(s, laptop);
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
        AppEvent::RecentServices(recent) => {
            s.recent = recent;
            home::refresh_tiles(s, laptop);
        }
        AppEvent::ShowSetup => {
            laptop.global::<Setup>().set_step(0);
            laptop.global::<Nav>().set_page("setup".into());
        }
        AppEvent::Toast(msg) => show_toast(s, laptop, msg, TOAST_FOR),
        AppEvent::LongToast(msg) => show_toast(s, laptop, msg, LONG_TOAST_FOR),
    }
    if on_setup(laptop) {
        accounts::refresh_setup(s, laptop);
    }
}

/// Map a Sports/Scoreboard filter choice value to an ESPN CDN league (or NCAA) logo URL.
pub(crate) fn league_logo_url(value: &str) -> Option<&'static str> {
    let lower = value.to_ascii_lowercase();
    if lower == "top25" {
        return Some("https://a.espncdn.com/i/espn/misc_logos/500/ncaa.png");
    }
    if lower.contains("nfl") {
        return Some("https://a.espncdn.com/i/teamlogos/leagues/500/nfl.png");
    }
    if lower.contains("wnba") {
        return Some("https://a.espncdn.com/i/teamlogos/leagues/500/wnba.png");
    }
    if lower.contains("nba") {
        return Some("https://a.espncdn.com/i/teamlogos/leagues/500/nba.png");
    }
    if lower.contains("mlb") {
        return Some("https://a.espncdn.com/i/teamlogos/leagues/500/mlb.png");
    }
    if lower.contains("nhl") {
        return Some("https://a.espncdn.com/i/teamlogos/leagues/500/nhl.png");
    }
    if lower.contains("mls") || lower.contains("usa.1") {
        return Some("https://a.espncdn.com/i/teamlogos/leagues/500/mls.png");
    }
    if lower.contains("epl") || lower.contains("eng.1") {
        return Some("https://a.espncdn.com/i/leaguelogos/soccer/500/23.png");
    }
    if lower.contains("efl") || lower.contains("eng.2") {
        return Some("https://a.espncdn.com/i/leaguelogos/soccer/500/24.png");
    }
    if lower.contains("uefa.champions") || lower.contains("ucl") {
        return Some("https://a.espncdn.com/i/leaguelogos/soccer/500/2.png");
    }
    if lower.contains("college-football") || lower.contains("mens-college-basketball") || lower.contains("womens-college-basketball") || lower.contains("big12") {
        return Some("https://a.espncdn.com/i/espn/misc_logos/500/ncaa.png");
    }
    None
}

fn refresh_sports(s: &UiState, laptop: &LaptopWindow) {
    let Some(page) = &s.sports_page else { return };
    let sp = laptop.global::<Sports>();
    let choices: Vec<SportsChoiceData> = page
        .choices
        .iter()
        .map(|c| {
            let logo = league_logo_url(&c.value).and_then(|u| s.sports_logos.peek(&u.to_string()));
            SportsChoiceData {
                value: c.value.as_str().into(),
                label: c.label.as_str().into(),
                has_logo: logo.is_some(),
                logo: logo.cloned().unwrap_or_default(),
            }
        })
        .collect();
    let label = page
        .choices
        .iter()
        .find(|c| c.value == page.schedule)
        .map(|c| c.label.as_str())
        .unwrap_or("All schedules");

    let mut rows = Vec::new();
    for day in &page.days {
        rows.push(SportsRowData {
            kind: "day".into(),
            header_text: day.date_heading.as_str().into(),
            ..Default::default()
        });
        for lg in &day.leagues {
            rows.push(SportsRowData {
                kind: "league".into(),
                header_text: lg.league.as_str().into(),
                ..Default::default()
            });
            for g in &lg.games {
                let away_logo = g.away_logo_url.as_ref().and_then(|u| s.sports_logos.peek(u));
                let home_logo = g.home_logo_url.as_ref().and_then(|u| s.sports_logos.peek(u));
                rows.push(SportsRowData {
                    kind: "game".into(),
                    header_text: Default::default(),
                    sport: g.sport.as_str().into(),
                    is_neutral: g.is_neutral,
                    away_name: g.away_name.as_str().into(),
                    away_label: g.away_label.as_str().into(),
                    away_has_logo: away_logo.is_some(),
                    away_logo: away_logo.cloned().unwrap_or_default(),
                    away_initials: g.away_initials.as_str().into(),
                    home_name: g.home_name.as_str().into(),
                    home_label: g.home_label.as_str().into(),
                    home_has_logo: home_logo.is_some(),
                    home_logo: home_logo.cloned().unwrap_or_default(),
                    home_initials: g.home_initials.as_str().into(),
                    time_text: g.time_text.as_str().into(),
                    is_tbd: g.is_tbd,
                    city: g.city.as_str().into(),
                    venue: g.venue.as_str().into(),
                    tv: g.tv.as_str().into(),
                    radio: g.radio.as_str().into(),
                    stats_url: g.stats_url.as_deref().unwrap_or_default().into(),
                    stream_url: g.stream_url.as_deref().unwrap_or_default().into(),
                    audio_url: g.audio_url.as_deref().unwrap_or_default().into(),
                    details_url: g.details_url.as_deref().unwrap_or_default().into(),
                });
            }
        }
    }

    sp.set_schedule(page.schedule.as_str().into());
    sp.set_schedule_label(label.into());
    sp.set_choices(ModelRc::new(VecModel::from(choices)));
    sp.set_total_count(page.total_count as i32);
    sp.set_page(page.page as i32);
    sp.set_total_pages(page.total_pages as i32);
    sp.set_last_updated(page.last_updated.as_str().into());
    sp.set_rows(ModelRc::new(VecModel::from(rows)));
}

fn refresh_scoreboard(s: &UiState, laptop: &LaptopWindow) {
    let Some(page) = &s.scoreboard_page else { return };
    let sb = laptop.global::<Scoreboard>();
    let choices: Vec<ScoreboardChoiceData> = page
        .choices
        .iter()
        .map(|c| {
            let logo = league_logo_url(&c.value).and_then(|u| s.sports_logos.peek(&u.to_string()));
            ScoreboardChoiceData {
                value: c.value.as_str().into(),
                label: c.label.as_str().into(),
                group: c.group.as_str().into(),
                has_logo: logo.is_some(),
                logo: logo.cloned().unwrap_or_default(),
            }
        })
        .collect();
    let show_label = page
        .choices
        .iter()
        .find(|c| c.value == page.show)
        .map(|c| c.label.as_str())
        .unwrap_or("All sports");

    let mut rows = Vec::new();
    let mut next_focus_id = 2i32;
    for group in &page.groups {
        rows.push(ScoreboardRowData {
            kind: "group".into(),
            focus_id: -1,
            group_title: group.title.as_str().into(),
            group_subtitle: group.subtitle.as_str().into(),
            is_live: group.is_live,
            ..Default::default()
        });
        for g in &group.games {
            let away_logo = g.away_logo_url.as_ref().and_then(|u| s.sports_logos.peek(u));
            let home_logo = g.home_logo_url.as_ref().and_then(|u| s.sports_logos.peek(u));
            let away_display = match g.away_rank {
                Some(r) => format!("#{r} {}", g.away_name),
                None => g.away_name.clone(),
            };
            let home_display = match g.home_rank {
                Some(r) => format!("#{r} {}", g.home_name),
                None => g.home_name.clone(),
            };
            let is_starred = s
                .sports_alerts
                .status
                .starred
                .iter()
                .any(|st| st.event_id == g.event_id);
            let is_following = s.sports_alerts.status.followed.iter().any(|f| {
                f.league == g.league && (f.id == g.away_team_id || f.id == g.home_team_id)
            });
            let focus_id = next_focus_id;
            next_focus_id += 1;
            rows.push(ScoreboardRowData {
                kind: "game".into(),
                focus_id,
                event_id: g.event_id.as_str().into(),
                is_starred,
                is_following,
                league_label: g.league_label.as_str().into(),
                status: g.status.as_str().into(),
                header_text: g.header_text.as_str().into(),
                away_name: away_display.into(),
                away_score: g.away_score.as_str().into(),
                away_winner: g.away_winner,
                away_leading: g.away_leading,
                away_has_logo: away_logo.is_some(),
                away_logo: away_logo.cloned().unwrap_or_default(),
                away_initials: g.away_initials.as_str().into(),
                home_name: home_display.into(),
                home_score: g.home_score.as_str().into(),
                home_winner: g.home_winner,
                home_leading: g.home_leading,
                home_has_logo: home_logo.is_some(),
                home_logo: home_logo.cloned().unwrap_or_default(),
                home_initials: g.home_initials.as_str().into(),
                details_text: g.details_text.as_str().into(),
                warning_text: g.warning_text.as_str().into(),
                source_url: g.source_url.as_deref().unwrap_or_default().into(),
                ..Default::default()
            });
        }
    }

    sb.set_day(page.day.as_str().into());
    sb.set_day_label(page.day_label.as_str().into());
    sb.set_prev_day(page.prev_day.as_str().into());
    sb.set_next_day(page.next_day.as_str().into());
    sb.set_is_today(page.is_today);
    sb.set_show(page.show.as_str().into());
    sb.set_show_label(show_label.into());
    sb.set_final_only(page.final_only);
    sb.set_choices(ModelRc::new(VecModel::from(choices)));
    sb.set_coverage_warning(page.coverage_warning.as_str().into());
    sb.set_empty_message(page.empty_message.as_str().into());
    sb.set_rows(ModelRc::new(VecModel::from(rows)));

    // Build shared ScoreboardView (§10)
    let starred_set: std::collections::HashSet<String> = s
        .sports_alerts
        .status
        .starred
        .iter()
        .map(|st| st.event_id.clone())
        .collect();
    let follows_vec: Vec<(String, String)> = s
        .sports_alerts
        .status
        .followed
        .iter()
        .map(|f| (f.league.clone(), f.id.clone()))
        .collect();
    let tz = s.tz.unwrap_or(chrono_tz::America::Chicago);
    let now_local = chrono::Utc::now().with_timezone(&tz);
    let clock = s.clock.unwrap_or(tvdash_core::config::ClockFormat::H24);
    let view = crate::scoreboard_view::build_view(page, &starred_set, &follows_vec, &page.show, now_local, clock);

    let mut pill_id = 7i32; // 0..1 tabs, 2..6 controls, 7.. card pills
    let slint_groups: Vec<crate::ScoreGroup> = view
        .groups
        .into_iter()
        .map(|grp| {
            let cards: Vec<crate::ScoreCard> = grp
                .cards
                .into_iter()
                .map(|c| {
                    let star_focus_id = pill_id;
                    pill_id += 1;
                    let has_url = c.game_url.as_ref().is_some_and(|u| !u.is_empty());
                    let game_focus_id = if has_url {
                        let id = pill_id;
                        pill_id += 1;
                        id
                    } else {
                        -1
                    };
                    let to_side = |side: crate::scoreboard_view::SideView| {
                        let logo = side.logo_url.as_ref().and_then(|u| s.sports_logos.peek(u)).cloned();
                        let periods: Vec<slint::SharedString> =
                            side.periods.into_iter().map(Into::into).collect();
                        crate::ScoreSide {
                            rank: side.rank.into(),
                            short: side.short.into(),
                            full: side.full.into(),
                            initials: side.initials.into(),
                            has_logo: logo.is_some(),
                            logo: logo.unwrap_or_default(),
                            periods: ModelRc::new(VecModel::from(periods)),
                            total: side.total.into(),
                            emphasis: match side.emphasis {
                                crate::scoreboard_view::Emphasis::Lead => crate::ScoreEmphasis::Lead,
                                crate::scoreboard_view::Emphasis::Trail => crate::ScoreEmphasis::Trail,
                                crate::scoreboard_view::Emphasis::Even => crate::ScoreEmphasis::Even,
                            },
                        }
                    };
                    let p_labels: Vec<slint::SharedString> =
                        c.period_labels.into_iter().map(Into::into).collect();
                    crate::ScoreCard {
                        event_id: c.event_id.into(),
                        league: c.league.into(),
                        status: c.status.into(),
                        tone: match c.tone {
                            crate::scoreboard_view::StatusTone::Live => crate::ScoreTone::Live,
                            crate::scoreboard_view::StatusTone::Upcoming => crate::ScoreTone::Upcoming,
                            crate::scoreboard_view::StatusTone::Final => crate::ScoreTone::Final,
                            crate::scoreboard_view::StatusTone::Muted => crate::ScoreTone::Muted,
                        },
                        period_labels: ModelRc::new(VecModel::from(p_labels)),
                        away: to_side(c.away),
                        home: to_side(c.home),
                        meta: c.meta.into(),
                        starred: c.starred,
                        following: c.following,
                        game_url: c.game_url.unwrap_or_default().into(),
                        star_focus_id,
                        game_focus_id,
                    }
                })
                .collect();
            crate::ScoreGroup {
                is_live: grp.kind == crate::scoreboard_view::GroupKind::Live,
                is_postponed: grp.kind == crate::scoreboard_view::GroupKind::Postponed,
                title: grp.title.into(),
                count: grp.count.into(),
                cards: ModelRc::new(VecModel::from(cards)),
            }
        })
        .collect();

    sb.set_date_label(view.date_label.into());
    sb.set_fresh(view.fresh.into());
    sb.set_stale(view.stale);
    sb.set_groups(ModelRc::new(VecModel::from(slint_groups)));
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
