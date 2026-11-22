//! Shared Scoreboard view model for Blades and Classic (§2–§4, §7, §10).

use std::collections::HashSet;

use chrono::{DateTime, Duration, Timelike};
use chrono_tz::Tz;
use tvdash_core::{
    config::ClockFormat,
    models::{ScoreboardGame, ScoreboardPage},
};

use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Live,
    Upcoming,
    Final,
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Live,
    Upcoming,
    Final,
    Postponed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Lead,
    Trail,
    Even,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideView {
    pub rank: String,
    pub short: String,
    pub full: String,
    pub initials: String,
    pub logo_url: Option<String>,
    pub periods: Vec<String>,
    pub total: String,
    pub emphasis: Emphasis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardView {
    pub event_id: String,
    pub league: String,
    pub status: String,
    pub tone: StatusTone,
    pub period_labels: Vec<String>,
    pub away: SideView,
    pub home: SideView,
    pub meta: String,
    pub starred: bool,
    pub following: bool,
    pub game_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupView {
    pub kind: GroupKind,
    pub title: String,
    pub count: String,
    pub cards: Vec<CardView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreboardView {
    pub date_label: String,
    pub fresh: String,
    pub stale: bool,
    pub groups: Vec<GroupView>,
}

fn format_period_label(p: i32) -> String {
    match p {
        1 => "1st".into(),
        2 => "2nd".into(),
        3 => "3rd".into(),
        4 => "4th".into(),
        5 => "OT".into(),
        n if n > 5 => format!("{}OT", n - 4),
        _ => String::new(),
    }
}

fn break_label(sport: &str, league: &str, period: Option<i32>, status_text: &str) -> Option<String> {
    let st_lower = status_text.trim().to_ascii_lowercase();
    let is_break = st_lower.contains("halftime")
        || st_lower.contains("intermission")
        || st_lower.starts_with("end ");
    let is_zero_clock_or_break = is_break;
    if !is_zero_clock_or_break {
        return None;
    }
    if st_lower.contains("halftime") {
        return Some("Halftime".into());
    }
    let p = period.unwrap_or(0);
    if sport == "hockey" || league == "nhl" {
        return Some(match p {
            1 => "1st Int".into(),
            2 => "2nd Int".into(),
            3 => "3rd Int".into(),
            _ => "Int".into(),
        });
    }
    if sport == "soccer" || league.contains("soccer") || league.contains("mls") || league.contains("epl") {
        return Some("Halftime".into());
    }
    if league == "mens-college-basketball" {
        return Some(match p {
            1 => "Halftime".into(),
            2 => "End 2nd".into(),
            _ => "End OT".into(),
        });
    }
    if sport == "football" || league == "nfl" || league == "college-football" || league == "nba" || league == "wnba" || league == "womens-college-basketball" {
        return Some(match p {
            1 => "End 1st".into(),
            2 => "Halftime".into(),
            3 => "End 3rd".into(),
            4 => "End 4th".into(),
            _ => "End OT".into(),
        });
    }
    None
}

/// Status text & tone for a Scoreboard card (§3).
pub fn status_text(
    g: &ScoreboardGame,
    now_local: DateTime<Tz>,
    clock: ClockFormat,
) -> (String, StatusTone) {
    let st_norm = g.status.trim().to_ascii_lowercase();
    match st_norm.as_str() {
        "live" => {
            let clk = g.display_clock.as_deref().map(str::trim).unwrap_or("");
            let st_lower = g.status_text.trim().to_ascii_lowercase();
            let clock_zero = clk == "0:00" || clk == "0.0";
            if let Some(lbl) = break_label(&g.sport, &g.league, g.period, &g.status_text) {
                return (lbl, StatusTone::Live);
            }
            if clock_zero && g.period.is_some() {
                // Treat 0:00 clock with a period as break/intermission per §3
                let fake_break = if g.sport == "hockey" || g.league == "nhl" {
                    "intermission"
                } else {
                    "end "
                };
                if let Some(lbl) = break_label(&g.sport, &g.league, g.period, fake_break) {
                    return (lbl, StatusTone::Live);
                }
            }
            if st_lower.contains("shootout") || (g.league == "nhl" && g.period == Some(5)) {
                return ("SO".into(), StatusTone::Live);
            }
            if !clk.is_empty() && !clock_zero {
                let p = g.period.map(format_period_label).unwrap_or_default();
                let txt = if p.is_empty() { clk.to_string() } else { format!("{p} · {clk}") };
                return (txt, StatusTone::Live);
            }
            let raw_st = g.status_text.trim();
            if !raw_st.is_empty() && !matches!(st_lower.as_str(), "live" | "in progress" | "in_progress") {
                return (raw_st.to_string(), StatusTone::Live);
            }
            ("LIVE".into(), StatusTone::Live)
        }
        "upcoming" | "scheduled" | "pre" => {
            if g.time_tbd || g.starts_at_utc.is_none() {
                return ("Time TBD".into(), StatusTone::Upcoming);
            }
            let ts = g.starts_at_utc.unwrap();
            let Some(utc) = DateTime::from_timestamp(ts, 0) else {
                return ("Time TBD".into(), StatusTone::Upcoming);
            };
            let local_dt = utc.with_timezone(&now_local.timezone());
            let today = now_local.date_naive();
            let d = local_dt.date_naive();
            let hm = format::hm(local_dt.naive_local(), clock);
            let day_word = if d == today {
                if local_dt.hour() >= 17 { "Tonight" } else { "Today" }
            } else if d == today + Duration::days(1) {
                "Tomorrow"
            } else {
                return (format!("{} · {hm}", local_dt.format("%a")), StatusTone::Upcoming);
            };
            (format!("{day_word} · {hm}"), StatusTone::Upcoming)
        }
        "final" | "completed" => {
            let raw = g.status_text.trim();
            let txt = if raw.is_empty() { "Final".into() } else { raw.to_string() };
            (txt, StatusTone::Final)
        }
        "postponed" => ("Postponed".into(), StatusTone::Muted),
        "delayed" => ("Delayed".into(), StatusTone::Muted),
        "suspended" => ("Suspended".into(), StatusTone::Muted),
        "cancelled" | "canceled" => ("Cancelled".into(), StatusTone::Muted),
        _ => {
            let raw = g.status_text.trim();
            let txt = if raw.is_empty() { g.header_text.clone() } else { raw.to_string() };
            (txt, StatusTone::Muted)
        }
    }
}

fn sum_ot(vals: &[String], start_idx: usize) -> String {
    let mut sum = 0i64;
    let mut any = false;
    for v in vals.iter().skip(start_idx) {
        if let Ok(n) = v.trim().parse::<i64>() {
            sum += n;
            any = true;
        }
    }
    if any { sum.to_string() } else { String::new() }
}

/// Box score columns per sport/league (§7). Returns `(labels, away_vals, home_vals)`.
pub fn period_columns(
    sport: &str,
    league: &str,
    away: &[String],
    home: &[String],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let max_len = away.len().max(home.len());
    if max_len == 0 {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let get = |v: &[String], i: usize| v.get(i).cloned().unwrap_or_default();

    if sport == "football" || league == "nfl" || league == "college-football" {
        let mut labels: Vec<String> = ["1", "2", "3", "4"].into_iter().map(str::to_owned).collect();
        let mut a: Vec<String> = (0..4).map(|i| get(away, i)).collect();
        let mut h: Vec<String> = (0..4).map(|i| get(home, i)).collect();
        if max_len > 4 {
            labels.push("OT".into());
            a.push(sum_ot(away, 4));
            h.push(sum_ot(home, 4));
        }
        return (labels, a, h);
    }

    if sport == "hockey" || league == "nhl" {
        let mut labels: Vec<String> = ["1", "2", "3"].into_iter().map(str::to_owned).collect();
        let mut a: Vec<String> = (0..3).map(|i| get(away, i)).collect();
        let mut h: Vec<String> = (0..3).map(|i| get(home, i)).collect();
        if max_len >= 4 {
            labels.push("OT".into());
            a.push(get(away, 3));
            h.push(get(home, 3));
        }
        if max_len >= 5 {
            labels.push("SO".into());
            a.push(get(away, 4));
            h.push(get(home, 4));
        }
        return (labels, a, h);
    }

    if league == "mens-college-basketball" {
        let mut labels: Vec<String> = ["1st", "2nd"].into_iter().map(str::to_owned).collect();
        let mut a: Vec<String> = (0..2).map(|i| get(away, i)).collect();
        let mut h: Vec<String> = (0..2).map(|i| get(home, i)).collect();
        if max_len > 2 {
            labels.push("OT".into());
            a.push(sum_ot(away, 2));
            h.push(sum_ot(home, 2));
        }
        return (labels, a, h);
    }

    if sport == "basketball" || league == "nba" || league == "wnba" || league == "womens-college-basketball" {
        let mut labels: Vec<String> = ["1", "2", "3", "4"].into_iter().map(str::to_owned).collect();
        let mut a: Vec<String> = (0..4).map(|i| get(away, i)).collect();
        let mut h: Vec<String> = (0..4).map(|i| get(home, i)).collect();
        if max_len > 4 {
            labels.push("OT".into());
            a.push(sum_ot(away, 4));
            h.push(sum_ot(home, 4));
        }
        return (labels, a, h);
    }

    if sport == "soccer" || league.contains("soccer") || league.contains("mls") || league.contains("epl") {
        let mut labels: Vec<String> = ["1st", "2nd"].into_iter().map(str::to_owned).collect();
        let mut a: Vec<String> = (0..2).map(|i| get(away, i)).collect();
        let mut h: Vec<String> = (0..2).map(|i| get(home, i)).collect();
        if max_len >= 3 {
            labels.push("ET".into());
            a.push(get(away, 2));
            h.push(get(home, 2));
        }
        if max_len >= 4 {
            labels.push("PK".into());
            a.push(get(away, 3));
            h.push(get(home, 3));
        }
        return (labels, a, h);
    }

    // Baseball & anything else: total only (§7)
    (Vec::new(), Vec::new(), Vec::new())
}

fn group_kind_for(status: &str) -> GroupKind {
    match status.trim().to_ascii_lowercase().as_str() {
        "live" => GroupKind::Live,
        "upcoming" | "scheduled" | "pre" => GroupKind::Upcoming,
        "final" | "completed" => GroupKind::Final,
        _ => GroupKind::Postponed,
    }
}

fn game_count_label(n: usize) -> String {
    if n == 1 { "1 game".into() } else { format!("{n} games") }
}

/// Build the shared [`ScoreboardView`] for both Blades and Classic (§10).
pub fn build_view(
    page: &ScoreboardPage,
    starred: &HashSet<String>,
    follows: &[(String, String)],
    filter: &str,
    now_local: DateTime<Tz>,
    clock: ClockFormat,
) -> ScoreboardView {
    let f_norm = filter.trim().to_ascii_lowercase();
    let mut all_games: Vec<&ScoreboardGame> = page
        .groups
        .iter()
        .flat_map(|grp| grp.games.iter())
        .filter(|g| match f_norm.as_str() {
            "" | "all" => true,
            "top25" => g.away_rank.is_some() || g.home_rank.is_some(),
            other => g.league.eq_ignore_ascii_case(other),
        })
        .collect();

    all_games.sort_by_key(|g| {
        (
            g.time_tbd,
            g.starts_at_utc.unwrap_or(i64::MAX),
            g.league_label.clone(),
            g.away_name.clone(),
        )
    });

    let mut buckets: [(GroupKind, &str, Vec<&ScoreboardGame>); 4] = [
        (GroupKind::Live, "Live", Vec::new()),
        (GroupKind::Upcoming, "Upcoming", Vec::new()),
        (GroupKind::Final, "Final", Vec::new()),
        (GroupKind::Postponed, "Postponed & cancelled", Vec::new()),
    ];

    for g in all_games {
        let kind = group_kind_for(&g.status);
        if let Some(b) = buckets.iter_mut().find(|(k, _, _)| *k == kind) {
            b.2.push(g);
        }
    }

    let groups = buckets
        .into_iter()
        .filter(|(_, _, games)| !games.is_empty())
        .map(|(kind, title, games)| {
            let count = game_count_label(games.len());
            let cards = games
                .into_iter()
                .map(|g| {
                    let (status, tone) = status_text(g, now_local, clock);
                    let (period_labels, away_p, home_p) = if matches!(kind, GroupKind::Live | GroupKind::Final) {
                        period_columns(&g.sport, &g.league, &g.away_periods, &g.home_periods)
                    } else {
                        (Vec::new(), Vec::new(), Vec::new())
                    };

                    let (away_emp, home_emp) = if kind == GroupKind::Upcoming || kind == GroupKind::Postponed {
                        (Emphasis::Even, Emphasis::Even)
                    } else {
                        let a_num = g.away_score.trim().parse::<i64>().ok();
                        let h_num = g.home_score.trim().parse::<i64>().ok();
                        match (a_num, h_num) {
                            (Some(a), Some(h)) if a > h => (Emphasis::Lead, Emphasis::Trail),
                            (Some(a), Some(h)) if h > a => (Emphasis::Trail, Emphasis::Lead),
                            _ if g.away_winner && !g.home_winner => (Emphasis::Lead, Emphasis::Trail),
                            _ if g.home_winner && !g.away_winner => (Emphasis::Trail, Emphasis::Lead),
                            _ => (Emphasis::Even, Emphasis::Even),
                        }
                    };

                    let (away_total, home_total) = if kind == GroupKind::Upcoming {
                        if g.neutral_site {
                            ("Team".into(), "Team".into())
                        } else {
                            ("Away".into(), "Home".into())
                        }
                    } else if kind == GroupKind::Postponed {
                        (String::new(), String::new())
                    } else {
                        (g.away_score.clone(), g.home_score.clone())
                    };

                    let mut meta_parts = Vec::new();
                    if !g.venue.is_empty() {
                        meta_parts.push(g.venue.as_str());
                    }
                    if !g.broadcasts.is_empty() {
                        meta_parts.push(g.broadcasts.as_str());
                    }

                    let following = follows.iter().any(|(l, tid)| {
                        l == &g.league && (tid == &g.away_team_id || tid == &g.home_team_id)
                    });

                    CardView {
                        event_id: g.event_id.clone(),
                        league: g.league_label.to_uppercase(),
                        status,
                        tone,
                        period_labels,
                        away: SideView {
                            rank: g.away_rank.map(|r| format!("#{r}")).unwrap_or_default(),
                            short: g.away_name.clone(),
                            full: g.away_name.clone(),
                            initials: g.away_initials.clone(),
                            logo_url: g.away_logo_url.clone(),
                            periods: away_p,
                            total: away_total,
                            emphasis: away_emp,
                        },
                        home: SideView {
                            rank: g.home_rank.map(|r| format!("#{r}")).unwrap_or_default(),
                            short: g.home_name.clone(),
                            full: g.home_name.clone(),
                            initials: g.home_initials.clone(),
                            logo_url: g.home_logo_url.clone(),
                            periods: home_p,
                            total: home_total,
                            emphasis: home_emp,
                        },
                        meta: meta_parts.join(" · "),
                        starred: starred.contains(&g.event_id),
                        following,
                        game_url: g.source_url.clone(),
                    }
                })
                .collect();
            GroupView {
                kind,
                title: title.into(),
                count,
                cards,
            }
        })
        .collect();

    let (fresh, stale) = match page.max_observed_at_utc {
        Some(ts) => {
            let mins = (now_local.timestamp() - ts).max(0) / 60;
            if mins > 25 {
                (format!("Data is {mins} min old"), true)
            } else if let Some(utc) = DateTime::from_timestamp(ts, 0) {
                let local = utc.with_timezone(&now_local.timezone());
                (
                    format!("Updates ~10m · last {}", format::hm(local.naive_local(), clock)),
                    false,
                )
            } else {
                ("Updates ~10m".into(), false)
            }
        }
        None => (String::new(), false),
    };

    ScoreboardView {
        date_label: page.day_label.clone(),
        fresh,
        stale,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use chrono_tz::America::Chicago;
    use tvdash_core::models::ScoreboardGroup;

    use super::*;

    fn base_game(id: &str, status: &str) -> ScoreboardGame {
        ScoreboardGame {
            key: id.into(),
            event_id: id.into(),
            sport: "football".into(),
            league: "college-football".into(),
            away_team_id: "a".into(),
            home_team_id: "h".into(),
            league_label: "College Football".into(),
            status: status.into(),
            status_text: status.into(),
            period: Some(3),
            display_clock: Some("3:38".into()),
            starts_at_utc: Some(1_795_200_000),
            time_tbd: false,
            neutral_site: false,
            venue: "Stadium".into(),
            broadcasts: "FOX".into(),
            last_observed_at_utc: Some(1_795_200_000),
            away_periods: vec!["10".into(), "7".into(), "0".into()],
            home_periods: vec!["0".into(), "0".into(), "14".into()],
            header_text: String::new(),
            away_name: "Utah".into(),
            away_rank: Some(2),
            away_score: "17".into(),
            away_winner: false,
            away_leading: true,
            away_logo_url: None,
            away_initials: "UTA".into(),
            home_name: "TCU".into(),
            home_rank: None,
            home_score: "14".into(),
            home_winner: false,
            home_leading: false,
            home_logo_url: None,
            home_initials: "TCU".into(),
            details_text: String::new(),
            warning_text: String::new(),
            source_url: None,
        }
    }

    #[test]
    fn status_text_handles_clock_breaks_ot_and_upcoming() {
        let now = Chicago.with_ymd_and_hms(2026, 11, 21, 14, 0, 0).unwrap();

        let g = base_game("1", "live");
        assert_eq!(status_text(&g, now, ClockFormat::H12), ("3rd · 3:38".into(), StatusTone::Live));

        // NHL intermission (0:00 in 2nd) -> "2nd Int", never "2nd · 0:00"
        let mut nhl = base_game("2", "live");
        nhl.sport = "hockey".into();
        nhl.league = "nhl".into();
        nhl.period = Some(2);
        nhl.display_clock = Some("0:00".into());
        nhl.status_text = "End of 2nd".into();
        assert_eq!(status_text(&nhl, now, ClockFormat::H12), ("2nd Int".into(), StatusTone::Live));

        // No period data -> "LIVE"
        let mut no_p = base_game("3", "live");
        no_p.period = None;
        no_p.display_clock = None;
        no_p.status_text = "In Progress".into();
        assert_eq!(status_text(&no_p, now, ClockFormat::H12), ("LIVE".into(), StatusTone::Live));

        // Upcoming TBD
        let mut tbd = base_game("4", "upcoming");
        tbd.time_tbd = true;
        assert_eq!(status_text(&tbd, now, ClockFormat::H12), ("Time TBD".into(), StatusTone::Upcoming));
    }

    #[test]
    fn period_columns_match_sport_and_ot_rules() {
        let (lbls, a, h) = period_columns(
            "football",
            "college-football",
            &["7".into(), "10".into(), "0".into(), "3".into(), "7".into()],
            &["0".into(), "7".into(), "7".into(), "6".into(), "0".into()],
        );
        assert_eq!(lbls, ["1", "2", "3", "4", "OT"]);
        assert_eq!(a, ["7", "10", "0", "3", "7"]);
        assert_eq!(h, ["0", "7", "7", "6", "0"]);

        // Baseball has no period columns
        let (b_lbls, _, _) = period_columns("baseball", "mlb", &["1".into()], &["0".into()]);
        assert!(b_lbls.is_empty());
    }

    #[test]
    fn build_view_groups_sorts_filters_and_emphasizes() {
        let now = Chicago.with_ymd_and_hms(2026, 11, 21, 14, 0, 0).unwrap();
        let mut g_live = base_game("live1", "live");
        g_live.last_observed_at_utc = Some(now.timestamp() - 120);
        let mut g_unranked = base_game("up1", "upcoming");
        g_unranked.away_rank = None;
        g_unranked.starts_at_utc = Some(now.timestamp() + 3600);
        let page = ScoreboardPage {
            day_label: "Sat, Nov 21".into(),
            max_observed_at_utc: Some(now.timestamp() - 120),
            groups: vec![ScoreboardGroup {
                title: "All".into(),
                subtitle: String::new(),
                is_live: false,
                games: vec![g_unranked, g_live],
            }],
            ..Default::default()
        };

        let mut starred = HashSet::new();
        starred.insert("live1".into());
        let view = build_view(&page, &starred, &[], "top25", now, ClockFormat::H12);
        assert_eq!(view.groups.len(), 1);
        assert_eq!(view.groups[0].kind, GroupKind::Live);
        assert_eq!(view.groups[0].cards[0].away.emphasis, Emphasis::Lead);
        assert_eq!(view.groups[0].cards[0].home.emphasis, Emphasis::Trail);
        assert!(view.groups[0].cards[0].starred);
        assert!(!view.stale);
    }
}
