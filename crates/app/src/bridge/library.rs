//! The Library page: Continue Watching and Recently Added from both servers,
//! the source filter, search, and the detail panel.

use slint::{ComponentHandle, ModelRc, VecModel};
use tvdash_core::models::{
    CollectionDetail, EpisodeInfo, LibraryShelves, MediaItem, ShowDetail, SourceKind,
};

use super::{STATE, UiState, home::cards_for, in_order};
use crate::{
    CollectionCardData, CollectionItemData, CollectionPageState, DetailData, EpisodeRowData, FinishSeasonRow,
    GenreTileData, LaptopWindow, Library, PopularCardData, SeasonTabData, ShowPageState, format, tint,
};

fn filter_items(items: Vec<MediaItem>, filter: &str) -> Vec<MediaItem> {
    match filter {
        "plex" => items.into_iter().filter(|i| i.source == SourceKind::Plex).collect(),
        _ => items,
    }
}

pub(super) fn rebuild(s: &mut UiState, laptop: &LaptopWindow) {
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
    lib.set_metadata_refreshing(false);
    lib.set_selected_index(-1);
}

pub(super) fn on_shelves(s: &UiState, laptop: &LaptopWindow, shelves: LibraryShelves) {
    let lib = laptop.global::<Library>();
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
    lib.set_added_week(ModelRc::new(VecModel::from(cards_for(s, &s.added_this_week_or_empty(&shelves)))));
    lib.set_watch_again(ModelRc::new(VecModel::from(cards_for(s, &shelves.watch_again))));

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
            CollectionCardData {
                rating_key: c.rating_key.as_str().into(),
                title: c.title.as_str().into(),
                detail: detail.into(),
                progress: frac,
                smart: c.smart,
            }
        })
        .collect();
    lib.set_collections(ModelRc::new(VecModel::from(collections)));
}

trait ShelvesExt {
    fn added_this_week_or_empty<'a>(&self, shelves: &'a LibraryShelves) -> &'a [MediaItem];
}

impl ShelvesExt for UiState {
    fn added_this_week_or_empty<'a>(&self, shelves: &'a LibraryShelves) -> &'a [MediaItem] {
        &shelves.added_this_week
    }
}

pub(super) fn on_show_detail(s: &UiState, laptop: &LaptopWindow, res: Result<ShowDetail, String>) {
    let sp = laptop.global::<ShowPageState>();
    sp.set_loading(false);
    let Ok(detail) = res else { return };
    let hue = s.hues.get(&(detail.item.source, detail.item.id.clone())).copied().unwrap_or(tint::DEFAULT);
    sp.set_tint(tint::card_tint(hue, s.effective_theme()));
    sp.set_rating_key(detail.item.id.as_str().into());
    sp.set_title(detail.item.title.as_str().into());
    sp.set_meta(format::item_meta(&detail.item).into());
    sp.set_summary(detail.item.subtitle.as_deref().unwrap_or("").into());

    if let Some(ep) = &detail.up_next_episode {
        let left = ep
            .duration_ms
            .map(|d| format::duration(d.saturating_sub(ep.view_offset_ms.unwrap_or(0))))
            .unwrap_or_default();
        sp.set_up_next(format!("S{} · E{} — {} · {} left", ep.season_index, ep.index, ep.title, left).into());
        sp.set_up_next_key(ep.rating_key.as_str().into());
        sp.set_can_resume(ep.view_offset_ms.unwrap_or(0) > 0);
    } else {
        sp.set_up_next("".into());
        sp.set_up_next_key(detail.item.id.as_str().into());
        sp.set_can_resume(false);
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
    if let Some(sel) = detail.up_next_season_key {
        sp.set_selected_season(sel.into());
    }
    sp.set_open(true);
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
    let rows: Vec<EpisodeRowData> = eps
        .into_iter()
        .map(|ep| {
            let dur = ep.duration_ms.unwrap_or(0);
            let off = ep.view_offset_ms.unwrap_or(0);
            let watched = ep.view_count > 0 && off == 0;
            let progress = if dur > 0 && off > 0 { (off as f32 / dur as f32).clamp(0.0, 1.0) } else { 0.0 };
            let meta = if off > 0 && dur > off {
                format!("{} · {} left", format::duration(dur), format::duration(dur - off))
            } else if watched {
                format!("{} · watched", format::duration(dur))
            } else {
                format::duration(dur)
            };
            let thumb = s.posters.get(&(SourceKind::Plex, ep.rating_key.clone()));
            EpisodeRowData {
                rating_key: ep.rating_key.into(),
                num: format!("{:02}", ep.index).into(),
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
