//! Text shared by the laptop "On the TV" card and the TV overlay.

use chrono::{Local, NaiveDateTime, TimeDelta, Utc};
use tvdash_core::{config::ClockFormat, events::PlayerStatus, models::MediaItem, models::PlayState};

/// Current wall-clock time in the configured zone (system local if `None`).
pub fn now_local(tz: Option<chrono_tz::Tz>) -> NaiveDateTime {
    match tz {
        Some(tz) => Utc::now().with_timezone(&tz).naive_local(),
        None => Local::now().naive_local(),
    }
}

pub fn hm(t: NaiveDateTime, format: ClockFormat) -> String {
    match format {
        ClockFormat::H24 => t.format("%H:%M").to_string(),
        ClockFormat::H12 => t.format("%-I:%M %p").to_string(),
    }
}

/// `12:34` or `1:02:03`.
pub fn duration(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// "Some Show · S2 · E5 · The Pilot" or "A Film · 1999".
pub fn item_line(item: &MediaItem) -> String {
    let mut parts = vec![item.title.as_str()];
    parts.extend(item.episode_label.as_deref());
    parts.extend(item.subtitle.as_deref());
    parts.join(" · ")
}

/// Episode/subtitle part only, for a second line under the title.
pub fn item_detail(item: &MediaItem) -> String {
    let parts: Vec<&str> = item.episode_label.iter().chain(item.subtitle.iter()).map(String::as_str).collect();
    parts.join(" · ")
}

/// "45 min" / "2 h 5 min".
pub fn length(ms: u64) -> String {
    let mins = (ms + 30_000) / 60_000;
    match (mins / 60, mins % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Detail-panel line: "Episode · 45 min · 30 min left", "Movie · 2 h 5 min".
pub fn item_meta(item: &MediaItem) -> String {
    use tvdash_core::models::MediaKind;
    let mut parts: Vec<String> = Vec::new();
    match item.kind {
        MediaKind::Movie => parts.push("Movie".into()),
        MediaKind::Series => parts.push("Show".into()),
        MediaKind::Episode => parts.push("Episode".into()),
        MediaKind::Other => {}
    }
    if let Some(d) = item.duration_ms.filter(|&d| d > 0) {
        parts.push(length(d));
        if let Some(pos) = item.resume_ms {
            parts.push(format!("{} left", length(d.saturating_sub(pos))));
        }
    }
    parts.join(" · ")
}

/// "12:34 / 45:00 · ends 21:40" (end time only while playing and known).
pub fn progress_line(status: &PlayerStatus, now: NaiveDateTime, format: ClockFormat) -> String {
    let pos = duration(status.position_ms);
    match status.duration_ms {
        Some(dur) if dur > 0 => {
            let left = dur.saturating_sub(status.position_ms);
            let ends = now + TimeDelta::milliseconds(left as i64);
            format!("{pos} / {} · ends {}", duration(dur), hm(ends, format))
        }
        _ => pos,
    }
}

/// Two-line mpv OSD overlay.
pub fn overlay(status: &PlayerStatus, now: NaiveDateTime, format: ClockFormat) -> String {
    let state = if status.state == PlayState::Paused { "Paused · " } else { "" };
    format!("{}\n{state}{}", item_line(&status.item), progress_line(status, now, format))
}

pub fn fraction(status: &PlayerStatus) -> f32 {
    match status.duration_ms {
        Some(d) if d > 0 => (status.position_ms as f32 / d as f32).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tvdash_core::models::{MediaKind, SourceKind};

    fn status(state: PlayState) -> PlayerStatus {
        PlayerStatus {
            item: MediaItem {
                source: SourceKind::Jellyfin,
                id: "1".into(),
                kind: MediaKind::Episode,
                title: "Some Show".into(),
                subtitle: Some("The Pilot".into()),
                episode_label: Some("S2 · E5".into()),
                duration_ms: Some(2_700_000),
                resume_ms: None,
                poster: None,
            },
            state,
            position_ms: 754_000,
            duration_ms: Some(2_700_000),
        }
    }

    #[test]
    fn meta_line() {
        let mut item = status(PlayState::Playing).item;
        assert_eq!(item_meta(&item), "Episode · 45 min");
        item.resume_ms = Some(900_000);
        assert_eq!(item_meta(&item), "Episode · 45 min · 30 min left");
        item.duration_ms = Some(7_500_000);
        item.kind = MediaKind::Movie;
        item.resume_ms = None;
        assert_eq!(item_meta(&item), "Movie · 2 h 5 min");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(754_000), "12:34");
        assert_eq!(duration(3_723_000), "1:02:03");
    }

    #[test]
    fn overlay_text() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap().and_hms_opt(21, 0, 0).unwrap();
        let s = overlay(&status(PlayState::Paused), now, ClockFormat::H24);
        assert_eq!(s, "Some Show · S2 · E5 · The Pilot\nPaused · 12:34 / 45:00 · ends 21:32");
        assert_eq!(item_detail(&status(PlayState::Playing).item), "S2 · E5 · The Pilot");
    }
}

// ── Header widgets ──────────────────────────────────────────────────

use tvdash_core::models::Weather;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipText {
    pub value: String,
    pub detail: String,
}

fn chip(value: impl Into<String>, detail: impl Into<String>) -> ChipText {
    ChipText { value: value.into(), detail: detail.into() }
}

pub fn weather_chip(w: &Weather) -> ChipText {
    let deg = |t: f64| format!("{}°", t.round() as i64);
    let detail = match (w.high, w.low) {
        (Some(h), Some(l)) => format!("H {} · L {}", deg(h), deg(l)),
        _ => String::new(),
    };
    chip(format!("{} {}", deg(w.temperature), w.condition), detail)
}

#[cfg(test)]
mod widget_tests {
    use super::*;

    #[test]
    fn weather_text() {
        let w = Weather {
            temperature: 48.4,
            code: 2,
            condition: "Partly cloudy".into(),
            high: Some(55.1),
            low: Some(38.6),
        };
        assert_eq!(weather_chip(&w), chip("48° Partly cloudy", "H 55° · L 39°"));
    }
}
