//! Moves `AppEvent`s from background tasks onto the UI thread and keeps the
//! UI-side state (rows, posters, account status) that models are built from.

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, VecModel, Weak};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::debug;
use tvdash_core::{
    AppEvent,
    config::ClockFormat,
    models::{AccountStatus, CalendarState, MediaItem, PlayState, Session, SignInState, SourceKind},
};

use crate::{AccountPrompt, LaptopWindow, PosterCardData, SessionRow, Theme, TvWindow, format};

/// Display order for sources everywhere (rows, prompts, sessions).
const SOURCES: [SourceKind; 2] = [SourceKind::Jellyfin, SourceKind::Plex];

const TOAST_FOR: Duration = Duration::from_secs(5);
const LONG_TOAST_FOR: Duration = Duration::from_secs(30);

/// UI-thread state. Only touched from the Slint event loop.
#[derive(Default)]
pub struct UiState {
    pub tz: Option<chrono_tz::Tz>,
    pub clock: Option<ClockFormat>,
    continue_watching: HashMap<SourceKind, Vec<MediaItem>>,
    /// Flattened rows, in display order; index = card index in the UI.
    rows: Vec<MediaItem>,
    posters: HashMap<(SourceKind, String), Image>,
    accounts: HashMap<SourceKind, AccountStatus>,
    sign_in: HashMap<SourceKind, SignInState>,
    cards: Rc<VecModel<PosterCardData>>,
    prompts: Rc<VecModel<AccountPrompt>>,
    tv_prompts: Rc<VecModel<AccountPrompt>>,
    sessions: Rc<VecModel<SessionRow>>,
    calendar: Option<CalendarState>,
    toast_timer: Option<Timer>,
}

thread_local! {
    pub static STATE: RefCell<UiState> = RefCell::new(UiState::default());
}

/// Call once on the UI thread before the event loop starts.
pub fn init(laptop: &LaptopWindow, tz: Option<chrono_tz::Tz>, clock: ClockFormat) {
    STATE.with_borrow_mut(|s| {
        s.tz = tz;
        s.clock = Some(clock);
        laptop.set_continue_watching(ModelRc::from(s.cards.clone()));
        laptop.set_prompts(ModelRc::from(s.prompts.clone()));
        laptop.set_tv_prompts(ModelRc::from(s.tv_prompts.clone()));
        laptop.set_sessions(ModelRc::from(s.sessions.clone()));
    });
}

/// The item behind Continue Watching card `index`.
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

fn show_idle(laptop: &LaptopWindow, tv: &TvWindow) {
    laptop.set_now_playing_title("Nothing playing".into());
    laptop.set_now_playing_detail("The TV is showing the idle screen.".into());
    laptop.set_now_playing_progress(0.0);
    laptop.set_paused(false);
    laptop.set_transport_enabled(false);
    laptop.set_stop_enabled(false);
    tv.set_message("".into());
}

fn apply(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, event: AppEvent) {
    let theme = laptop.global::<Theme>();
    match event {
        AppEvent::KeyringStatus(Ok(())) => {
            laptop.set_keyring_status("GNOME Keyring connected".into());
            laptop.set_keyring_dot(theme.get_ok());
        }
        AppEvent::KeyringStatus(Err(e)) => {
            laptop.set_keyring_status(format!("Keyring unavailable: {e}").into());
            laptop.set_keyring_dot(theme.get_alert());
        }
        AppEvent::Account(kind, status) => {
            s.accounts.insert(kind, status);
            update_cw_state(s, laptop);
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
            }
            update_cw_state(s, laptop);
        }
        AppEvent::ContinueWatching(kind, items) => {
            s.continue_watching.insert(kind, items);
            rebuild_rows(s);
            update_cw_state(s, laptop);
        }
        AppEvent::Poster { source, item_id, image } => {
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&image.pixels, image.width, image.height);
            let img = Image::from_rgba8(buffer);
            for (i, item) in s.rows.iter().enumerate() {
                if item.source == source && item.id == item_id {
                    s.cards.set_row_data(i, card(item, Some(&img)));
                }
            }
            s.posters.insert((source, item_id), img);
        }
        AppEvent::Player(Some(status)) => {
            let now = format::now_local(s.tz);
            let clock = s.clock.unwrap_or(ClockFormat::H24);
            let detail = [format::item_detail(&status.item), format::progress_line(&status, now, clock)]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            laptop.set_now_playing_title(status.item.title.as_str().into());
            laptop.set_now_playing_detail(format!("{} · {detail}", status.item.source.label()).into());
            laptop.set_now_playing_progress(format::fraction(&status));
            laptop.set_paused(status.state == PlayState::Paused);
            laptop.set_transport_enabled(true);
            laptop.set_stop_enabled(false);
            tv.set_message(format!("Starting {}…", status.item.title).into());
        }
        AppEvent::Player(None) | AppEvent::Browser(None) => show_idle(laptop, tv),
        AppEvent::Browser(Some(site)) => {
            laptop.set_now_playing_title(site.as_str().into());
            laptop.set_now_playing_detail("Streaming in the browser on the TV. Stop closes it.".into());
            laptop.set_now_playing_progress(0.0);
            laptop.set_transport_enabled(false);
            laptop.set_stop_enabled(true);
            tv.set_message(format!("Opening {site}…").into());
        }
        AppEvent::NowPlaying(sessions) => {
            let mut sessions = sessions;
            sessions.sort_by_key(|x| SOURCES.iter().position(|k| *k == x.source));
            s.sessions.set_vec(sessions.iter().map(session_row).collect::<Vec<_>>());
        }
        AppEvent::Weather(Some(w)) => {
            let c = format::weather_chip(&w);
            laptop.set_weather_text(c.value.into());
            laptop.set_weather_detail(c.detail.into());
        }
        AppEvent::Weather(None) => {
            laptop.set_weather_text("No location".into());
            laptop.set_weather_detail("Set latitude/longitude".into());
        }
        AppEvent::Calendar(state) => {
            s.calendar = Some(state);
            update_calendar(s, laptop, tv);
        }
        AppEvent::Homelab(h) => {
            let c = format::homelab_chip(&h);
            laptop.set_homelab_text(c.value.into());
            laptop.set_homelab_detail(c.detail.into());
            laptop.set_homelab_dot(severity_color(&theme, c.severity));
        }
        AppEvent::Toast(msg) => show_toast(s, laptop, msg, TOAST_FOR),
        AppEvent::LongToast(msg) => show_toast(s, laptop, msg, LONG_TOAST_FOR),
        other => debug!(event = ?other, "event not handled yet"),
    }
}

