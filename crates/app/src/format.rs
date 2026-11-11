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

use tvdash_core::models::{CalendarEvent, HomelabStatus, Probe, Weather};

/// How a chip's dot should look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    None,
    Ok,
    Warn,
    Alert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipText {
    pub value: String,
    pub detail: String,
    pub severity: Severity,
}

fn chip(value: impl Into<String>, detail: impl Into<String>, severity: Severity) -> ChipText {
    ChipText { value: value.into(), detail: detail.into(), severity }
}

pub fn weather_chip(w: &Weather) -> ChipText {
    let deg = |t: f64| format!("{}°", t.round() as i64);
    let detail = match (w.high, w.low) {
        (Some(h), Some(l)) => format!("H {} · L {}", deg(h), deg(l)),
        _ => String::new(),
    };
    chip(format!("{} {}", deg(w.temperature), w.condition), detail, Severity::None)
}

/// Chip text plus the TV's "Up next" line for a calendar event.
pub fn event_chip(
    e: &CalendarEvent,
    now: chrono::DateTime<Utc>,
    tz: Option<chrono_tz::Tz>,
    clock: ClockFormat,
) -> (ChipText, String) {
    let local = |t: chrono::DateTime<Utc>| match tz {
        Some(tz) => t.with_timezone(&tz).naive_local(),
        None => t.with_timezone(&Local).naive_local(),
    };
    let (start, end, today) = (local(e.start), local(e.end), local(now).date());
    let day = if start.date() <= today { "Today" } else { "Tomorrow" };
    if e.all_day {
        return (chip(e.title.clone(), format!("{day} · all day"), Severity::None), format!("{day} · {}", e.title));
    }
    let at = hm(start, clock);
    if e.start <= now {
        let until = format!("Now, until {}", hm(end, clock));
        return (chip(format!("{at} {}", e.title), until.clone(), Severity::None), format!("{} · {until}", e.title));
    }
    let tv = if day == "Today" { format!("{at} {}", e.title) } else { format!("Tomorrow {at} {}", e.title) };
    (chip(format!("{at} {}", e.title), day, Severity::None), tv)
}

pub fn homelab_chip(h: &HomelabStatus) -> ChipText {
    let containers = match &h.containers {
        Probe::Ok(c) => format!("{} / {} containers", c.running, c.total),
        Probe::Failed(_) => "Portainer unavailable".into(),
        Probe::NotConfigured => String::new(),
    };
    match (&h.monitors, &h.containers) {
        (Probe::Ok(m), _) if !m.down.is_empty() => {
            chip(format!("{} / {} up", m.up, m.total), format!("Down: {}", m.down.join(", ")), Severity::Alert)
        }
        (Probe::Ok(m), _) => chip(format!("{} / {} up", m.up, m.total), containers, Severity::Ok),
        (Probe::Failed(_), _) => chip("Unavailable", containers, Severity::Warn),
        (Probe::NotConfigured, Probe::Ok(c)) => chip(
            format!("{} / {} running", c.running, c.total),
            "containers",
            if c.running == c.total { Severity::Ok } else { Severity::Warn },
        ),
        (Probe::NotConfigured, Probe::Failed(_)) => chip("Unavailable", "Portainer", Severity::Warn),
        (Probe::NotConfigured, Probe::NotConfigured) => chip("Not set up", "Uptime Kuma, Portainer", Severity::None),
    }
}

#[cfg(test)]
mod widget_tests {
    use super::*;
    use tvdash_core::models::{ContainerCounts, MonitorCounts};

    fn utc(s: &str) -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn ev(title: &str, start: &str, end: &str, all_day: bool) -> CalendarEvent {
        CalendarEvent { title: title.into(), start: utc(start), end: utc(end), all_day }
    }

    const CHI: Option<chrono_tz::Tz> = Some(chrono_tz::America::Chicago);

    #[test]
    fn event_texts() {
        let now = utc("2026-11-11T14:00:00Z"); // 08:00 Chicago
        let (c, tv) = event_chip(
            &ev("Standup", "2026-11-11T15:30:00Z", "2026-11-11T15:45:00Z", false),
            now,
            CHI,
            ClockFormat::H24,
        );
        assert_eq!((c.value.as_str(), c.detail.as_str(), tv.as_str()), ("09:30 Standup", "Today", "09:30 Standup"));

        let (c, tv) = event_chip(
            &ev("Standup", "2026-11-11T13:30:00Z", "2026-11-11T14:15:00Z", false),
            now,
            CHI,
            ClockFormat::H24,
        );
        assert_eq!((c.detail.as_str(), tv.as_str()), ("Now, until 08:15", "Standup · Now, until 08:15"));

        let (c, tv) = event_chip(
            &ev("Dentist", "2026-11-12T20:00:00Z", "2026-11-12T21:00:00Z", false),
            now,
            CHI,
            ClockFormat::H12,
        );
        assert_eq!(
            (c.value.as_str(), c.detail.as_str(), tv.as_str()),
            ("2:00 PM Dentist", "Tomorrow", "Tomorrow 2:00 PM Dentist")
        );

        let (c, _) = event_chip(
            &ev("Holiday", "2026-11-11T06:00:00Z", "2026-11-12T06:00:00Z", true),
            now,
            CHI,
            ClockFormat::H24,
        );
        assert_eq!((c.value.as_str(), c.detail.as_str()), ("Holiday", "Today · all day"));
    }

    #[test]
    fn weather_text() {
        let w = Weather {
            temperature: 48.4,
            code: 2,
            condition: "Partly cloudy".into(),
            high: Some(55.1),
            low: Some(38.6),
        };
        assert_eq!(weather_chip(&w), chip("48° Partly cloudy", "H 55° · L 39°", Severity::None));
    }

    #[test]
    fn homelab_texts() {
        let m = |up, total, down: &[&str]| {
            Probe::Ok(MonitorCounts { up, total, down: down.iter().map(|s| s.to_string()).collect() })
        };
        let c = Probe::Ok(ContainerCounts { running: 23, total: 24 });
        let h = |monitors, containers| HomelabStatus { monitors, containers };
        assert_eq!(homelab_chip(&h(m(6, 6, &[]), c.clone())), chip("6 / 6 up", "23 / 24 containers", Severity::Ok));
        assert_eq!(homelab_chip(&h(m(5, 6, &["NAS"]), c.clone())), chip("5 / 6 up", "Down: NAS", Severity::Alert));
        assert_eq!(homelab_chip(&h(Probe::Failed("x".into()), c.clone())).severity, Severity::Warn);
        assert_eq!(homelab_chip(&h(Probe::NotConfigured, c)).value, "23 / 24 running");
        assert_eq!(homelab_chip(&h(Probe::NotConfigured, Probe::NotConfigured)).value, "Not set up");
    }
}
