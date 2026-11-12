//! A show's page: details and up next, the season selector and the episode list.

use slint::{ComponentHandle, ModelRc, VecModel};
use tvdash_core::{
    Command,
    models::{MediaItem, ShowData, ShowSeason, SourceKind},
};

use super::{STATE, UiState, library};
use crate::{
    EpisodeRow, LaptopWindow, SeasonTab, Show, format,
    shelves::{self, UpNext},
    tint::{self, Hue},
};

/// Show page state on the UI thread.
#[derive(Default)]
pub struct ShowUi {
    /// The show asked for (its page shows this while the seasons load).
    pub show: Option<MediaItem>,
    pub data: Option<ShowData>,
    /// Seasons in order, Specials last.
    seasons: Vec<ShowSeason>,
    season: usize,
    /// The chosen season's episodes, in order.
    pub episodes: Vec<MediaItem>,
}

fn still(e: &MediaItem) -> MediaItem {
    MediaItem { id: format!("still:{}", e.id), poster: e.still.clone(), ..Default::default() }
}

fn backdrop(show: &MediaItem) -> MediaItem {
    MediaItem { id: format!("art:show:{}", show.id), poster: show.art.clone(), ..Default::default() }
}

/// Open a show's page (from the Library, Browse all or search).
pub fn open_show(laptop: &LaptopWindow, show: MediaItem) {
    STATE.with_borrow_mut(|s| {
        s.show = ShowUi { show: Some(show.clone()), ..Default::default() };
        let page = laptop.global::<Show>();
        page.set_state("loading".into());
        page.set_picker_open(false);
        page.set_seasons(ModelRc::default());
        page.set_episodes(ModelRc::default());
        fill(s, laptop);
    });
    library::want(vec![show.clone(), backdrop(&show)]);
    library::send(Command::OpenShow(show));
    laptop.global::<crate::Nav>().set_page("show".into());
}

/// The seasons and episodes arrived.
pub(super) fn on_show(s: &mut UiState, laptop: &LaptopWindow, data: ShowData) {
    if s.show.show.as_ref().is_none_or(|x| x.id != data.show.id) {
        return; // a page that's no longer open
    }
    s.show.seasons = shelves::order_seasons(&data.seasons);
    // Open on the season holding the up-next episode.
    let up = shelves::show_up_next(&data.episodes).map(|u| u.item().season.unwrap_or(0));
    s.show.season = up.and_then(|n| s.show.seasons.iter().position(|x| x.index == n)).unwrap_or(0);
    // Keep the poster and art the library row had if the full item lacks them.
    let mut show = data.show.clone();
    if let Some(row) = &s.show.show {
        show.poster = show.poster.or(row.poster.clone());
    }
    s.show.show = Some(show.clone());
    s.show.data = Some(data);
    library::want(vec![backdrop(&show)]);
    laptop.global::<Show>().set_state("ready".into());
    pick_season(s, laptop, s.show.season);
}

pub fn select_season(laptop: &LaptopWindow, index: usize) {
    STATE.with_borrow_mut(|s| pick_season(s, laptop, index));
    laptop.global::<Show>().set_picker_open(false);
}

fn pick_season(s: &mut UiState, laptop: &LaptopWindow, index: usize) {
    let Some(data) = &s.show.data else { return };
    let Some(season) = s.show.seasons.get(index) else { return };
    s.show.season = index;
    s.show.episodes = shelves::season_episodes(&data.episodes, season.index);
    library::want(s.show.episodes.iter().map(still).collect());
    fill(s, laptop);
}

/// A poster, backdrop or still arrived.
pub(super) fn on_poster(s: &UiState, laptop: &LaptopWindow, key: &(SourceKind, String)) {
    let Some(show) = &s.show.show else { return };
    let id = key.1.as_str();
    let ours = id == show.id
        || id == format!("art:show:{}", show.id)
        || id.strip_prefix("still:").is_some_and(|e| s.show.episodes.iter().any(|x| x.id == e));
    if ours {
        fill(s, laptop);
    }
}

