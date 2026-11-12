//! Text shared by the laptop "On the TV" card and the TV overlay.

use chrono::{Local, NaiveDateTime, TimeDelta, Utc};
use tvdash_core::{
    config::ClockFormat,
    events::PlayerStatus,
    models::{MediaItem, MediaKind, PlayState},
};

/// Current wall-clock time in the configured zone (system local if `None`).
pub fn now_local(tz: Option<chrono_tz::Tz>) -> NaiveDateTime {
    now_local_at(tz, Utc::now())
}

/// `now` as wall-clock time in the configured zone (system local if `None`).
pub fn now_local_at(tz: Option<chrono_tz::Tz>, now: chrono::DateTime<Utc>) -> NaiveDateTime {
    match tz {
        Some(tz) => now.with_timezone(&tz).naive_local(),
        None => now.with_timezone(&Local).naive_local(),
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

/// On the TV card's two lines under the progress bar:
/// ("24:10 / 52:00", "28 min left · ends 21:40").
pub fn tv_times(status: &PlayerStatus, now: NaiveDateTime, format: ClockFormat) -> (String, String) {
    let pos = duration(status.position_ms);
    match status.duration_ms {
        Some(dur) if dur > 0 => {
            let left = dur.saturating_sub(status.position_ms);
            let ends = now + TimeDelta::milliseconds(left as i64);
            (format!("{pos} / {}", duration(dur)), format!("{} left · ends {}", length(left), hm(ends, format)))
        }
        _ => (pos, String::new()),
    }
}

/// "14 min left" for a partly watched item.
pub fn time_left(item: &MediaItem) -> Option<String> {
    let (pos, dur) = (item.resume_ms?, item.duration_ms.filter(|&d| d > 0)?);
    Some(format!("{} left", length(dur.saturating_sub(pos))))
}

/// "Season 2 · Episode 7 — Crossroads" for an episode; "" for anything else.
pub fn episode_line(item: &MediaItem) -> String {
    let title = item.subtitle.as_deref().filter(|t| !t.is_empty());
    let numbers = match (item.season, item.episode) {
        (Some(s), Some(e)) => Some(format!("Season {s} · Episode {e}")),
        (None, Some(e)) => Some(format!("Episode {e}")),
        _ => None,
    };
    match (item.kind, numbers, title) {
        (MediaKind::Episode, Some(n), Some(t)) => format!("{n} — {t}"),
        (MediaKind::Episode, Some(n), None) => n,
        (MediaKind::Episode, None, Some(t)) => t.to_owned(),
        _ => String::new(),
    }
}

/// "2024 · TV-14 · 52 min · Drama": year, rating, runtime, first genre (whichever are known).
pub fn facts(item: &MediaItem) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(item.year.map(|y| y.to_string()));
    parts.extend(item.content_rating.clone());
    parts.extend(item.duration_ms.filter(|&d| d > 0).map(length));
    parts.extend(item.genres.first().cloned());
    parts.join(" · ")
}

/// "Drama · Road trip" (up to three genres).
pub fn genres(item: &MediaItem) -> String {
    item.genres.iter().take(3).cloned().collect::<Vec<_>>().join(" · ")
}

/// Poster badge: "S1 · E3" for an episode, "22 min left" for a movie in
/// progress, "Watched" for one seen; nothing otherwise.
pub fn badge(item: &MediaItem) -> String {
    match item.kind {
        MediaKind::Episode => item.episode_label.clone().unwrap_or_else(|| "Episode".into()),
        MediaKind::Series if item.leaf_count.is_some() && item.viewed_leaf_count == item.leaf_count => "Watched".into(),
        MediaKind::Series => String::new(),
        _ => time_left(item).unwrap_or_else(|| if item.view_count > 0 { "Watched".into() } else { String::new() }),
    }
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
    use tvdash_core::models::SourceKind;

    fn status(state: PlayState) -> PlayerStatus {
        PlayerStatus {
            item: MediaItem {
                source: SourceKind::Plex,
                id: "1".into(),
                kind: MediaKind::Episode,
                title: "Some Show".into(),
                subtitle: Some("The Pilot".into()),
                episode_label: Some("S2 · E5".into()),
                duration_ms: Some(2_700_000),
                resume_ms: None,
                poster: None,
                ..Default::default()
            },
            state,
            position_ms: 754_000,
            duration_ms: Some(2_700_000),
        }
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
        let (at, left) = tv_times(&status(PlayState::Playing), now, ClockFormat::H24);
        assert_eq!((at.as_str(), left.as_str()), ("12:34 / 45:00", "32 min left · ends 21:32"));
        let mut item = status(PlayState::Playing).item;
        assert_eq!(time_left(&item), None);
        item.resume_ms = Some(900_000);
        assert_eq!(time_left(&item).as_deref(), Some("30 min left"));
    }
}

// ── Header widgets ──────────────────────────────────────────────────

use tvdash_core::{config::Units, models::Weather};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipText {
    pub value: String,
    pub detail: String,
}

fn chip(value: impl Into<String>, detail: impl Into<String>) -> ChipText {
    ChipText { value: value.into(), detail: detail.into() }
}

/// ("51°F", "Partly cloudy · H 58 L 40").
pub fn weather_chip(w: &Weather, units: Units) -> ChipText {
    let unit = match units {
        Units::Fahrenheit => "F",
        Units::Celsius => "C",
    };
    let round = |t: f64| t.round() as i64;
    let detail = match (w.high, w.low) {
        (Some(h), Some(l)) => format!("{} · H {} L {}", w.condition, round(h), round(l)),
        _ => w.condition.clone(),
    };
    chip(format!("{}°{unit}", round(w.temperature)), detail)
}

/// Icon name (icons.slint) for a WMO weather code.
pub fn weather_icon(code: u8) -> &'static str {
    match code {
        0 | 1 => "clear",
        2 => "partly",
        3 => "cloud",
        45 | 48 => "fog",
        51..=67 | 80..=82 => "rain",
        71..=77 | 85 | 86 => "snow",
        95..=99 => "storm",
        _ => "cloud",
    }
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
        assert_eq!(weather_chip(&w, Units::Fahrenheit), chip("48°F", "Partly cloudy · H 55 L 39"));
        assert_eq!(weather_chip(&Weather { high: None, ..w }, Units::Celsius), chip("48°C", "Partly cloudy"));
        assert_eq!((weather_icon(0), weather_icon(61), weather_icon(95)), ("clear", "rain", "storm"));
    }
}

