use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike, Utc};
use chrono_tz::America::Chicago;
use tvdash_core::models::{ScheduleChoice, SportsDayGroup, SportsGame, SportsLeagueGroup};

/// Raw row returned by the upcoming-games SQL query.
#[derive(Debug, Clone)]
pub struct RawGameRow {
    pub id: i32,
    pub url: String,
    pub title: String,
    pub sport: Option<String>,
    pub opponent: Option<String>,
    pub is_away: Option<bool>,
    pub location: Option<String>,
    pub event_date: NaiveDate,
    pub starts_at_utc: Option<NaiveDateTime>,
    pub time_tbd: bool,
    pub tv: Option<String>,
    pub stream_url: Option<String>,
    pub live_stats_url: Option<String>,
    pub team_logo_url: Option<String>,
    pub opponent_logo_url: Option<String>,
    pub school_name: Option<String>,
    pub is_neutral: Option<bool>,
    pub radio: Option<String>,
    pub audio_url: Option<String>,
    pub schedule_status: Option<String>,
    pub provider_key: Option<String>,
}

/// Today's calendar date in `America/Chicago`.
pub fn today_central() -> NaiveDate {
    Utc::now().with_timezone(&Chicago).date_naive()
}

/// Format `MAX(LastSeenAt)` (stored as UTC `datetime2`) in Central time, e.g.
/// `"Nov 19, 2026, 9:52 AM (UTC-6)"`.
pub fn format_last_updated(utc_dt: Option<NaiveDateTime>) -> String {
    let Some(naive) = utc_dt else {
        return String::new();
    };
    let utc = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
    let local = utc.with_timezone(&Chicago);
    let (_, h12) = local.hour12();
    let ampm = if local.hour() < 12 { "AM" } else { "PM" };
    let offset_secs = chrono::Offset::fix(local.offset()).local_minus_utc();
    let offset_hrs = offset_secs / 3600;
    let sign = if offset_hrs >= 0 { "+" } else { "" };
    format!(
        "{} {}, {}, {}:{:02} {} (UTC{}{})",
        local.format("%b"),
        local.day(),
        local.year(),
        h12,
        local.minute(),
        ampm,
        sign,
        offset_hrs
    )
}

/// Format a day heading, e.g. `"Wednesday, November 19, 2026"`.
pub fn format_day_heading(date: NaiveDate) -> String {
    format!("{}, {} {}, {}", date.format("%A"), date.format("%B"), date.day(), date.year())
}

/// Format a game's start time or status label. Returns `(text, is_tbd)`.
pub fn format_event_time(
    schedule_status: Option<&str>,
    time_tbd: bool,
    starts_at_utc: Option<NaiveDateTime>,
) -> (String, bool) {
    match schedule_status.map(str::trim) {
        Some("STATUS_POSTPONED") => return ("Postponed · Time TBD".into(), true),
        Some("STATUS_DELAYED") => return ("Delayed · Time TBD".into(), true),
        Some("STATUS_SUSPENDED") => return ("Suspended · Time TBD".into(), true),
        _ => {}
    }
    if time_tbd {
        return ("Time TBD".into(), true);
    }
    let Some(naive) = starts_at_utc else {
        return ("Time TBD".into(), true);
    };
    let utc = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
    let local = utc.with_timezone(&Chicago);
    let (_, h12) = local.hour12();
    let ampm = if local.hour() < 12 { "A.M." } else { "P.M." };
    (format!("{}:{:02} {} CT", h12, local.minute(), ampm), false)
}

/// Split `Location` on the first `" / "` into `(city, venue)`.
pub fn split_location(location: Option<&str>) -> (String, String) {
    let raw = location.unwrap_or("").trim();
    if raw.is_empty() {
        return (String::new(), String::new());
    }
    match raw.split_once(" / ") {
        Some((city, venue)) => (city.trim().to_string(), venue.trim().to_string()),
        None => (raw.to_string(), String::new()),
    }
}

