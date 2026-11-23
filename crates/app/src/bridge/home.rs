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
            card(item, s.posters.peek(&key), s.hues.get(&key).copied(), s.effective_theme())
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
    let ll = laptop.global::<crate::LibraryLanding>();
    let shelf2 = ll.get_shelf2();
    for i in 0..shelf2.row_count() {
        if let Some(mut row) = shelf2.row_data(i)
            && row.rating_key.as_str() == key.1
        {
            row.has_poster = true;
            row.poster = img.clone();
            shelf2.set_row_data(i, row);
        }
    }
    if source == SourceKind::Plex {
        let cols = laptop.global::<Library>().get_collections();
        for i in 0..cols.row_count() {
            if let Some(mut row) = cols.row_data(i)
                && row.rating_key.as_str() == key.1
            {
                row.has_poster = true;
                row.poster = img.clone();
                cols.set_row_data(i, row);
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
    let evicted = s.posters.insert(key.clone(), img.clone());
    for k in &evicted {
        s.hues.remove(k);
    }
    s.forget_images(evicted, Vec::new(), Vec::new());
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
        if key.1 == sp.get_rating_key().as_str() || key.1 == sp.get_season().rating_key.as_str() {
            super::library::refresh_show_season_poster(s, laptop);
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
                brand: crate::brand::brand_for_with_logo(id, &site.name, s.logos.get(&site.name).cloned()),
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
    laptop
        .global::<crate::AppsPage>()
        .set_apps(ModelRc::new(VecModel::from(crate::apps::apps_page_tiles(&s.sites, &s.logos))));
    let recent_ids: Vec<String> = s.recent.iter().map(|r| r.id.clone()).collect();
    let bar_ids = crate::apps::apps_bar(&recent_ids, &fixed_ids, 7);
    let apps_bar: Vec<crate::AppTile> = bar_ids
        .iter()
        .filter_map(|id| {
            visible
                .iter()
                .find(|(_, vid, _)| vid == id)
                .map(|(_, _, site)| crate::apps::app_tile(site, &s.logos))
        })
        .collect();
    laptop.global::<crate::HomeState>().set_apps_bar(ModelRc::new(VecModel::from(apps_bar)));
    refresh_last_played(s, laptop);
}

/// Find the config index of a streaming site by normalized service ID.
pub fn site_index_by_id(id: &str) -> Option<usize> {
    super::STATE.with_borrow(|s| {
        s.sites
            .iter()
            .position(|site| !site.hidden && crate::brand::service_id(&site.name) == id)
    })
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
            let poster = s.posters.peek(&key);
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
                let brand = crate::brand::brand_for_with_logo(&id, &site.name, s.logos.get(&site.name).cloned());
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
pub(super) fn refresh_tv_card(s: &mut UiState, laptop: &LaptopWindow) {
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
    refresh_home_state(s, laptop);
}

/// Update `HomeState.whats_on` and `HomeState.up_next` (§3–§4).
pub(super) fn refresh_home_state(s: &mut UiState, laptop: &LaptopWindow) {
    use crate::whats_on::{HeroPick, hero_status, hero_subtitle};

    let cw = in_order(&s.continue_watching);
    let recent = in_order(&s.recently_added);
    let last_media = match &s.last_played {
        Some(super::LastPlayed::Media(m)) => Some(m),
        _ => None,
    };

    // Determine the first Up Next candidate in case we fall through to rule 4.
    let first_candidate = s
        .next_ep
        .as_ref()
        .or_else(|| cw.first())
        .or_else(|| recent.first());

    let pick = crate::whats_on::pick_hero(s.player.as_ref(), s.browser.as_deref(), last_media, first_candidate);
    let now = format::now_local(s.tz);
    let clock = s.clock.unwrap_or(ClockFormat::H24);
    let status = hero_status(&pick, now, clock);

    let art_of = |item: &MediaItem| {
        let key = (item.source, item.id.clone());
        s.backdrops.peek(&key).or_else(|| s.posters.peek(&key)).cloned()
    };

    let (whats_on, hero_id, hero_is_ep) = match &pick {
        HeroPick::Player(st) => {
            let art = art_of(&st.item);
            s.hero_item = None;
            (
                crate::WhatsOn {
                    has_item: true,
                    source: st.item.source.label().to_uppercase().into(),
                    title: st.item.title.as_str().into(),
                    subtitle: hero_subtitle(&st.item).into(),
                    plot: st.item.summary.as_deref().unwrap_or_default().into(),
                    has_art: art.is_some(),
                    art: art.unwrap_or_default(),
                    progress: format::fraction(st),
                    status: status.into(),
                    is_app: false,
                    ..Default::default()
                },
                Some(st.item.id.clone()),
                st.item.kind == MediaKind::Episode,
            )
        }
        HeroPick::Browser(site) => {
            let id = crate::brand::service_id(site);
            let b = crate::brand::brand_for_with_logo(&id, site, s.logos.get(*site).cloned());
            s.hero_item = None;
            (
                crate::WhatsOn {
                    has_item: true,
                    source: site.to_uppercase().into(),
                    title: (*site).into(),
                    subtitle: "Open on the TV".into(),
                    plot: "".into(),
                    progress: -1.0,
                    status: status.into(),
                    is_app: true,
                    app_color: b.fill,
                    app_logo: b.logo,
                    app_has_logo: b.has_logo,
                    ..Default::default()
                },
                None,
                false,
            )
        }
        HeroPick::Unfinished(item) | HeroPick::UpNext(item) => {
            let art = art_of(item);
            s.hero_item = Some((*item).clone());
            (
                crate::WhatsOn {
                    has_item: true,
                    source: item.source.label().to_uppercase().into(),
                    title: item.title.as_str().into(),
                    subtitle: hero_subtitle(item).into(),
                    plot: item.summary.as_deref().unwrap_or_default().into(),
                    has_art: art.is_some(),
                    art: art.unwrap_or_default(),
                    progress: item.progress().filter(|&p| p > 0.0).unwrap_or(-1.0),
                    status: status.into(),
                    is_app: false,
                    ..Default::default()
                },
                Some(item.id.clone()),
                item.kind == MediaKind::Episode,
            )
        }
        HeroPick::Empty => {
            s.hero_item = None;
            (
                crate::WhatsOn {
                    has_item: false,
                    progress: -1.0,
                    ..Default::default()
                },
                None,
                false,
            )
        }
    };

    let hero_next_ep = if hero_is_ep {
        s.next_ep
            .as_ref()
            .filter(|ep| hero_id.as_deref() != Some(ep.id.as_str()))
    } else {
        None
    };

    let up_next_items = crate::up_next::build_up_next(hero_id.as_deref(), hero_next_ep, &cw, &recent);
    let up_next_cards: Vec<crate::UpNextCard> = up_next_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let thumb = art_of(item);
            crate::UpNextCard {
                key: item.id.as_str().into(),
                title: item.title.as_str().into(),
                meta: crate::up_next::up_next_meta(item).into(),
                has_thumb: thumb.is_some(),
                thumb: thumb.unwrap_or_default(),
                progress: item.progress().unwrap_or(0.0),
                up_next: i == 0,
            }
        })
        .collect();
    s.up_next_items = up_next_items;

    let just_added = recent.into_iter().next();
    let just_added_card = match &just_added {
        Some(item) => {
            let key = (item.source, item.id.clone());
            let art = s.posters.peek(&key).or_else(|| s.backdrops.peek(&key)).cloned();
            let kind = match item.kind {
                MediaKind::Episode => "EPISODE",
                MediaKind::Series => "SERIES",
                MediaKind::Movie => "MOVIE",
                MediaKind::Other => "MEDIA",
            };
            crate::JustAddedCard {
                visible: true,
                kind: kind.into(),
                source: item.source.label().to_uppercase().into(),
                title: item.title.as_str().into(),
                subtitle: format::item_detail(item).into(),
                meta: format::item_meta(item).into(),
                has_art: art.is_some(),
                art: art.unwrap_or_default(),
            }
        }
        None => crate::JustAddedCard::default(),
    };
    s.just_added_item = just_added;

    let hs = laptop.global::<crate::HomeState>();
    hs.set_whats_on(whats_on);
    hs.set_up_next(ModelRc::new(VecModel::from(up_next_cards)));
    hs.set_just_added(just_added_card);
}

pub fn just_added_item() -> Option<MediaItem> {
    super::STATE.with_borrow(|s| s.just_added_item.clone())
}

pub enum HeroAction {
    FocusPlayer,
    OpenSite(usize),
    Play(MediaItem),
    OpenSearch,
}

pub fn hero_action() -> HeroAction {
    super::STATE.with_borrow(|s| {
        if s.player.is_some() {
            return HeroAction::FocusPlayer;
        }
        if let Some(site) = &s.browser
            && let Some(idx) = s.sites.iter().position(|x| x.name == *site)
        {
            return HeroAction::OpenSite(idx);
        }
        if let Some(item) = &s.hero_item {
            return HeroAction::Play(item.clone());
        }
        HeroAction::OpenSearch
    })
}

pub fn up_next_item_by_key(key: &str) -> Option<MediaItem> {
    super::STATE.with_borrow(|s| s.up_next_items.iter().find(|i| i.id == key).cloned())
}

fn parse_hex_color(hex: &str) -> slint::Color {
    let s = hex.trim_start_matches('#');
    if s.len() == 6
        && let Ok(v) = u32::from_str_radix(s, 16)
    {
        return slint::Color::from_rgb_u8(((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8);
    }
    slint::Color::from_rgb_u8(0x28, 0x30, 0x3D)
}

pub(super) fn on_next_game(
    s: &mut UiState,
    laptop: &LaptopWindow,
    dto: Option<tvdash_core::events::NextGameDto>,
) {
    s.next_game = dto;
    refresh_next_game(s, laptop);
}

pub(super) fn refresh_next_game(s: &UiState, laptop: &LaptopWindow) {
    let hs = laptop.global::<crate::HomeState>();
    let Some(g) = &s.next_game else {
        hs.set_next_game(crate::NextGame {
            visible: false,
            ..Default::default()
        });
        return;
    };
    let to_side = |side: &tvdash_core::events::NextGameSideDto| {
        let logo = side.logo_url.as_ref().and_then(|u| s.sports_logos.peek(u)).cloned();
        crate::GameSide {
            rank: side.rank.as_str().into(),
            init: side.init.as_str().into(),
            has_logo: logo.is_some(),
            logo: logo.unwrap_or_default(),
            color: parse_hex_color(&side.color),
            name: side.name.as_str().into(),
            score: side.score.as_str().into(),
            lead: side.lead,
        }
    };
    let state = match g.state.as_str() {
        "live" => crate::GameState::Live,
        "final" => crate::GameState::Final,
        _ => crate::GameState::Upcoming,
    };
    hs.set_next_game(crate::NextGame {
        visible: true,
        event_id: g.event_id.as_str().into(),
        date: g.date.as_str().into(),
        league: g.league.as_str().into(),
        state,
        state_text: g.state_text.as_str().into(),
        away: to_side(&g.away),
        home: to_side(&g.home),
        meta: g.meta.as_str().into(),
    });
}

fn hero_art_and_hue(s: &mut UiState, item: &MediaItem) -> (Option<Image>, Hue) {
    let key = (item.source, item.id.clone());
    let art = s.backdrops.get(&key).or_else(|| s.posters.get(&key));
    (art, s.hues.get(&key).copied().unwrap_or(tint::DEFAULT))
}

fn tv_card(s: &mut UiState) -> (TvCard, Hue) {
    let base = TvCard { label: "ON THE TV".into(), ..Default::default() };
    if let Some(st) = s.player.clone() {
        let (poster, hue) = hero_art_and_hue(s, &st.item);
        let now = format::now_local(s.tz);
        let (at, left) = format::tv_times(&st, now, s.clock.unwrap_or(ClockFormat::H24));
        let card = TvCard {
            mode: "playing".into(),
            source: st.item.source.label().into(),
            title: st.item.title.as_str().into(),
            subtitle: format::item_detail(&st.item).into(),
            summary: st.item.summary.as_deref().unwrap_or_default().into(),
            at_text: at.into(),
            left_text: left.into(),
            progress: format::fraction(&st),
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
    if let Some(item) = in_order(&s.continue_watching).into_iter().next() {
        let (poster, hue) = hero_art_and_hue(s, &item);
        let card = TvCard {
            mode: "next".into(),
            label: "CURRENTLY PLAYING".into(),
            source: item.source.label().into(),
            note: "Nothing on the TV. Pick up where you left off:".into(),
            title: item.title.as_str().into(),
            subtitle: format::item_detail(&item).into(),
            summary: item.summary.as_deref().unwrap_or_default().into(),
            at_text: item.resume_ms.map(|p| format!("Stopped at {}", format::duration(p))).unwrap_or_default().into(),
            left_text: format::time_left(&item).unwrap_or_default().into(),
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
