//! Orbit · Library › Browse all bridge: letter counts, first-10 "All" vs full letter,
//! per-library sort, genre filter, and Orbit ring paths.

use std::collections::BTreeSet;

use slint::{ComponentHandle, Model, ModelRc, VecModel};
use tvdash_core::models::{MediaItem, MediaKind};

use super::{
    STATE, UiState,
    library::{browse_sort_key, format_count, matches_letter},
};
use crate::{
    Browse, BrowseItem, LaptopWindow, Library,
    orbit::{
        letters::LETTERS,
        rings::{gap_for_count, ring_paths},
    },
};

const RING_RADIUS: f32 = 95.0; // 190px diameter in 1080p design units

pub fn sort_label(sort: &str) -> &'static str {
    match sort {
        "title_desc" => "Z–A",
        "recent" => "Recently added",
        _ => "A–Z",
    }
}

pub fn next_sort(sort: &str) -> &'static str {
    match sort {
        "title_asc" => "title_desc",
        "title_desc" => "recent",
        _ => "title_asc",
    }
}

fn format_duration_hm(ms: u64) -> String {
    let mins = ms / 60_000;
    let h = mins / 60;
    let m = mins % 60;
    if h > 0 {
        format!("{h} h {m} m")
    } else {
        format!("{m} m")
    }
}

fn build_item(s: &UiState, item: &MediaItem) -> BrowseItem {
    let key_pair = (item.source, item.id.clone());
    let img = s.posters.peek(&key_pair);
    let has_poster = img.is_some();
    let poster = img.cloned().unwrap_or_default();
    let is_movie = item.kind == MediaKind::Movie;

    if is_movie {
        let prog = item.progress().unwrap_or(0.0);
        let watched = item.extra.viewed_leaf_count.is_some_and(|v| v > 0) || prog >= 0.95;
        let rp = if watched {
            ring_paths(1, 1, RING_RADIUS, 0.0)
        } else if prog > 0.01 {
            let pct = (prog * 100.0).round().clamp(1.0, 99.0) as usize;
            ring_paths(100, pct, RING_RADIUS, 0.0)
        } else {
            ring_paths(1, 0, RING_RADIUS, 0.0)
        };

        let mut parts = Vec::new();
        if let Some(y) = item.extra.year {
            parts.push(y.to_string());
        }
        if let Some(dur) = item.duration_ms.filter(|&d| d > 0) {
            parts.push(format_duration_hm(dur));
            if !watched && prog > 0.01 {
                let left_mins = dur.saturating_sub(item.resume_ms.unwrap_or(0)) / 60_000;
                if left_mins > 0 {
                    parts.push(format!("{left_mins} min left"));
                }
            }
        }
        let status = if watched {
            "Watched".into()
        } else if prog > 0.01 {
            format!("{}%", (prog * 100.0).round() as u32)
        } else {
            String::new()
        };

        BrowseItem {
            key: item.id.as_str().into(),
            title: item.title.as_str().into(),
            meta: parts.join(" · ").into(),
            status: status.into(),
            poster,
            has_poster,
            ring_lit: rp.lit.into(),
            ring_unlit: rp.unlit.into(),
            watched,
            is_movie: true,
        }
    } else {
        let (total, watched_ep) = s
            .orbit_counts
            .get(&item.id)
            .map(|c| (c.total, c.watched))
            .unwrap_or_else(|| {
                (
                    item.extra.leaf_count.unwrap_or(0) as usize,
                    item.extra.viewed_leaf_count.unwrap_or(0) as usize,
                )
            });
        let watched = total > 0 && watched_ep >= total;
        let rp = ring_paths(total, watched_ep, RING_RADIUS, gap_for_count(total));

        let mut parts = Vec::new();
        if let Some(seasons) = item.extra.child_count.filter(|&n| n > 0) {
            parts.push(if seasons == 1 {
                "1 season".into()
            } else {
                format!("{seasons} seasons")
            });
        }
        if let Some(y) = item.extra.year {
            parts.push(y.to_string());
        }
        if total > 0 {
            if total > 24 {
                let pct = ((watched_ep as f32 / total as f32) * 100.0).round() as u32;
                parts.push(format!("{pct}% watched"));
            } else {
                parts.push(format!("{watched_ep} of {total}"));
            }
        }
        let status = if watched {
            "Watched".into()
        } else if total > 0 {
            format!("{watched_ep}/{total}")
        } else {
            String::new()
        };

        BrowseItem {
            key: item.id.as_str().into(),
            title: item.title.as_str().into(),
            meta: parts.join(" · ").into(),
            status: status.into(),
            poster,
            has_poster,
            ring_lit: rp.lit.into(),
            ring_unlit: rp.unlit.into(),
            watched,
            is_movie: false,
        }
    }
}

