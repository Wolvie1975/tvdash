//! The Library page: Continue Watching and Recently Added from both servers,
//! the source filter, search, and the detail panel.

use std::cell::RefCell;
use std::time::Duration;

use chrono::{Datelike, Local};
use rand::{SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use tvdash_core::models::{
    CollectionDetail, EpisodeInfo, LibraryShelves, MediaItem, MediaKind, PlayState, RgbaImage, Session, ShowDetail,
    SourceKind,
};

use super::{STATE, UiState, home::cards_for, in_order, to_image};
use crate::{
    CollectionCardData, CollectionItemData, CollectionPageState, DetailData, EpisodeRowData, FinishSeasonRow,
    FocusInfoData, GenreTileData, LaptopWindow, Library, LibraryLanding, PopularCardData, RankedTileData,
    SeasonColumnData, SeasonTabData, Shelf1TileData, ShowPageState, WatchingNowRowData, format, tint,
};

fn filter_items(items: Vec<MediaItem>, filter: &str) -> Vec<MediaItem> {
    match filter {
        "plex" => items.into_iter().filter(|i| i.source == SourceKind::Plex).collect(),
        _ => items,
    }
}

fn sort_title(title: &str) -> &str {
    let t = title.trim_start();
    let lower = t.to_ascii_lowercase();
    for prefix in ["the ", "an ", "a "] {
        if lower.starts_with(prefix) {
            let rest = t[prefix.len()..].trim_start();
            if !rest.is_empty() {
                return rest;
            }
        }
    }
    t
}

fn matches_letter(title: &str, letter: &str) -> bool {
    if letter.is_empty() {
        return true;
    }
    let Some(first) = sort_title(title).chars().next() else {
        return false;
    };
    if letter == "#" {
        return !first.is_ascii_alphabetic();
    }
    first.to_ascii_uppercase().to_string() == letter
}

fn browse_sort_key(title: &str) -> (u8, String) {
    let trimmed = sort_title(title);
    let group = match trimmed.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => 1,
        _ => 0,
    };
    (group, trimmed.to_ascii_lowercase())
}

fn rebuild_browse(s: &mut UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let filter = lib.get_filter();
    let letter = lib.get_browse_letter();
    let desc = lib.get_sort() == "title_desc";
    let movies = lib.get_media_tab() == "movies";
    let mut items: Vec<MediaItem> = filter_items(s.lib_browse_all.clone(), &filter)
        .into_iter()
        .filter(|i| if movies { i.kind == MediaKind::Movie } else { i.kind != MediaKind::Movie })
        .filter(|i| matches_letter(&i.title, &letter))
        .collect();
    items.sort_by(|a, b| {
        let ord = browse_sort_key(&a.title).cmp(&browse_sort_key(&b.title));
        if desc { ord.reverse() } else { ord }
    });
    s.lib_browse = items;
    s.lib_browse_cards.set_vec(cards_for(s, &s.lib_browse));
}

pub(super) fn rebuild(s: &mut UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let filter = lib.get_filter();
    let movies = lib.get_media_tab() == "movies";
    s.lib_continue = filter_items(in_order(&s.continue_watching), &filter)
        .into_iter()
        .filter(|i| if movies { i.kind == MediaKind::Movie } else { i.kind != MediaKind::Movie })
        .take(6)
        .collect();
    s.lib_recent = filter_items(in_order(&s.recently_added), &filter)
        .into_iter()
        .filter(|i| if movies { i.kind == MediaKind::Movie } else { i.kind != MediaKind::Movie })
        .collect();
    s.lib_results = filter_items(s.search.as_ref().map(|(_, i)| i.clone()).unwrap_or_default(), &filter);
    s.lib_continue_cards.set_vec(cards_for(s, &s.lib_continue));
    s.lib_recent_cards.set_vec(cards_for(s, &s.lib_recent));
    s.lib_result_cards.set_vec(cards_for(s, &s.lib_results));
    rebuild_browse(s, laptop);
    lib.set_results_title(match &s.search {
        Some((q, _)) => format!("RESULTS FOR “{}”", q.to_uppercase()).into(),
        None => "".into(),
    });
    // Selection indexes are stale after a rebuild.
    lib.set_selected_index(-1);
    rebuild_landing(s, laptop);
}

pub(super) fn on_results(s: &mut UiState, laptop: &LaptopWindow, query: String, items: Vec<MediaItem>) {
    // Ignore results for a query the user has since changed.
    if s.search.as_ref().is_some_and(|(q, _)| *q == query) {
        s.search = Some((query, items));
        laptop.global::<Library>().set_searching(false);
        rebuild(s, laptop);
    }
}

/// Filter buttons.
pub fn set_filter(laptop: &LaptopWindow, filter: &str) {
    laptop.global::<Library>().set_filter(filter.into());
    STATE.with_borrow_mut(|s| rebuild(s, laptop));
}

/// Switch Library between TV Series (`"tv"`) and Movies (`"movies"`).
pub fn set_media_tab(laptop: &LaptopWindow, tab: &str) {
    laptop.global::<Library>().set_media_tab(tab.into());
    STATE.with_borrow_mut(|s| rebuild(s, laptop));
}

/// Filter Browse All by starting letter (`""`, `"#"`, or `"A"`..`"Z"`).
pub fn set_browse_letter(laptop: &LaptopWindow, letter: &str) {
    let lib = laptop.global::<Library>();
    let next = if lib.get_browse_letter().as_str() == letter { "" } else { letter };
    lib.set_browse_letter(next.into());
    lib.set_selected_index(-1);
    STATE.with_borrow_mut(|s| rebuild_browse(s, laptop));
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
        rebuild(s, laptop);
    });
    true
}

