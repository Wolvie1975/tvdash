//! Home "Next Game" card selection and formatting (§5.1–5.2).

use std::collections::HashSet;

use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use tvdash_core::events::{NextGameDto, NextGameSideDto};
use tvdash_events::{alerts::NextGameRow, format::team_initials};

use crate::{sports_follows::league_display_name, sports_watch::team_badge_color};

fn normalize_status(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "live" | "in_progress" | "in progress" | "halftime" => Some("live"),
        "upcoming" | "scheduled" | "pre" => Some("upcoming"),
        "final" | "completed" => Some("final"),
        // Ignore postponed, cancelled, suspended, delayed, unknown (§5.2)
        _ => None,
    }
}

fn best_rank(g: &NextGameRow) -> Option<u8> {
    [g.away_rank, g.home_rank]
        .into_iter()
        .flatten()
        .filter(|&r| (1..=25).contains(&r))
        .min()
}

fn is_followed_or_starred(
    g: &NextGameRow,
    follows: &[(String, String)],
    starred: &HashSet<String>,
) -> bool {
    if starred.contains(&g.event_id) {
        return true;
    }
    follows.iter().any(|(league, team_id)| {
        league == &g.league && (team_id == &g.home_team_id || team_id == &g.away_team_id)
    })
}

fn pick_from<'a>(
    pool: &[&'a NextGameRow],
    now_utc: NaiveDateTime,
    today_local: NaiveDate,
    tz: Tz,
) -> Option<&'a NextGameRow> {
    let mut live_ranked: Vec<&NextGameRow> = Vec::new();
    let mut live_any: Vec<&NextGameRow> = Vec::new();
    let mut upcoming_today_ranked: Vec<&NextGameRow> = Vec::new();
    let mut upcoming_any: Vec<&NextGameRow> = Vec::new();
    let mut recent_final: Vec<&NextGameRow> = Vec::new();

    let final_cutoff = now_utc - Duration::hours(5);

    for &g in pool {
        match normalize_status(&g.status) {
            Some("live") => {
                if best_rank(g).is_some() {
                    live_ranked.push(g);
                }
                live_any.push(g);
            }
            Some("upcoming") => {
                let game_date = g
                    .starts_at_utc
                    .map(|dt| Utc.from_utc_datetime(&dt).with_timezone(&tz).date_naive())
                    .unwrap_or(g.scoreboard_date);
                if game_date == today_local && best_rank(g).is_some() {
                    upcoming_today_ranked.push(g);
                }
                upcoming_any.push(g);
            }
            Some("final") => {
                if let Some(st) = g.starts_at_utc
                    && st >= final_cutoff
                {
                    recent_final.push(g);
                }
            }
            _ => {}
        }
    }

    if !live_ranked.is_empty() {
        live_ranked.sort_by_key(|g| (best_rank(g).unwrap_or(255), g.starts_at_utc, g.event_id.as_str()));
        return live_ranked.first().copied();
    }
    if !live_any.is_empty() {
        live_any.sort_by_key(|g| (g.starts_at_utc, g.event_id.as_str()));
        return live_any.first().copied();
    }
    if !upcoming_today_ranked.is_empty() {
        upcoming_today_ranked.sort_by_key(|g| {
            (
                g.starts_at_utc,
                best_rank(g).unwrap_or(255),
                g.event_id.as_str(),
            )
        });
        return upcoming_today_ranked.first().copied();
    }
    if !upcoming_any.is_empty() {
        upcoming_any.sort_by_key(|g| (g.starts_at_utc, g.event_id.as_str()));
        return upcoming_any.first().copied();
    }
    if !recent_final.is_empty() {
        recent_final.sort_by(|a, b| {
            b.starts_at_utc
                .cmp(&a.starts_at_utc)
                .then_with(|| a.event_id.cmp(&b.event_id))
        });
        return recent_final.first().copied();
    }
    None
}

/// Pick the single Next Game card for Home (§5.2).
pub fn pick_next_game<'a>(
    rows: &'a [NextGameRow],
    follows: &[(String, String)],
    starred: &HashSet<String>,
    now_utc: NaiveDateTime,
    today_local: NaiveDate,
    tz: Tz,
) -> Option<&'a NextGameRow> {
    let valid: Vec<&NextGameRow> = rows
        .iter()
        .filter(|g| normalize_status(&g.status).is_some())
        .collect();

    // Step 0: followed team or starred game first
    let followed: Vec<&NextGameRow> = valid
        .iter()
        .copied()
        .filter(|g| is_followed_or_starred(g, follows, starred))
        .collect();
    if let Some(g) = pick_from(&followed, now_utc, today_local, tz) {
        return Some(g);
    }

    // Steps 1–5 across all games
    pick_from(&valid, now_utc, today_local, tz)
}

fn format_period(p: i32) -> String {
    match p {
        1 => "1st".into(),
        2 => "2nd".into(),
        3 => "3rd".into(),
        4 => "4th".into(),
        n if n > 4 => format!("OT{}", n - 4),
        _ => String::new(),
    }
}

fn is_generic_live_status(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "" | "live" | "in progress" | "in_progress"
    )
}

