//! Moves `AppEvent`s from background tasks onto the UI thread and keeps the
//! UI-side state (rows, posters, accounts, library) that models are built from.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, VecModel, Weak};
use tokio::sync::mpsc::UnboundedReceiver;
use tvdash_core::{
    AppEvent,
    config::{ClockFormat, StreamingSite, ThemeMode, Units},
    events::{PlayerStatus, Service},
    models::{AccountStatus, MediaItem, PlayState, Session, SignInState, SourceKind, Weather},
};

use crate::{
    AccountPrompt, AccountRow, Accounts, Dash, DetailData, Diagnostics, Display, HealthRow, LaptopWindow, Library, Nav,
    PosterCardData, SessionRow, Setup, SiteEditor, SiteRow, SiteTileData, Theme, TvCard, TvWindow, UiTheme, format,
    tint::{self, Hue},
};

/// Display order for sources everywhere (rows, prompts, sessions).
const SOURCES: [SourceKind; 2] = [SourceKind::Jellyfin, SourceKind::Plex];

const TOAST_FOR: Duration = Duration::from_secs(5);
const LONG_TOAST_FOR: Duration = Duration::from_secs(30);

/// UI-thread state. Only touched from the Slint event loop.
#[derive(Default)]
pub struct UiState {
    pub tz: Option<chrono_tz::Tz>,
    pub clock: Option<ClockFormat>,
    units: Option<Units>,
    theme: ThemeMode,
    /// Streaming sites in tile order, hidden ones included.
    sites: Vec<StreamingSite>,
    /// What tvdash is showing on the TV: mpv playback, or the browser's site.
    player: Option<PlayerStatus>,
    browser: Option<String>,
    continue_watching: HashMap<SourceKind, Vec<MediaItem>>,
    recently_added: HashMap<SourceKind, Vec<MediaItem>>,
    /// Dashboard Continue Watching rows; index = card index.
    rows: Vec<MediaItem>,
    posters: HashMap<(SourceKind, String), Image>,
    /// TMDB token state.
    tmdb: Option<AccountStatus>,
    /// Last weather and when it arrived; the last fetch error, if the latest fetch failed.
    weather: Option<(Weather, DateTime<Utc>)>,
    weather_err: Option<String>,
    /// Settings › Diagnostics: last success and last error per service.
    health: HashMap<Service, Health>,
    /// Service logos (from TMDB) by site name.
    logos: HashMap<String, Image>,
    /// Dominant hue of each poster, for tints.
    hues: HashMap<(SourceKind, String), Hue>,
    accounts: HashMap<SourceKind, AccountStatus>,
    servers: HashMap<SourceKind, String>,
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

thread_local! {
    pub static STATE: RefCell<UiState> = RefCell::new(UiState::default());
}

/// Last success and failure of a service, with when.
#[derive(Default, Clone)]
struct Health {
    ok: Option<(DateTime<Utc>, String)>,
    err: Option<(DateTime<Utc>, String)>,
}

/// Weather older than this is shown dimmed with its age (it refreshes every 10 min).
const WEATHER_STALE: chrono::TimeDelta = chrono::TimeDelta::minutes(20);

/// Settings the UI needs from the config.
pub struct UiConfig {
    pub tz: Option<chrono_tz::Tz>,
    pub clock: ClockFormat,
    pub seconds: bool,
    /// "Log file: …" lines for Settings › Diagnostics.
    pub paths: Vec<String>,
    pub units: Units,
    pub theme: ThemeMode,
    pub sites: Vec<StreamingSite>,
    pub jellyfin_url: String,
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
        laptop.global::<Setup>().set_jellyfin_url(c.jellyfin_url.as_str().into());
        let d = laptop.global::<Display>();
        d.set_celsius(c.units == Units::Celsius);
        d.set_h12(c.clock == ClockFormat::H12);
        d.set_seconds(c.seconds);
        let paths: Vec<slint::SharedString> = c.paths.iter().map(|p| p.as_str().into()).collect();
        laptop.global::<Diagnostics>().set_paths(ModelRc::new(VecModel::from(paths)));
        refresh_diagnostics(s, laptop);
        apply_theme(s, laptop, c.theme);
        refresh_accounts(s, laptop);
    });
}

/// Settings › Display changed: re-render times and temperatures (the weather
/// poller fetches again in the new units by itself).
pub fn set_display(laptop: &LaptopWindow, units: Units, clock: ClockFormat, seconds: bool) {
    crate::clock::set_format(clock, seconds);
    STATE.with_borrow_mut(|s| {
        s.units = Some(units);
        s.clock = Some(clock);
        refresh_tv_card(s, laptop);
        refresh_diagnostics(s, laptop);
    });
}

/// Once a minute: ages ("updated 25 min ago"), end times.
pub fn on_minute(laptop: &LaptopWindow, _tv: &TvWindow) {
    STATE.with_borrow(|s| {
        refresh_weather(s, laptop);
        refresh_diagnostics(s, laptop);
        refresh_tv_card(s, laptop);
    });
    crate::tv_nav::tick(laptop);
}

fn ago(t: DateTime<Utc>) -> String {
    match (Utc::now() - t).num_minutes() {
        ..1 => "just now".into(),
        1 => "1 min ago".into(),
        m @ ..120 => format!("{m} min ago"),
        m => format!("{} h ago", m / 60),
    }
}

fn refresh_weather(s: &UiState, laptop: &LaptopWindow) {
    let dash = laptop.global::<Dash>();
    let warn = s.weather_err.is_some();
    dash.set_weather_warn(warn);
    match &s.weather {
        None => {
            if let Some(e) = &s.weather_err {
                dash.set_weather_on(true);
                dash.set_weather_temp("—".into());
                dash.set_weather_stale(true);
                dash.set_weather_note(format!("Unavailable: {e}").into());
            }
        }
        Some((w, at)) => {
            let c = format::weather_chip(w, s.units.unwrap_or(Units::Fahrenheit));
            dash.set_weather_on(true);
            dash.set_weather_temp(c.value.into());
            dash.set_weather_detail(c.detail.into());
            dash.set_weather_icon(format::weather_icon(w.code).into());
            let stale = Utc::now() - *at > WEATHER_STALE;
            dash.set_weather_stale(stale);
            dash.set_weather_note(if stale { format!("Updated {}", ago(*at)) } else { String::new() }.into());
        }
    }
}