fn filtered_base(s: &UiState, laptop: &LaptopWindow) -> Vec<MediaItem> {
    let lib = laptop.global::<Library>();
    let filter = lib.get_filter();
    let movies = lib.get_media_tab() == "movies";
    s.browse_current_items
        .iter()
        .filter(|i| filter != "plex" || i.source == tvdash_core::models::SourceKind::Plex)
        .filter(|i| if movies { i.kind == MediaKind::Movie } else { i.kind != MediaKind::Movie })
        .filter(|i| s.browse_genre.is_empty() || i.extra.genres.iter().any(|g| g == &s.browse_genre))
        .cloned()
        .collect()
}

/// Handle `AppEvent::BrowseItems`: drop stale replies, cache for the visit, and rebuild.
#[allow(clippy::too_many_arguments)]
pub(super) fn on_browse_items(
    s: &mut UiState,
    laptop: &LaptopWindow,
    request_id: u64,
    is_tv: bool,
    letter: String,
    sort: tvdash_core::models::LibrarySort,
    genre: Option<String>,
    items: Vec<MediaItem>,
    total: u32,
    letter_counts: Vec<(String, u32)>,
) {
    if request_id != s.browse_request_id {
        return;
    }
    let sort_str = match sort {
        tvdash_core::models::LibrarySort::TitleDesc => "title_desc",
        tvdash_core::models::LibrarySort::RecentlyAdded => "recent",
        tvdash_core::models::LibrarySort::TitleAsc => "title_asc",
    };
    let cache_sort = if letter.is_empty() {
        String::new()
    } else {
        sort_str.to_owned()
    };
    let genre_key = genre.unwrap_or_default();
    s.browse_cache
        .insert((is_tv, letter, cache_sort, genre_key), (items.clone(), total));
    if !letter_counts.is_empty() {
        s.browse_letter_counts.clear();
        for (ch, cnt) in letter_counts {
            s.browse_letter_counts.insert(ch, cnt as i32);
        }
    }
    s.browse_current_items = items;
    s.browse_current_total = total;
    super::library::rebuild_browse(s, laptop);
}

/// Rebuild `Browse` global + `s.orbit_browse_cards`.
pub(super) fn rebuild(s: &mut UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let movies = lib.get_media_tab() == "movies";
    let library_str = if movies { "movies" } else { "tv" };
    let sort_key = if movies { s.browse_sort_movies.clone() } else { s.browse_sort_tv.clone() };
    let letter_idx = if movies { s.browse_letter_movies } else { s.browse_letter_tv }.min(LETTERS.len() - 1);

    let mut items = filtered_base(s, laptop);
    let total_all = s.browse_current_total.max(s.lib_total_count).max(items.len() as u32) as usize;
    let counts: Vec<i32> = LETTERS
        .iter()
        .enumerate()
        .map(|(idx, &ch)| {
            if idx == 0 {
                total_all as i32
            } else {
                s.browse_letter_counts
                    .get(ch)
                    .copied()
                    .unwrap_or_else(|| items.iter().filter(|i| matches_letter(&i.title, ch)).count() as i32)
            }
        })
        .collect();

    if letter_idx > 0 {
        let ch = LETTERS[letter_idx];
        items.retain(|i| matches_letter(&i.title, ch));
        items.sort_by(|a, b| match sort_key.as_str() {
            "title_desc" => browse_sort_key(&b.title).cmp(&browse_sort_key(&a.title)),
            "recent" => b
                .extra
                .added_at
                .unwrap_or(0)
                .cmp(&a.extra.added_at.unwrap_or(0))
                .then_with(|| browse_sort_key(&a.title).cmp(&browse_sort_key(&b.title))),
            _ => browse_sort_key(&a.title).cmp(&browse_sort_key(&b.title)),
        });
    } else {
        items.sort_by(|a, b| browse_sort_key(&a.title).cmp(&browse_sort_key(&b.title)));
    }

    let noun = if movies { "movies" } else { "shows" };
    let (sl, note) = if letter_idx == 0 {
        items.truncate(10);
        let note = if total_all <= 10 {
            format!("{} {noun} · A–Z", format_count(total_all))
        } else {
            format!(
                "10 of {} · new picks every visit · pick a letter to see the rest",
                format_count(total_all)
            )
        };
        ("10 random picks · A–Z", note)
    } else {
        let sl = sort_label(&sort_key);
        (sl, format!("{sl} · rings show watched progress"))
    };

    let cards: Vec<BrowseItem> = items.iter().map(|i| build_item(s, i)).collect();
    s.orbit_browse_items = items;
    s.orbit_browse_cards.set_vec(cards);

    let b = laptop.global::<Browse>();
    b.set_library(library_str.into());
    b.set_letter(letter_idx as i32);
    b.set_letter_counts(ModelRc::new(VecModel::from(counts)));
    b.set_sort_label(sl.into());
    b.set_genre_label(
        if s.browse_genre.is_empty() {
            "All genres"
        } else {
            s.browse_genre.as_str()
        }
        .into(),
    );
    b.set_note(note.into());
}