pub fn clear_search(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        s.search = None;
        laptop.global::<Library>().set_searching(false);
        rebuild(s, laptop);
    });
}

/// Open the detail panel for a library card.
pub fn select(laptop: &LaptopWindow, section: &str, index: usize) {
    STATE.with_borrow_mut(|s| {
        let list = match section {
            "continue" => &s.lib_continue,
            "recent" => &s.lib_recent,
            "added-week" => &s.lib_added_week,
            "watch-again" => &s.lib_watch_again,
            "browse" => &s.lib_browse,
            _ => &s.lib_results,
        };
        let Some(item) = list.get(index).cloned() else { return };
        let poster = s.posters.get(&(item.source, item.id.clone())).cloned();
        let lib = laptop.global::<Library>();
        let show_key = if item.kind == MediaKind::Series { item.id.as_str() } else { "" };
        lib.set_detail(DetailData {
            title: item.title.as_str().into(),
            subtitle: format::item_detail(&item).into(),
            badge: item.source.label().to_uppercase().into(),
            meta: format::item_meta(&item).into(),
            has_poster: poster.is_some(),
            poster: poster.unwrap_or_default(),
            progress: item.progress().unwrap_or(0.0),
            can_resume: item.resume_ms.is_some(),
            show_key: show_key.into(),
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
    lib.set_metadata_refreshing(false);
    lib.set_selected_index(-1);
}

pub fn ensure_library_loaded(laptop: &LaptopWindow) {
    let need = STATE.with(|cell| {
        let Ok(mut s) = cell.try_borrow_mut() else {
            return false;
        };
        if s.lib_browse_all.is_empty() && !s.lib_shelves_loading {
            s.lib_shelves_loading = true;
            true
        } else {
            false
        }
    });
    if need {
        let lib = laptop.global::<Library>();
        lib.invoke_set_media_tab(lib.get_media_tab());
    }
}

pub(super) fn on_shelves(s: &mut UiState, laptop: &LaptopWindow, shelves: LibraryShelves) {
    s.lib_shelves_loading = false;
    let lib = laptop.global::<Library>();
    s.lib_finish_season = shelves.finish_season.clone();
    let finish_rows: Vec<FinishSeasonRow> = shelves
        .finish_season
        .iter()
        .map(|f| FinishSeasonRow {
            rating_key: f.item.id.as_str().into(),
            title: f.item.title.as_str().into(),
            season: f.season_title.as_str().into(),
            watched: f.watched_episodes as i32,
            total: f.total_episodes as i32,
        })
        .collect();
    lib.set_finish_season(ModelRc::new(VecModel::from(finish_rows)));
    s.lib_added_week = s.added_this_week_or_empty(&shelves).to_vec();
    s.lib_recently_released = shelves.recently_released;
    s.lib_watch_again = shelves.watch_again;
    s.lib_browse_all = shelves.browse_items;
    s.lib_added_week_cards.set_vec(cards_for(s, &s.lib_added_week));
    s.lib_watch_again_cards.set_vec(cards_for(s, &s.lib_watch_again));
    rebuild_browse(s, laptop);

    let popular: Vec<PopularCardData> = shelves
        .popular
        .iter()
        .map(|p| PopularCardData {
            rank: p.rank as i32,
            rating_key: p.rating_key.as_str().into(),
            title: p.title.as_str().into(),
            plays: format!("{} plays", p.plays).into(),
        })
        .collect();
    lib.set_popular(ModelRc::new(VecModel::from(popular)));

    let genres: Vec<GenreTileData> = shelves
        .genres
        .iter()
        .take(6)
        .map(|g| GenreTileData {
            key: g.key.as_str().into(),
            title: g.title.as_str().into(),
            count: g.count as i32,
        })
        .collect();
    lib.set_genres(ModelRc::new(VecModel::from(genres)));

    let collections: Vec<CollectionCardData> = shelves
        .collections
        .iter()
        .map(|c| {
            let frac = if c.child_count > 0 { c.watched_count as f32 / c.child_count as f32 } else { 0.0 };
            let mut detail = format!("{} movies · {} watched", c.child_count, c.watched_count);
            if c.watched_count >= c.child_count && c.child_count > 0 {
                detail = format!("{} movies · all watched", c.child_count);
            }
            if c.smart {
                detail.push_str(" · smart");
            }
            let poster = s.posters.get(&(SourceKind::Plex, c.rating_key.clone()));
            CollectionCardData {
                rating_key: c.rating_key.as_str().into(),
                title: c.title.as_str().into(),
                detail: detail.into(),
                progress: frac,
                smart: c.smart,
                has_poster: poster.is_some(),
                poster: poster.cloned().unwrap_or_default(),
            }
        })
        .collect();
    lib.set_collections(ModelRc::new(VecModel::from(collections)));
    rebuild_landing(s, laptop);
}

trait ShelvesExt {
    fn added_this_week_or_empty<'a>(&self, shelves: &'a LibraryShelves) -> &'a [MediaItem];
}

impl ShelvesExt for UiState {
    fn added_this_week_or_empty<'a>(&self, shelves: &'a LibraryShelves) -> &'a [MediaItem] {
        &shelves.added_this_week
    }
}

pub(super) fn on_show_detail(s: &mut UiState, laptop: &LaptopWindow, res: Result<ShowDetail, String>) {
    let sp = laptop.global::<ShowPageState>();
    sp.set_loading(false);
    let Ok(detail) = res else { return };
    let hue = s
        .backdrop_hues
        .get(&(detail.item.source, detail.item.id.clone()))
        .or_else(|| s.hues.get(&(detail.item.source, detail.item.id.clone())))
        .copied()
        .unwrap_or(tint::DEFAULT);
    sp.set_tint(tint::card_tint(hue, s.effective_theme()));
    sp.set_rating_key(detail.item.id.as_str().into());
    sp.set_title(detail.item.title.as_str().into());

    let season_count = detail.seasons.iter().filter(|sec| sec.index > 0).count();
    let total_eps: u32 = detail.seasons.iter().filter(|sec| sec.index > 0).map(|sec| sec.leaf_count).sum();
    let watched_eps: u32 = detail.seasons.iter().filter(|sec| sec.index > 0).map(|sec| sec.viewed_leaf_count).sum();
    let mut meta_parts = Vec::new();
    let base_meta = format::item_meta(&detail.item);
    if !base_meta.is_empty() {
        meta_parts.push(base_meta);
    }
    if season_count > 0 {
        meta_parts.push(if season_count == 1 {
            "1 season".to_string()
        } else {
            format!("{season_count} seasons")
        });
    }
    if total_eps > 0 {
        meta_parts.push(format!("{watched_eps} of {total_eps} watched"));
    }
    sp.set_meta(meta_parts.join(" · ").into());
    sp.set_summary(detail.item.subtitle.as_deref().unwrap_or("").into());

    let bd = s.backdrops.get(&(detail.item.source, detail.item.id.clone()));
    sp.set_has_backdrop(bd.is_some());
    sp.set_backdrop(bd.cloned().unwrap_or_default());

    if let Some(ep) = &detail.up_next_episode {
        let dur = ep.duration_ms.unwrap_or(0);
        let off = ep.view_offset_ms.unwrap_or(0);
        let left = if dur > 0 { format::duration(dur.saturating_sub(off)) } else { String::new() };
        sp.set_up_next(format!("S{} · E{} — {} · {} left", ep.season_index, ep.index, ep.title, left).into());
        sp.set_up_next_key(ep.rating_key.as_str().into());
        sp.set_can_resume(off > 0);
        sp.set_up_next_progress(if dur > 0 && off > 0 { (off as f32 / dur as f32).clamp(0.0, 1.0) } else { 0.0 });
    } else {
        sp.set_up_next("".into());
        sp.set_up_next_key(detail.item.id.as_str().into());
        sp.set_can_resume(false);
        sp.set_up_next_progress(0.0);
    }

    let seasons: Vec<SeasonTabData> = detail
        .seasons
        .iter()
        .map(|sec| {
            let title = if sec.index == 0 { "Specials".to_string() } else { sec.title.clone() };
            let frac = if sec.leaf_count > 0 { sec.viewed_leaf_count as f32 / sec.leaf_count as f32 } else { 0.0 };
            SeasonTabData {
                rating_key: sec.rating_key.as_str().into(),
                index: sec.index as i32,
                title: title.into(),
                progress: format!("{}/{}", sec.viewed_leaf_count, sec.leaf_count).into(),
                fraction: frac,
            }
        })
        .collect();
    sp.set_seasons(ModelRc::new(VecModel::from(seasons)));
    sp.set_episodes(ModelRc::default());
    sp.set_focused_episode(EpisodeRowData::default());
    sp.set_scrolled(false);
    let sel_season = detail.up_next_season_key.clone().unwrap_or_default();
    sp.set_selected_season(sel_season.as_str().into());
    s.show_detail = Some(detail);
    update_season_column(s, laptop, &sel_season, &[]);
    sp.set_open(true);
}

fn format_hours_mins(ms: u64) -> String {
    let mins = ms / 60_000;
    if mins == 0 {
        return String::new();
    }
    let h = mins / 60;
    let m = mins % 60;
    match (h, m) {
        (0, m) => format!("{m} m"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} m"),
    }
}

fn build_season_column(
    s: &UiState,
    detail: &ShowDetail,
    sec: &tvdash_core::models::SeasonInfo,
    eps: &[EpisodeInfo],
) -> (SeasonColumnData, String) {
    let name = if sec.index == 0 { "Specials".to_string() } else { sec.title.clone() };
    let same_as_show = sec.thumb.is_some() && sec.thumb == detail.item.poster;
    let season_poster = if !same_as_show {
        s.posters.get(&(detail.item.source, sec.rating_key.clone()))
    } else {
        None
    };
    let (poster, has_poster, poster_is_fallback, fallback_label) = if let Some(img) = season_poster {
        (img.clone(), true, false, String::new())
    } else {
        let show_poster = s.posters.get(&(detail.item.source, detail.item.id.clone()));
        let label = if sec.index == 0 {
            "SPECIALS".to_string()
        } else {
            format!("SEASON {}", sec.index)
        };
        (show_poster.cloned().unwrap_or_default(), show_poster.is_some(), true, label)
    };

    let year = sec.year.or_else(|| {
        eps.first()
            .and_then(|e| e.originally_available_at.as_deref())
            .and_then(|d| d.get(..4))
            .and_then(|y| y.parse::<u32>().ok())
    });
    let ep_count = if !eps.is_empty() { eps.len() as u32 } else { sec.leaf_count };
    let ep_word = if ep_count == 1 { "episode" } else { "episodes" };
    let total_ms: u64 = eps.iter().filter_map(|e| e.duration_ms).sum();
    let dur_str = format_hours_mins(total_ms);

    let mut meta_parts = Vec::new();
    if let Some(y) = year {
        meta_parts.push(y.to_string());
    }
    meta_parts.push(format!("{ep_count} {ep_word}"));
    if !dur_str.is_empty() {
        meta_parts.push(dur_str.clone());
    }

    let summary_line = if dur_str.is_empty() {
        format!("{ep_count} {ep_word}")
    } else {
        format!("{ep_count} {ep_word} · {dur_str}")
    };

    let progress = if sec.leaf_count > 0 {
        (sec.viewed_leaf_count as f32 / sec.leaf_count as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (
        SeasonColumnData {
            rating_key: sec.rating_key.as_str().into(),
            name: name.into(),
            meta: meta_parts.join(" · ").into(),
            summary: sec.summary.as_deref().unwrap_or("").into(),
            progress,
            progress_label: format!("{} of {}", sec.viewed_leaf_count, sec.leaf_count).into(),
            poster,
            has_poster,
            poster_is_fallback,
            fallback_label: fallback_label.into(),
        },
        summary_line,
    )
}

fn update_season_column(s: &UiState, laptop: &LaptopWindow, season_key: &str, eps: &[EpisodeInfo]) {
    let Some(detail) = &s.show_detail else { return };
    let Some((idx, sec)) = detail.seasons.iter().enumerate().find(|(_, sec)| sec.rating_key == season_key) else {
        return;
    };
    let (col, summary_line) = build_season_column(s, detail, sec, eps);
    let sp = laptop.global::<ShowPageState>();
    sp.set_season(col);
    sp.set_season_summary_line(summary_line.into());
    if let Some(next) = detail.seasons.get(idx + 1) {
        let next_name = if next.index == 0 {
            "Specials".to_string()
        } else {
            format!("Season {}", next.index)
        };
        sp.set_next_season_key(next.rating_key.as_str().into());
        sp.set_next_season_label(format!("Continue to {next_name} ›").into());
    } else {
        sp.set_next_season_key("".into());
        sp.set_next_season_label("".into());
    }
}

pub fn on_season_selected(laptop: &LaptopWindow, season_key: &str) {
    STATE.with_borrow(|s| update_season_column(s, laptop, season_key, &[]));
}

pub(super) fn refresh_show_season_poster(s: &UiState, laptop: &LaptopWindow) {
    let sp = laptop.global::<ShowPageState>();
    let Some(detail) = &s.show_detail else { return };
    let season_key = sp.get_selected_season();
    let Some(sec) = detail.seasons.iter().find(|sec| sec.rating_key == season_key.as_str()) else {
        return;
    };
    let same_as_show = sec.thumb.is_some() && sec.thumb == detail.item.poster;
    let season_poster = if !same_as_show {
        s.posters.get(&(detail.item.source, sec.rating_key.clone()))
    } else {
        None
    };
    let mut col = sp.get_season();
    if let Some(img) = season_poster {
        col.poster = img.clone();
        col.has_poster = true;
        col.poster_is_fallback = false;
        col.fallback_label = "".into();
    } else if let Some(show_img) = s.posters.get(&(detail.item.source, detail.item.id.clone())) {
        col.poster = show_img.clone();
        col.has_poster = true;
        col.poster_is_fallback = true;
        col.fallback_label = if sec.index == 0 {
            "SPECIALS".into()
        } else {
            format!("SEASON {}", sec.index).into()
        };
    }
    sp.set_season(col);
}

pub(super) fn on_season_episodes(
    s: &UiState,
    laptop: &LaptopWindow,
    season_key: &str,
    res: Result<Vec<EpisodeInfo>, String>,
) {
    let sp = laptop.global::<ShowPageState>();
    if sp.get_selected_season().as_str() != season_key {
        return;
    }
    let Ok(eps) = res else { return };
    update_season_column(s, laptop, season_key, &eps);
    let len = eps.len();
    let rows: Vec<EpisodeRowData> = eps
        .into_iter()
        .enumerate()
        .map(|(idx, ep)| {
            let dur = ep.duration_ms.unwrap_or(0);
            let off = ep.view_offset_ms.unwrap_or(0);
            let watched = ep.view_count > 0 && off == 0;
            let progress = if dur > 0 && off > 0 { (off as f32 / dur as f32).clamp(0.0, 1.0) } else { 0.0 };
            let is_finale = idx + 1 == len && len > 1 && ep.season_index > 0;
            let meta = if off > 0 && dur > off {
                format!("{} · {} left", format::duration(dur), format::duration(dur - off))
            } else if watched {
                format!("{} · watched", format::duration(dur))
            } else if is_finale {
                format!("{} · season finale", format::duration(dur))
            } else {
                format::duration(dur)
            };
            let num = if ep.season_index == 0 {
                format!("S{}", ep.index)
            } else {
                format!("{:02}", ep.index)
            };
            let thumb = s.posters.get(&(SourceKind::Plex, ep.rating_key.clone()));
            EpisodeRowData {
                rating_key: ep.rating_key.into(),
                num: num.into(),
                title: ep.title.into(),
                meta: meta.into(),
                summary: ep.summary.unwrap_or_default().into(),
                progress,
                watched,
                can_resume: off > 0,
                has_thumb: thumb.is_some(),
                thumb: thumb.cloned().unwrap_or_default(),
            }
        })
        .collect();
    sp.set_focused_episode(rows.first().cloned().unwrap_or_default());
    sp.set_episodes(ModelRc::new(VecModel::from(rows)));
}

pub(super) fn on_collection_detail(s: &mut UiState, laptop: &LaptopWindow, res: Result<CollectionDetail, String>) {
    let cp = laptop.global::<CollectionPageState>();
    cp.set_loading(false);
    let Ok(detail) = res else { return };
    let hue = s
        .hues
        .get(&(SourceKind::Plex, detail.collection.rating_key.clone()))
        .or_else(|| detail.items.first().and_then(|i| s.hues.get(&(i.source, i.id.clone()))))
        .copied()
        .unwrap_or(tint::DEFAULT);
    cp.set_tint(tint::card_tint(hue, s.effective_theme()));
    cp.set_rating_key(detail.collection.rating_key.as_str().into());
    cp.set_title(detail.collection.title.as_str().into());
    cp.set_meta(format!("{} movies", detail.items.len()).into());
    cp.set_progress_text(
        format!("{} of {} watched", detail.collection.watched_count, detail.items.len()).into(),
    );
    cp.set_summary(detail.collection.summary.as_deref().unwrap_or("").into());
    if let Some(k) = &detail.up_next_key {
        cp.set_up_next_key(k.as_str().into());
        let has_resume = detail.items.iter().find(|i| &i.id == k).is_some_and(|i| i.resume_ms.is_some());
        cp.set_up_next_label(if has_resume { "Resume next on TV" } else { "Play next on TV" }.into());
    }
    s.collection_detail = Some(detail);
    rebuild_collection_items(s, laptop);
    cp.set_open(true);
}

pub fn reorder_collection(laptop: &LaptopWindow) {
    STATE.with_borrow(|s| rebuild_collection_items(s, laptop));
}

pub(super) fn rebuild_collection_items(s: &UiState, laptop: &LaptopWindow) {
    let Some(detail) = &s.collection_detail else { return };
    let cp = laptop.global::<CollectionPageState>();
    let order = cp.get_order();
    let hide_watched = cp.get_hide_watched();
    let mut items: Vec<(usize, &MediaItem)> = detail.items.iter().enumerate().collect();
    if hide_watched {
        items.retain(|(_, i)| !detail.watched_keys.contains(&i.id));
    }
    match order.as_str() {
        "release" => items.sort_by_key(|(_, i)| i.subtitle.clone()),
        "alpha" => items.sort_by(|(_, a), (_, b)| a.title.cmp(&b.title)),
        _ => {}
    }
    let rows: Vec<CollectionItemData> = items
        .into_iter()
        .map(|(idx, i)| {
            let can_resume = i.resume_ms.is_some();
            let watched = !can_resume && detail.watched_keys.contains(&i.id);
            let status = if can_resume {
                format::time_left(i).unwrap_or_else(|| "In progress".into())
            } else if watched {
                "Watched".into()
            } else {
                "".into()
            };
            let poster = s.posters.get(&(i.source, i.id.clone()));
            CollectionItemData {
                rating_key: i.id.as_str().into(),
                num: format!("#{}", idx + 1).into(),
                title: i.title.as_str().into(),
                status: status.into(),
                watched,
                can_resume,
                has_poster: poster.is_some(),
                poster: poster.cloned().unwrap_or_default(),
            }
        })
        .collect();
    cp.set_items(ModelRc::new(VecModel::from(rows)));
}

/// Same 6 unstarted shows all week; reshuffled every Monday.
pub fn start_watching(pool: &[MediaItem]) -> Vec<MediaItem> {
    let mut picks: Vec<MediaItem> = pool
        .iter()
        .filter(|s| s.kind == MediaKind::Series && s.extra.viewed_leaf_count.unwrap_or(0) == 0)
        .cloned()
        .collect();
    picks.sort_by(|a, b| a.id.cmp(&b.id));
    let w = Local::now().iso_week();
    let seed = ((w.year() as u64) << 8) | w.week() as u64;
    picks.shuffle(&mut ChaCha8Rng::seed_from_u64(seed));
    picks.truncate(6);
    picks
}

fn format_added_ago(ts: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let days = ((now - ts).max(0)) / 86_400;
    match days {
        0 => "Added today".into(),
        1 => "Added yesterday".into(),
        n => format!("Added {n} days ago"),
    }
}

fn summary_or_fallback(item: &MediaItem) -> String {
    if let Some(s) = &item.summary
        && !s.trim().is_empty()
    {
        return s.clone();
    }
    let genre = item.extra.genres.first().cloned().unwrap_or_default();
    let cast = item.extra.cast.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
    match (genre.is_empty(), cast.is_empty()) {
        (false, false) => format!("{genre} · with {cast}"),
        (false, true) => genre,
        (true, false) => format!("With {cast}"),
        (true, true) => "No description available.".into(),
    }
}

fn focus_info_for(s: &UiState, item: &MediaItem, is_shelf1: bool, is_tv: bool) -> FocusInfoData {
    let key = (item.source, item.id.clone());
    let hue = s
        .backdrop_hues
        .get(&key)
        .or_else(|| s.hues.get(&key))
        .copied()
        .unwrap_or(tint::DEFAULT);
    let t = tint::card_tint(hue, s.effective_theme());

    let (kicker, meta) = if is_tv {
        if is_shelf1 {
            let fin = s.lib_finish_season.iter().find(|f| f.item.id == item.id);
            let kicker = fin
                .map(|f| format!("{} · Episode {} next", f.season_title, f.watched_episodes + 1))
                .unwrap_or_else(|| "Finish the season".into());
            let mut parts = Vec::new();
            if let Some(f) = fin {
                let left = f.total_episodes.saturating_sub(f.watched_episodes);
                parts.push(format!("{left} {} left", if left == 1 { "episode" } else { "episodes" }));
            }
            if let Some(g) = item.extra.genres.first() {
                parts.push(g.clone());
            }
            if let Some(r) = &item.extra.content_rating {
                parts.push(r.clone());
            }
            if let Some(y) = item.extra.year {
                parts.push(y.to_string());
            }
            (kicker, parts.join(" · "))
        } else {
            let seasons = item.extra.child_count.unwrap_or(1);
            let kicker = format!("Start watching · {seasons} {}", if seasons == 1 { "season" } else { "seasons" });
            let mut parts = Vec::new();
            if let Some(y) = item.extra.year {
                parts.push(y.to_string());
            }
            if let Some(g) = item.extra.genres.first() {
                parts.push(g.clone());
            }
            if let Some(r) = &item.extra.content_rating {
                parts.push(r.clone());
            }
            if let Some(eps) = item.extra.leaf_count {
                parts.push(format!("{eps} episodes"));
            }
            (kicker, parts.join(" · "))
        }
    } else if is_shelf1 {
        let kicker = format::time_left(item).unwrap_or_else(|| "Continue watching".into());
        let mut parts = Vec::new();
        if let Some(y) = item.extra.year {
            parts.push(y.to_string());
        }
        if let Some(g) = item.extra.genres.first() {
            parts.push(g.clone());
        }
        if let Some(d) = item.duration_ms {
            parts.push(format::duration(d));
        }
        if let Some(r) = &item.extra.content_rating {
            parts.push(r.clone());
        }
        (kicker, parts.join(" · "))
    } else {
        let kicker = item
            .extra
            .added_at
            .map(format_added_ago)
            .unwrap_or_else(|| "Recently added".into());
        let mut parts = Vec::new();
        if let Some(y) = item.extra.year {
            parts.push(y.to_string());
        }
        if let Some(g) = item.extra.genres.first() {
            parts.push(g.clone());
        }
        if let Some(d) = item.duration_ms {
            parts.push(format::duration(d));
        }
        if let Some(r) = &item.extra.content_rating {
            parts.push(r.clone());
        }
        (kicker, parts.join(" · "))
    };

    FocusInfoData {
        kicker: kicker.into(),
        title: item.title.as_str().into(),
        meta: meta.into(),
        summary: summary_or_fallback(item).into(),
        tint_accent: t.accent,
        tint_poster: t.poster,
        tint_card: t.card,
    }
}

fn apply_focus_item(s: &mut UiState, laptop: &LaptopWindow, item: &MediaItem, is_shelf1: bool, is_tv: bool) {
    let key = (item.source, item.id.clone());
    s.lib_focused_key = Some(key.clone());
    let ll = laptop.global::<LibraryLanding>();
    ll.set_focus(focus_info_for(s, item, is_shelf1, is_tv));
    let img = s.backdrops.get(&key).or_else(|| s.posters.get(&key)).cloned();
    if let Some(img) = img {
        let show_b = !ll.get_backdrop_show_b();
        if show_b {
            ll.set_backdrop_b(img);
        } else {
            ll.set_backdrop_a(img);
        }
        ll.set_backdrop_show_b(show_b);
        ll.set_has_backdrop(true);
    } else {
        ll.set_has_backdrop(false);
    }
}

fn ranked_tiles(s: &UiState, items: &[MediaItem]) -> Vec<RankedTileData> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let key = (item.source, item.id.clone());
            let poster = s.posters.get(&key);
            RankedTileData {
                rating_key: item.id.as_str().into(),
                rank: (i + 1) as i32,
                title: item.title.as_str().into(),
                has_poster: poster.is_some(),
                poster: poster.cloned().unwrap_or_default(),
            }
        })
        .collect()
}

