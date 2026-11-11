//! Google Calendar via its secret iCal address (keyring `calendar_ics_url`).
//!
//! Fetched every 5 minutes. Events are expanded for today and tomorrow in the
//! configured timezone, including recurrences (`RRULE`, `EXDATE`), moved or
//! cancelled instances (`RECURRENCE-ID`), all-day dates and floating times.

use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Utc};
use chrono_tz::Tz;
use tracing::{debug, warn};
use tvdash_core::models::CalendarEvent;

use crate::{Result, WidgetError};

pub const REFRESH: Duration = Duration::from_secs(5 * 60);

/// Download the feed. 401/403/404 mean the secret address was reset.
pub async fn fetch(http: &reqwest::Client, url: &str) -> Result<String> {
    let resp = http.get(url).send().await?;
    match resp.status().as_u16() {
        200..=299 => Ok(resp.text().await?),
        401 | 403 | 404 => Err(WidgetError::Revoked),
        s => Err(WidgetError::Status(s)),
    }
}

/// Start of today through the end of tomorrow, in `tz`.
pub fn window(now: DateTime<Utc>, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = now.with_timezone(&tz).date_naive();
    (midnight(today, tz), midnight(today + TimeDelta::days(2), tz))
}

/// The event to show in the header: the next timed event that hasn't ended
/// (an in-progress one counts), else the next all-day event.
pub fn next(events: &[CalendarEvent], now: DateTime<Utc>) -> Option<&CalendarEvent> {
    let pick = |all_day: bool| events.iter().filter(|e| e.all_day == all_day && e.end > now).min_by_key(|e| e.start);
    pick(false).or_else(|| pick(true))
}

/// Every occurrence overlapping `[from, to)`, sorted by start. `tz` is used for
/// floating times, all-day dates and unknown `TZID`s.
pub fn occurrences(ics: &str, tz: Tz, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<CalendarEvent> {
    let events = parse_events(ics);

    // Moved/cancelled instances, keyed by (UID, original start).
    let mut overrides: HashMap<(String, DateTime<Utc>), &RawEvent> = HashMap::new();
    for e in &events {
        if let Some(rid) = e.recurrence_id.as_ref().and_then(|p| parse_when(&p.value, &p.params, tz)) {
            overrides.insert((e.uid.clone(), rid.instant(tz)), e);
        }
    }

    let mut out = Vec::new();
    for e in &events {
        let Some(start) = e.dtstart.as_ref().and_then(|p| parse_when(&p.value, &p.params, tz)) else {
            continue;
        };
        let length = e.length(&start, tz);
        let all_day = matches!(start, When::Date(_));
        let title = e.summary.clone().unwrap_or_else(|| "(no title)".into());
        let mut push = |s: DateTime<Utc>| {
            let end = s + length;
            if end > from && s < to {
                out.push(CalendarEvent { title: title.clone(), start: s, end, all_day });
            }
        };

        if e.recurrence_id.is_some() || e.rrule.is_none() {
            // Single event, or a moved instance (it carries its own DTSTART).
            if !e.cancelled() {
                push(start.instant(tz));
            }
            continue;
        }

        let rule = e.rrule.as_deref().unwrap_or_default();
        match expand(rule, &start, &e.exdates, tz, from - length, to) {
            Ok(starts) => {
                for s in starts {
                    match overrides.get(&(e.uid.clone(), s)) {
                        Some(_) => {} // the override (or its cancellation) replaces this one
                        None => push(s),
                    }
                }
            }
            Err(err) => {
                warn!(title, rule, error = %err, "couldn't expand recurrence; showing first occurrence only");
                push(start.instant(tz));
            }
        }
    }
    out.sort_by_key(|e| e.start);
    out.dedup();
    out
}

fn expand(
    rule: &str,
    start: &When,
    exdates: &[Prop],
    tz: Tz,
    after: DateTime<Utc>,
    before: DateTime<Utc>,
) -> std::result::Result<Vec<DateTime<Utc>>, String> {
    let (dt_start, zone) = match start {
        When::Utc(t) => (*t, chrono_tz::UTC),
        When::Local(_, z) => (start.instant(tz), *z),
        When::Date(_) => (start.instant(tz), tz),
    };
    let rz = rrule::Tz::Tz(zone);
    let parsed: rrule::RRule<rrule::Unvalidated> = rule.parse().map_err(|e: rrule::RRuleError| e.to_string())?;
    let mut set = parsed.build(dt_start.with_timezone(&rz)).map_err(|e| e.to_string())?;
    for p in exdates {
        for v in p.value.split(',') {
            if let Some(w) = parse_when(v, &p.params, tz) {
                set = set.exdate(w.instant(tz).with_timezone(&rz));
            }
        }
    }
    let result = set.after(after.with_timezone(&rz)).before(before.with_timezone(&rz)).all(500);
    Ok(result.dates.into_iter().map(|d| d.with_timezone(&Utc)).collect())
}

// ── Time values ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum When {
    /// All-day (floating date).
    Date(NaiveDate),
    /// Wall-clock time in a zone (`TZID=` or floating → default zone).
    Local(NaiveDateTime, Tz),
    Utc(DateTime<Utc>),
}