/// Validate an external URL (must be absolute `http://` or `https://`).
pub fn safe_url(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    let lower = s.to_ascii_lowercase();
    if (lower.starts_with("https://") || lower.starts_with("http://")) && !s.contains(char::is_whitespace) {
        Some(s.to_string())
    } else {
        None
    }
}

/// Validate a logo URL and upgrade `http://` to `https://`.
pub fn safe_logo_url(raw: Option<&str>) -> Option<String> {
    let u = safe_url(raw)?;
    if u.len() >= 7 && u[..7].eq_ignore_ascii_case("http://") {
        Some(format!("https://{}", &u[7..]))
    } else {
        Some(u)
    }
}

/// Generate a 1–3 character initials badge for a team name.
pub fn team_initials(name: &str) -> String {
    let words: Vec<String> = name
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();
    match words.len() {
        0 => "?".into(),
        1 => words[0].chars().take(3).collect::<String>().to_uppercase(),
        _ => words
            .iter()
            .take(3)
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_uppercase(),
    }
}

/// Decode common HTML entities and strip HTML tags from `Radio`, collapsing whitespace.
pub fn clean_radio_html(raw: Option<&str>) -> String {
    let Some(s) = raw else { return String::new() };
    let decoded = decode_html_entities(s);
    let bytes = decoded.as_bytes();
    let mut out = String::with_capacity(decoded.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && i + 1 < bytes.len()
            && (bytes[i + 1].is_ascii_alphabetic() || matches!(bytes[i + 1], b'/' | b'!' | b'?'))
        {
            // Skip until '>' outside quotes, or end of string.
            i += 2;
            let mut quote: Option<u8> = None;
            while i < bytes.len() {
                let b = bytes[i];
                if let Some(q) = quote {
                    if b == q {
                        quote = None;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'>' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(' ');
        } else {
            let ch = decoded[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_html_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        if let Some(semi) = rest.find(';').filter(|&p| p <= 10) {
            let entity = &rest[1..semi];
            let replacement = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ if entity.starts_with('#') => {
                    let num = &entity[1..];
                    let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num.parse::<u32>().ok()
                    };
                    code.and_then(char::from_u32)
                }
                _ => None,
            };
            if let Some(ch) = replacement {
                out.push(ch);
                rest = &rest[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

/// Map `(provider_key, sport)` to its display league name (section 6).
pub fn league_name(provider_key: Option<&str>, sport: Option<&str>) -> String {
    let prov = provider_key.unwrap_or("").trim();
    let sp = sport.unwrap_or("").trim();
    match prov {
        "espn-championship" => "EFL Championship".into(),
        "espn-epl" => "Premier League".into(),
        "espn-mlb" => "MLB".into(),
        "espn-mls" => "MLS".into(),
        "espn-nwsl" => "NWSL".into(),
        "espn-wnba" => "WNBA".into(),
        "espn-nhl" => "NHL".into(),
        "espn-nfl" => "NFL".into(),
        "big12" => match sp {
            "Football" => "College Football".into(),
            "Men's Basketball" => "Men's College Basketball".into(),
            "Women's Basketball" => "Women's College Basketball".into(),
            "Soccer" => "Women's College Soccer".into(),
            "Volleyball" => "Women's College Volleyball".into(),
            "" => "Other events".into(),
            other => other.into(),
        },
        _ => {
            if sp.is_empty() {
                "Other events".into()
            } else {
                sp.into()
            }
        }
    }
}

/// Build sorted schedule dropdown choices from `(provider, sport)` pairs (section 4).
pub fn build_schedule_choices(pairs: &[(Option<String>, Option<String>)]) -> Vec<ScheduleChoice> {
    let mut seen = BTreeSet::new();
    let mut has_big12 = false;
    let mut big12_entries: Vec<(String, String)> = Vec::new();
    let mut other_entries: Vec<(String, String)> = Vec::new();

    for (prov, sport) in pairs {
        let p = prov.as_deref().unwrap_or("").trim();
        let s = sport.as_deref().unwrap_or("").trim();
        let value = format!("{p}|{s}");
        if !seen.insert(value.clone()) {
            continue;
        }
        if p == "big12" {
            has_big12 = true;
            if !s.is_empty() {
                big12_entries.push((format!("Big 12 {s}"), value));
            }
        } else {
            let label = match p {
                "espn-epl" => "Premier League".into(),
                "espn-championship" => "EFL Championship".into(),
                "espn-mls" => "MLS".into(),
                "espn-nwsl" => "NWSL".into(),
                "espn-mlb" => "MLB".into(),
                "espn-wnba" => "WNBA".into(),
                "espn-nhl" => "NHL".into(),
                "espn-nfl" => "NFL".into(),
                _ if !s.is_empty() => s.into(),
                _ => "Other events".into(),
            };
            other_entries.push((label, value));
        }
    }

    big12_entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    other_entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut choices = Vec::new();
    choices.push(ScheduleChoice { value: String::new(), label: "All schedules".into() });
    if has_big12 {
        let mut combined: Vec<(String, String)> = Vec::new();
        combined.push(("Big 12 · All sports".into(), "big12|".into()));
        combined.extend(big12_entries);
        // Merge Big 12 block with others sorted by "Big 12" vs other labels.
        let mut inserted_big12 = false;
        for (label, value) in other_entries {
            if !inserted_big12 && "big 12" < label.to_lowercase().as_str() {
                for (bl, bv) in combined.drain(..) {
                    choices.push(ScheduleChoice { value: bv, label: bl });
                }
                inserted_big12 = true;
            }
            choices.push(ScheduleChoice { value, label });
        }
        if !inserted_big12 {
            for (bl, bv) in combined {
                choices.push(ScheduleChoice { value: bv, label: bl });
            }
        }
    } else {
        for (label, value) in other_entries {
            choices.push(ScheduleChoice { value, label });
        }
    }
    choices
}

/// Group raw rows by day -> league (A–Z) -> start time / TBD / ID (sections 6 & 7).
pub fn group_games(rows: Vec<RawGameRow>) -> Vec<SportsDayGroup> {
    let mut by_day: BTreeMap<NaiveDate, Vec<RawGameRow>> = BTreeMap::new();
    for r in rows {
        by_day.entry(r.event_date).or_default().push(r);
    }

    let mut days = Vec::with_capacity(by_day.len());
    for (date, day_rows) in by_day {
        let mut by_league: BTreeMap<String, Vec<RawGameRow>> = BTreeMap::new();
        for r in day_rows {
            let lg = league_name(r.provider_key.as_deref(), r.sport.as_deref());
            by_league.entry(lg).or_default().push(r);
        }

        // Sort league names A-Z ignoring case.
        let mut league_pairs: Vec<(String, Vec<RawGameRow>)> = by_league.into_iter().collect();
        league_pairs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        let mut leagues = Vec::with_capacity(league_pairs.len());
        for (league, mut lg_rows) in league_pairs {
            lg_rows.sort_by_key(|r| {
                let (_, is_tbd) =
                    format_event_time(r.schedule_status.as_deref(), r.time_tbd, r.starts_at_utc);
                let has_known_time = !is_tbd && r.starts_at_utc.is_some();
                (!has_known_time, r.starts_at_utc, r.id)
            });

            let games = lg_rows.into_iter().map(format_game).collect();
            leagues.push(SportsLeagueGroup { league, games });
        }

        days.push(SportsDayGroup { date_heading: format_day_heading(date), leagues });
    }
    days
}

fn format_game(r: RawGameRow) -> SportsGame {
    let school = r
        .school_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Our team")
        .to_string();
    let opponent = r
        .opponent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Opponent to be announced")
        .to_string();
    let neutral = r.is_neutral == Some(true);
    let away = !neutral && r.is_away == Some(true);
    let known = !neutral && r.is_away.is_some();

    let team_logo = safe_logo_url(r.team_logo_url.as_deref());
    let opp_logo = safe_logo_url(r.opponent_logo_url.as_deref());

    let (away_name, away_logo_url, home_name, home_logo_url) = if away {
        (school, team_logo, opponent, opp_logo)
    } else {
        (opponent, opp_logo, school, team_logo)
    };
    let (away_label, home_label) = if neutral {
        ("Team".to_string(), "Team".to_string())
    } else if known {
        ("Away".to_string(), "Home".to_string())
    } else {
        (String::new(), String::new())
    };

    let (time_text, is_tbd) =
        format_event_time(r.schedule_status.as_deref(), r.time_tbd, r.starts_at_utc);
    let (city, venue) = split_location(r.location.as_deref());

    SportsGame {
        id: r.id,
        sport: r.sport.unwrap_or_default().trim().to_string(),
        is_neutral: neutral,
        away_initials: team_initials(&away_name),
        home_initials: team_initials(&home_name),
        away_name,
        away_label,
        away_logo_url,
        home_name,
        home_label,
        home_logo_url,
        time_text,
        is_tbd,
        city,
        venue,
        tv: r.tv.unwrap_or_default().trim().to_string(),
        radio: clean_radio_html(r.radio.as_deref()),
        stats_url: safe_url(r.live_stats_url.as_deref()),
        stream_url: safe_url(r.stream_url.as_deref()),
        audio_url: safe_url(r.audio_url.as_deref()),
        details_url: safe_url(Some(&r.url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    #[test]
    fn initials_match_spec() {
        assert_eq!(team_initials("Kansas"), "KAN");
        assert_eq!(team_initials("Grand Canyon"), "GC");
        assert_eq!(team_initials("Texas Tech Red Raiders"), "TTR");
        assert_eq!(team_initials("..."), "?");
        assert_eq!(team_initials(""), "?");
    }

    #[test]
    fn radio_html_cleaning_strips_tags_and_decodes() {
        let raw = r#"KCSP 610 AM &amp; <img src="x" onerror="a>b"> <b>SiriusXM</b><img src="broken"#;
        assert_eq!(clean_radio_html(Some(raw)), "KCSP 610 AM & SiriusXM");
    }

    #[test]
    fn safe_urls_reject_non_http_and_upgrade_logos() {
        assert_eq!(safe_url(Some("javascript:alert(1)")), None);
        assert_eq!(safe_url(Some("/relative/path")), None);
        assert_eq!(safe_url(Some("https://example.com/game")), Some("https://example.com/game".into()));
        assert_eq!(
            safe_logo_url(Some("http://a.espncdn.com/logo.png")),
            Some("https://a.espncdn.com/logo.png".into())
        );
    }

    #[test]
    fn time_formatting_central_and_statuses() {
        let dt = NaiveDate::from_ymd_opt(2026, 11, 19)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(18, 5, 0).unwrap());
        // Nov 19 is CST (UTC-6): 18:05 UTC = 12:05 P.M. CT
        assert_eq!(format_event_time(None, false, Some(dt)), ("12:05 P.M. CT".into(), false));
        assert_eq!(
            format_event_time(Some("STATUS_POSTPONED"), false, Some(dt)),
            ("Postponed · Time TBD".into(), true)
        );
        assert_eq!(format_event_time(None, true, Some(dt)), ("Time TBD".into(), true));
    }

    #[test]
    fn schedule_choices_order_and_big12_group() {
        let pairs = vec![
            (Some("espn-nfl".into()), Some("Football".into())),
            (Some("big12".into()), Some("Volleyball".into())),
            (Some("big12".into()), Some("Football".into())),
            (Some("espn-epl".into()), Some("Soccer".into())),
        ];
        let choices = build_schedule_choices(&pairs);
        let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["All schedules", "Big 12 · All sports", "Big 12 Football", "Big 12 Volleyball", "NFL", "Premier League"]
        );
    }
}