pub(super) fn rebuild_landing(s: &mut UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let ll = laptop.global::<LibraryLanding>();
    let is_tv = lib.get_media_tab() == "tv";

    if is_tv {
        let (s1_items, s1_tiles, s1_title) = if !s.lib_finish_season.is_empty() {
            let items: Vec<MediaItem> = s.lib_finish_season.iter().take(4).map(|f| f.item.clone()).collect();
            let tiles: Vec<Shelf1TileData> = s
                .lib_finish_season
                .iter()
                .take(4)
                .map(|f| {
                    let key = (f.item.source, f.item.id.clone());
                    let art = s.backdrops.get(&key).or_else(|| s.posters.get(&key));
                    let left = f.total_episodes.saturating_sub(f.watched_episodes);
                    let frac = if f.total_episodes > 0 {
                        f.watched_episodes as f32 / f.total_episodes as f32
                    } else {
                        0.0
                    };
                    Shelf1TileData {
                        rating_key: f.item.id.as_str().into(),
                        title: f.item.title.as_str().into(),
                        right_label: format!("{left} left").into(),
                        has_art: art.is_some(),
                        art: art.cloned().unwrap_or_default(),
                        progress: frac,
                        segments: f.total_episodes as i32,
                        watched_segments: f.watched_episodes as i32,
                    }
                })
                .collect();
            (items, tiles, "FINISH THE SEASON")
        } else {
            let items: Vec<MediaItem> = s.lib_continue.iter().take(4).cloned().collect();
            let tiles: Vec<Shelf1TileData> = items
                .iter()
                .map(|item| {
                    let key = (item.source, item.id.clone());
                    let art = s.backdrops.get(&key).or_else(|| s.posters.get(&key));
                    Shelf1TileData {
                        rating_key: item.id.as_str().into(),
                        title: item.title.as_str().into(),
                        right_label: format::time_left(item).unwrap_or_default().into(),
                        has_art: art.is_some(),
                        art: art.cloned().unwrap_or_default(),
                        progress: item.progress().unwrap_or(0.0),
                        segments: 0,
                        watched_segments: 0,
                    }
                })
                .collect();
            (items, tiles, "CONTINUE WATCHING")
        };
        let s2_items = start_watching(&s.lib_browse_all);
        let s2_tiles = ranked_tiles(s, &s2_items);
        let s3_items: Vec<MediaItem> = s.lib_recently_released.iter().take(6).cloned().collect();
        let s3_tiles = ranked_tiles(s, &s3_items);
        ll.set_shelf1_title(s1_title.into());
        ll.set_shelf1_count(format!("{} in progress", s1_tiles.len()).into());
        ll.set_shelf2_title("START WATCHING".into());
        ll.set_shelf2_count("Not started yet · new picks every Monday".into());
        ll.set_shelf3_title("RECENTLY RELEASED EPISODES".into());
        ll.set_shelf3_count("Newest air dates".into());
        ll.set_shelf1(ModelRc::new(VecModel::from(s1_tiles)));
        ll.set_shelf2(ModelRc::new(VecModel::from(s2_tiles)));
        ll.set_shelf3(ModelRc::new(VecModel::from(s3_tiles)));
        s.lib_shelf1_items = s1_items;
        s.lib_shelf2_items = s2_items;
        s.lib_shelf3_items = s3_items;
    } else {
        let s1_items: Vec<MediaItem> = s.lib_continue.iter().take(4).cloned().collect();
        let s1_tiles: Vec<Shelf1TileData> = s1_items
            .iter()
            .map(|item| {
                let key = (item.source, item.id.clone());
                let art = s.backdrops.get(&key).or_else(|| s.posters.get(&key));
                Shelf1TileData {
                    rating_key: item.id.as_str().into(),
                    title: item.title.as_str().into(),
                    right_label: format::time_left(item).unwrap_or_default().into(),
                    has_art: art.is_some(),
                    art: art.cloned().unwrap_or_default(),
                    progress: item.progress().unwrap_or(0.0),
                    segments: 0,
                    watched_segments: 0,
                }
            })
            .collect();
        let s2_items: Vec<MediaItem> = if !s.lib_added_week.is_empty() {
            s.lib_added_week.iter().take(6).cloned().collect()
        } else {
            s.lib_recent.iter().take(6).cloned().collect()
        };
        let s2_tiles = ranked_tiles(s, &s2_items);
        let s3_items: Vec<MediaItem> = s.lib_recently_released.iter().take(6).cloned().collect();
        let s3_tiles = ranked_tiles(s, &s3_items);
        ll.set_shelf1_title("CONTINUE WATCHING".into());
        ll.set_shelf1_count(format!("{} in progress", s1_tiles.len()).into());
        ll.set_shelf2_title("RECENTLY ADDED".into());
        ll.set_shelf2_count("Newest first".into());
        ll.set_shelf3_title("RECENTLY RELEASED".into());
        ll.set_shelf3_count("Newest release dates".into());
        ll.set_shelf1(ModelRc::new(VecModel::from(s1_tiles)));
        ll.set_shelf2(ModelRc::new(VecModel::from(s2_tiles)));
        ll.set_shelf3(ModelRc::new(VecModel::from(s3_tiles)));
        s.lib_shelf1_items = s1_items;
        s.lib_shelf2_items = s2_items;
        s.lib_shelf3_items = s3_items;
    }

    if let Some(first) = s.lib_shelf1_items.first().cloned() {
        apply_focus_item(s, laptop, &first, true, is_tv);
    } else if let Some(first) = s.lib_shelf2_items.first().cloned() {
        apply_focus_item(s, laptop, &first, false, is_tv);
    } else {
        let total = s.lib_browse_all.len();
        let t = tint::card_tint(tint::DEFAULT, s.effective_theme());
        ll.set_focus(FocusInfoData {
            kicker: "LIBRARY".into(),
            title: "Nothing in progress".into(),
            meta: format!("{total} titles available").into(),
            summary: "Pick Browse all above to explore everything in your library.".into(),
            tint_accent: t.accent,
            tint_poster: t.poster,
            tint_card: t.card,
        });
        ll.set_has_backdrop(false);
    }
}

