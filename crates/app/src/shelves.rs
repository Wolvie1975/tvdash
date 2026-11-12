//! The Library's smart shelves, worked out locally from what Plex reports
//! (watch counts, progress, when things were added, genres). No UI here.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveTime, TimeZone, Utc};
use tvdash_core::models::{LibraryData, MediaItem, MediaKind, Season};

use crate::format;

/// The Library's Movies / TV switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tv,
    Movies,
}

impl Tab {
    pub fn parse(s: &str) -> Self {
        if s == "movies" { Self::Movies } else { Self::Tv }
    }
}

/// A card in the big box: a season to finish, or a movie to finish.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxItem {
    /// The show (to play its next episode) or the movie.
    pub item: MediaItem,
    /// "Season 1 · 2 episodes left" / "22 min left".
    pub left: String,
    /// One bar segment per episode (seasons); 0 = one continuous bar (movies).
    pub segments: u32,
    pub done: u32,
    pub progress: f32,
}

/// Shelves with at most three cards in the big box.
pub const BOX_ITEMS: usize = 3;
/// Genre tiles.
pub const GENRES: usize = 6;

/// Most episodes drawn as separate segments; longer seasons get one bar.
const MAX_SEGMENTS: u32 = 24;

/// Tonight's pick candidates: shows you haven't started / movies you haven't watched.
pub fn pick_candidates(lib: &LibraryData, tab: Tab) -> Vec<&MediaItem> {
    match tab {
        Tab::Tv => {
            lib.shows.iter().filter(|s| s.viewed_leaf_count == Some(0) && s.leaf_count.unwrap_or(0) > 0).collect()
        }
        Tab::Movies => lib.movies.iter().filter(|m| m.view_count == 0 && m.resume_ms.is_none()).collect(),
    }
}

/// Seasons with 1–3 episodes left, fewest left first.
pub fn finish_a_season(lib: &LibraryData) -> Vec<BoxItem> {
    let shows: HashMap<&str, &MediaItem> = lib.shows.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut seasons: Vec<&Season> =
        lib.seasons.iter().filter(|s| (1..=3).contains(&s.leaf_count.saturating_sub(s.viewed_leaf_count))).collect();
    seasons.sort_by_key(|s| (s.leaf_count - s.viewed_leaf_count, s.show_title.to_lowercase()));
    seasons
        .into_iter()
        .take(BOX_ITEMS)
        .map(|s| {
            let left = s.leaf_count - s.viewed_leaf_count;
            let item = shows.get(s.show_id.as_str()).map(|m| (*m).clone()).unwrap_or_else(|| MediaItem {
                id: s.show_id.clone(),
                kind: MediaKind::Series,
                title: s.show_title.clone(),
                ..Default::default()
            });
            let season = s.index.map(|i| format!("Season {i} · ")).unwrap_or_default();
            BoxItem {
                item,
                left: format!("{season}{left} episode{} left", if left == 1 { "" } else { "s" }),
                segments: if s.leaf_count <= MAX_SEGMENTS { s.leaf_count } else { 0 },
                done: s.viewed_leaf_count,
                progress: s.viewed_leaf_count as f32 / s.leaf_count as f32,
            }
        })
        .collect()
}

/// Movies started but not finished.
pub fn movies_in_progress(lib: &LibraryData) -> Vec<BoxItem> {
    lib.movies
        .iter()
        .filter(|m| m.resume_ms.is_some())
        .take(BOX_ITEMS)
        .map(|m| BoxItem {
            item: m.clone(),
            left: format::time_left(m).unwrap_or_default(),
            segments: 0,
            done: 0,
            progress: m.progress().unwrap_or(0.0),
        })
        .collect()
}

/// Start of this week (Monday 00:00) in `tz` (system local if `None`), as Unix seconds.
pub fn week_start(tz: Option<chrono_tz::Tz>, now: chrono::DateTime<Utc>) -> i64 {
    let local = format::now_local_at(tz, now);
    let monday = local.date() - chrono::Days::new(u64::from(local.weekday().num_days_from_monday()));
    let midnight = monday.and_time(NaiveTime::MIN);
    let at = match tz {
        Some(tz) => tz.from_local_datetime(&midnight).earliest().map(|t| t.timestamp()),
        None => chrono::Local.from_local_datetime(&midnight).earliest().map(|t| t.timestamp()),
    };
    at.unwrap_or_else(|| midnight.and_utc().timestamp())
}

