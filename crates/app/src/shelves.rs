//! The Library's smart shelves, worked out locally from what Plex reports
//! (watch counts, progress, when things were added, genres). No UI here.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveTime, TimeZone, Utc};
use tvdash_core::{
    config::SortOrder,
    models::{Collection, LibraryData, MediaItem, MediaKind, Season, ShowSeason},
};

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

/// Title as Plex sorts it: its sort title if set, else the title without a
/// leading "The", "A" or "An" ("The Long Drive Home" sorts under L).
pub fn sort_key(item: &MediaItem) -> String {
    let t = item.sort_title.as_deref().unwrap_or(&item.title).trim().to_lowercase();
    ["the ", "a ", "an "].iter().find_map(|a| t.strip_prefix(a)).map(str::to_owned).unwrap_or(t)
}

/// Put `items` in the chosen order (stable, so ties keep their order).
pub fn sort(items: &mut [MediaItem], order: SortOrder) {
    match order {
        SortOrder::Added => items.sort_by_key(|i| std::cmp::Reverse(i.added_at)),
        SortOrder::Az => items.sort_by_cached_key(sort_key),
        SortOrder::Za => items.sort_by_cached_key(|i| std::cmp::Reverse(sort_key(i))),
    }
}

/// Every genre in the tab, biggest first (Browse all's chips).
pub fn all_genres(lib: &LibraryData, tab: Tab) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for g in all(lib, tab).iter().flat_map(|i| i.genres.iter()) {
        *counts.entry(g.as_str()).or_default() += 1;
    }
    let mut v: Vec<(&str, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    v.into_iter().map(|(g, _)| g.to_owned()).collect()
}

/// What Browse all shows: "all", "unwatched", "added", "again" or "genre:<name>",
/// in `order`.
pub fn browse(
    lib: &LibraryData,
    tab: Tab,
    filter: &str,
    since: i64,
    unwatched_only: bool,
    order: SortOrder,
) -> Vec<MediaItem> {
    let unwatched = |i: &MediaItem| match i.kind {
        MediaKind::Series => i.viewed_leaf_count.unwrap_or(0) < i.leaf_count.unwrap_or(0),
        _ => i.view_count == 0,
    };
    let mut items: Vec<MediaItem> = match filter {
        "added" => added_since(lib, tab, since),
        "again" => watch_again(lib, tab),
        f => {
            let genre = f.strip_prefix("genre:");
            all(lib, tab).iter().filter(|i| genre.is_none_or(|g| i.genres.iter().any(|x| x == g))).cloned().collect()
        }
    };
    if unwatched_only || filter == "unwatched" {
        items.retain(unwatched);
    }
    sort(&mut items, order);
    items
}

// ── Collections ─────────────────────────────────────────────────────

/// A collection's numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionStats {
    pub total: usize,
    pub watched: usize,
    /// First and last release year.
    pub years: Option<(u32, u32)>,
    pub runtime_ms: u64,
    /// Newest `addedAt` of its movies.
    pub latest_added: Option<i64>,
}

pub fn stats(c: &Collection) -> CollectionStats {
    let years: Vec<u32> = c.movies.iter().filter_map(|m| m.year).collect();
    CollectionStats {
        total: c.movies.len(),
        watched: c.movies.iter().filter(|m| m.view_count > 0).count(),
        years: years.iter().min().zip(years.iter().max()).map(|(a, b)| (*a, *b)),
        runtime_ms: c.movies.iter().filter_map(|m| m.duration_ms).sum(),
        latest_added: c.movies.iter().filter_map(|m| m.added_at).max(),
    }
}

/// The Collections row: ones with something left to watch first, then by the
/// most recently added movie.
pub fn collection_row(lib: &LibraryData) -> Vec<&Collection> {
    let mut v: Vec<(&Collection, CollectionStats)> = lib.collections.iter().map(|c| (c, stats(c))).collect();
    v.sort_by_key(|(_, s)| (s.watched >= s.total, std::cmp::Reverse(s.latest_added)));
    v.into_iter().map(|(c, _)| c).collect()
}

/// A collection page's order buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionOrder {
    /// The collection's own order (`collectionSort`), shown as #1, #2…
    Collection,
    Release,
    Az,
}