impl When {
    fn instant(&self, default: Tz) -> DateTime<Utc> {
        match self {
            When::Utc(t) => *t,
            When::Local(n, z) => local_to_utc(*n, *z),
            When::Date(d) => midnight(*d, default),
        }
    }
}

fn midnight(d: NaiveDate, tz: Tz) -> DateTime<Utc> {
    local_to_utc(d.and_hms_opt(0, 0, 0).expect("midnight"), tz)
}

/// DST-safe: ambiguous → earlier; skipped (spring forward) → an hour later.
fn local_to_utc(n: NaiveDateTime, tz: Tz) -> DateTime<Utc> {
    tz.from_local_datetime(&n)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(n + TimeDelta::hours(1))).earliest())
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&n))
}

fn parse_when(value: &str, params: &[(String, String)], default: Tz) -> Option<When> {
    let value = value.trim();
    let param = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    if param("VALUE") == Some("DATE") || value.len() == 8 {
        return NaiveDate::parse_from_str(value, "%Y%m%d").ok().map(When::Date);
    }
    if let Some(v) = value.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
        return Some(When::Utc(Utc.from_utc_datetime(&n)));
    }
    let n = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let zone = match param("TZID") {
        Some(id) => id.parse().unwrap_or_else(|_| {
            debug!(tzid = id, "unknown TZID; using the configured timezone");
            default
        }),
        None => default,
    };
    Some(When::Local(n, zone))
}

/// `P1D`, `PT1H30M`, `P1DT2H`, `P2W`.
fn parse_duration(s: &str) -> Option<TimeDelta> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut rest = s.strip_prefix('P')?;
    let mut total = TimeDelta::zero();
    let mut in_time = false;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('T') {
            in_time = true;
            rest = r;
            continue;
        }
        let digits = rest.find(|c: char| !c.is_ascii_digit())?;
        let n: i64 = rest[..digits].parse().ok()?;
        let unit = rest[digits..].chars().next()?;
        total += match (unit, in_time) {
            ('W', false) => TimeDelta::weeks(n),
            ('D', false) => TimeDelta::days(n),
            ('H', true) => TimeDelta::hours(n),
            ('M', true) => TimeDelta::minutes(n),
            ('S', true) => TimeDelta::seconds(n),
            _ => return None,
        };
        rest = &rest[digits + 1..];
    }
    Some(if neg { -total } else { total })
}

// ── iCalendar text ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

#[derive(Debug, Default)]
struct RawEvent {
    uid: String,
    summary: Option<String>,
    status: Option<String>,
    dtstart: Option<Prop>,
    dtend: Option<Prop>,
    duration: Option<String>,
    rrule: Option<String>,
    exdates: Vec<Prop>,
    recurrence_id: Option<Prop>,
}

impl RawEvent {
    fn cancelled(&self) -> bool {
        self.status.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED"))
    }

    fn length(&self, start: &When, tz: Tz) -> TimeDelta {
        if let Some(end) = self.dtend.as_ref().and_then(|p| parse_when(&p.value, &p.params, tz)) {
            let d = end.instant(tz) - start.instant(tz);
            if d >= TimeDelta::zero() {
                return d;
            }
        }
        if let Some(d) = self.duration.as_deref().and_then(parse_duration) {
            return d;
        }
        match start {
            When::Date(_) => TimeDelta::days(1),
            _ => TimeDelta::zero(),
        }
    }
}

/// Undo RFC 5545 line folding (CRLF followed by a space or tab).
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(cont) = line.strip_prefix([' ', '\t'])
            && let Some(last) = lines.last_mut()
        {
            last.push_str(cont);
        } else if !line.is_empty() {
            lines.push(line.to_owned());
        }
    }
    lines
}