fn refresh_diagnostics(s: &UiState, laptop: &LaptopWindow) {
    let clock = s.clock.unwrap_or(ClockFormat::H24);
    let at = |t: &DateTime<Utc>| {
        let local = match s.tz {
            Some(tz) => t.with_timezone(&tz).naive_local(),
            None => t.with_timezone(&chrono::Local).naive_local(),
        };
        format!("{} · {}", format::hm(local, clock), ago(*t))
    };
    let rows: Vec<HealthRow> = Service::ALL
        .iter()
        .map(|svc| {
            let h = s.health.get(svc).cloned().unwrap_or_default();
            let failing = match (&h.ok, &h.err) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some((ok, _)), Some((err, _))) => err > ok,
            };
            HealthRow {
                name: svc.label().into(),
                state: if failing {
                    "warn"
                } else if h.ok.is_some() {
                    "ok"
                } else {
                    "off"
                }
                .into(),
                last_ok: h.ok.as_ref().map(|(t, note)| format!("{} · {note}", at(t))).unwrap_or_default().into(),
                last_error: h.err.as_ref().map(|(t, e)| format!("{} · {e}", at(t))).unwrap_or_default().into(),
            }
        })
        .collect();
    laptop.global::<Diagnostics>().set_rows(ModelRc::new(VecModel::from(rows)));
}

/// Switch the laptop's colours (the TV window keeps its own, dark, Theme).
pub fn set_theme(laptop: &LaptopWindow, mode: ThemeMode) {
    STATE.with_borrow_mut(|s| apply_theme(s, laptop, mode));
}

pub fn ui_theme(mode: ThemeMode) -> UiTheme {
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
    refresh_tiles(s, laptop);
    rebuild_rows(s);
    rebuild_library(s, laptop);
    refresh_tv_card(s, laptop);
}

/// The sites as the UI knows them (for the editor).
pub fn sites() -> Vec<StreamingSite> {
    STATE.with_borrow(|s| s.sites.clone())
}

/// (background, foreground) of a site's monogram in the current theme.
fn site_colors(s: &UiState, index: usize) -> (slint::Color, slint::Color) {
    let name = s.sites.get(index).map(|x| x.name.as_str()).unwrap_or_default();
    tint::site_colors(tint::site_hue(name, index), s.theme)
}

fn refresh_tiles(s: &UiState, laptop: &LaptopWindow) {
    let tiles: Vec<SiteTileData> = s
        .sites
        .iter()
        .enumerate()
        .filter(|(_, site)| !site.hidden)
        .map(|(i, site)| {
            let (bg, fg) = site_colors(s, i);
            let logo = s.logos.get(&site.name);
            SiteTileData {
                index: i as i32,
                name: site.name.as_str().into(),
                monogram: monogram(&site.name).into(),
                bg,
                fg,
                has_logo: logo.is_some(),
                logo: logo.cloned().unwrap_or_default(),
            }
        })
        .collect();
    laptop.global::<Dash>().set_streaming_sites(ModelRc::new(VecModel::from(tiles)));
}

/// First letter of a site name ("Disney+" → "D", "Apple TV+" → "A").
fn monogram(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// The item behind dashboard Continue Watching card `index`.
pub fn row(index: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| s.rows.get(index).cloned())
}

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

fn on_setup(laptop: &LaptopWindow) -> bool {
    laptop.global::<Nav>().get_page() == "setup"
}

#[cfg(test)]
pub fn apply_for_test(laptop: &LaptopWindow, tv: &TvWindow, event: AppEvent) {
    STATE.with_borrow_mut(|s| apply(s, laptop, tv, event));
}