impl CollectionOrder {
    pub fn parse(s: &str) -> Self {
        match s {
            "release" => Self::Release,
            "az" => Self::Az,
            _ => Self::Collection,
        }
    }
}

/// Release date for ordering: the full date if known, else the year.
fn release_key(m: &MediaItem) -> String {
    m.released.clone().or_else(|| m.year.map(|y| format!("{y:04}"))).unwrap_or_else(|| "9999".into())
}

/// The movies to show: (position in collection order, movie).
pub fn collection_movies(c: &Collection, order: CollectionOrder, hide_watched: bool) -> Vec<(usize, MediaItem)> {
    let mut v: Vec<(usize, MediaItem)> = c
        .movies
        .iter()
        .enumerate()
        .filter(|(_, m)| !hide_watched || m.view_count == 0)
        .map(|(i, m)| (i + 1, m.clone()))
        .collect();
    match order {
        CollectionOrder::Collection => {}
        CollectionOrder::Release => v.sort_by_cached_key(|(_, m)| release_key(m)),
        CollectionOrder::Az => v.sort_by_cached_key(|(_, m)| sort_key(m)),
    }
    v
}

/// What the collection's big button plays.
#[derive(Debug, Clone, PartialEq)]
pub enum UpNext {
    /// A movie that's started.
    Resume(MediaItem),
    /// The first unwatched movie in collection order.
    Play(MediaItem),
    /// Everything's watched: the first movie, from the start.
    FromStart(MediaItem),
}

impl UpNext {
    pub fn item(&self) -> &MediaItem {
        match self {
            Self::Resume(m) | Self::Play(m) | Self::FromStart(m) => m,
        }
    }
}

pub fn up_next(c: &Collection) -> Option<UpNext> {
    if let Some(m) = c.movies.iter().find(|m| m.resume_ms.is_some()) {
        return Some(UpNext::Resume(m.clone()));
    }
    if let Some(m) = c.movies.iter().find(|m| m.view_count == 0) {
        return Some(UpNext::Play(m.clone()));
    }
    c.movies.first().map(|m| UpNext::FromStart(MediaItem { resume_ms: None, ..m.clone() }))
}

/// A random unwatched movie in the collection (not `except`).
pub fn shuffle_unwatched(c: &Collection, except: Option<&str>) -> Option<MediaItem> {
    let pool: Vec<&MediaItem> =
        c.movies.iter().filter(|m| m.view_count == 0 && Some(m.id.as_str()) != except).collect();
    pool.get(random(pool.len())).map(|m| (*m).clone())
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

// ── Show page ───────────────────────────────────────────────────────

/// Seasons in order, Specials (season 0) last.
pub fn order_seasons(seasons: &[ShowSeason]) -> Vec<ShowSeason> {
    let mut v = seasons.to_vec();
    v.sort_by_key(|s| (s.index == 0, s.index));
    v
}

/// ("Season 2" / "Specials", "S2" / "Sp").
pub fn season_names(s: &ShowSeason) -> (String, String) {
    if s.index == 0 {
        ("Specials".into(), "Sp".into())
    } else {
        (format!("Season {}", s.index), format!("S{}", s.index))
    }
}

/// "Watched", "2 of 10 watched" or "10 episodes".
pub fn season_progress(s: &ShowSeason) -> String {
    match (s.viewed_leaf_count, s.leaf_count) {
        (v, n) if n > 0 && v >= n => "Watched".into(),
        (0, n) => format!("{n} episode{}", if n == 1 { "" } else { "s" }),
        (v, n) => format!("{v} of {n} watched"),
    }
}

/// The season selector for this many seasons: full tabs, compact pills, or a picker.
pub fn season_selector(count: usize) -> &'static str {
    match count {
        0..=6 => "tabs",
        7..=14 => "pills",
        _ => "picker",
    }
}

/// A season's episodes, in order.
pub fn season_episodes(eps: &[MediaItem], season: u32) -> Vec<MediaItem> {
    let mut v: Vec<MediaItem> = eps.iter().filter(|e| e.season.unwrap_or(0) == season).cloned().collect();
    v.sort_by_key(|e| e.episode.unwrap_or(0));
    v
}