/// Format a selected [`NextGameRow`] into [`NextGameDto`] (§5.1).
pub fn to_dto(g: &NextGameRow, today_local: NaiveDate, tz: Tz) -> NextGameDto {
    let state = normalize_status(&g.status).unwrap_or("upcoming");
    let state_text = match state {
        "live" => {
            if let Some(clock) = g.display_clock.as_deref().filter(|s| !s.is_empty()) {
                let p = g.period.map(format_period).unwrap_or_default();
                if p.is_empty() {
                    format!("LIVE · {clock}")
                } else {
                    format!("LIVE · {p} · {clock}")
                }
            } else if !is_generic_live_status(&g.status_text) {
                format!("LIVE · {}", g.status_text.trim())
            } else {
                "LIVE".into()
            }
        }
        "upcoming" => {
            let local_dt = g
                .starts_at_utc
                .map(|dt| Utc.from_utc_datetime(&dt).with_timezone(&tz));
            let day_label = match local_dt.map(|d| d.date_naive()) {
                Some(d) if d == today_local => {
                    if local_dt.is_some_and(|t| t.hour() >= 17) {
                        "TONIGHT"
                    } else {
                        "TODAY"
                    }
                }
                Some(d) if d == today_local + Duration::days(1) => "TOMORROW",
                _ => "UPCOMING",
            };
            if g.time_tbd || local_dt.is_none() {
                format!("{day_label} · Time TBD")
            } else {
                let t = local_dt.unwrap().format("%-I:%M %p").to_string();
                format!("{day_label} · {t}")
            }
        }
        _ => {
            let st = g.status_text.trim();
            if st.is_empty() {
                "FINAL".into()
            } else {
                st.to_uppercase()
            }
        }
    };

    let (away_lead, home_lead) = if state == "upcoming" {
        (true, true)
    } else {
        match (g.away_score, g.home_score) {
            (Some(a), Some(h)) if a > h => (true, false),
            (Some(a), Some(h)) if h > a => (false, true),
            _ => (true, true),
        }
    };

    let side = |rank: Option<u8>, name: &str, logo: &Option<String>, score: Option<i32>, lead: bool| {
        NextGameSideDto {
            rank: rank.map(|r| format!("#{r}")).unwrap_or_default(),
            init: team_initials(name),
            logo_url: logo.clone(),
            color: team_badge_color(name),
            name: name.to_string(),
            score: if state == "upcoming" {
                String::new()
            } else {
                score.map(|s| s.to_string()).unwrap_or_default()
            },
            lead,
        }
    };

    let mut meta_parts: Vec<&str> = Vec::new();
    if !g.venue.is_empty() {
        meta_parts.push(&g.venue);
    }
    if let Some(b) = g.broadcasts.as_deref().filter(|s| !s.is_empty()) {
        meta_parts.push(b);
    }

    NextGameDto {
        event_id: g.event_id.clone(),
        date: g.scoreboard_date.format("%Y-%m-%d").to_string(),
        league: league_display_name(&g.league).to_uppercase(),
        state: state.to_string(),
        state_text,
        away: side(g.away_rank, &g.away_name, &g.away_logo_url, g.away_score, away_lead),
        home: side(g.home_rank, &g.home_name, &g.home_logo_url, g.home_score, home_lead),
        meta: meta_parts.join(" · "),
    }
}

#[cfg(test)]
mod tests {
    use chrono_tz::America::Chicago;

    use super::*;

    fn game(id: &str, status: &str, starts_offset_mins: i64, away_rank: Option<u8>, home_rank: Option<u8>) -> NextGameRow {
        let now = NaiveDate::from_ymd_opt(2026, 11, 21)
            .unwrap()
            .and_hms_opt(18, 0, 0)
            .unwrap();
        NextGameRow {
            event_id: id.into(),
            scoreboard_date: NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(),
            starts_at_utc: Some(now + Duration::minutes(starts_offset_mins)),
            time_tbd: false,
            status: status.into(),
            status_text: status.into(),
            period: Some(3),
            display_clock: Some("4:12".into()),
            league: "college-football".into(),
            home_team_id: format!("h-{id}"),
            home_name: "Home".into(),
            home_score: Some(21),
            home_logo_url: None,
            home_rank,
            away_team_id: format!("a-{id}"),
            away_name: "Away".into(),
            away_score: Some(24),
            away_logo_url: None,
            away_rank,
            venue: "Stadium".into(),
            broadcasts: Some("ESPN".into()),
        }
    }

    #[test]
    fn live_ranked_beats_live_unranked_and_best_rank_wins() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap().and_hms_opt(18, 0, 0).unwrap();
        let today = now.date();
        let rows = vec![
            game("unranked", "live", -60, None, None),
            game("r17", "live", -30, Some(17), None),
            game("r2", "live", -15, None, Some(2)),
        ];
        let picked = pick_next_game(&rows, &[], &HashSet::new(), now, today, Chicago).unwrap();
        assert_eq!(picked.event_id, "r2");
    }

    #[test]
    fn skips_rank_steps_when_no_ranks_and_prefers_live_then_upcoming_then_recent_final() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap().and_hms_opt(18, 0, 0).unwrap();
        let today = now.date();

        // Postponed/cancelled ignored; old final (>5h) ignored; nothing → None
        let ignored = vec![
            game("post", "postponed", 30, Some(1), None),
            game("canc", "cancelled", 30, Some(2), None),
            game("old_fin", "final", -360, Some(3), None),
        ];
        assert!(pick_next_game(&ignored, &[], &HashSet::new(), now, today, Chicago).is_none());

        // Upcoming today ranked beats earlier unranked upcoming
        let upcoming = vec![
            game("u_early", "upcoming", 30, None, None),
            game("u_ranked", "upcoming", 120, Some(8), None),
        ];
        let picked = pick_next_game(&upcoming, &[], &HashSet::new(), now, today, Chicago).unwrap();
        assert_eq!(picked.event_id, "u_ranked");

        // Recent final within 5h when no live/upcoming
        let finals = vec![
            game("f_old", "final", -360, None, None),
            game("f_recent", "final", -180, None, None),
        ];
        let picked = pick_next_game(&finals, &[], &HashSet::new(), now, today, Chicago).unwrap();
        assert_eq!(picked.event_id, "f_recent");
    }
}