fn apply(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, event: AppEvent) {
    let theme = laptop.global::<Theme>();
    let dash = laptop.global::<Dash>();
    match event {
        AppEvent::Health { service, result } => {
            let h = s.health.entry(service).or_default();
            match result {
                Ok(note) => h.ok = Some((Utc::now(), note)),
                Err(e) => h.err = Some((Utc::now(), e)),
            }
            if service == Service::Weather {
                s.weather_err = s.health[&service].err.as_ref().and_then(|(t, e)| {
                    let newer = s.health[&service].ok.as_ref().is_none_or(|(ok, _)| t > ok);
                    newer.then(|| e.clone())
                });
                refresh_weather(s, laptop);
            }
            refresh_diagnostics(s, laptop);
        }
        AppEvent::KeyringStatus(Ok(())) => {
            s.health.entry(Service::Keyring).or_default().ok = Some((Utc::now(), "read/write OK".into()));
            refresh_diagnostics(s, laptop);
            dash.set_keyring_status("GNOME Keyring connected".into());
            dash.set_keyring_dot(theme.get_ok());
        }
        AppEvent::KeyringStatus(Err(e)) => {
            s.health.entry(Service::Keyring).or_default().err = Some((Utc::now(), e.clone()));
            refresh_diagnostics(s, laptop);
            dash.set_keyring_status(format!("Keyring unavailable: {e}").into());
            dash.set_keyring_dot(theme.get_alert());
        }
        AppEvent::Account(kind, status) => {
            s.accounts.insert(kind, status);
            update_cw_state(s, laptop);
        }
        AppEvent::Server(kind, url) => {
            match url {
                Some(u) => s.servers.insert(kind, u),
                None => s.servers.remove(&kind),
            };
            refresh_accounts(s, laptop);
        }
        AppEvent::SignIn(kind, state) => {
            match &state {
                SignInState::Done | SignInState::Cancelled => {
                    s.sign_in.remove(&kind);
                }
                _ => {
                    s.sign_in.insert(kind, state.clone());
                }
            }
            if state == SignInState::Done {
                show_toast(s, laptop, format!("Signed in to {}", kind.label()), TOAST_FOR);
                // Setup moves on by itself once a step's sign-in succeeds.
                let setup = laptop.global::<Setup>();
                let step = setup.get_step();
                if on_setup(laptop)
                    && ((kind == SourceKind::Plex && step == 0) || (kind == SourceKind::Jellyfin && step == 1))
                {
                    setup.set_step(step + 1);
                }
            }
            update_cw_state(s, laptop);
        }
        AppEvent::ContinueWatching(kind, items) => {
            s.continue_watching.insert(kind, items);
            rebuild_rows(s);
            rebuild_library(s, laptop);
            update_cw_state(s, laptop);
            refresh_tv_card(s, laptop);
        }
        AppEvent::RecentlyAdded(kind, items) => {
            s.recently_added.insert(kind, items);
            rebuild_library(s, laptop);
        }
        AppEvent::SearchResults { query, items } => {
            // Ignore results for a query the user has since changed.
            if s.search.as_ref().is_some_and(|(q, _)| *q == query) {
                s.search = Some((query, items));
                laptop.global::<Library>().set_searching(false);
                rebuild_library(s, laptop);
            }
        }
        AppEvent::Poster { source, item_id, image } => {
            s.hues.insert((source, item_id.clone()), tint::poster_hue(&image));
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&image.pixels, image.width, image.height);
            let img = Image::from_rgba8(buffer);
            let models = [
                (&s.rows, &s.cards),
                (&s.lib_continue, &s.lib_continue_cards),
                (&s.lib_recent, &s.lib_recent_cards),
                (&s.lib_results, &s.lib_result_cards),
            ];
            let hue = s.hues.get(&(source, item_id.clone())).copied();
            for (items, model) in models {
                for (i, item) in items.iter().enumerate() {
                    if item.source == source && item.id == item_id {
                        model.set_row_data(i, card(item, Some(&img), hue, s.theme));
                    }
                }
            }
            let key = (source, item_id);
            if s.selected.as_ref().is_some_and(|i| (i.source, i.id.clone()) == key) {
                let lib = laptop.global::<Library>();
                let mut d = lib.get_detail();
                d.poster = img.clone();
                d.has_poster = true;
                lib.set_detail(d);
            }
            s.posters.insert(key, img);
            refresh_tv_card(s, laptop);
        }
        AppEvent::Player(status) => {
            match &status {
                Some(st) => {
                    if s.player.as_ref().is_none_or(|p| p.item.id != st.item.id) {
                        tv.set_message(format!("Starting {}…", st.item.title).into());
                    }
                }
                None => tv.set_message("".into()),
            }
            s.player = status;
            refresh_tv_card(s, laptop);
        }
        AppEvent::Browser(site) => {
            tv.set_message(site.as_ref().map(|n| format!("Opening {n}…")).unwrap_or_default().into());
            s.browser = site;
            refresh_tv_card(s, laptop);
        }
        AppEvent::NowPlaying(sessions) => {
            let mut sessions = sessions;
            sessions.sort_by_key(|x| SOURCES.iter().position(|k| *k == x.source));
            s.sessions.set_vec(sessions.iter().map(session_row).collect::<Vec<_>>());
        }
        AppEvent::Weather(Some(w)) => {
            s.weather = Some((w, Utc::now()));
            s.weather_err = None;
            refresh_weather(s, laptop);
        }
        AppEvent::Weather(None) => {
            s.weather = None;
            dash.set_weather_on(false);
        }
        AppEvent::Tmdb(status) => {
            let acc = laptop.global::<Accounts>();
            if acc.get_editing() == "tmdb" {
                match &status {
                    AccountStatus::Unreachable(e) => acc.set_edit_error(e.as_str().into()),
                    _ => {
                        acc.set_editing("".into());
                        acc.set_edit_error("".into());
                    }
                }
            }
            s.tmdb = Some(status);
            refresh_accounts(s, laptop);
        }
        AppEvent::WatchResults { query, hits, error } => crate::watch_ui::on_results(laptop, query, hits, error),
        AppEvent::WatchPoster { tmdb_id, is_tv, image } => crate::watch_ui::on_poster(laptop, tmdb_id, is_tv, image),
        AppEvent::SiteLogos(logos) => {
            s.logos = logos
                .into_iter()
                .map(|(name, img)| {
                    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&img.pixels, img.width, img.height);
                    (name, Image::from_rgba8(buf))
                })
                .collect();
            refresh_tiles(s, laptop);
        }
        AppEvent::ProviderMatches { name, ids, names } => {
            let ed = laptop.global::<SiteEditor>();
            if ed.get_open() && ed.get_name().trim().eq_ignore_ascii_case(name.trim()) {
                if ids.is_empty() {
                    ed.set_note(format!("TMDB has no provider called “{name}”. Enter its IDs by hand.").into());
                } else {
                    let list: Vec<String> = ids.iter().map(u32::to_string).collect();
                    ed.set_providers(list.join(", ").into());
                    ed.set_note(format!("Found: {}", names.join(", ")).into());
                }
            }
        }
        AppEvent::Sites(sites) => {
            s.sites = sites;
            refresh_tiles(s, laptop);
            refresh_accounts(s, laptop);
        }
        AppEvent::Streaming(map) => {
            s.streaming = map;
            refresh_accounts(s, laptop);
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
        refresh_setup(s, laptop);
    }
}

// ── Rows ────────────────────────────────────────────────────────────

fn card(item: &MediaItem, poster: Option<&Image>, hue: Option<Hue>, theme: ThemeMode) -> PosterCardData {
    let hue = hue.unwrap_or(tint::DEFAULT);
    PosterCardData {
        accent: tint::progress_color(hue),
        placeholder: tint::card_tint(Hue { saturation: hue.saturation.max(0.3), ..hue }, theme).poster,
        title: item.title.as_str().into(),
        subtitle: format::item_detail(item).into(),
        badge: item.source.label().into(),
        progress: item.progress().unwrap_or(0.0),
        has_poster: poster.is_some(),
        poster: poster.cloned().unwrap_or_default(),
    }
}

fn cards_for(s: &UiState, items: &[MediaItem]) -> Vec<PosterCardData> {
    items
        .iter()
        .map(|item| {
            let key = (item.source, item.id.clone());
            card(item, s.posters.get(&key), s.hues.get(&key).copied(), s.theme)
        })
        .collect()
}

