//! Home "Up next" row candidate list and card metadata (§4.1–4.2).

use std::collections::HashSet;

use tvdash_core::models::{MediaItem, MediaKind};

use crate::{format, whats_on::short_runtime};

/// Build the ordered Up next list (up to 4 items), dropping `hero_id` and keeping
/// at most one card per show (§4.2).
pub fn build_up_next(
    hero_id: Option<&str>,
    hero_next_ep: Option<&MediaItem>,
    continue_watching: &[MediaItem],
    recently_added: &[MediaItem],
) -> Vec<MediaItem> {
    let mut out = Vec::with_capacity(4);
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut seen_shows: HashSet<String> = HashSet::new();

    if let Some(hid) = hero_id {
        seen_ids.insert(hid.to_owned());
    }

    let candidates = hero_next_ep
        .into_iter()
        .chain(continue_watching.iter())
        .chain(recently_added.iter());

    for item in candidates {
        if out.len() == 4 {
            break;
        }
        if !seen_ids.insert(item.id.clone()) {
            continue;
        }
        if item.kind == MediaKind::Episode {
            let show_key = item.title.to_lowercase();
            if !seen_shows.insert(show_key) {
                continue;
            }
        }
        out.push(item.clone());
    }

    out
}

/// Format the metadata line under an Up next card (§4.1):
/// - Episode: `"S2 · E4 · 21 min left"` or `"S2 · E5"` when unstarted
/// - Movie: `"1 h 46 m left"` or `"2019 · 1 h 58 m"`
pub fn up_next_meta(item: &MediaItem) -> String {
    match item.kind {
        MediaKind::Episode => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(ep) = &item.episode_label {
                parts.push(ep.clone());
            }
            if item.resume_ms.is_some_and(|p| p > 0)
                && let Some(left) = format::time_left(item)
            {
                parts.push(left);
            }
            parts.join(" · ")
        }
        _ => {
            if item.resume_ms.is_some_and(|p| p > 0)
                && let (Some(pos), Some(dur)) = (item.resume_ms, item.duration_ms.filter(|&d| d > 0))
            {
                return format!("{} left", short_runtime(dur.saturating_sub(pos)));
            }
            let mut parts: Vec<String> = Vec::new();
            if let Some(y) = item.extra.year {
                parts.push(y.to_string());
            }
            if let Some(d) = item.duration_ms.filter(|&d| d > 0) {
                parts.push(short_runtime(d));
            }
            parts.join(" · ")
        }
    }
}

#[cfg(test)]
mod tests {
    use tvdash_core::models::{MediaExtra, SourceKind};

    use super::*;

    fn ep(id: &str, show: &str, label: &str) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.into(),
            kind: MediaKind::Episode,
            title: show.into(),
            subtitle: Some("Ep".into()),
            episode_label: Some(label.into()),
            duration_ms: Some(2_700_000),
            resume_ms: None,
            poster: None,
            summary: None,
            extra: MediaExtra::default(),
        }
    }

    fn mov(id: &str, title: &str) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.into(),
            kind: MediaKind::Movie,
            title: title.into(),
            subtitle: None,
            episode_label: None,
            duration_ms: Some(7_080_000),
            resume_ms: None,
            poster: None,
            summary: None,
            extra: MediaExtra { year: Some(2019), ..Default::default() },
        }
    }

    #[test]
    fn build_up_next_dedupes_drops_hero_and_caps_at_four() {
        let hero_next = ep("102", "Severance", "S2 · E2");
        let cw = vec![
            ep("101", "Severance", "S2 · E1"), // hero
            ep("103", "Severance", "S2 · E3"), // same show as hero_next → skipped
            ep("201", "Andor", "S1 · E4"),
            mov("301", "Dune"),
        ];
        let recent = vec![
            mov("301", "Dune"), // duplicate
            mov("302", "Arrival"),
            mov("303", "Extra"),
        ];

        let res = build_up_next(Some("101"), Some(&hero_next), &cw, &recent);
        let ids: Vec<&str> = res.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["102", "201", "301", "302"]);
    }
}
