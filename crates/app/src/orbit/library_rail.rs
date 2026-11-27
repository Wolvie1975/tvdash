//! Pure data helpers for the Orbit Library landing shelf rail (Concept 2C).

use std::collections::HashSet;

use tvdash_core::models::{FinishSeasonItem, MediaItem, MediaKind};

use super::rings::{gap_for_count, ring_paths};
use crate::format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailShelf {
    pub title: &'static str,
    pub note: &'static str,
    pub count: usize,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct TvContinueEntry {
    pub item: MediaItem,
    pub ring_lit: String,
    pub ring_unlit: String,
    pub meta: String,
    pub finish_total: Option<u32>,
    pub finish_watched: Option<u32>,
    pub season_title: Option<String>,
}

/// Build the three rail shelves for Movies or TV Series (counts capped at 6).
pub fn rail_for(is_tv: bool, cw_count: usize, mid_count: usize, rel_count: usize) -> Vec<RailShelf> {
    let cw = cw_count.min(6);
    let mid = mid_count.min(6);
    let rel = rel_count.min(6);
    if is_tv {
        vec![
            RailShelf {
                title: "Continue Watching",
                note: "rings = episodes this season",
                count: cw,
                enabled: cw > 0,
            },
            RailShelf {
                title: "Start Watching",
                note: "new picks every Monday",
                count: mid,
                enabled: mid > 0,
            },
            RailShelf {
                title: "Recently Released",
                note: "newest release dates",
                count: rel,
                enabled: rel > 0,
            },
        ]
    } else {
        vec![
            RailShelf {
                title: "Continue Watching",
                note: "rings = progress",
                count: cw,
                enabled: cw > 0,
            },
            RailShelf {
                title: "Recently Added",
                note: "newest first",
                count: mid,
                enabled: mid > 0,
            },
            RailShelf {
                title: "Recently Released",
                note: "newest release dates",
                count: rel,
                enabled: rel > 0,
            },
        ]
    }
}

/// First enabled shelf index, or 0 if all shelves are empty.
pub fn default_rail_idx(rail: &[RailShelf]) -> usize {
    rail.iter().position(|s| s.enabled).unwrap_or(0)
}

/// Build TV Continue Watching entries (one per show, max 6) with season-segment or episode-% rings.
pub fn tv_continue(
    continue_items: &[MediaItem],
    finish_season: &[FinishSeasonItem],
    radius: f32,
) -> Vec<TvContinueEntry> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in continue_items {
        if item.kind == MediaKind::Movie {
            continue;
        }
        let show_key = (item.source, item.title.to_ascii_lowercase());
        if !seen.insert(show_key) {
            continue;
        }
        let fin = finish_season
            .iter()
            .find(|f| f.item.id == item.id || (f.item.source == item.source && f.item.title == item.title));
        let rp = if let Some(f) = fin {
            ring_paths(
                f.total_episodes as usize,
                f.watched_episodes as usize,
                radius,
                gap_for_count(f.total_episodes as usize),
            )
        } else {
            let prog = item.progress().unwrap_or(0.0);
            if prog > 0.0 {
                let pct = (prog * 100.0).round().clamp(1.0, 100.0) as usize;
                ring_paths(100, pct, radius, 0.0)
            } else {
                ring_paths(1, 0, radius, 0.0)
            }
        };
        let meta = tv_continue_meta(item, fin);
        out.push(TvContinueEntry {
            item: item.clone(),
            ring_lit: rp.lit,
            ring_unlit: rp.unlit,
            meta,
            finish_total: fin.map(|f| f.total_episodes),
            finish_watched: fin.map(|f| f.watched_episodes),
            season_title: fin.map(|f| f.season_title.clone()),
        });
        if out.len() >= 6 {
            break;
        }
    }
    out
}

/// Meta line under a TV Continue Watching poster.
pub fn tv_continue_meta(item: &MediaItem, fin: Option<&FinishSeasonItem>) -> String {
    let ep = item.episode_label.as_deref().unwrap_or("").trim();
    let base = if item.resume_ms.unwrap_or(0) > 0 {
        match (ep.is_empty(), format::time_left(item)) {
            (false, Some(left)) => format!("{ep} · {left}"),
            (true, Some(left)) => left,
            (false, None) => format!("{ep} next"),
            (true, None) => String::new(),
        }
    } else if !ep.is_empty() {
        format!("{ep} next")
    } else {
        String::new()
    };
    if let Some(f) = fin {
        let left = f.total_episodes.saturating_sub(f.watched_episodes);
        if (1..=3).contains(&left) {
            return if base.is_empty() {
                format!("{left} left")
            } else {
                format!("{base} · {left} left")
            };
        }
    }
    base
}