fn in_order(map: &HashMap<SourceKind, Vec<MediaItem>>) -> Vec<MediaItem> {
    SOURCES.iter().filter_map(|k| map.get(k)).flatten().cloned().collect()
}

fn rebuild_rows(s: &mut UiState) {
    // Fixed source order keeps cards from jumping between refreshes.
    s.rows = in_order(&s.continue_watching);
    s.cards.set_vec(cards_for(s, &s.rows));
}

// ── On the TV ───────────────────────────────────────────────────────

/// Build the On the TV card: mpv playback, else the browser, else the top
/// Continue Watching item ("up next"), else idle. Its tint follows the poster.
fn refresh_tv_card(s: &UiState, laptop: &LaptopWindow) {
    let dash = laptop.global::<Dash>();
    let (card, hue) = tv_card(s);
    dash.set_tint(tint::card_tint(hue, s.theme));
    dash.set_tv(card);
}

fn tv_card(s: &UiState) -> (TvCard, Hue) {
    let poster_of = |item: &MediaItem| {
        let key = (item.source, item.id.clone());
        (s.posters.get(&key).cloned(), s.hues.get(&key).copied().unwrap_or(tint::DEFAULT))
    };
    let base = TvCard { label: "ON THE TV".into(), ..Default::default() };
    if let Some(st) = &s.player {
        let (poster, hue) = poster_of(&st.item);
        let now = format::now_local(s.tz);
        let (at, left) = format::tv_times(st, now, s.clock.unwrap_or(ClockFormat::H24));
        let card = TvCard {
            mode: "playing".into(),
            source: st.item.source.label().into(),
            title: st.item.title.as_str().into(),
            subtitle: format::item_detail(&st.item).into(),
            at_text: at.into(),
            left_text: left.into(),
            progress: format::fraction(st),
            paused: st.state == PlayState::Paused,
            has_poster: poster.is_some(),
            poster: poster.unwrap_or_default(),
            ..base
        };
        return (card, hue);
    }
    if let Some(site) = &s.browser {
        let index = s.sites.iter().position(|x| x.name == *site).unwrap_or(0);
        let card = TvCard {
            mode: "streaming".into(),
            source: "Browser".into(),
            title: site.as_str().into(),
            subtitle: "Streaming full screen on the TV. Close the browser to go back to the idle screen.".into(),
            monogram: monogram(site).into(),
            ..base
        };
        return (card, Hue { degrees: tint::site_hue(site, index), saturation: 0.7 });
    }
    if let Some(item) = s.rows.first() {
        let (poster, hue) = poster_of(item);
        let card = TvCard {
            mode: "next".into(),
            label: "UP NEXT".into(),
            source: item.source.label().into(),
            note: "Nothing on the TV. Pick up where you left off:".into(),
            title: item.title.as_str().into(),
            subtitle: format::item_detail(item).into(),
            at_text: item.resume_ms.map(|p| format!("Stopped at {}", format::duration(p))).unwrap_or_default().into(),
            left_text: format::time_left(item).unwrap_or_default().into(),
            progress: item.progress().unwrap_or(0.0),
            has_poster: poster.is_some(),
            poster: poster.unwrap_or_default(),
            ..base
        };
        return (card, hue);
    }
    let card = TvCard {
        mode: "idle".into(),
        title: "Nothing playing".into(),
        subtitle: "The TV is showing the idle screen.".into(),
        ..base
    };
    (card, tint::DEFAULT)
}

fn session_row(x: &Session) -> SessionRow {
    let mut detail = vec![x.device.clone()];
    if x.state == PlayState::Paused {
        detail.push("Paused".into());
    }
    detail.push(match x.item.duration_ms {
        Some(d) => format!("{} / {}", format::duration(x.position_ms), format::duration(d)),
        None => format::duration(x.position_ms),
    });
    SessionRow {
        badge: x.source.label().into(),
        title: format::item_line(&x.item).into(),
        detail: detail.join(" · ").into(),
    }
}

pub fn source_id(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Jellyfin => "jellyfin",
        SourceKind::Plex => "plex",
    }
}

fn code_instructions(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Jellyfin => "On a signed-in Jellyfin app, open Settings › Quick Connect and enter this code.",
        SourceKind::Plex => "On any device, go to plex.tv/link and enter this code.",
    }
}

/// What, if anything, an account needs from the user.
fn prompt_for(
    kind: SourceKind,
    sign_in: Option<&SignInState>,
    account: Option<&AccountStatus>,
) -> Option<AccountPrompt> {
    let label = kind.label();
    let (message, action, action_id, warn) = match (sign_in, account) {
        (Some(SignInState::Failed(e)), _) => {
            (format!("{label} sign-in failed: {e}"), "Try again".into(), "sign-in", true)
        }
        (_, Some(AccountStatus::NotSetUp)) => {
            (format!("{label} isn't connected."), format!("Sign in to {label}"), "sign-in", false)
        }
        (_, Some(AccountStatus::Expired)) => (format!("{label} sign-in expired."), "Reconnect".into(), "sign-in", true),
        (_, Some(AccountStatus::Unreachable(e))) => {
            (format!("Can't reach {label}: {e}"), "Retry".into(), "retry", true)
        }
        _ => return None,
    };
    Some(AccountPrompt {
        source: source_id(kind).into(),
        message: message.into(),
        action: action.into(),
        action_id: action_id.into(),
        warn,
    })
}

