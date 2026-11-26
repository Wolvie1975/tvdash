//! Movie page helpers: play-button label/countdown and collection link builder.

use std::collections::HashSet;

use chrono::{DateTime, Local, TimeDelta, TimeZone};
use tvdash_core::{
    config::ClockFormat,
    events::{CollectionLink, MovieDetail},
    models::{CollectionInfo, MediaItem},
};

use crate::format;

#[derive(Debug, Clone, PartialEq)]
pub struct PlayLabel {
    pub label: String,
    pub meta: String,
    pub progress: f32,
    pub show_from_start: bool,
}

/// Format a runtime in milliseconds as `"1 h 52 m"`, `"2 h"`, or `"40 min"`.
pub fn runtime_short(ms: u64) -> String {
    let mins = (ms + 30_000) / 60_000;
    match (mins / 60, mins % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} m"),
    }
}

/// Compute the Play / Resume / Play again button copy and progress bar fraction.
/// `now` is local time; the "ends at" time uses the remaining runtime.
pub fn play_label(d: &MovieDetail, now: DateTime<Local>, h24: bool) -> PlayLabel {
    let clock = if h24 { ClockFormat::H24 } else { ClockFormat::H12 };

    if d.view_offset_ms > 0 {
        let left_ms = d.duration_ms.saturating_sub(d.view_offset_ms);
        let left_label = if left_ms < 60_000 {
            "< 1 min".to_string()
        } else {
            format::length(left_ms)
        };
        let ends = now + TimeDelta::milliseconds(left_ms as i64);
        let ends_str = format::hm(ends.naive_local(), clock);
        let progress = if d.duration_ms > 0 {
            (d.view_offset_ms as f32 / d.duration_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        return PlayLabel {
            label: "Resume".into(),
            meta: format!("{left_label} left · ends at {ends_str}"),
            progress,
            show_from_start: true,
        };
    }

    if d.view_count > 0 {
        let meta = match d.last_viewed_at.and_then(|ts| Local.timestamp_opt(ts, 0).single()) {
            Some(dt) => format!("Watched · {}", dt.format("%b %Y")),
            None => "Watched".into(),
        };
        return PlayLabel {
            label: "Play again".into(),
            meta,
            progress: 1.0,
            show_from_start: false,
        };
    }

    let meta = if d.duration_ms > 0 {
        let ends = now + TimeDelta::milliseconds(d.duration_ms as i64);
        let ends_str = format::hm(ends.naive_local(), clock);
        format!("{} · ends at {ends_str}", runtime_short(d.duration_ms))
    } else {
        String::new()
    };
    PlayLabel {
        label: "Play".into(),
        meta,
        progress: 0.0,
        show_from_start: false,
    }
}

/// Pick the best matching collection from `candidates` for `detail`.
/// Prefers the first matching collection with an unwatched item, else the first match.
pub fn pick_collection<'a>(
    detail: &MovieDetail,
    candidates: &'a [CollectionInfo],
) -> Option<&'a CollectionInfo> {
    let matches: Vec<&CollectionInfo> = candidates
        .iter()
        .filter(|c| detail.collections.iter().any(|t| t == &c.title))
        .collect();
    matches
        .iter()
        .copied()
        .find(|c| c.watched_count < c.child_count)
        .or_else(|| matches.into_iter().next())
}

/// Build the `CollectionLink` from a matched collection and its children.
pub fn build_collection_link(
    movie_id: &str,
    col: &CollectionInfo,
    items: &[MediaItem],
    watched_keys: &HashSet<String>,
) -> Option<CollectionLink> {
    let total = items.len();
    if total == 0 {
        return None;
    }
    let position = items.iter().position(|i| i.id == movie_id).map(|idx| idx + 1)?;
    let next_unwatched = items
        .iter()
        .find(|i| i.id != movie_id && !watched_keys.contains(&i.id))
        .map(|i| i.title.clone());
    Some(CollectionLink {
        key: col.rating_key.clone(),
        title: col.title.clone(),
        position,
        total,
        watched: watched_keys.len(),
        next_unwatched,
        poster: col.thumb.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tvdash_core::models::{MediaItem, MediaKind, SourceKind};

    fn base_detail() -> MovieDetail {
        MovieDetail {
            item: MediaItem {
                source: SourceKind::Plex,
                id: "100".into(),
                kind: MediaKind::Movie,
                title: "Test Movie".into(),
                subtitle: Some("2019".into()),
                episode_label: None,
                duration_ms: Some(112 * 60_000),
                resume_ms: None,
                poster: None,
                summary: None,
                extra: Default::default(),
            },
            summary: String::new(),
            year: Some(2019),
            content_rating: Some("PG-13".into()),
            audience_rating: Some(7.8),
            genres: vec!["Drama".into()],
            duration_ms: 112 * 60_000,
            view_offset_ms: 0,
            view_count: 0,
            last_viewed_at: None,
            resolution: Some("4K".into()),
            hdr: None,
            audio: Some("5.1".into()),
            crew: vec![],
            cast: vec![],
            collections: vec![],
            art: None,
            thumb: None,
            primary_extra_key: None,
        }
    }

    #[test]
    fn play_label_part_watched_12h_and_24h() {
        let mut d = base_detail();
        d.view_offset_ms = 72 * 60_000; // 40 min left
        let now = Local.with_ymd_and_hms(2026, 11, 25, 22, 32, 0).unwrap();

        let p12 = play_label(&d, now, false);
        assert_eq!(p12.label, "Resume");
        assert_eq!(p12.meta, "40 min left · ends at 11:12 PM");
        assert!(p12.show_from_start);
        assert!((p12.progress - (72.0 / 112.0)).abs() < 0.01);

        let p24 = play_label(&d, now, true);
        assert_eq!(p24.meta, "40 min left · ends at 23:12");
    }

    #[test]
    fn play_label_watched() {
        let mut d = base_detail();
        d.view_count = 1;
        let viewed = Local.with_ymd_and_hms(2026, 3, 10, 20, 0, 0).unwrap();
        d.last_viewed_at = Some(viewed.timestamp());
        let now = Local.with_ymd_and_hms(2026, 11, 25, 21, 35, 0).unwrap();

        let p = play_label(&d, now, false);
        assert_eq!(p.label, "Play again");
        assert_eq!(p.meta, "Watched · Mar 2026");
        assert_eq!(p.progress, 1.0);
        assert!(!p.show_from_start);
    }

    #[test]
    fn play_label_unwatched() {
        let d = base_detail();
        let now = Local.with_ymd_and_hms(2026, 11, 25, 21, 35, 0).unwrap();

        let p12 = play_label(&d, now, false);
        assert_eq!(p12.label, "Play");
        assert_eq!(p12.meta, "1 h 52 m · ends at 11:27 PM");
        assert_eq!(p12.progress, 0.0);
        assert!(!p12.show_from_start);

        let p24 = play_label(&d, now, true);
        assert_eq!(p24.meta, "1 h 52 m · ends at 23:27");
    }

    #[test]
    fn play_label_near_end_still_part_watched() {
        let mut d = base_detail();
        d.view_offset_ms = d.duration_ms - 30_000; // 30 seconds left
        let now = Local.with_ymd_and_hms(2026, 11, 25, 21, 35, 0).unwrap();

        let p = play_label(&d, now, true);
        assert_eq!(p.label, "Resume");
        assert!(p.show_from_start);
        assert!(p.meta.starts_with("< 1 min left · ends at "));
    }
}
