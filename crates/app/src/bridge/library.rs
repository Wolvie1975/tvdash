//! The Library page: Continue Watching and Recently Added from both servers,
//! the source filter, search, and the detail panel.

use slint::ComponentHandle;
use tvdash_core::models::{MediaItem, SourceKind};

use super::{STATE, UiState, home::cards_for, in_order};
use crate::{DetailData, LaptopWindow, Library, format};

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
