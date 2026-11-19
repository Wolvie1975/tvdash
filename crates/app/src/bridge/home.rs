//! Home: Continue Watching cards, the On the TV card, other devices' sessions,
//! streaming tiles, and posters (whose arrival also updates the Library's cards).

use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};
use tvdash_core::{
    config::{ClockFormat, ThemeMode},
    events::PlayerStatus,
    models::{AccountStatus, MediaItem, MediaKind, PlayState, RgbaImage, Session, SignInState, SourceKind},
};

use super::{SOURCES, UiState, accounts, in_order, to_image};
use crate::{
    AccountPrompt, CollectionPageState, Dash, LaptopWindow, Library, PosterCardData, SessionRow, ShowPageState,
    SiteTileData, TvCard, TvWindow, format,
    tint::{self, Hue},
};

/// The item behind dashboard Continue Watching card `index`.
pub fn row(index: usize) -> Option<MediaItem> {
    super::STATE.with_borrow(|s| s.rows.get(index).cloned())
}

/// The top Continue Watching item ("UP NEXT" on the On the TV card).
pub fn up_next() -> Option<MediaItem> {
    super::STATE.with_borrow(|s| in_order(&s.continue_watching).into_iter().next())
}

/// Switch Continue Watching between TV / Series (`"tv"`) and Movies (`"movies"`).
pub fn set_cw_filter(laptop: &LaptopWindow, filter: String) {
    super::STATE.with_borrow_mut(|s| {
        s.cw_filter = filter.clone();
        laptop.global::<Dash>().set_cw_filter(filter.into());
        rebuild_rows(s);
        update_cw_state(s, laptop);
    });
}

// ── Cards ───────────────────────────────────────────────────────────

fn card(item: &MediaItem, poster: Option<&Image>, hue: Option<Hue>, theme: ThemeMode) -> PosterCardData {
    let hue = hue.unwrap_or(tint::DEFAULT);
    let badge_text = if item.kind == MediaKind::Movie {
        format::time_left(item).unwrap_or_else(|| format::item_detail(item))
    } else {
        format::item_detail(item)
    };
    PosterCardData {
        accent: tint::progress_color(hue),
        placeholder: tint::card_tint(Hue { saturation: hue.saturation.max(0.3), ..hue }, theme).poster,
        title: item.title.as_str().into(),
        subtitle: badge_text.into(),
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
            card(item, s.posters.get(&key), s.hues.get(&key).copied(), s.effective_theme())
        })
        .collect()
}

pub(super) fn rebuild_rows(s: &mut UiState) {
    let all = in_order(&s.continue_watching);
    if s.tv_layout || s.cw_filter == "all" {
        s.rows = all.into_iter().take(6).collect();
    } else {
        let movies = s.cw_filter == "movies";
        s.rows = all
            .into_iter()
            .filter(|i| if movies { i.kind == MediaKind::Movie } else { i.kind != MediaKind::Movie })
            .take(6)
            .collect();
    }
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
        (&s.lib_added_week, &s.lib_added_week_cards),
        (&s.lib_watch_again, &s.lib_watch_again_cards),
        (&s.lib_browse, &s.lib_browse_cards),
    ];
    for (items, model) in models {
        for (i, item) in items.iter().enumerate() {
            if item.source == source && item.id == key.1 {
                model.set_row_data(i, card(item, Some(&img), Some(hue), s.effective_theme()));
            }
        }
    }
    if s.last_played.as_ref().is_some_and(|lp| matches!(lp, super::LastPlayed::Media(item) if item.source == source && item.id == key.1)) {
        refresh_last_played(s, laptop);
    }
    if s.selected.as_ref().is_some_and(|i| i.source == source && i.id == key.1) {
        let lib = laptop.global::<Library>();
        let mut d = lib.get_detail();
        d.poster = img.clone();
        d.has_poster = true;
        lib.set_detail(d);
    }
    s.posters.insert(key.clone(), img.clone());
    let sp = laptop.global::<ShowPageState>();
    if sp.get_open() {
        let eps = sp.get_episodes();
        for i in 0..eps.row_count() {
            if let Some(mut row) = eps.row_data(i)
                && row.rating_key == key.1.as_str()
            {
                row.has_thumb = true;
                row.thumb = img.clone();
                eps.set_row_data(i, row);
            }
        }
    }
    if laptop.global::<CollectionPageState>().get_open()
        && s.collection_detail.as_ref().is_some_and(|d| d.items.iter().any(|i| i.id == key.1))
    {
        super::library::rebuild_collection_items(s, laptop);
    }
    refresh_tv_card(s, laptop);
}

// ── Streaming tiles ─────────────────────────────────────────────────