/// Byte offsets of `sep` outside double quotes.
fn unquoted(s: &str, sep: char) -> impl Iterator<Item = usize> + '_ {
    let mut in_quotes = false;
    s.char_indices().filter_map(move |(i, c)| {
        if c == '"' {
            in_quotes = !in_quotes;
        }
        (c == sep && !in_quotes).then_some(i)
    })
}

/// `NAME;P1=V1;P2="a:b":value` → Prop. Colons and semicolons inside quotes are data.
fn parse_line(line: &str) -> Option<Prop> {
    let colon = unquoted(line, ':').next()?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut cuts: Vec<usize> = unquoted(head, ';').collect();
    cuts.push(head.len());
    let name = head[..cuts[0]].to_ascii_uppercase();
    let params = cuts
        .windows(2)
        .filter_map(|w| {
            let (k, v) = head[w[0] + 1..w[1]].split_once('=')?;
            Some((k.to_ascii_uppercase(), v.trim_matches('"').to_owned()))
        })
        .collect();
    Some(Prop { name, params, value: value.to_owned() })
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_events(ics: &str) -> Vec<RawEvent> {
    let mut events = Vec::new();
    let mut current: Option<RawEvent> = None;
    let mut nested = 0; // VALARM etc. inside a VEVENT
    for line in unfold(ics) {
        let Some(p) = parse_line(&line) else { continue };
        match (p.name.as_str(), p.value.trim().to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") if current.is_none() => current = Some(RawEvent::default()),
            ("END", "VEVENT") if nested == 0 => events.extend(current.take()),
            ("BEGIN", _) if current.is_some() => nested += 1,
            ("END", _) if current.is_some() && nested > 0 => nested -= 1,
            _ => {
                let Some(e) = current.as_mut().filter(|_| nested == 0) else { continue };
                match p.name.as_str() {
                    "UID" => e.uid = p.value.clone(),
                    "SUMMARY" => e.summary = Some(unescape(&p.value)),
                    "STATUS" => e.status = Some(p.value.clone()),
                    "DTSTART" => e.dtstart = Some(p),
                    "DTEND" => e.dtend = Some(p),
                    "DURATION" => e.duration = Some(p.value),
                    "RRULE" => e.rrule = Some(p.value),
                    "EXDATE" => e.exdates.push(p),
                    "RECURRENCE-ID" => e.recurrence_id = Some(p),
                    _ => {}
                }
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHI: Tz = chrono_tz::America::Chicago;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    /// Shaped like Google's export: VTIMEZONE, folded lines, VALARM, escapes.
    const GOOGLE: &str = "BEGIN:VCALENDAR\r
PRODID:-//Google Inc//Google Calendar 70.9054//EN\r
VERSION:2.0\r
X-WR-TIMEZONE:America/Chicago\r
BEGIN:VTIMEZONE\r
TZID:America/Chicago\r
BEGIN:STANDARD\r
DTSTART:19701101T020000\r
END:STANDARD\r
END:VTIMEZONE\r
BEGIN:VEVENT\r
DTSTART;TZID=America/Chicago:20261002T093000\r
DTEND;TZID=America/Chicago:20261002T094500\r
RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;UNTIL=20271231T055959Z\r
EXDATE;TZID=America/Chicago:20261112T093000\r
UID:standup@google.com\r
SUMMARY:Stand\\, up\r
BEGIN:VALARM\r
ACTION:DISPLAY\r
SUMMARY:alarm text must not leak\r
TRIGGER:-P0DT0H10M0S\r
END:VALARM\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;TZID=America/Chicago:20261111T140000\r
DTEND;TZID=America/Chicago:20261111T150000\r
RECURRENCE-ID;TZID=America/Chicago:20261111T093000\r
UID:standup@google.com\r
SUMMARY:Stand\\, up (moved)\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;VALUE=DATE:20261111\r
DTEND;VALUE=DATE:20261112\r
UID:holiday@google.com\r
SUMMARY:Veterans Day\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART:20261112T200000Z\r
DTEND:20261112T210000Z\r
UID:dentist@google.com\r
SUMMARY:Dentist with a very long descr\r
 iption folded\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART:20261111T230000Z\r
DURATION:PT30M\r
STATUS:CANCELLED\r
UID:cancelled@google.com\r
SUMMARY:Cancelled thing\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART:20261201T100000Z\r
UID:later@google.com\r
SUMMARY:Out of window\r
END:VEVENT\r
END:VCALENDAR\r
";

    #[test]
    fn expands_google_feed_for_today_and_tomorrow() {
        // Wed 11 Nov 2026, 08:00 in Chicago (UTC−6).
        let now = utc("2026-11-11T14:00:00Z");
        let (from, to) = window(now, CHI);
        assert_eq!(from, utc("2026-11-11T06:00:00Z"));
        assert_eq!(to, utc("2026-11-13T06:00:00Z"));

        let evs = occurrences(GOOGLE, CHI, from, to);
        let titles: Vec<(&str, String)> = evs
            .iter()
            .map(|e| (e.title.as_str(), e.start.with_timezone(&CHI).format("%a %H:%M").to_string()))
            .collect();
        assert_eq!(
            titles,
            [
                ("Veterans Day", "Wed 00:00".into()),
                // Wed 09:30 standup moved to 14:00; Thu 09:30 excluded by EXDATE.
                ("Stand, up (moved)", "Wed 14:00".into()),
                ("Dentist with a very long description folded", "Thu 14:00".into()),
            ]
        );
        assert!(evs[0].all_day && evs[0].end - evs[0].start == TimeDelta::days(1));
        assert_eq!(evs[1].end - evs[1].start, TimeDelta::hours(1));
    }

    #[test]
    fn recurrence_without_exceptions() {
        let now = utc("2026-11-16T14:00:00Z"); // Mon
        let (from, to) = window(now, CHI);
        let evs = occurrences(GOOGLE, CHI, from, to);
        let standups: Vec<_> = evs.iter().filter(|e| e.title == "Stand, up").collect();
        assert_eq!(standups.len(), 2, "Mon + Tue");
        assert_eq!(standups[0].start, utc("2026-11-16T15:30:00Z"));
        assert_eq!(standups[0].end - standups[0].start, TimeDelta::minutes(15));
    }

    #[test]
    fn next_prefers_timed_and_in_progress() {
        let now = utc("2026-11-11T14:00:00Z");
        let (from, to) = window(now, CHI);
        let evs = occurrences(GOOGLE, CHI, from, to);
        assert_eq!(next(&evs, now).unwrap().title, "Stand, up (moved)");
        // During the moved standup it's still "next" (in progress).
        assert_eq!(next(&evs, utc("2026-11-11T20:30:00Z")).unwrap().title, "Stand, up (moved)");
        // After the last timed event only the all-day one would remain, but it ended.
        assert_eq!(next(&evs, utc("2026-11-12T21:30:00Z")), None);
        // Only all-day left → it's shown.
        let only_all_day: Vec<_> = evs.iter().filter(|e| e.all_day).cloned().collect();
        assert_eq!(next(&only_all_day, now).unwrap().title, "Veterans Day");
    }

    #[test]
    fn floating_and_unknown_tzid_use_default_zone() {
        let w = parse_when("20261111T090000", &[], CHI).unwrap();
        assert_eq!(w.instant(CHI), utc("2026-11-11T15:00:00Z"));
        let params = vec![("TZID".to_string(), "Central Standard Time".to_string())];
        assert_eq!(parse_when("20261111T090000", &params, CHI).unwrap().instant(CHI), utc("2026-11-11T15:00:00Z"));
    }

    #[test]
    fn dst_gap_and_durations() {
        // 02:30 doesn't exist on 8 Mar 2026 in Chicago → treated as 03:30 CDT.
        let n = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap().and_hms_opt(2, 30, 0).unwrap();
        assert_eq!(local_to_utc(n, CHI), utc("2026-03-08T08:30:00Z"));
        assert_eq!(parse_duration("PT1H30M"), Some(TimeDelta::minutes(90)));
        assert_eq!(parse_duration("P1DT2H"), Some(TimeDelta::hours(26)));
        assert_eq!(parse_duration("P2W"), Some(TimeDelta::weeks(2)));
        assert_eq!(parse_duration("-P0DT0H10M0S"), Some(TimeDelta::minutes(-10)));
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn quoted_params_and_escapes() {
        let p = parse_line(r#"ATTENDEE;CN="Doe; Jane";X-URL="https://a:b":mailto:j@x"#).unwrap();
        assert_eq!(p.name, "ATTENDEE");
        assert_eq!(p.params, [("CN".into(), "Doe; Jane".into()), ("X-URL".into(), "https://a:b".into())]);
        assert_eq!(p.value, "mailto:j@x");
        assert_eq!(unescape(r"a\, b\; c\nd\\e"), r"a, b; c d\e");
    }
}