/// Pick what the dashboard's Continue Watching area shows, and the account prompts.
fn update_cw_state(s: &UiState, laptop: &LaptopWindow) {
    let dash = laptop.global::<Dash>();
    let prompts: Vec<(SourceKind, AccountPrompt)> =
        SOURCES.iter().filter_map(|&k| prompt_for(k, s.sign_in.get(&k), s.accounts.get(&k)).map(|p| (k, p))).collect();
    // Expired sign-ins go on the On the TV card (they also hide that server's
    // sessions); everything else sits with Continue Watching.
    let (expired, others): (Vec<_>, Vec<_>) = prompts.into_iter().partition(|(k, _)| {
        s.accounts.get(k) == Some(&AccountStatus::Expired) && !matches!(s.sign_in.get(k), Some(SignInState::Failed(_)))
    });
    s.tv_prompts.set_vec(expired.into_iter().map(|(_, p)| p).collect::<Vec<_>>());
    s.prompts.set_vec(others.into_iter().map(|(_, p)| p).collect::<Vec<_>>());

    // A running sign-in takes over the area.
    let active = SOURCES.iter().find_map(|k| match s.sign_in.get(k) {
        Some(st @ (SignInState::Starting | SignInState::Code(_))) => Some((*k, st)),
        _ => None,
    });
    let (state, message) = match active {
        Some((kind, SignInState::Code(code))) => {
            dash.set_signin_code(code.as_str().into());
            dash.set_signin_source(source_id(kind).into());
            ("code", code_instructions(kind).to_owned())
        }
        Some((kind, _)) => {
            dash.set_signin_source(source_id(kind).into());
            ("busy", format!("Contacting {}…", kind.label()))
        }
        None if !s.rows.is_empty() => ("items", String::new()),
        None if s.prompts.row_count() > 0 => ("prompts", String::new()),
        None if !s.continue_watching.is_empty() => ("empty", String::new()),
        None => ("loading", String::new()),
    };
    dash.set_cw_state(state.into());
    dash.set_cw_message(message.into());
    refresh_accounts(s, laptop);
}

// ── Library ─────────────────────────────────────────────────────────

fn filter_items(items: Vec<MediaItem>, filter: &str) -> Vec<MediaItem> {
    match filter {
        "plex" => items.into_iter().filter(|i| i.source == SourceKind::Plex).collect(),
        "jellyfin" => items.into_iter().filter(|i| i.source == SourceKind::Jellyfin).collect(),
        _ => items,
    }
}

fn rebuild_library(s: &mut UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let filter = lib.get_filter();
    s.lib_continue = filter_items(in_order(&s.continue_watching), &filter);
    s.lib_recent = filter_items(in_order(&s.recently_added), &filter);
    s.lib_results = filter_items(s.search.as_ref().map(|(_, i)| i.clone()).unwrap_or_default(), &filter);
    s.lib_continue_cards.set_vec(cards_for(s, &s.lib_continue));
    s.lib_recent_cards.set_vec(cards_for(s, &s.lib_recent));
    s.lib_result_cards.set_vec(cards_for(s, &s.lib_results));
    lib.set_results_title(match &s.search {
        Some((q, _)) => format!("RESULTS FOR “{}”", q.to_uppercase()).into(),
        None => "".into(),
    });
    // Selection indexes are stale after a rebuild.
    lib.set_selected_index(-1);
}

/// Filter buttons.
pub fn set_filter(laptop: &LaptopWindow, filter: &str) {
    laptop.global::<Library>().set_filter(filter.into());
    STATE.with_borrow_mut(|s| rebuild_library(s, laptop));
}

/// Returns false for an empty query (nothing to send).
pub fn start_search(laptop: &LaptopWindow, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return false;
    }
    STATE.with_borrow_mut(|s| {
        s.search = Some((q.to_owned(), Vec::new()));
        laptop.global::<Library>().set_searching(true);
        rebuild_library(s, laptop);
    });
    true
}

pub fn clear_search(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        s.search = None;
        laptop.global::<Library>().set_searching(false);
        rebuild_library(s, laptop);
    });
}

/// Open the detail panel for a library card.
pub fn select(laptop: &LaptopWindow, section: &str, index: usize) {
    STATE.with_borrow_mut(|s| {
        let list = match section {
            "continue" => &s.lib_continue,
            "recent" => &s.lib_recent,
            _ => &s.lib_results,
        };
        let Some(item) = list.get(index).cloned() else { return };
        let poster = s.posters.get(&(item.source, item.id.clone())).cloned();
        let lib = laptop.global::<Library>();
        lib.set_detail(DetailData {
            title: item.title.as_str().into(),
            subtitle: format::item_detail(&item).into(),
            badge: item.source.label().to_uppercase().into(),
            meta: format::item_meta(&item).into(),
            has_poster: poster.is_some(),
            poster: poster.unwrap_or_default(),
            progress: item.progress().unwrap_or(0.0),
            can_resume: item.resume_ms.is_some(),
        });
        lib.set_detail_open(true);
        s.selected = Some(item);
    });
}

pub fn selected() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.selected.clone())
}

pub fn close_detail(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| s.selected = None);
    let lib = laptop.global::<Library>();
    lib.set_detail_open(false);
    lib.set_selected_index(-1);
}

// ── Settings › Accounts and setup ───────────────────────────────────

/// A button: (label, action id). `NONE` = no button.
type Button<'a> = (&'a str, &'a str);
const NONE: Button = ("", "");

struct Row<'a> {
    id: &'a str,
    name: &'a str,
    status: String,
    detail: String,
    state: &'a str,
    primary: Button<'a>,
    secondary: Button<'a>,
}

impl From<Row<'_>> for AccountRow {
    fn from(r: Row<'_>) -> Self {
        AccountRow {
            id: r.id.into(),
            name: r.name.into(),
            status: r.status.into(),
            detail: r.detail.into(),
            state: r.state.into(),
            primary: r.primary.0.into(),
            primary_id: r.primary.1.into(),
            secondary: r.secondary.0.into(),
            secondary_id: r.secondary.1.into(),
            input_hint: "".into(),
        }
    }
}

fn tmdb_row(s: &UiState) -> AccountRow {
    let row = |status: &str, detail: String, state: &str, primary, secondary| -> AccountRow {
        AccountRow {
            input_hint: "TMDB API Read Access Token (themoviedb.org › Settings › API)".into(),
            ..Row {
                id: "tmdb",
                name: "TMDB · Where to watch",
                status: status.into(),
                detail,
                state,
                primary,
                secondary,
            }
            .into()
        }
    };
    let (replace, remove) = (("Replace token", "edit"), ("Remove", "remove"));
    match &s.tmdb {
        Some(AccountStatus::Connected) => {
            row("Connected", "Search, where things stream, and service logos".into(), "ok", NONE, replace)
        }
        Some(AccountStatus::Expired) => row("TMDB rejected the token", String::new(), "warn", replace, remove),
        Some(AccountStatus::Unreachable(e)) => {
            row("Couldn't use that token", e.clone(), "warn", ("Add token", "edit"), NONE)
        }
        _ => row(
            "Not set up",
            "Paste your TMDB API Read Access Token to search where things stream".into(),
            "off",
            ("Add token", "edit"),
            NONE,
        ),
    }
}