/// (background, foreground) of a site's monogram in the current theme.
pub(super) fn site_colors(s: &UiState, index: usize) -> (slint::Color, slint::Color) {
    let name = s.sites.get(index).map(|x| x.name.as_str()).unwrap_or_default();
    tint::site_colors(tint::site_hue(name, index), s.effective_theme())
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

    let now = chrono::Utc::now().timestamp();
    let visible: Vec<(usize, String, &tvdash_core::config::StreamingSite)> = s
        .sites
        .iter()
        .enumerate()
        .filter(|(_, site)| !site.hidden)
        .map(|(i, site)| (i, crate::brand::service_id(&site.name), site))
        .collect();

    let tv_services: Vec<crate::TvServiceTile> = visible
        .iter()
        .map(|(i, id, site)| {
            let recent_entry = s.recent.iter().find(|r| r.id == *id);
            crate::TvServiceTile {
                index: *i as i32,
                brand: crate::brand::brand_for(id, &site.name),
                is_recent: recent_entry.is_some(),
                ago_text: recent_entry.map(|r| crate::recent::ago(r.at, now)).unwrap_or_default().into(),
            }
        })
        .collect();

    let fixed_ids: Vec<String> = visible.iter().map(|(_, id, _)| id.clone()).collect();
    let home_ids = crate::recent::home_three(&s.recent, &fixed_ids);
    let home_services: Vec<crate::TvServiceTile> = home_ids
        .into_iter()
        .filter_map(|id| tv_services.iter().find(|t| t.brand.id.as_str() == id).cloned())
        .collect();

    let tv_ui = laptop.global::<crate::TvUi>();
    tv_ui.set_services(ModelRc::new(VecModel::from(tv_services)));
    tv_ui.set_home_services(ModelRc::new(VecModel::from(home_services)));
    refresh_last_played(s, laptop);
}

pub(super) fn refresh_last_played(s: &UiState, laptop: &LaptopWindow) {
    let tv = laptop.global::<crate::TvUi>();
    let target = s.last_played.clone().or_else(|| {
        s.rows.first().cloned().map(super::LastPlayed::Media).or_else(|| {
            s.recent.first().and_then(|r| {
                s.sites
                    .iter()
                    .position(|site| !site.hidden && crate::brand::service_id(&site.name) == r.id)
                    .map(super::LastPlayed::Site)
            })
        })
    });
    match target {
        Some(super::LastPlayed::Media(item)) => {
            let key = (item.source, item.id.clone());
            let poster = s.posters.get(&key);
            tv.set_last_played_kind("media".into());
            tv.set_last_played_title(item.title.as_str().into());
            tv.set_last_played_subtitle(crate::format::item_detail(&item).into());
            tv.set_last_played_has_poster(poster.is_some());
            tv.set_last_played_poster(poster.cloned().unwrap_or_default());
            tv.set_last_played_progress(item.progress().unwrap_or(0.0));
        }
        Some(super::LastPlayed::Site(idx)) => {
            if let Some(site) = s.sites.get(idx) {
                let id = crate::brand::service_id(&site.name);
                let brand = crate::brand::brand_for(&id, &site.name);
                tv.set_last_played_kind("site".into());
                tv.set_last_played_title(site.name.as_str().into());
                tv.set_last_played_subtitle("Click or press OK to launch".into());
                tv.set_last_played_has_poster(false);
                tv.set_last_played_progress(0.0);
                tv.set_last_played_brand(brand);
            }
        }
        None => {
            tv.set_last_played_kind("".into());
            tv.set_last_played_title("".into());
            tv.set_last_played_subtitle("".into());
            tv.set_last_played_has_poster(false);
            tv.set_last_played_progress(0.0);
        }
    }
}

// ── On the TV ───────────────────────────────────────────────────────

pub(super) fn on_player(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, status: Option<PlayerStatus>) {
    match &status {
        Some(st) => {
            if s.player.as_ref().is_none_or(|p| p.item.id != st.item.id) {
                tv.set_message(format!("Starting {}…", st.item.title).into());
            }
            s.last_played = Some(super::LastPlayed::Media(st.item.clone()));
        }
        None => tv.set_message("".into()),
    }
    s.player = status;
    refresh_tv_card(s, laptop);
    refresh_last_played(s, laptop);
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
    dash.set_tint(tint::card_tint(hue, s.effective_theme()));
    dash.set_tv(card);
    let ends_at = s
        .player
        .as_ref()
        .and_then(|st| {
            let dur = st.duration_ms?;
            let rem = dur.saturating_sub(st.position_ms).min(i64::MAX as u64) as i64;
            let end = format::now_local(s.tz) + chrono::TimeDelta::milliseconds(rem);
            Some(format::hm(end, s.clock.unwrap_or(ClockFormat::H24)))
        })
        .unwrap_or_default();
    dash.set_tv_ends_at(ends_at.into());
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
            is_episode: st.item.kind == MediaKind::Episode,
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
    if let Some(item) = in_order(&s.continue_watching).first() {
        let (poster, hue) = poster_of(item);
        let card = TvCard {
            mode: "next".into(),
            label: "CURRENTLY PLAYING".into(),
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