#[cfg(test)]
mod box_tests {
    use super::*;

    #[test]
    fn box_lines() {
        let mut ep = MediaItem {
            kind: MediaKind::Episode,
            title: "Some Show".into(),
            subtitle: Some("The Pilot".into()),
            episode_label: Some("S2 · E5".into()),
            duration_ms: Some(2_700_000),
            ..Default::default()
        };
        ep.season = Some(2);
        ep.episode = Some(5);
        ep.year = Some(2024);
        ep.content_rating = Some("TV-14".into());
        ep.genres = vec!["Drama".into(), "Road trip".into()];
        assert_eq!(episode_line(&ep), "Season 2 · Episode 5 — The Pilot");
        assert_eq!(facts(&ep), "2024 · TV-14 · 45 min · Drama");
        assert_eq!(genres(&ep), "Drama · Road trip");
        assert_eq!(badge(&ep), "S2 · E5");
        let movie = MediaItem {
            kind: MediaKind::Movie,
            title: "A Film".into(),
            duration_ms: Some(7_200_000),
            resume_ms: Some(5_880_000),
            ..Default::default()
        };
        assert_eq!(episode_line(&movie), "");
        assert_eq!(facts(&movie), "2 h");
        assert_eq!(badge(&movie), "22 min left");
        let seen = MediaItem { resume_ms: None, view_count: 1, ..movie.clone() };
        assert_eq!(badge(&seen), "Watched");
        assert_eq!(badge(&MediaItem { view_count: 0, ..seen }), "", "nothing to say");
    }
}