fn media_account_row(s: &UiState, kind: SourceKind) -> AccountRow {
    let server = s.servers.get(&kind).cloned().unwrap_or_default();
    let sign_out = ("Sign out", "sign-out");
    let row = |status: String, detail: String, state, primary, secondary| Row {
        id: source_id(kind),
        name: kind.label(),
        status,
        detail,
        state,
        primary,
        secondary,
    };
    match (s.sign_in.get(&kind), s.accounts.get(&kind)) {
        (Some(SignInState::Starting), _) => {
            row("Contacting the server…".into(), String::new(), "busy", ("Cancel", "cancel"), NONE)
        }
        (Some(SignInState::Code(code)), _) => {
            row(format!("Code {code}"), code_instructions(kind).into(), "busy", ("Cancel", "cancel"), NONE)
        }
        (Some(SignInState::Failed(e)), _) => {
            row(format!("Sign-in failed: {e}"), String::new(), "warn", ("Try again", "sign-in"), NONE)
        }
        (_, Some(AccountStatus::Connected)) => row("Connected".into(), server, "ok", NONE, sign_out),
        (_, Some(AccountStatus::Expired)) => {
            row("Sign-in expired".into(), server, "warn", ("Reconnect", "sign-in"), sign_out)
        }
        (_, Some(AccountStatus::Unreachable(e))) => {
            row("Can't reach the server".into(), e.clone(), "warn", ("Retry", "retry"), sign_out)
        }
        _ => row("Not connected".into(), String::new(), "off", ("Sign in", "sign-in"), NONE),
    }
    .into()
}

fn site_status(opened: Option<&DateTime<Utc>>) -> String {
    match opened {
        None => "Not signed in yet".into(),
        Some(t) => match (Utc::now() - *t).num_days() {
            0 => "Set up · opened today".into(),
            1 => "Set up · opened yesterday".into(),
            n => format!("Set up · opened {n} days ago"),
        },
    }
}

fn refresh_accounts(s: &UiState, laptop: &LaptopWindow) {
    s.account_rows.set_vec(vec![
        media_account_row(s, SourceKind::Plex),
        media_account_row(s, SourceKind::Jellyfin),
        tmdb_row(s),
    ]);
    let sites: Vec<SiteRow> = s
        .sites
        .iter()
        .enumerate()
        .map(|(i, site)| {
            let opened = s.streaming.get(&site.name);
            let (bg, fg) = site_colors(s, i);
            SiteRow {
                index: i as i32,
                name: site.name.as_str().into(),
                url: site.url.as_str().into(),
                status: site_status(opened).into(),
                opened: opened.is_some(),
                hidden: site.hidden,
                monogram: monogram(&site.name).into(),
                bg,
                fg,
            }
        })
        .collect();
    s.site_rows.set_vec(sites);
    if on_setup(laptop) {
        refresh_setup(s, laptop);
    }
}

/// (state, code, message) for a sign-in step on the setup page.
fn setup_sign_in(s: &UiState, kind: SourceKind) -> (&'static str, String, String) {
    match (s.sign_in.get(&kind), s.accounts.get(&kind)) {
        (Some(SignInState::Starting), _) => ("busy", String::new(), String::new()),
        (Some(SignInState::Code(c)), _) => ("code", c.clone(), String::new()),
        (Some(SignInState::Failed(e)), _) => ("error", String::new(), format!("Sign-in failed: {e}")),
        (_, Some(AccountStatus::Connected)) => {
            let server = s.servers.get(&kind).map(|u| format!(" ({u})")).unwrap_or_default();
            ("done", String::new(), format!("Connected to {}{server}", kind.label()))
        }
        _ => ("idle", String::new(), String::new()),
    }
}

fn refresh_setup(s: &UiState, laptop: &LaptopWindow) {
    let setup = laptop.global::<Setup>();
    let (st, code, msg) = setup_sign_in(s, SourceKind::Plex);
    setup.set_plex_state(st.into());
    setup.set_plex_code(code.into());
    setup.set_plex_message(msg.into());
    let (st, code, msg) = setup_sign_in(s, SourceKind::Jellyfin);
    setup.set_jellyfin_state(st.into());
    setup.set_jellyfin_code(code.into());
    setup.set_jellyfin_message(msg.into());
}

/// Run the setup page's derived state now (e.g. right after navigating to it).
pub fn sync_setup(laptop: &LaptopWindow) {
    STATE.with_borrow(|s| refresh_setup(s, laptop));
}