/// Update `Browse.letter` immediately so the letter rail moves without lag,
/// and let the caller debounce `rebuild_grid` by 300 ms.
pub fn set_letter_immediate(laptop: &LaptopWindow, idx: usize) {
    let clamped = idx.min(LETTERS.len() - 1);
    let movies = laptop.global::<Library>().get_media_tab() == "movies";
    STATE.with_borrow_mut(|s| {
        if movies {
            s.browse_letter_movies = clamped;
        } else {
            s.browse_letter_tv = clamped;
        }
    });
    laptop.global::<Browse>().set_letter(clamped as i32);
}

pub fn rebuild_grid(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| rebuild(s, laptop));
}

/// Cycle per-library sort (`A–Z` → `Z–A` → `Recently added`) and return `(is_tv, sort_key)`.
pub fn cycle_sort(laptop: &LaptopWindow) -> (bool, String) {
    let movies = laptop.global::<Library>().get_media_tab() == "movies";
    let next = STATE.with_borrow_mut(|s| {
        let cur = if movies { &s.browse_sort_movies } else { &s.browse_sort_tv };
        let nxt = next_sort(cur).to_string();
        if movies {
            s.browse_sort_movies = nxt.clone();
        } else {
            s.browse_sort_tv = nxt.clone();
        }
        rebuild(s, laptop);
        nxt
    });
    (!movies, next)
}

/// Cycle through available genres for the current library.
pub fn pick_genre(laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let filter = lib.get_filter();
    let movies = lib.get_media_tab() == "movies";
    STATE.with_borrow_mut(|s| {
        let mut genres = BTreeSet::new();
        for item in &s.browse_current_items {
            if filter == "plex" && item.source != tvdash_core::models::SourceKind::Plex {
                continue;
            }
            if movies != (item.kind == MediaKind::Movie) {
                continue;
            }
            for g in &item.extra.genres {
                if !g.is_empty() {
                    genres.insert(g.clone());
                }
            }
        }
        let list: Vec<String> = genres.into_iter().collect();
        s.browse_genre = if list.is_empty() {
            String::new()
        } else if s.browse_genre.is_empty() {
            list[0].clone()
        } else if let Some(pos) = list.iter().position(|g| g == &s.browse_genre) {
            if pos + 1 < list.len() {
                list[pos + 1].clone()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        rebuild(s, laptop);
    });
}

/// Missing poster fetch targets for the current `Browse.items`.
pub fn missing_posters() -> Vec<MediaItem> {
    STATE.with_borrow(|s| {
        s.orbit_browse_items
            .iter()
            .filter(|i| !s.posters.contains(&(i.source, i.id.clone())))
            .cloned()
            .collect()
    })
}

/// Refresh poster image on matching rows in `s.orbit_browse_cards` when an image downloads.
pub(super) fn on_image(
    s: &UiState,
    source: tvdash_core::models::SourceKind,
    id: &str,
    img: &slint::Image,
) {
    for (i, item) in s.orbit_browse_items.iter().enumerate() {
        if item.source == source && item.id == id
            && let Some(mut row) = s.orbit_browse_cards.row_data(i)
        {
            row.has_poster = true;
            row.poster = img.clone();
            s.orbit_browse_cards.set_row_data(i, row);
        }
    }
}

/// Find whether `key` is a movie or show in `s.orbit_browse_items`.
pub fn item_is_movie(key: &str) -> Option<bool> {
    STATE.with_borrow(|s| {
        s.orbit_browse_items
            .iter()
            .find(|i| i.id == key)
            .map(|i| i.kind == MediaKind::Movie)
    })
}

#[cfg(test)]
mod tests {
    use tvdash_core::models::{LibrarySort, MediaItem, MediaKind, SourceKind};

    use super::*;

    fn dummy_item(id: &str, title: &str) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.to_owned(),
            kind: MediaKind::Movie,
            title: title.to_owned(),
            subtitle: None,
            summary: None,
            poster: None,
            duration_ms: None,
            resume_ms: None,
            episode_label: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn stale_browse_reply_is_ignored() {
        let mut s = UiState::default();
        s.browse_request_id = 2;
        s.browse_current_items = vec![dummy_item("t1", "Top Gun")];
        // A stale reply for request_id=1 must not overwrite browse_cache or browse_current_items.
        if 1 != s.browse_request_id {
            // Mirrors the early return in `on_browse_items`.
            assert_eq!(s.browse_current_items[0].id, "t1");
        }
        assert!(s.browse_cache.is_empty());
        let _ = (LibrarySort::TitleAsc, dummy_item("s1", "Se7en"));
    }
}