/// New since `since` (Unix seconds), newest first. TV: shows added, and shows
/// with new episodes.
pub fn added_since(lib: &LibraryData, tab: Tab, since: i64) -> Vec<MediaItem> {
    let new = |i: &MediaItem| i.added_at.is_some_and(|t| t >= since);
    match tab {
        Tab::Movies => {
            let mut v: Vec<MediaItem> = lib.movies.iter().filter(|m| new(m)).cloned().collect();
            v.sort_by_key(|m| std::cmp::Reverse(m.added_at));
            v
        }
        Tab::Tv => {
            // (when, show id), newest first: the show itself, or its newest new episode.
            let mut when: HashMap<&str, i64> = HashMap::new();
            for s in lib.shows.iter().filter(|s| new(s)) {
                when.insert(&s.id, s.added_at.unwrap_or(0));
            }
            for e in lib.new_episodes.iter().filter(|e| new(e)) {
                if let Some(show) = e.show_id.as_deref() {
                    let t = when.entry(show).or_insert(0);
                    *t = (*t).max(e.added_at.unwrap_or(0));
                }
            }
            let mut v: Vec<(i64, MediaItem)> =
                lib.shows.iter().filter_map(|s| Some((*when.get(s.id.as_str())?, s.clone()))).collect();
            v.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
            v.into_iter().map(|(_, s)| s).collect()
        }
    }
}

/// Movies played more than once / shows watched all the way through.
pub fn watch_again(lib: &LibraryData, tab: Tab) -> Vec<MediaItem> {
    match tab {
        Tab::Movies => lib.movies.iter().filter(|m| m.view_count > 1).cloned().collect(),
        Tab::Tv => lib
            .shows
            .iter()
            .filter(|s| s.leaf_count.unwrap_or(0) > 0 && s.viewed_leaf_count == s.leaf_count)
            .cloned()
            .collect(),
    }
}

/// Every movie or show in the tab.
pub fn all(lib: &LibraryData, tab: Tab) -> &[MediaItem] {
    match tab {
        Tab::Movies => &lib.movies,
        Tab::Tv => &lib.shows,
    }
}

