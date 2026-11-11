//! Moves `AppEvent`s from background tasks onto the UI thread and keeps the
//! UI-side state (rows, posters, account status) that models are built from.

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, VecModel, Weak};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::debug;
use tvdash_core::{
    AppEvent,
    config::ClockFormat,
    models::{AccountStatus, MediaItem, SignInState, SourceKind},
};

use crate::{LaptopWindow, PosterCardData, Theme, TvWindow, format};

const TOAST_FOR: Duration = Duration::from_secs(5);

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

fn apply(s: &mut UiState, laptop: &LaptopWindow, _tv: &TvWindow, event: AppEvent) {
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
                show_toast(s, laptop, format!("Signed in to {}", kind.label()));
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
            laptop.set_paused(status.state == tvdash_core::models::PlayState::Paused);
            laptop.set_transport_enabled(true);
        }
        AppEvent::Player(None) => {
            laptop.set_now_playing_title("Nothing playing".into());
            laptop.set_now_playing_detail("The TV is showing the idle screen.".into());
            laptop.set_now_playing_progress(0.0);
            laptop.set_paused(false);
            laptop.set_transport_enabled(false);
        }
        AppEvent::Toast(msg) => show_toast(s, laptop, msg),
        other => debug!(event = ?other, "event not handled yet"),
    }
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
    s.rows = [SourceKind::Jellyfin, SourceKind::Plex]
        .iter()
        .filter_map(|k| s.continue_watching.get(k))
        .flatten()
        .cloned()
        .collect();
    let cards: Vec<PosterCardData> = s
        .rows
        .iter()
        .map(|item| card(item, s.posters.get(&(item.source, item.id.clone()))))
        .collect();
    s.cards.set_vec(cards);
}

/// Pick what the Continue Watching area shows. Jellyfin drives it in M2.
fn update_cw_state(s: &UiState, laptop: &LaptopWindow) {
    let kind = SourceKind::Jellyfin;
    let (state, message) = match (s.sign_in.get(&kind), s.accounts.get(&kind)) {
        (Some(SignInState::Starting), _) => ("busy", String::new()),
        (Some(SignInState::Code(code)), _) => {
            laptop.set_signin_code(code.as_str().into());
            ("code", String::new())
        }
        (Some(SignInState::Failed(e)), _) => ("error", format!("Jellyfin sign-in failed: {e}")),
        _ if !s.rows.is_empty() => ("items", String::new()),
        (_, Some(AccountStatus::NotSetUp)) => ("signin", String::new()),
        (_, Some(AccountStatus::Expired)) => ("expired", String::new()),
        (_, Some(AccountStatus::Unreachable(e))) => ("error", format!("Can't reach Jellyfin: {e}")),
        (_, Some(AccountStatus::Connected)) if s.continue_watching.contains_key(&kind) => ("empty", String::new()),
        _ => ("loading", String::new()),
    };
    laptop.set_cw_state(state.into());
    laptop.set_cw_message(message.into());
}

fn show_toast(s: &mut UiState, laptop: &LaptopWindow, msg: String) {
    laptop.set_toast(msg.into());
    let weak = laptop.as_weak();
    let timer = Timer::default();
    timer.start(slint::TimerMode::SingleShot, TOAST_FOR, move || {
        if let Some(l) = weak.upgrade() {
            l.set_toast("".into());
        }
    });
    s.toast_timer = Some(timer); // replacing the old timer cancels it
}
