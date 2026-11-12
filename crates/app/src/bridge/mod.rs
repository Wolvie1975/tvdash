//! Moves `AppEvent`s from background tasks onto the UI thread and keeps the
//! UI-side state (rows, posters, accounts, library) that models are built from.
//!
//! Both layouts read the same globals (the TV-only shell is a view inside the
//! main window), so this one module feeds both. By concern:
//!
//! - [`home`]: Continue Watching cards, the On the TV card, sessions, tiles, posters
//! - [`library`]: the Library's shelves, Browse all and its detail panel
//! - [`accounts`]: Settings › Accounts and Streaming rows, the site editor, first-run setup
//! - [`status`]: weather, location, Settings › Diagnostics

mod accounts;
mod home;
mod library;
mod saver;
mod show;
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
    models::{AccountStatus, MediaItem, RgbaImage, SignInState, SourceKind, Weather},
};

pub use accounts::sync_setup;
pub use home::{row, tab_row};
pub use show::{open_show, select_season};
pub mod show_actions {
    pub use super::show::{episode, first, shuffle, up_next};
}
pub use library::{
    box_item, browse, browse_item, collection_details, collection_movie, collection_shuffle, collection_up_next,
    open_collection, open_item, pick, play_command, select, select_row, selected, set_collection_hide,
    set_collection_order, set_commands, set_genre, set_sort, set_tab, set_unwatched, shelf_item, shuffle,
};

use crate::{
    AccountPrompt, AccountRow, Accounts, Dash, Diagnostics, Display, LaptopWindow, Library, Nav, PosterCardData,
    SessionRow, Setup, SiteRow, Theme, TvWindow, UiTheme, tint::Hue,
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
    /// The ten Watch on TV apps, in tile order (same order as `APPS`).
    sites: Vec<StreamingSite>,
    /// What tvdash is showing on the TV: mpv playback, or the browser's site.
    player: Option<PlayerStatus>,
    browser: Option<String>,
    continue_watching: HashMap<SourceKind, Vec<MediaItem>>,
    recently_added: HashMap<SourceKind, Vec<MediaItem>>,
    /// Everything in progress, newest first (Up next, the TV layout); index = card index.
    rows: Vec<MediaItem>,
    /// The dashboard's two tabs of it.
    rows_tv: Vec<MediaItem>,
    rows_movies: Vec<MediaItem>,
    /// Full details (summary, genres, rating) of what the On the TV box shows.
    details: HashMap<(SourceKind, String), MediaItem>,
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
    /// Dominant hue of each poster, for tints.
    hues: HashMap<(SourceKind, String), Hue>,
    accounts: HashMap<SourceKind, AccountStatus>,
    servers: HashMap<SourceKind, String>,
    sign_in: HashMap<SourceKind, SignInState>,
    streaming: BTreeMap<String, DateTime<Utc>>,
    // Library
    lib: library::LibraryUi,
    show: show::ShowUi,
    lib_continue: Vec<MediaItem>,
    lib_recent: Vec<MediaItem>,
    selected: Option<MediaItem>,
    // Models
    cards: Rc<VecModel<PosterCardData>>,
    cards_tv: Rc<VecModel<PosterCardData>>,
    cards_movies: Rc<VecModel<PosterCardData>>,
    prompts: Rc<VecModel<AccountPrompt>>,
    tv_prompts: Rc<VecModel<AccountPrompt>>,
    sessions: Rc<VecModel<SessionRow>>,
    lib_continue_cards: Rc<VecModel<PosterCardData>>,
    lib_recent_cards: Rc<VecModel<PosterCardData>>,
    account_rows: Rc<VecModel<AccountRow>>,
    site_rows: Rc<VecModel<SiteRow>>,
    toast_timer: Option<Timer>,
    /// The screensaver's drift step.
    saver_step: usize,
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
    pub sites: Vec<StreamingSite>,
    pub library: tvdash_core::config::LibraryConfig,
}

/// Call once on the UI thread before the event loop starts.
pub fn init(laptop: &LaptopWindow, c: UiConfig) {
    STATE.with_borrow_mut(|s| {
        s.tz = c.tz;
        s.clock = Some(c.clock);
        s.units = Some(c.units);
        s.sites = c.sites;
        let dash = laptop.global::<Dash>();
        dash.set_continue_watching(ModelRc::from(s.cards.clone()));
        dash.set_cw_tv(ModelRc::from(s.cards_tv.clone()));
        dash.set_cw_movies(ModelRc::from(s.cards_movies.clone()));
        dash.set_prompts(ModelRc::from(s.prompts.clone()));
        dash.set_tv_prompts(ModelRc::from(s.tv_prompts.clone()));
        dash.set_sessions(ModelRc::from(s.sessions.clone()));
        library::init(s, laptop);
        library::set_sorts(s, laptop, c.library.sort_movies, c.library.sort_tv);
        let acc = laptop.global::<Accounts>();
        acc.set_accounts(ModelRc::from(s.account_rows.clone()));
        acc.set_sites(ModelRc::from(s.site_rows.clone()));
        let d = laptop.global::<Display>();
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
    STATE.with_borrow_mut(|s| {
        status::refresh_weather(s, laptop);
        status::refresh_diagnostics(s, laptop);
        home::refresh_tv_card(s, laptop);
        saver::drift(s, laptop);
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
        ThemeMode::Dark => UiTheme::Dark,
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
    laptop.global::<Theme>().set_mode(ui_theme(mode));
    home::refresh_tiles(s, laptop);
    accounts::refresh(s, laptop);
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
            laptop.global::<Library>().set_state(library::state(s).into());
        }
        AppEvent::Library(_, data) => library::on_library(s, laptop, data),
        AppEvent::Show(data) => show::on_show(s, laptop, data),
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
        AppEvent::Poster { source, item_id, image } => home::on_poster(s, laptop, source, item_id, &image),
        AppEvent::Details(item) => {
            s.details.insert((item.source, item.id.clone()), item);
            home::refresh_tv_card(s, laptop);
        }
        AppEvent::Player(status) => home::on_player(s, laptop, tv, status),
        AppEvent::Browser(site) => home::on_browser(s, laptop, tv, site),
        AppEvent::NowPlaying(sessions) => home::on_sessions(s, sessions),
        AppEvent::Weather(weather) => status::on_weather(s, laptop, weather),
        AppEvent::Places { query, result } => status::on_places(s, laptop, &query, result),
        AppEvent::Location { name, timezone } => status::on_location(s, laptop, name, &timezone),
        AppEvent::Tmdb(status) => accounts::on_tmdb(s, laptop, status),
        AppEvent::WatchResults { query, hits, error } => crate::watch_ui::on_results(laptop, query, hits, error),
        AppEvent::WatchPoster { tmdb_id, is_tv, image } => crate::watch_ui::on_poster(laptop, tmdb_id, is_tv, image),
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