fn severity_color(theme: &Theme, s: format::Severity) -> slint::Color {
    match s {
        format::Severity::None => slint::Color::default(), // transparent: no dot
        format::Severity::Ok => theme.get_ok(),
        format::Severity::Warn => theme.get_warn(),
        format::Severity::Alert => theme.get_alert(),
    }
}

/// Show a 30 s toast from UI-thread code.
pub fn long_toast(laptop: &LaptopWindow, msg: &str) {
    STATE.with_borrow_mut(|s| show_toast(s, laptop, msg.to_owned(), LONG_TOAST_FOR));
}

/// Re-pick the next event; called on new data and every minute by the clock.
pub fn on_minute(laptop: &LaptopWindow, tv: &TvWindow) {
    STATE.with_borrow(|s| update_calendar(s, laptop, tv));
}

fn update_calendar(s: &UiState, laptop: &LaptopWindow, tv: &TvWindow) {
    let (value, detail, action, up_next) = match &s.calendar {
        None => return,
        Some(CalendarState::NotSetUp) => ("Not set up".into(), String::new(), "Add calendar", String::new()),
        Some(CalendarState::Revoked) => ("Address stopped working".into(), String::new(), "Reconnect", String::new()),
        Some(CalendarState::Events(events)) => {
            let now = chrono::Utc::now();
            match tvdash_widgets::calendar::next(events, now) {
                Some(e) => {
                    let (c, tv_line) = format::event_chip(e, now, s.tz, s.clock.unwrap_or(ClockFormat::H24));
                    (c.value, c.detail, "", tv_line)
                }
                None => ("Nothing today".into(), "or tomorrow".into(), "", String::new()),
            }
        }
    };
    laptop.set_event_text(value.into());
    laptop.set_event_detail(detail.into());
    laptop.set_event_action(action.into());
    tv.set_up_next(up_next.into());
}

fn card(item: &MediaItem, poster: Option<&Image>) -> PosterCardData {
    let subtitle = format::item_detail(item);
    PosterCardData {
        title: item.title.as_str().into(),
        subtitle: subtitle.into(),
        badge: item.source.label().into(),
        progress: item.progress().unwrap_or(0.0),
        has_poster: poster.is_some(),
        poster: poster.cloned().unwrap_or_default(),
    }
}

fn rebuild_rows(s: &mut UiState) {
    // Fixed source order keeps cards from jumping between refreshes.
    s.rows = SOURCES.iter().filter_map(|k| s.continue_watching.get(k)).flatten().cloned().collect();
    let cards: Vec<PosterCardData> =
        s.rows.iter().map(|item| card(item, s.posters.get(&(item.source, item.id.clone())))).collect();
    s.cards.set_vec(cards);
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

fn source_id(kind: SourceKind) -> &'static str {
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

/// Pick what the Continue Watching area shows, and the account prompts.
fn update_cw_state(s: &UiState, laptop: &LaptopWindow) {
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
            laptop.set_signin_code(code.as_str().into());
            laptop.set_signin_source(source_id(kind).into());
            ("code", code_instructions(kind).to_owned())
        }
        Some((kind, _)) => {
            laptop.set_signin_source(source_id(kind).into());
            ("busy", format!("Contacting {}…", kind.label()))
        }
        None if !s.rows.is_empty() => ("items", String::new()),
        None if s.prompts.row_count() > 0 => ("prompts", String::new()),
        None if !s.continue_watching.is_empty() => ("empty", String::new()),
        None => ("loading", String::new()),
    };
    laptop.set_cw_state(state.into());
    laptop.set_cw_message(message.into());
}

fn show_toast(s: &mut UiState, laptop: &LaptopWindow, msg: String, duration: Duration) {
    laptop.set_toast(msg.into());
    let weak = laptop.as_weak();
    let timer = Timer::default();
    timer.start(slint::TimerMode::SingleShot, duration, move || {
        if let Some(l) = weak.upgrade() {
            l.set_toast("".into());
        }
    });
    s.toast_timer = Some(timer); // replacing the old timer cancels it
}