// ── Toasts ──────────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    //! Drive the real windows headless (Slint's testing backend) through events
    //! and check what the pages would show.

    use super::*;
    use tvdash_core::models::{MediaKind, RgbaImage};

    fn windows() -> (LaptopWindow, TvWindow) {
        i_slint_backend_testing::init_no_event_loop();
        let (l, t) = (LaptopWindow::new().unwrap(), TvWindow::new().unwrap());
        let sites = vec!["Netflix".to_string(), "Hulu".to_string()];
        init(
            &l,
            UiConfig {
                tz: Some(chrono_tz::America::Chicago),
                clock: ClockFormat::H24,
                seconds: false,
                paths: vec!["Log file: /tmp/tvdash.log".into()],
                units: Units::Fahrenheit,
                theme: ThemeMode::Dark,
                sites: sites
                    .into_iter()
                    .map(|name| StreamingSite { url: format!("https://{name}.example"), name, ..Default::default() })
                    .collect(),
                jellyfin_url: "http://jf:8096".into(),
            },
        );
        (l, t)
    }

    fn send(l: &LaptopWindow, t: &TvWindow, e: AppEvent) {
        STATE.with_borrow_mut(|s| apply(s, l, t, e));
    }

    fn item(source: SourceKind, id: &str, title: &str) -> MediaItem {
        MediaItem {
            source,
            id: id.into(),
            kind: MediaKind::Movie,
            title: title.into(),
            subtitle: Some("1999".into()),
            episode_label: None,
            duration_ms: Some(7_200_000),
            resume_ms: Some(3_600_000),
            poster: Some("p".into()),
        }
    }

    fn account(l: &LaptopWindow, id: &str) -> AccountRow {
        let rows = l.global::<Accounts>().get_accounts();
        rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("no row {id}"))
    }

    #[test]
    fn setup_flow_advances_on_success() {
        let (l, t) = windows();
        let setup = l.global::<Setup>();
        assert_eq!(setup.get_jellyfin_url(), "http://jf:8096");
        send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp));
        send(&l, &t, AppEvent::Account(SourceKind::Jellyfin, AccountStatus::NotSetUp));
        send(&l, &t, AppEvent::ShowSetup);
        assert_eq!(l.global::<Nav>().get_page(), "setup");
        assert_eq!((setup.get_step(), setup.get_plex_state().as_str()), (0, "idle"));

        // Plex: code shown on the setup page and in Settings; success moves on.
        send(&l, &t, AppEvent::SignIn(SourceKind::Plex, SignInState::Code("WXYZ".into())));
        assert_eq!((setup.get_plex_state().as_str(), setup.get_plex_code().as_str()), ("code", "WXYZ"));
        assert_eq!(account(&l, "plex").status, "Code WXYZ");
        send(&l, &t, AppEvent::SignIn(SourceKind::Plex, SignInState::Done));
        send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::Connected));
        send(&l, &t, AppEvent::Server(SourceKind::Plex, Some("http://10.0.0.5:32400".into())));
        assert_eq!(setup.get_step(), 1);
        assert_eq!(setup.get_plex_state(), "done");
        assert!(setup.get_plex_message().contains("10.0.0.5"));
        assert_eq!((account(&l, "plex").state.as_str(), account(&l, "plex").secondary.as_str()), ("ok", "Sign out"));

        // Jellyfin failure stays on the step with the reason.
        send(&l, &t, AppEvent::SignIn(SourceKind::Jellyfin, SignInState::Failed("wrong username or password".into())));
        assert_eq!((setup.get_step(), setup.get_jellyfin_state().as_str()), (1, "error"));
        assert!(setup.get_jellyfin_message().contains("wrong username"));
    }

    #[test]
    fn tv_card_follows_playback_browser_and_theme() {
        let (l, t) = windows();
        let dash = l.global::<Dash>();
        assert_eq!(dash.get_tv().mode, "idle");
        assert_eq!(dash.get_streaming_sites().row_count(), 2);
        assert_eq!(dash.get_streaming_sites().row_data(0).unwrap().monogram, "N");

        // Something to resume: the card offers it, tinted from its poster.
        send(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![item(SourceKind::Plex, "p1", "Plex Film")]));
        let tv = dash.get_tv();
        assert_eq!((tv.mode.as_str(), tv.label.as_str(), tv.source.as_str()), ("next", "UP NEXT", "Plex"));
        assert_eq!((tv.at_text.as_str(), tv.left_text.as_str()), ("Stopped at 1:00:00", "1 h left"));
        let before = dash.get_tint().card;
        let red = RgbaImage { width: 2, height: 2, pixels: [200, 30, 30, 255].repeat(4) };
        send(&l, &t, AppEvent::Poster { source: SourceKind::Plex, item_id: "p1".into(), image: red });
        assert!(dash.get_tv().has_poster);
        assert_ne!(dash.get_tint().card, before, "tint follows the poster");

        // Playback wins over "up next"; the browser shows when mpv isn't running.
        let status = PlayerStatus {
            item: item(SourceKind::Jellyfin, "j1", "Jelly Film"),
            state: PlayState::Paused,
            position_ms: 754_000,
            duration_ms: Some(2_700_000),
        };
        send(&l, &t, AppEvent::Player(Some(status)));
        let tv = dash.get_tv();
        assert_eq!((tv.mode.as_str(), tv.title.as_str(), tv.paused), ("playing", "Jelly Film", true));
        assert_eq!(tv.at_text, "12:34 / 45:00");
        assert!(tv.left_text.starts_with("32 min left · ends "));
        assert_eq!(t.get_message(), "Starting Jelly Film…");
        send(&l, &t, AppEvent::Player(None));
        send(&l, &t, AppEvent::Browser(Some("Hulu".into())));
        let tv = dash.get_tv();
        assert_eq!((tv.mode.as_str(), tv.monogram.as_str()), ("streaming", "H"));
        send(&l, &t, AppEvent::Browser(None));
        assert_eq!(dash.get_tv().mode, "next");
        assert_eq!(t.get_message(), "");

        // Theme: laptop switches, tiles and tint follow; the TV stays dark.
        let dark_tile = dash.get_streaming_sites().row_data(0).unwrap().bg;
        set_theme(&l, ThemeMode::Stone);
        assert_eq!(l.global::<Theme>().get_mode(), UiTheme::Stone);
        assert_ne!(dash.get_streaming_sites().row_data(0).unwrap().bg, dark_tile);
        assert_eq!(t.global::<Theme>().get_mode(), UiTheme::Dark);
    }

    #[test]
    fn hidden_sites_leave_the_dashboard_only() {
        let (l, t) = windows();
        let site = |name: &str, hidden| StreamingSite {
            name: name.into(),
            url: "https://x.example".into(),
            hidden,
            ..Default::default()
        };
        send(&l, &t, AppEvent::Sites(vec![site("Netflix", false), site("Hulu", true), site("Max", false)]));
        let tiles = l.global::<Dash>().get_streaming_sites();
        let got: Vec<(i32, String)> = tiles.iter().map(|t| (t.index, t.name.to_string())).collect();
        assert_eq!(got, [(0, "Netflix".to_string()), (2, "Max".into())], "tiles keep config positions");
        let rows = l.global::<Accounts>().get_sites();
        assert_eq!(rows.row_count(), 3);
        assert!(rows.row_data(1).unwrap().hidden);
        assert_eq!(sites().len(), 3);
    }

    #[test]
    fn weather_states_and_diagnostics() {
        let (l, t) = windows();
        let dash = l.global::<Dash>();
        // Failing before any data: shown as unavailable with the reason.
        send(&l, &t, AppEvent::Health { service: Service::Weather, result: Err("HTTP 502".into()) });
        assert!(dash.get_weather_on() && dash.get_weather_warn());
        assert_eq!(dash.get_weather_note(), "Unavailable: HTTP 502");

        let w = Weather { temperature: 51.0, code: 2, condition: "Partly cloudy".into(), high: None, low: None };
        send(&l, &t, AppEvent::Weather(Some(w)));
        send(&l, &t, AppEvent::Health { service: Service::Weather, result: Ok("51 Partly cloudy".into()) });
        assert!(!dash.get_weather_warn() && !dash.get_weather_stale());
        assert_eq!((dash.get_weather_temp().as_str(), dash.get_weather_note().as_str()), ("51°F", ""));

        // Old data: dimmed with its age; a later failure adds the amber dot.
        STATE.with_borrow_mut(|s| s.weather.as_mut().unwrap().1 -= chrono::TimeDelta::minutes(25));
        send(&l, &t, AppEvent::Health { service: Service::Weather, result: Err("timed out".into()) });
        assert!(dash.get_weather_stale() && dash.get_weather_warn());
        assert_eq!(dash.get_weather_note(), "Updated 25 min ago");

        // Diagnostics: one row per service; the newest result decides the dot.
        let rows = l.global::<Diagnostics>().get_rows();
        let weather = rows.iter().find(|r| r.name == "Weather (Open-Meteo)").unwrap();
        assert_eq!(weather.state, "warn");
        assert!(weather.last_error.ends_with("timed out") && weather.last_ok.ends_with("51 Partly cloudy"));
        assert_eq!(rows.iter().find(|r| r.name == "Plex").unwrap().state, "off");
        send(&l, &t, AppEvent::KeyringStatus(Ok(())));
        let rows = l.global::<Diagnostics>().get_rows();
        assert_eq!(rows.iter().find(|r| r.name == "GNOME Keyring").unwrap().state, "ok");
        assert_eq!(l.global::<Diagnostics>().get_paths().row_count(), 1);

        // Display settings: the reading keeps its units until the new fetch arrives.
        set_display(&l, Units::Celsius, ClockFormat::H12, true);
        assert_eq!(dash.get_weather_temp(), "51°F");
    }

    #[test]
    fn library_filter_search_and_detail() {
        let (l, t) = windows();
        let lib = l.global::<Library>();
        send(
            &l,
            &t,
            AppEvent::ContinueWatching(SourceKind::Jellyfin, vec![item(SourceKind::Jellyfin, "j1", "Jelly Film")]),
        );
        send(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![item(SourceKind::Plex, "p1", "Plex Film")]));
        send(&l, &t, AppEvent::RecentlyAdded(SourceKind::Plex, vec![item(SourceKind::Plex, "p2", "New One")]));
        assert_eq!((lib.get_continue_watching().row_count(), lib.get_recently_added().row_count()), (2, 1));
        assert_eq!(l.global::<Dash>().get_cw_state(), "items");

        set_filter(&l, "plex");
        let cw = lib.get_continue_watching();
        assert_eq!((cw.row_count(), cw.row_data(0).unwrap().title.as_str()), (1, "Plex Film"));

        // Search: stale results for an old query are ignored.
        assert!(start_search(&l, " film "));
        assert!(lib.get_searching());
        assert_eq!(lib.get_results_title(), "RESULTS FOR “FILM”");
        send(
            &l,
            &t,
            AppEvent::SearchResults { query: "old".into(), items: vec![item(SourceKind::Plex, "x", "Stale")] },
        );
        assert_eq!(lib.get_results().row_count(), 0);
        let found = vec![item(SourceKind::Jellyfin, "j1", "Jelly Film"), item(SourceKind::Plex, "p1", "Plex Film")];
        send(&l, &t, AppEvent::SearchResults { query: "film".into(), items: found });
        assert!(!lib.get_searching());
        assert_eq!(lib.get_results().row_count(), 1, "filtered to Plex");

        // Detail panel, then its poster arrives.
        select(&l, "results", 0);
        assert!(lib.get_detail_open());
        let d = lib.get_detail();
        assert_eq!((d.title.as_str(), d.badge.as_str(), d.can_resume), ("Plex Film", "PLEX", true));
        assert_eq!(d.meta, "Movie · 2 h · 1 h left");
        assert!(!d.has_poster);
        let image = RgbaImage { width: 1, height: 1, pixels: vec![255; 4] };
        send(&l, &t, AppEvent::Poster { source: SourceKind::Plex, item_id: "p1".into(), image });
        assert!(lib.get_detail().has_poster);
        assert!(lib.get_results().row_data(0).unwrap().has_poster);
        assert_eq!(selected().unwrap().id, "p1");
        clear_search(&l);
        assert_eq!(lib.get_results_title(), "");
    }

    #[test]
    fn expired_states_and_streaming_sites() {
        let (l, t) = windows();
        let dash = l.global::<Dash>();
        send(&l, &t, AppEvent::Account(SourceKind::Jellyfin, AccountStatus::Connected));
        send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::Expired));
        // Expired → On the TV card, not the Continue Watching banners.
        assert_eq!(dash.get_tv_prompts().row_count(), 1);
        assert_eq!(dash.get_prompts().row_count(), 0);
        let plex = account(&l, "plex");
        assert_eq!(
            (plex.state.as_str(), plex.primary.as_str(), plex.primary_id.as_str()),
            ("warn", "Reconnect", "sign-in")
        );

        send(&l, &t, AppEvent::Streaming(BTreeMap::from([("Netflix".to_string(), Utc::now())])));
        let sites = l.global::<Accounts>().get_sites();
        assert_eq!(sites.row_data(0).unwrap().status, "Set up · opened today");
        assert_eq!(sites.row_data(1).unwrap().status, "Not signed in yet");
    }
}