/// The show's up-next episode: the one in progress, else the first unwatched one
/// after the last watched, else the first unwatched, else the first (from the start).
/// Specials only count when there's nothing else.
pub fn show_up_next(eps: &[MediaItem]) -> Option<UpNext> {
    let mut v: Vec<&MediaItem> = eps.iter().filter(|e| e.season.unwrap_or(0) > 0).collect();
    if v.is_empty() {
        v = eps.iter().collect();
    }
    v.sort_by_key(|e| (e.season.unwrap_or(0), e.episode.unwrap_or(0)));
    if let Some(e) = v.iter().find(|e| e.resume_ms.is_some()) {
        return Some(UpNext::Resume((*e).clone()));
    }
    let after_last = v.iter().rposition(|e| e.view_count > 0).map_or(0, |i| i + 1);
    if let Some(e) = v[after_last..].iter().chain(v[..after_last].iter()).find(|e| e.view_count == 0) {
        return Some(UpNext::Play((*e).clone()));
    }
    v.first().map(|e| UpNext::FromStart(MediaItem { resume_ms: None, ..(*e).clone() }))
}

/// "S1 · E3" for an episode.
pub fn episode_code(e: &MediaItem) -> String {
    match (e.season, e.episode) {
        (Some(0), Some(n)) => format!("Special {n}"),
        (Some(s), Some(n)) => format!("S{s} · E{n}"),
        _ => e.episode_label.clone().unwrap_or_default(),
    }
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
            collections: vec![],
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

        let added = SortOrder::Added;
        assert_eq!(ids(browse(&lib, Tab::Movies, "genre:Drama", 0, false, added)), ["once", "new", "started"]);
        assert_eq!(ids(browse(&lib, Tab::Movies, "all", 0, true, added)), ["new", "started"]);
        assert_eq!(ids(browse(&lib, Tab::Tv, "unwatched", 0, false, SortOrder::Az)), ["fresh", "mid"]);
        assert_eq!(ids(browse(&lib, Tab::Movies, "all", 0, false, SortOrder::Za)), ["twice", "started", "once", "new"]);
    }

    #[test]
    fn sorting_uses_the_sort_title() {
        let t = |title: &str, sort: Option<&str>, added| MediaItem {
            title: title.into(),
            sort_title: sort.map(Into::into),
            added_at: Some(added),
            ..Default::default()
        };
        let mut v = vec![
            t("The Long Drive Home", None, 3),
            t("Zeta", None, 1),
            t("Alpha", None, 2),
            t("Mike", Some("Bravo"), 4),
        ];
        let titles = |v: &[MediaItem]| v.iter().map(|i| i.title.clone()).collect::<Vec<_>>();
        sort(&mut v, SortOrder::Az);
        assert_eq!(titles(&v), ["Alpha", "Mike", "The Long Drive Home", "Zeta"]);
        sort(&mut v, SortOrder::Za);
        assert_eq!(titles(&v), ["Zeta", "The Long Drive Home", "Mike", "Alpha"]);
        sort(&mut v, SortOrder::Added);
        assert_eq!(titles(&v), ["Mike", "The Long Drive Home", "Alpha", "Zeta"]);
    }

    fn collection(id: &str, movies: Vec<MediaItem>) -> Collection {
        Collection { id: id.into(), title: id.into(), movies, ..Default::default() }
    }

    fn film(id: &str, year: u32, views: u32, resume: Option<u64>, added: i64) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind: MediaKind::Movie,
            title: id.into(),
            year: Some(year),
            view_count: views,
            resume_ms: resume,
            duration_ms: Some(3_600_000),
            added_at: Some(added),
            ..Default::default()
        }
    }

    #[test]
    fn collections_order_stats_and_up_next() {
        let done = collection("done", vec![film("a", 2001, 1, None, 900)]);
        let old = collection("old", vec![film("b", 1998, 0, None, 100), film("c", 2024, 1, None, 200)]);
        let new = collection("new", vec![film("d", 2010, 0, None, 500)]);
        let lib = LibraryData { collections: vec![done.clone(), old.clone(), new], ..Default::default() };
        let ids: Vec<&str> = collection_row(&lib).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["new", "old", "done"], "unfinished first, newest addition first");

        let st = stats(&old);
        assert_eq!((st.total, st.watched, st.years, st.runtime_ms), (2, 1, Some((1998, 2024)), 7_200_000));

        // Up next: started, else first unwatched in collection order, else from the start.
        assert_eq!(up_next(&old), Some(UpNext::Play(old.movies[0].clone())));
        let started = collection("s", vec![film("x", 2000, 0, None, 1), film("y", 2001, 0, Some(5), 1)]);
        assert!(matches!(up_next(&started), Some(UpNext::Resume(ref m)) if m.id == "y"));
        assert!(matches!(up_next(&done), Some(UpNext::FromStart(ref m)) if m.id == "a" && m.resume_ms.is_none()));
        assert_eq!(up_next(&collection("empty", vec![])), None);

        // Orders and Hide watched; positions stay the collection's.
        let order = |o, hide| collection_movies(&old, o, hide).into_iter().map(|(n, m)| (n, m.id)).collect::<Vec<_>>();
        assert_eq!(order(CollectionOrder::Collection, false), [(1, "b".to_string()), (2, "c".into())]);
        assert_eq!(order(CollectionOrder::Release, true), [(1, "b".to_string())]);
        assert_eq!(shuffle_unwatched(&old, None).map(|m| m.id), Some("b".into()));
        assert_eq!(shuffle_unwatched(&old, Some("b")), None);
    }

    fn ep(season: u32, n: u32, views: u32, resume: Option<u64>) -> MediaItem {
        MediaItem {
            id: format!("{season}x{n}"),
            kind: MediaKind::Episode,
            season: Some(season),
            episode: Some(n),
            view_count: views,
            resume_ms: resume,
            ..Default::default()
        }
    }

    #[test]
    fn show_page_rules() {
        let season = |index: u32, leaves, viewed| ShowSeason {
            id: index.to_string(),
            index,
            title: String::new(),
            leaf_count: leaves,
            viewed_leaf_count: viewed,
        };
        let order: Vec<u32> =
            order_seasons(&[season(0, 2, 0), season(2, 10, 2), season(1, 8, 8)]).iter().map(|s| s.index).collect();
        assert_eq!(order, [1, 2, 0], "Specials last");
        assert_eq!(season_names(&season(0, 1, 0)), ("Specials".to_string(), "Sp".to_string()));
        assert_eq!(season_progress(&season(1, 8, 8)), "Watched");
        assert_eq!(season_progress(&season(2, 10, 2)), "2 of 10 watched");
        assert_eq!(season_progress(&season(3, 10, 0)), "10 episodes");
        assert_eq!(
            (season_selector(6), season_selector(7), season_selector(14), season_selector(15)),
            ("tabs", "pills", "pills", "picker")
        );

        let id = |u: Option<UpNext>| u.map(|u| u.item().id.clone());
        // In progress wins; then the first unwatched after the last watched.
        assert_eq!(
            id(show_up_next(&[ep(1, 1, 1, None), ep(1, 2, 0, None), ep(1, 3, 0, Some(9))])).as_deref(),
            Some("1x3")
        );
        assert_eq!(
            id(show_up_next(&[ep(1, 1, 0, None), ep(1, 2, 1, None), ep(2, 1, 0, None)])).as_deref(),
            Some("2x1")
        );
        // Skipped an early one, watched to the end: back to the gap.
        assert_eq!(id(show_up_next(&[ep(1, 1, 0, None), ep(1, 2, 1, None)])).as_deref(), Some("1x1"));
        // Specials don't count; all watched: from the start.
        let all = [ep(0, 1, 0, None), ep(1, 1, 1, None), ep(1, 2, 1, Some(5))];
        assert!(matches!(show_up_next(&all[..2]), Some(UpNext::FromStart(ref e)) if e.id == "1x1"));
        assert!(matches!(show_up_next(&all), Some(UpNext::Resume(ref e)) if e.id == "1x2"));
        assert_eq!(
            season_episodes(&[ep(1, 2, 0, None), ep(2, 1, 0, None), ep(1, 1, 0, None)], 1)
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["1x1", "1x2"]
        );
        assert_eq!(episode_code(&ep(0, 2, 0, None)), "Special 2");
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