fn fill(s: &UiState, laptop: &LaptopWindow) {
    let Some(show) = &s.show.show else { return };
    let page = laptop.global::<Show>();
    let hue = s.hues.get(&(show.source, show.id.clone())).copied().unwrap_or(tint::DEFAULT);
    page.set_tint(tint::card_tint(hue, s.theme));
    page.set_crumb(format!("TV / Series › {}", show.title).into());
    page.set_title(show.title.as_str().into());
    page.set_summary(show.summary.clone().unwrap_or_default().into());
    let art = s.posters.get(&(SourceKind::Plex, format!("art:show:{}", show.id))).cloned();
    page.set_has_backdrop(art.is_some());
    page.set_backdrop(art.unwrap_or_default());

    let seasons = s.show.seasons.iter().filter(|x| x.index > 0).count();
    let mut meta: Vec<String> = Vec::new();
    meta.extend(show.year.map(|y| y.to_string()));
    meta.extend(show.content_rating.clone());
    if seasons > 0 {
        meta.push(format!("{seasons} season{}", if seasons == 1 { "" } else { "s" }));
    }
    meta.extend(show.genres.first().cloned());
    page.set_meta(meta.join(" · ").into());

    let Some(data) = &s.show.data else {
        page.set_up_next("".into());
        page.set_action("".into());
        page.set_first("".into());
        return;
    };
    let (up, action) = match shelves::show_up_next(&data.episodes) {
        Some(UpNext::Resume(e)) => {
            let left = format::time_left(&e).map(|l| format!(" · {l}")).unwrap_or_default();
            (
                format!("Up next: {}{}{left}", shelves::episode_code(&e), dash_title(&e)),
                format!("Resume {} on TV", shelves::episode_code(&e)),
            )
        }
        Some(UpNext::Play(e)) => (
            format!("Up next: {}{}", shelves::episode_code(&e), dash_title(&e)),
            format!("Play {} on TV", shelves::episode_code(&e)),
        ),
        Some(UpNext::FromStart(e)) => ("All watched.".into(), format!("Play {} on TV", shelves::episode_code(&e))),
        None => (String::new(), String::new()),
    };
    page.set_up_next(up.into());
    page.set_action(action.into());
    page.set_first(
        first_episode(data).map(|e| format!("Play from {}", shelves::episode_code(&e))).unwrap_or_default().into(),
    );

    let tabs: Vec<SeasonTab> = s
        .show
        .seasons
        .iter()
        .map(|x| {
            let (title, short) = shelves::season_names(x);
            SeasonTab {
                title: title.into(),
                short: short.into(),
                progress_text: shelves::season_progress(x).into(),
                progress: if x.leaf_count == 0 { 0.0 } else { x.viewed_leaf_count as f32 / x.leaf_count as f32 },
            }
        })
        .collect();
    page.set_selector(shelves::season_selector(tabs.len()).into());
    let done = s.show.seasons.iter().filter(|x| x.leaf_count > 0 && x.viewed_leaf_count >= x.leaf_count).count();
    page.set_seasons_text(format!("{} seasons · {done} watched", s.show.seasons.len()).into());
    page.set_seasons(ModelRc::new(VecModel::from(tabs)));
    page.set_season(s.show.season as i32);

    let current = shelves::show_up_next(&data.episodes).and_then(|u| match u {
        UpNext::Resume(e) => Some(e.id),
        _ => None,
    });
    let rows: Vec<EpisodeRow> = s
        .show
        .episodes
        .iter()
        .map(|e| {
            let key = (SourceKind::Plex, format!("still:{}", e.id));
            let img = s.posters.get(&key).cloned();
            let h = s.hues.get(&(show.source, show.id.clone())).copied().unwrap_or(tint::DEFAULT);
            let mut line: Vec<String> = Vec::new();
            line.extend(e.duration_ms.filter(|&d| d > 0).map(format::length));
            if e.resume_ms.is_some() {
                line.extend(format::time_left(e));
            } else if e.view_count > 0 {
                line.push("watched".into());
            }
            let number = match (e.season, e.episode) {
                (Some(0), Some(n)) => format!("S{n}"),
                (_, Some(n)) => format!("{n:02}"),
                _ => String::new(),
            };
            EpisodeRow {
                number: number.into(),
                title: e.subtitle.clone().unwrap_or_default().into(),
                line: line.join(" · ").into(),
                summary: e.summary.clone().unwrap_or_default().replace('\n', " ").into(),
                has_still: img.is_some(),
                still: img.unwrap_or_default(),
                placeholder: tint::card_tint(Hue { saturation: h.saturation.max(0.3), ..h }, s.theme).poster,
                accent: tint::progress_color(h),
                watched: e.view_count > 0 && e.resume_ms.is_none(),
                progress: e.progress().unwrap_or(0.0),
                current: current.as_deref() == Some(e.id.as_str()),
            }
        })
        .collect();
    page.set_episodes(ModelRc::new(VecModel::from(rows)));
}

/// " — The Breakwater" when the episode has a title.
fn dash_title(e: &MediaItem) -> String {
    e.subtitle.as_deref().filter(|t| !t.is_empty()).map(|t| format!(" — {t}")).unwrap_or_default()
}

/// The first regular episode (S1 · E1), from the start.
fn first_episode(data: &ShowData) -> Option<MediaItem> {
    let mut v: Vec<&MediaItem> = data.episodes.iter().filter(|e| e.season.unwrap_or(0) > 0).collect();
    v.sort_by_key(|e| (e.season, e.episode));
    v.first().map(|e| MediaItem { resume_ms: None, ..(*e).clone() })
}

pub fn up_next() -> Option<MediaItem> {
    STATE.with_borrow(|s| {
        s.show.data.as_ref().and_then(|d| shelves::show_up_next(&d.episodes)).map(|u| u.item().clone())
    })
}

pub fn first() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.show.data.as_ref().and_then(first_episode))
}

/// Shuffle episodes: any regular episode, from the start (or where it was left).
pub fn shuffle() -> Option<MediaItem> {
    STATE.with_borrow(|s| {
        let eps: Vec<&MediaItem> =
            s.show.data.as_ref()?.episodes.iter().filter(|e| e.season.unwrap_or(0) > 0).collect();
        eps.get(shelves::random(eps.len())).map(|e| (*e).clone())
    })
}

pub fn episode(index: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| s.show.episodes.get(index).cloned())
}
