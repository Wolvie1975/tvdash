//! Home "What's On" hero selection, subtitle, and status formatting (§3.1–3.4).

use chrono::{NaiveDateTime, TimeDelta};
use tvdash_core::{
    config::ClockFormat,
    events::PlayerStatus,
    models::{MediaItem, MediaKind, PlayState},
};

use crate::format;

/// Which source drives the Home hero card (§3.2).
#[derive(Debug, Clone, PartialEq)]
pub enum HeroPick<'a> {
    Player(&'a PlayerStatus),
    Browser(&'a str),
    Unfinished(&'a MediaItem),
    UpNext(&'a MediaItem),
    Empty,
}

/// Pick the hero item in priority order (§3.2):
/// 1. Loaded in mpv (playing or paused)
/// 2. Streaming app open in Brave
/// 3. Last played item if unfinished (`resume_ms > 0`)
/// 4. First Up Next candidate
/// 5. Empty
pub fn pick_hero<'a>(
    player: Option<&'a PlayerStatus>,
    browser: Option<&'a str>,
    last_played: Option<&'a MediaItem>,
    first_up_next: Option<&'a MediaItem>,
) -> HeroPick<'a> {
    if let Some(st) = player {
        return HeroPick::Player(st);
    }
    if let Some(site) = browser {
        return HeroPick::Browser(site);
    }
    if let Some(item) = last_played.filter(|it| it.resume_ms.is_some_and(|r| r > 0)) {
        return HeroPick::Unfinished(item);
    }
    if let Some(item) = first_up_next {
        return HeroPick::UpNext(item);
    }
    HeroPick::Empty
}

/// Compact runtime string: `"1 h 43 m"` or `"43 m"`.
pub fn short_runtime(ms: u64) -> String {
    let mins = (ms + 30_000) / 60_000;
    match (mins / 60, mins % 60) {
        (0, m) => format!("{m} m"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} m"),
    }
}

/// Hero subtitle (§3.1):
/// - Movie: `"2016 · Action, Fantasy · 1 h 43 m"`
/// - Episode: `"S2 · E11 · Episode title"`
pub fn hero_subtitle(item: &MediaItem) -> String {
    match item.kind {
        MediaKind::Episode => format::item_detail(item),
        _ => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(y) = item.extra.year {
                parts.push(y.to_string());
            }
            if !item.extra.genres.is_empty() {
                parts.push(item.extra.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
            }
            if let Some(d) = item.duration_ms.filter(|&d| d > 0) {
                parts.push(short_runtime(d));
            }
            parts.join(" · ")
        }
    }
}

/// Hero status line (§3.3).
pub fn hero_status(pick: &HeroPick<'_>, now: NaiveDateTime, clock: ClockFormat) -> String {
    match pick {
        HeroPick::Player(st) => {
            if st.state == PlayState::Paused {
                let pos = format::duration(st.position_ms);
                match st.duration_ms.filter(|&d| d > 0) {
                    Some(dur) => {
                        let left = dur.saturating_sub(st.position_ms);
                        format!("Paused at {pos} · {} left", format::length(left))
                    }
                    None => format!("Paused at {pos}"),
                }
            } else {
                match st.duration_ms.filter(|&d| d > 0) {
                    Some(dur) => {
                        let left = dur.saturating_sub(st.position_ms).min(i64::MAX as u64) as i64;
                        let ends = now + TimeDelta::milliseconds(left);
                        format!("Ends at {}", format::hm(ends, clock))
                    }
                    None => format!("Playing · {}", format::duration(st.position_ms)),
                }
            }
        }
        HeroPick::Browser(_) => "Open on the TV".into(),
        HeroPick::Unfinished(item) => {
            let pos = item.resume_ms.unwrap_or(0);
            let at = format!("Stopped at {}", format::duration(pos));
            match format::time_left(item) {
                Some(left) => format!("{at} · {left}"),
                None => at,
            }
        }
        HeroPick::UpNext(item) => {
            if let Some(pos) = item.resume_ms.filter(|&p| p > 0) {
                let at = format!("Stopped at {}", format::duration(pos));
                match format::time_left(item) {
                    Some(left) => format!("{at} · {left}"),
                    None => at,
                }
            } else {
                match item.duration_ms.filter(|&d| d > 0) {
                    Some(dur) => format!("Not started · {}", short_runtime(dur)),
                    None => "Not started".into(),
                }
            }
        }
        HeroPick::Empty => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use tvdash_core::models::{MediaExtra, SourceKind};

    use super::*;

    fn movie(id: &str, resume_ms: Option<u64>, duration_ms: Option<u64>) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.into(),
            kind: MediaKind::Movie,
            title: "Arrival".into(),
            subtitle: None,
            episode_label: None,
            duration_ms,
            resume_ms,
            poster: None,
            summary: None,
            extra: MediaExtra {
                year: Some(2016),
                genres: vec!["Sci-Fi".into(), "Drama".into(), "Mystery".into()],
                ..Default::default()
            },
        }
    }

    #[test]
    fn pick_hero_respects_priority() {
        let up = movie("up", None, Some(6_000_000));
        let unf = movie("unf", Some(181_000), Some(6_180_000));
        let fin = movie("fin", Some(0), Some(6_180_000));
        let st = PlayerStatus {
            item: movie("mpv", Some(100_000), Some(6_000_000)),
            state: PlayState::Playing,
            position_ms: 100_000,
            duration_ms: Some(6_000_000),
        };

        assert!(matches!(
            pick_hero(Some(&st), Some("Netflix"), Some(&unf), Some(&up)),
            HeroPick::Player(_)
        ));
        assert!(matches!(
            pick_hero(None, Some("Netflix"), Some(&unf), Some(&up)),
            HeroPick::Browser("Netflix")
        ));
        assert!(matches!(
            pick_hero(None, None, Some(&unf), Some(&up)),
            HeroPick::Unfinished(_)
        ));
        // Finished last_played (resume_ms == 0) falls through to Up Next
        assert!(matches!(
            pick_hero(None, None, Some(&fin), Some(&up)),
            HeroPick::UpNext(_)
        ));
        assert_eq!(pick_hero(None, None, None, None), HeroPick::Empty);
    }

    #[test]
    fn status_lines_match_spec() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 21)
            .unwrap()
            .and_hms_opt(23, 0, 0)
            .unwrap();
        let m = movie("1", Some(181_000), Some(6_181_000)); // 3:01, 1h 40m left
        assert_eq!(
            hero_status(&HeroPick::Unfinished(&m), now, ClockFormat::H12),
            "Stopped at 3:01 · 1 h 40 min left"
        );

        let playing = PlayerStatus {
            item: m.clone(),
            state: PlayState::Playing,
            position_ms: 181_000,
            duration_ms: Some(6_181_000),
        };
        assert_eq!(
            hero_status(&HeroPick::Player(&playing), now, ClockFormat::H12),
            "Ends at 12:40 AM"
        );

        let paused = PlayerStatus { state: PlayState::Paused, ..playing };
        assert_eq!(
            hero_status(&HeroPick::Player(&paused), now, ClockFormat::H12),
            "Paused at 3:01 · 1 h 40 min left"
        );

        let unstarted = movie("2", None, Some(6_180_000)); // 1h 43m
        assert_eq!(
            hero_status(&HeroPick::UpNext(&unstarted), now, ClockFormat::H12),
            "Not started · 1 h 43 m"
        );
        assert_eq!(
            hero_status(&HeroPick::Browser("Netflix"), now, ClockFormat::H12),
            "Open on the TV"
        );
    }
}