/// Debounce (300 ms) Library focus → FocusBackdrop + FocusInfo update and neighbor prefetch.
pub fn on_library_focus_changed(
    laptop: &LaptopWindow,
    tile_idx: i32,
    prefetch: impl Fn(MediaItem) + Clone + 'static,
) {
    thread_local! {
        static BD_TIMER: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
    }
    if tile_idx < 4 {
        return;
    }
    let weak = laptop.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, Duration::from_millis(300), move || {
        let Some(laptop) = weak.upgrade() else { return };
        let to_prefetch: Vec<MediaItem> = STATE.with_borrow_mut(|s| {
            let is_tv = laptop.global::<Library>().get_media_tab() == "tv";
            let idx = (tile_idx - 4) as usize;
            let s1_len = s.lib_shelf1_items.len();
            let s2_len = s.lib_shelf2_items.len();
            let (item, is_shelf1, neighbors) = if idx < s1_len {
                let it = s.lib_shelf1_items.get(idx).cloned();
                let mut nb = Vec::new();
                if idx > 0 {
                    nb.extend(s.lib_shelf1_items.get(idx - 1).cloned());
                }
                nb.extend(s.lib_shelf1_items.get(idx + 1).cloned());
                (it, true, nb)
            } else if idx < s1_len + s2_len {
                let j = idx - s1_len;
                let it = s.lib_shelf2_items.get(j).cloned();
                let mut nb = Vec::new();
                if j > 0 {
                    nb.extend(s.lib_shelf2_items.get(j - 1).cloned());
                }
                nb.extend(s.lib_shelf2_items.get(j + 1).cloned());
                (it, false, nb)
            } else {
                let k = idx - s1_len - s2_len;
                let it = s.lib_shelf3_items.get(k).cloned();
                let mut nb = Vec::new();
                if k > 0 {
                    nb.extend(s.lib_shelf3_items.get(k - 1).cloned());
                }
                nb.extend(s.lib_shelf3_items.get(k + 1).cloned());
                (it, false, nb)
            };
            let mut out = Vec::new();
            if let Some(it) = item {
                apply_focus_item(s, &laptop, &it, is_shelf1, is_tv);
                if !s.backdrops.contains_key(&(it.source, it.id.clone())) {
                    out.push(it);
                }
            }
            for n in neighbors {
                if !s.backdrops.contains_key(&(n.source, n.id.clone())) {
                    out.push(n);
                }
            }
            out
        });
        for it in to_prefetch {
            prefetch(it);
        }
    });
    BD_TIMER.with_borrow_mut(|slot| *slot = Some(timer));
}