/// Meta line under a Movie grid poster.
pub fn movie_meta(item: &MediaItem) -> String {
    if let Some(left) = format::time_left(item) {
        return left;
    }
    if let Some(year) = item.extra.year {
        return year.to_string();
    }
    item.duration_ms.map(format::length).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvdash_core::models::{MediaExtra, SourceKind};

    fn make_ep(id: &str, show: &str, ep_label: &str, resume_ms: Option<u64>, duration_ms: Option<u64>) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.into(),
            kind: MediaKind::Episode,
            title: show.into(),
            subtitle: Some("Episode Title".into()),
            episode_label: Some(ep_label.into()),
            duration_ms,
            resume_ms,
            poster: None,
            summary: None,
            extra: MediaExtra::default(),
        }
    }

    fn make_movie(id: &str, title: &str, resume_ms: Option<u64>, duration_ms: Option<u64>) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.into(),
            kind: MediaKind::Movie,
            title: title.into(),
            subtitle: None,
            episode_label: None,
            duration_ms,
            resume_ms,
            poster: None,
            summary: None,
            extra: MediaExtra::default(),
        }
    }

    #[test]
    fn rail_for_movies_and_tv_titles_and_disabled() {
        let movies = rail_for(false, 0, 8, 2);
        assert_eq!(movies.len(), 3);
        assert_eq!(movies[0].title, "Continue Watching");
        assert_eq!(movies[0].count, 0);
        assert!(!movies[0].enabled);
        assert_eq!(movies[1].title, "Recently Added");
        assert_eq!(movies[1].count, 6);
        assert!(movies[1].enabled);
        assert_eq!(movies[2].title, "Recently Released");
        assert_eq!(movies[2].count, 2);
        assert_eq!(default_rail_idx(&movies), 1);

        let tv = rail_for(true, 3, 6, 0);
        assert_eq!(tv[0].title, "Continue Watching");
        assert_eq!(tv[1].title, "Start Watching");
        assert_eq!(tv[2].title, "Recently Released");
        assert!(!tv[2].enabled);
        assert_eq!(default_rail_idx(&tv), 0);
    }

    #[test]
    fn tv_continue_dedups_by_show_and_uses_finish_segments_or_pct_or_faint() {
        let items = vec![
            make_ep("1", "Severance", "S2 · E1", Some(1_860_000), Some(3_720_000)),
            make_ep("2", "Severance", "S2 · E2", None, Some(3_600_000)),
            make_ep("3", "The Bear", "S3 · E8", None, Some(1_800_000)),
            make_ep("4", "Andor", "S1 · E4", Some(900_000), Some(3_600_000)),
        ];
        let fin = vec![FinishSeasonItem {
            item: make_ep("30", "The Bear", "", None, None),
            season_title: "Season 3".into(),
            total_episodes: 10,
            watched_episodes: 8,
        }];
        let entries = tv_continue(&items, &fin, 52.0);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].item.id, "1");
        assert_eq!(entries[0].meta, "S2 · E1 · 31 min left");
        assert!(!entries[0].ring_lit.is_empty());

        // The Bear matches finish_season -> 8 lit, 2 unlit segments
        assert_eq!(entries[1].ring_lit.matches("M ").count(), 8);
        assert_eq!(entries[1].ring_unlit.matches("M ").count(), 2);
        assert_eq!(entries[1].meta, "S3 · E8 next · 2 left");

        // 0% progress -> faint full ring (unlit non-empty, lit empty)
        let zero = tv_continue(&[make_ep("5", "Silo", "S2 · E1", None, Some(3_600_000))], &[], 52.0);
        assert!(zero[0].ring_lit.is_empty());
        assert!(!zero[0].ring_unlit.is_empty());
        assert_eq!(zero[0].meta, "S2 · E1 next");
    }

    #[test]
    fn rings_switch_to_single_arc_above_24_episodes() {
        let items = vec![make_ep("1", "Long Show", "S1 · E10", None, None)];
        let fin = vec![FinishSeasonItem {
            item: make_ep("10", "Long Show", "", None, None),
            season_title: "Season 1".into(),
            total_episodes: 26,
            watched_episodes: 13,
        }];
        let entries = tv_continue(&items, &fin, 52.0);
        assert_eq!(entries[0].ring_lit.matches("M ").count(), 1);
        assert_eq!(entries[0].ring_unlit.matches("M ").count(), 1);
    }

    #[test]
    fn movie_meta_formats_time_left() {
        let m = make_movie("9", "Dune", Some(1_800_000), Some(7_560_000));
        assert_eq!(movie_meta(&m), "1 h 36 min left");
    }
}
