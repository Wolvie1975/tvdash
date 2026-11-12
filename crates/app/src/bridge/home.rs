//! Home: Continue Watching cards, the On the TV card, other devices' sessions,
//! streaming tiles, and posters (whose arrival also updates the Library's cards).

use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};
use tvdash_core::{
    config::{ClockFormat, ThemeMode},
    events::PlayerStatus,
    models::{AccountStatus, MediaItem, PlayState, RgbaImage, Session, SignInState, SourceKind},
};

use super::{SOURCES, UiState, accounts, in_order, to_image};
use crate::{
    AccountPrompt, Dash, LaptopWindow, Library, PosterCardData, SessionRow, SiteTileData, TvCard, TvWindow, format,
    tint::{self, Hue},
};

/// The item behind dashboard Continue Watching card `index`.
pub fn row(index: usize) -> Option<MediaItem> {
    super::STATE.with_borrow(|s| s.rows.get(index).cloned())
}

// ── Cards ───────────────────────────────────────────────────────────

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

/// Poster cards for `items`, with whatever posters and tints have arrived.
pub(super) fn cards_for(s: &UiState, items: &[MediaItem]) -> Vec<PosterCardData> {
    items
        .iter()
        .map(|item| {
            let key = (item.source, item.id.clone());
            card(item, s.posters.get(&key), s.hues.get(&key).copied(), s.theme)
        })
        .collect()
}

pub(super) fn rebuild_rows(s: &mut UiState) {
    s.rows = in_order(&s.continue_watching);
    s.cards.set_vec(cards_for(s, &s.rows));
}

/// A poster arrived: fill it into every card (Home and Library) and the detail panel showing it.
pub(super) fn on_poster(
    s: &mut UiState,
    laptop: &LaptopWindow,
    source: SourceKind,
    item_id: String,
    image: &RgbaImage,
) {
    let key = (source, item_id);
    let hue = tint::poster_hue(image);
    s.hues.insert(key.clone(), hue);
    let img = to_image(image);
    let models = [
        (&s.rows, &s.cards),
        (&s.lib_continue, &s.lib_continue_cards),
        (&s.lib_recent, &s.lib_recent_cards),
        (&s.lib_results, &s.lib_result_cards),
    ];
    for (items, model) in models {
        for (i, item) in items.iter().enumerate() {
            if item.source == source && item.id == key.1 {
                model.set_row_data(i, card(item, Some(&img), Some(hue), s.theme));
            }
        }
    }
    if s.selected.as_ref().is_some_and(|i| i.source == source && i.id == key.1) {
        let lib = laptop.global::<Library>();
        let mut d = lib.get_detail();
        d.poster = img.clone();
        d.has_poster = true;
        lib.set_detail(d);
    }
    s.posters.insert(key, img);
    refresh_tv_card(s, laptop);
}

// ── Streaming tiles ─────────────────────────────────────────────────

/// (background, foreground) of a site's monogram in the current theme.
pub(super) fn site_colors(s: &UiState, index: usize) -> (slint::Color, slint::Color) {
    let name = s.sites.get(index).map(|x| x.name.as_str()).unwrap_or_default();
    tint::site_colors(tint::site_hue(name, index), s.theme)
}

/// First letter of a site name ("Disney+" → "D", "Apple TV+" → "A").
pub(super) fn monogram(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

pub(super) fn refresh_tiles(s: &UiState, laptop: &LaptopWindow) {
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

// ── On the TV ───────────────────────────────────────────────────────

pub(super) fn on_player(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, status: Option<PlayerStatus>) {
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

pub(super) fn on_browser(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, site: Option<String>) {
    tv.set_message(site.as_ref().map(|n| format!("Opening {n}…")).unwrap_or_default().into());
    s.browser = site;
    refresh_tv_card(s, laptop);
}

/// Build the On the TV card: mpv playback, else the browser, else the top
/// Continue Watching item ("up next"), else idle. Its tint follows the poster.
pub(super) fn refresh_tv_card(s: &UiState, laptop: &LaptopWindow) {
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

// ── Other devices ───────────────────────────────────────────────────

pub(super) fn on_sessions(s: &UiState, mut sessions: Vec<Session>) {
    sessions.sort_by_key(|x| SOURCES.iter().position(|k| *k == x.source));
    s.sessions.set_vec(sessions.iter().map(session_row).collect::<Vec<_>>());
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

// ── Continue Watching area and account prompts ──────────────────────

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
        source: accounts::source_id(kind).into(),
        message: message.into(),
        action: action.into(),
        action_id: action_id.into(),
        warn,
    })
}

/// Pick what the dashboard's Continue Watching area shows, and the account prompts.
pub(super) fn update_cw_state(s: &UiState, laptop: &LaptopWindow) {
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
            dash.set_signin_source(accounts::source_id(kind).into());
            ("code", accounts::code_instructions(kind).to_owned())
        }
        Some((kind, _)) => {
            dash.set_signin_source(accounts::source_id(kind).into());
            ("busy", format!("Contacting {}…", kind.label()))
        }
        None if !s.rows.is_empty() => ("items", String::new()),
        None if s.prompts.row_count() > 0 => ("prompts", String::new()),
        None if !s.continue_watching.is_empty() => ("empty", String::new()),
        None => ("loading", String::new()),
    };
    dash.set_cw_state(state.into());
    dash.set_cw_message(message.into());
    accounts::refresh(s, laptop);
}