pub(super) fn on_backdrop(
    s: &mut UiState,
    laptop: &LaptopWindow,
    source: SourceKind,
    item_id: String,
    image: &RgbaImage,
) {
    let key = (source, item_id);
    let hue = tint::poster_hue(image);
    let img = to_image(image);
    s.backdrop_hues.insert(key.clone(), hue);
    s.backdrops.insert(key.clone(), img.clone());
    super::home::refresh_tv_card(s, laptop);

    let ll = laptop.global::<LibraryLanding>();
    let shelf1 = ll.get_shelf1();
    for i in 0..shelf1.row_count() {
        if let Some(mut row) = shelf1.row_data(i)
            && row.rating_key.as_str() == key.1
        {
            row.has_art = true;
            row.art = img.clone();
            shelf1.set_row_data(i, row);
        }
    }

    if s.lib_focused_key.as_ref() == Some(&key) {
        let is_tv = laptop.global::<Library>().get_media_tab() == "tv";
        if let Some(it) = s.lib_shelf1_items.iter().find(|i| (i.source, i.id.clone()) == key).cloned() {
            apply_focus_item(s, laptop, &it, true, is_tv);
        } else if let Some(it) = s.lib_shelf2_items.iter().find(|i| (i.source, i.id.clone()) == key).cloned() {
            apply_focus_item(s, laptop, &it, false, is_tv);
        } else if let Some(it) = s.lib_shelf3_items.iter().find(|i| (i.source, i.id.clone()) == key).cloned() {
            apply_focus_item(s, laptop, &it, false, is_tv);
        }
    }

    let sp = laptop.global::<ShowPageState>();
    if sp.get_open() && sp.get_rating_key().as_str() == key.1.as_str() {
        sp.set_has_backdrop(true);
        sp.set_backdrop(img);
    }
}

