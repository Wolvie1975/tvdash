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
    config::ClockFormat,
    models::{AccountStatus, MediaItem, PlayState, Session, SignInState, SourceKind},
};

use crate::{
    AccountPrompt, AccountRow, Accounts, Dash, DetailData, LaptopWindow, Library, Nav, PosterCardData, SessionRow,
    Setup, SiteRow, Theme, TvWindow, format,
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
    site_names: Vec<String>,
    continue_watching: HashMap<SourceKind, Vec<MediaItem>>,
    recently_added: HashMap<SourceKind, Vec<MediaItem>>,
    /// Dashboard Continue Watching rows; index = card index.
    rows: Vec<MediaItem>,
    posters: HashMap<(SourceKind, String), Image>,
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

/// Call once on the UI thread before the event loop starts.
pub fn init(
    laptop: &LaptopWindow,
    tz: Option<chrono_tz::Tz>,
    clock: ClockFormat,
    site_names: Vec<String>,
    jellyfin_url: &str,
) {
    STATE.with_borrow_mut(|s| {
        s.tz = tz;
        s.clock = Some(clock);
        s.site_names = site_names;
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
        laptop.global::<Setup>().set_jellyfin_url(jellyfin_url.into());
        refresh_accounts(s, laptop);
    });
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

fn show_idle(dash: &Dash, tv: &TvWindow) {
    dash.set_now_playing_title("Nothing playing".into());
    dash.set_now_playing_detail("The TV is showing the idle screen.".into());
    dash.set_now_playing_progress(0.0);
    dash.set_paused(false);
    dash.set_transport_enabled(false);
    dash.set_stop_enabled(false);
    tv.set_message("".into());
}

fn on_setup(laptop: &LaptopWindow) -> bool {
    laptop.global::<Nav>().get_page() == "setup"
}

fn apply(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, event: AppEvent) {
    let theme = laptop.global::<Theme>();
    let dash = laptop.global::<Dash>();
    match event {
        AppEvent::KeyringStatus(Ok(())) => {
            dash.set_keyring_status("GNOME Keyring connected".into());
            dash.set_keyring_dot(theme.get_ok());
        }
        AppEvent::KeyringStatus(Err(e)) => {
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
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&image.pixels, image.width, image.height);
            let img = Image::from_rgba8(buffer);
            let models = [
                (&s.rows, &s.cards),
                (&s.lib_continue, &s.lib_continue_cards),
                (&s.lib_recent, &s.lib_recent_cards),
                (&s.lib_results, &s.lib_result_cards),
            ];
            for (items, model) in models {
                for (i, item) in items.iter().enumerate() {
                    if item.source == source && item.id == item_id {
                        model.set_row_data(i, card(item, Some(&img)));
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
        }
        AppEvent::Player(Some(status)) => {
            let now = format::now_local(s.tz);
            let clock = s.clock.unwrap_or(ClockFormat::H24);
            let detail = [format::item_detail(&status.item), format::progress_line(&status, now, clock)]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            dash.set_now_playing_title(status.item.title.as_str().into());
            dash.set_now_playing_detail(format!("{} · {detail}", status.item.source.label()).into());
            dash.set_now_playing_progress(format::fraction(&status));
            dash.set_paused(status.state == PlayState::Paused);
            dash.set_transport_enabled(true);
            dash.set_stop_enabled(false);
            tv.set_message(format!("Starting {}…", status.item.title).into());
        }
        AppEvent::Player(None) | AppEvent::Browser(None) => show_idle(&dash, tv),
        AppEvent::Browser(Some(site)) => {
            dash.set_now_playing_title(site.as_str().into());
            dash.set_now_playing_detail("Streaming in the browser on the TV. Stop closes it.".into());
            dash.set_now_playing_progress(0.0);
            dash.set_transport_enabled(false);
            dash.set_stop_enabled(true);
            tv.set_message(format!("Opening {site}…").into());
        }
        AppEvent::NowPlaying(sessions) => {
            let mut sessions = sessions;
            sessions.sort_by_key(|x| SOURCES.iter().position(|k| *k == x.source));
            s.sessions.set_vec(sessions.iter().map(session_row).collect::<Vec<_>>());
        }
        AppEvent::Weather(Some(w)) => {
            let c = format::weather_chip(&w);
            dash.set_weather_text(c.value.into());
            dash.set_weather_detail(c.detail.into());
        }
        AppEvent::Weather(None) => {
            dash.set_weather_text("No location".into());
            dash.set_weather_detail("Set latitude/longitude".into());
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
    if on_setup(laptop) {
        refresh_setup(s, laptop);
    }
}

// ── Rows ────────────────────────────────────────────────────────────

fn card(item: &MediaItem, poster: Option<&Image>) -> PosterCardData {
    PosterCardData {
        title: item.title.as_str().into(),
        subtitle: format::item_detail(item).into(),
        badge: item.source.label().into(),
        progress: item.progress().unwrap_or(0.0),
        has_poster: poster.is_some(),
        poster: poster.cloned().unwrap_or_default(),
    }
}

fn cards_for(s: &UiState, items: &[MediaItem]) -> Vec<PosterCardData> {
    items.iter().map(|item| card(item, s.posters.get(&(item.source, item.id.clone())))).collect()
}

fn in_order(map: &HashMap<SourceKind, Vec<MediaItem>>) -> Vec<MediaItem> {
    SOURCES.iter().filter_map(|k| map.get(k)).flatten().cloned().collect()
}

fn rebuild_rows(s: &mut UiState) {
    // Fixed source order keeps cards from jumping between refreshes.
    s.rows = in_order(&s.continue_watching);
    s.cards.set_vec(cards_for(s, &s.rows));
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
        }
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
    s.account_rows.set_vec(vec![media_account_row(s, SourceKind::Plex), media_account_row(s, SourceKind::Jellyfin)]);
    let sites: Vec<SiteRow> = s
        .site_names
        .iter()
        .map(|n| {
            let opened = s.streaming.get(n);
            SiteRow { name: n.as_str().into(), status: site_status(opened).into(), opened: opened.is_some() }
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
        init(&l, Some(chrono_tz::America::Chicago), ClockFormat::H24, sites, "http://jf:8096");
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