/// The six biggest genres in the tab, with counts.
pub fn genres(lib: &LibraryData, tab: Tab) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for item in all(lib, tab) {
        let mut seen = HashSet::new();
        for g in item.genres.iter().filter(|g| seen.insert(g.as_str())) {
            *counts.entry(g.as_str()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts.into_iter().map(|(g, n)| (g.to_owned(), n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(GENRES);
    v
}

/// What Browse all shows: "all", "unwatched", "added", "again" or "genre:<name>".
pub fn browse(lib: &LibraryData, tab: Tab, filter: &str, since: i64, unwatched_only: bool) -> Vec<MediaItem> {
    let unwatched = |i: &MediaItem| match i.kind {
        MediaKind::Series => i.viewed_leaf_count.unwrap_or(0) < i.leaf_count.unwrap_or(0),
        _ => i.view_count == 0,
    };
    let mut items: Vec<MediaItem> = match filter {
        "added" => added_since(lib, tab, since),
        "again" => watch_again(lib, tab),
        f => {
            let genre = f.strip_prefix("genre:");
            let mut v: Vec<MediaItem> = all(lib, tab)
                .iter()
                .filter(|i| genre.is_none_or(|g| i.genres.iter().any(|x| x == g)))
                .cloned()
                .collect();
            // Recently added first.
            v.sort_by_key(|i| std::cmp::Reverse(i.added_at));
            v
        }
    };
    if unwatched_only || filter == "unwatched" {
        items.retain(unwatched);
    }
    items
}

/// A small random number (shuffling Tonight's pick); no crate needed.
pub fn random(below: usize) -> usize {
    use std::hash::{BuildHasher, Hasher};
    if below == 0 {
        return 0;
    }
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    (h.finish() % below as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(id: &str, views: u32, resume: Option<u64>, added: i64, genres: &[&str]) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind: MediaKind::Movie,
            title: id.into(),
            view_count: views,
            resume_ms: resume,
            duration_ms: Some(6_000_000),
            added_at: Some(added),
            genres: genres.iter().map(|g| g.to_string()).collect(),
            ..Default::default()
        }
    }

    fn show(id: &str, leaves: u32, viewed: u32, added: i64) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind: MediaKind::Series,
            title: id.into(),
            leaf_count: Some(leaves),
            viewed_leaf_count: Some(viewed),
            added_at: Some(added),
            genres: vec!["Drama".into()],
            ..Default::default()
        }
    }

    fn season(show: &str, index: u32, leaves: u32, viewed: u32) -> Season {
        Season {
            show_id: show.into(),
            show_title: show.into(),
            index: Some(index),
            leaf_count: leaves,
            viewed_leaf_count: viewed,
        }
    }

    fn lib() -> LibraryData {
        LibraryData {
            movies: vec![
                movie("new", 0, None, 900, &["Drama", "Mystery"]),
                movie("started", 0, Some(1_000_000), 100, &["Drama"]),
                movie("twice", 2, None, 100, &["Comedy"]),
                movie("once", 1, None, 950, &["Drama", "Drama"]),
            ],
            shows: vec![show("fresh", 8, 0, 100), show("done", 10, 10, 100), show("mid", 30, 22, 100)],
            seasons: vec![
                season("mid", 3, 10, 8),
                season("mid", 2, 10, 10),
                season("fresh", 1, 8, 0),
                season("x", 1, 4, 3),
            ],
            new_episodes: vec![MediaItem {
                id: "e".into(),
                kind: MediaKind::Episode,
                show_id: Some("mid".into()),
                added_at: Some(990),
                ..Default::default()
            }],
        }
    }

    #[test]
    fn picks_and_boxes() {
        let lib = lib();
        let ids = |v: Vec<&MediaItem>| v.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(pick_candidates(&lib, Tab::Tv)), ["fresh"]);
        assert_eq!(ids(pick_candidates(&lib, Tab::Movies)), ["new"], "started or watched movies aren't picked");

        let fin = finish_a_season(&lib);
        let got: Vec<(&str, &str, u32, u32)> =
            fin.iter().map(|b| (b.item.id.as_str(), b.left.as_str(), b.segments, b.done)).collect();
        assert_eq!(got, [("x", "Season 1 · 1 episode left", 4, 3), ("mid", "Season 3 · 2 episodes left", 10, 8)]);
        assert_eq!(fin[0].item.kind, MediaKind::Series, "a show not in the list still plays as a show");

        let cont = movies_in_progress(&lib);
        assert_eq!((cont.len(), cont[0].item.id.as_str(), cont[0].left.as_str()), (1, "started", "1 h 23 min left"));
        assert_eq!(cont[0].segments, 0);
    }

    #[test]
    fn shelves_and_genres() {
        let lib = lib();
        let ids = |v: Vec<MediaItem>| v.into_iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(ids(added_since(&lib, Tab::Movies, 900)), ["once", "new"]);
        assert_eq!(ids(added_since(&lib, Tab::Tv, 900)), ["mid"], "a new episode brings its show in");
        assert_eq!(ids(watch_again(&lib, Tab::Movies)), ["twice"]);
        assert_eq!(ids(watch_again(&lib, Tab::Tv)), ["done"]);
        assert_eq!(genres(&lib, Tab::Movies), [("Drama".into(), 3), ("Comedy".into(), 1), ("Mystery".into(), 1)]);

        assert_eq!(ids(browse(&lib, Tab::Movies, "genre:Drama", 0, false)), ["once", "new", "started"]);
        assert_eq!(ids(browse(&lib, Tab::Movies, "all", 0, true)), ["new", "started"]);
        assert_eq!(ids(browse(&lib, Tab::Tv, "unwatched", 0, false)), ["fresh", "mid"]);
    }

    #[test]
    fn week_starts_on_monday_midnight() {
        let tz = Some(chrono_tz::America::Chicago);
        // Thursday 2026-11-12 18:00 UTC = 12:00 in Chicago; Monday 00:00 CST = 06:00 UTC.
        let now = Utc.with_ymd_and_hms(2026, 11, 12, 18, 0, 0).unwrap();
        let monday = Utc.with_ymd_and_hms(2026, 11, 9, 6, 0, 0).unwrap().timestamp();
        assert_eq!(week_start(tz, now), monday);
        // On a Monday, it's that same day.
        let mon_noon = Utc.with_ymd_and_hms(2026, 11, 9, 18, 0, 0).unwrap();
        assert_eq!(week_start(tz, mon_noon), monday);
        assert!(random(3) < 3);
        assert_eq!(random(0), 0);
    }
}