pub(super) fn on_sessions(s: &mut UiState, laptop: &LaptopWindow, sessions: &[Session]) {
    s.raw_sessions = sessions.to_vec();
    let ll = laptop.global::<LibraryLanding>();
    ll.set_watching_now_count(sessions.len() as i32);
    let rows: Vec<WatchingNowRowData> = sessions
        .iter()
        .take(6)
        .map(|sess| {
            let key = (sess.item.source, sess.item.id.clone());
            let poster = s.posters.get(&key);
            let is_this_tv = s.player.as_ref().is_some_and(|p| p.item.id == sess.item.id);
            let stream_badge = if sess.transcoding { "Transcode" } else { "Direct play" };
            let state_text = if sess.state == PlayState::Paused { "Paused" } else { "Playing" };
            let prog = match sess.item.duration_ms {
                Some(d) if d > 0 => (sess.position_ms as f32 / d as f32).clamp(0.0, 1.0),
                _ => 0.0,
            };
            let time_left = match sess.item.duration_ms {
                Some(d) if d > sess.position_ms => format!("{} left", format::duration(d - sess.position_ms)),
                _ => String::new(),
            };
            WatchingNowRowData {
                rating_key: sess.item.id.as_str().into(),
                is_tv: sess.item.kind != MediaKind::Movie,
                title: sess.item.title.as_str().into(),
                subtitle: format::item_detail(&sess.item).into(),
                player_line: format!("{} · {}", sess.device, sess.source.label()).into(),
                stream_badge: stream_badge.into(),
                is_this_tv,
                state_text: state_text.into(),
                time_left: time_left.into(),
                progress: prog,
                has_poster: poster.is_some(),
                poster: poster.cloned().unwrap_or_default(),
            }
        })
        .collect();
    ll.set_watching_now_rows(ModelRc::new(VecModel::from(rows)));
}

pub fn watching_now_item(idx: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| s.raw_sessions.get(idx).map(|x| x.item.clone()))
}

#[cfg(test)]
mod tests {
    use super::{browse_sort_key, matches_letter};

    #[test]
    fn browse_sort_and_letter_ignore_articles() {
        let mut titles = ["Zorro", "The Matrix", "alpha", "300", "A Quiet Place", "An Education", "1917"];
        titles.sort_by_key(|t| browse_sort_key(t));
        assert_eq!(titles, ["1917", "300", "alpha", "An Education", "The Matrix", "A Quiet Place", "Zorro"]);

        assert!(matches_letter("The Matrix", "M"));
        assert!(!matches_letter("The Matrix", "T"));
        assert!(matches_letter("A Quiet Place", "Q"));
        assert!(matches_letter("An Education", "E"));
        assert!(matches_letter("300", "#"));
    }
}
