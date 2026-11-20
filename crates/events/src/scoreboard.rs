//! Read-only SQL Server client and formatter for Sports → Scoreboard.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Days, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::America::Chicago;
use serde_json::Value;
use tiberius::numeric::Numeric;
use tvdash_core::{
    config::EventsConfig,
    models::{ScoreboardChoice, ScoreboardGame, ScoreboardGroup, ScoreboardPage},
};

use crate::{
    EventsError, connect,
    format::{
        format_day_heading, format_event_time, format_last_updated, safe_logo_url, safe_url,
        team_initials, today_central,
    },
};

#[derive(Debug, Clone)]
struct CoverageRow {
    sport: String,
    league: String,
    last_success_at: Option<NaiveDateTime>,
    availability: String,
}

#[derive(Debug, Clone)]
struct FollowRow {
    id: i32,
    kind: String, // "league" | "conference" | "team"
    sport: String,
    league: String,
    team_id: Option<String>,
    name: String,
}

#[derive(Debug, Clone)]
enum Selection {
    All,
    Top25,
    Competition { sport: String, league: String },
    Follow(FollowRow),
    ConferenceGroup { name: String, follows: Vec<FollowRow> },
}

#[derive(Debug, Clone)]
struct RawScoreboardGame {
    id: i64,
    provider: String,
    sport: String,
    league: String,
    event_id: String,
    scoreboard_date: NaiveDate,
    starts_at_utc: Option<NaiveDateTime>,
    time_tbd: bool,
    home_team_id: Option<String>,
    away_team_id: Option<String>,
    home_name: String,
    away_name: String,
    home_logo_url: Option<String>,
    away_logo_url: Option<String>,
    home_score: Option<String>,
    away_score: Option<String>,
    status: String,
    completed: bool,
    status_text: Option<String>,
    period: Option<i32>,
    display_clock: Option<String>,
    neutral_site: bool,
    home_shootout: Option<i32>,
    away_shootout: Option<i32>,
    winner_side: Option<String>,
    venue: Option<String>,
    broadcasts: Option<String>,
    source_url: Option<String>,
    live_details_json: Option<String>,
    last_observed_at_utc: NaiveDateTime,
    is_stale: bool,
    request_availability: Option<String>,
    home_rank: Option<u8>,
    away_rank: Option<u8>,
}

const SCOREBOARD_COLS: &str = r#"
g.ID, g.Provider, g.Sport, g.League, g.EventId, g.ScoreboardDate, g.StartsAtUtc, g.TimeTbd,
g.HomeTeamId, g.AwayTeamId, g.HomeName, g.AwayName, g.HomeLogoUrl, g.AwayLogoUrl, g.HomeScore, g.AwayScore,
g.Status, g.Completed, g.StatusText, g.Period, g.DisplayClock, g.NeutralSite,
g.HomeShootout, g.AwayShootout, g.WinnerSide, g.Venue, g.Broadcasts, g.SourceUrl, g.LiveDetailsJson,
g.LastObservedAtUtc, g.IsStale, g.RequestAvailability, g.HomeRank, g.AwayRank
"#;

pub async fn fetch_scoreboard(
    cfg: &EventsConfig,
    password: &str,
    day_str: &str,
    show_str: &str,
    final_only: bool,
) -> Result<ScoreboardPage, EventsError> {
    if password.trim().is_empty() {
        return Err(EventsError::MissingPassword);
    }

    let mut client = connect(cfg, password).await?;
    let today = today_central();
    let yesterday = today.checked_sub_days(Days::new(1)).unwrap_or(today);
    let max_day = today.checked_add_days(Days::new(7)).unwrap_or(today);

    let day = NaiveDate::parse_from_str(day_str.trim(), "%Y-%m-%d")
        .ok()
        .filter(|&d| d >= yesterday && d <= max_day)
        .unwrap_or(today);
    let prev_day = if day > yesterday {
        day.checked_sub_days(Days::new(1)).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    let next_day = if day < max_day {
        day.checked_add_days(Days::new(1)).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
    } else {
        String::new()
    };

    // 1. Lookups: coverage, follows, conference members
    let cov_rows = client
        .query(
            "SELECT Sport, League, LastSuccessAt, Availability FROM dbo.SportsScoreboardCoverage WHERE League <> 'womens-college-basketball'",
            &[],
        )
        .await?
        .into_first_result()
        .await?;
    let coverage: Vec<CoverageRow> = cov_rows
        .into_iter()
        .map(|r| CoverageRow {
            sport: r.get::<&str, _>(0).unwrap_or_default().to_string(),
            league: r.get::<&str, _>(1).unwrap_or_default().to_string(),
            last_success_at: r.get::<NaiveDateTime, _>(2),
            availability: r.get::<&str, _>(3).unwrap_or_default().to_string(),
        })
        .collect();

    let follow_rows = client
        .query(
            r#"
SELECT f.ID, f.Kind, f.Sport, f.League, f.TeamId, f.Name
FROM dbo.ResultFollows f
WHERE f.Enabled = 1 AND f.League <> 'womens-college-basketball'
  AND EXISTS (SELECT 1 FROM dbo.SportsScoreboardCoverage c
              WHERE c.Sport = f.Sport AND c.League = f.League AND c.League <> 'womens-college-basketball')
ORDER BY CASE f.Kind WHEN 'league' THEN 0 WHEN 'conference' THEN 1 ELSE 2 END, f.Name, f.League
"#,
            &[],
        )
        .await?
        .into_first_result()
        .await?;
    let follows: Vec<FollowRow> = follow_rows
        .into_iter()
        .map(|r| FollowRow {
            id: r.get::<i32, _>(0).unwrap_or_default(),
            kind: r.get::<&str, _>(1).unwrap_or_default().to_string(),
            sport: r.get::<&str, _>(2).unwrap_or_default().to_string(),
            league: r.get::<&str, _>(3).unwrap_or_default().to_string(),
            team_id: r.get::<&str, _>(4).map(str::to_string),
            name: r.get::<&str, _>(5).unwrap_or_default().to_string(),
        })
        .collect();

    let member_rows = client
        .query(
            r#"
SELECT t.FollowId, t.TeamId
FROM dbo.ResultFollowTeams t
JOIN dbo.ResultFollows f ON f.ID = t.FollowId
WHERE f.Enabled = 1 AND f.League <> 'womens-college-basketball'
"#,
            &[],
        )
        .await?
        .into_first_result()
        .await?;
    let mut conf_members: HashMap<i32, HashSet<String>> = HashMap::new();
    for r in member_rows {
        let fid = r.get::<i32, _>(0).unwrap_or_default();
        if let Some(tid) = r.get::<&str, _>(1) {
            conf_members.entry(fid).or_default().insert(tid.to_string());
        }
    }

    let (choices, resolved_show, selection) = resolve_selection(show_str, &coverage, &follows);

    // 2. Day window in UTC (Central midnight -> next Central midnight)
    let next_d = day.checked_add_days(Days::new(1)).unwrap_or(day);
    let start_utc = central_midnight_utc(day);
    let end_utc = central_midnight_utc(next_d);
    let carry_over = day == today;

    let mut where_extra = String::new();
    let mut p_sport = String::new();
    let mut p_league = String::new();
    let mut has_comp_params = false;

    match &selection {
        Selection::Top25 => {
            where_extra.push_str(" AND g.League IN ('college-football','mens-college-basketball') AND (g.HomeRank IS NOT NULL OR g.AwayRank IS NOT NULL)");
        }
        Selection::Competition { sport, league } => {
            where_extra.push_str(" AND g.Sport = @P5 AND g.League = @P6");
            p_sport = sport.clone();
            p_league = league.clone();
            has_comp_params = true;
        }
        Selection::Follow(f) => {
            where_extra.push_str(" AND g.Sport = @P5 AND g.League = @P6");
            p_sport = f.sport.clone();
            p_league = f.league.clone();
            has_comp_params = true;
        }
        Selection::All | Selection::ConferenceGroup { .. } => {}
    }
    if final_only {
        where_extra.push_str(" AND g.Status = 'final' AND g.Completed = 1");
    }

    let sql = format!(
        r#"
SELECT {SCOREBOARD_COLS}
FROM dbo.SportsScoreboard g
WHERE (   (g.ScoreboardDate = @P1 AND (g.TimeTbd = 1 OR (g.StartsAtUtc >= @P3 AND g.StartsAtUtc < @P4)))
       OR (@P2 = 1 AND g.ScoreboardDate < @P1 AND g.Status IN ('live','delayed','suspended')))
  AND g.League <> 'womens-college-basketball'
  {where_extra}
"#
    );

    let rows = if has_comp_params {
        client
            .query(&sql, &[&day, &carry_over, &start_utc, &end_utc, &p_sport, &p_league])
            .await?
            .into_first_result()
            .await?
    } else {
        client
            .query(&sql, &[&day, &carry_over, &start_utc, &end_utc])
            .await?
            .into_first_result()
            .await?
    };

    let mut raw_games: Vec<RawScoreboardGame> = rows.into_iter().map(map_raw_game).collect();

    // 3. Follow filter, deduplicate, sort
    raw_games.retain(|g| matches_selection(g, &selection, &conf_members));
    let mut seen_keys = HashSet::new();
    raw_games.retain(|g| {
        let key = format!("{}|{}|{}|{}", g.provider, g.sport, g.league, g.event_id);
        seen_keys.insert(key)
    });
    raw_games.sort_by_key(|g| {
        let rank = match g.status.as_str() {
            "live" => 0,
            "delayed" | "suspended" => 1,
            "upcoming" => 2,
            _ => 3,
        };
        (rank, g.starts_at_utc, g.id)
    });

    // 4. Optional Top 25 weekly list
    let top25_week = if matches!(selection, Selection::Top25) {
        let week_sql = format!(
            "SELECT {SCOREBOARD_COLS} FROM dbo.SportsScoreboardTop25Week g ORDER BY g.ScoreboardDate, g.StartsAtUtc, g.ID"
        );
        let w_rows = client.query(&week_sql, &[]).await?.into_first_result().await?;
        w_rows.into_iter().map(map_raw_game).collect()
    } else {
        Vec::new()
    };

    let now_utc = Utc::now().naive_utc();
    let coverage_warning = compute_coverage_warning(&coverage, &selection);
    let groups = build_groups(raw_games, top25_week, &selection, today, now_utc);
    let empty_message = if groups.is_empty() {
        if matches!(selection, Selection::Top25) {
            "No ranked games are scheduled for this date.".into()
        } else if coverage.is_empty() {
            "No scoreboard coverage is currently enabled.".into()
        } else {
            "No games are currently stored for this day and selection. Coverage may still be updating.".into()
        }
    } else {
        String::new()
    };

    Ok(ScoreboardPage {
        day: day.format("%Y-%m-%d").to_string(),
        day_label: format_day_heading(day),
        prev_day,
        next_day,
        is_today: day == today,
        show: resolved_show,
        final_only,
        choices,
        coverage_warning,
        empty_message,
        groups,
    })
}

fn central_midnight_utc(day: NaiveDate) -> NaiveDateTime {
    let naive_midnight = day.and_hms_opt(0, 0, 0).unwrap();
    Chicago
        .from_local_datetime(&naive_midnight)
        .earliest()
        .or_else(|| Chicago.from_local_datetime(&naive_midnight).latest())
        .map(|dt| dt.with_timezone(&Utc).naive_utc())
        .unwrap_or(naive_midnight)
}

fn map_raw_game(r: tiberius::Row) -> RawScoreboardGame {
    RawScoreboardGame {
        id: r.get::<i64, _>(0).unwrap_or_default(),
        provider: r.get::<&str, _>(1).unwrap_or_default().to_string(),
        sport: r.get::<&str, _>(2).unwrap_or_default().to_string(),
        league: r.get::<&str, _>(3).unwrap_or_default().to_string(),
        event_id: r.get::<&str, _>(4).unwrap_or_default().to_string(),
        scoreboard_date: r.get::<NaiveDate, _>(5).unwrap_or_else(today_central),
        starts_at_utc: r.get::<NaiveDateTime, _>(6),
        time_tbd: r.get::<bool, _>(7).unwrap_or(false),
        home_team_id: r.get::<&str, _>(8).map(str::to_string),
        away_team_id: r.get::<&str, _>(9).map(str::to_string),
        home_name: r.get::<&str, _>(10).unwrap_or_default().to_string(),
        away_name: r.get::<&str, _>(11).unwrap_or_default().to_string(),
        home_logo_url: r.get::<&str, _>(12).map(str::to_string),
        away_logo_url: r.get::<&str, _>(13).map(str::to_string),
        home_score: r.get::<Numeric, _>(14).map(format_numeric),
        away_score: r.get::<Numeric, _>(15).map(format_numeric),
        status: r.get::<&str, _>(16).unwrap_or_default().to_ascii_lowercase(),
        completed: r.get::<bool, _>(17).unwrap_or(false),
        status_text: r.get::<&str, _>(18).map(str::to_string),
        period: r.get::<i32, _>(19),
        display_clock: r.get::<&str, _>(20).map(str::to_string),
        neutral_site: r.get::<bool, _>(21).unwrap_or(false),
        home_shootout: r.get::<i32, _>(22),
        away_shootout: r.get::<i32, _>(23),
        winner_side: r.get::<&str, _>(24).map(|s| s.trim().to_ascii_lowercase()),
        venue: r.get::<&str, _>(25).map(str::to_string),
        broadcasts: r.get::<&str, _>(26).map(str::to_string),
        source_url: r.get::<&str, _>(27).map(str::to_string),
        live_details_json: r.get::<&str, _>(28).map(str::to_string),
        last_observed_at_utc: r.get::<NaiveDateTime, _>(29).unwrap_or_default(),
        is_stale: r.get::<bool, _>(30).unwrap_or(false),
        request_availability: r.get::<&str, _>(31).map(str::to_string),
        home_rank: r.get::<u8, _>(32),
        away_rank: r.get::<u8, _>(33),
    }
}

pub fn format_numeric(n: Numeric) -> String {
    let s = n.to_string();
    if let Some((int_part, frac)) = s.split_once('.') {
        let trimmed = frac.trim_end_matches('0');
        if trimmed.is_empty() {
            int_part.to_string()
        } else {
            format!("{int_part}.{trimmed}")
        }
    } else {
        s
    }
}

pub fn competition_label(sport: &str, league: &str) -> String {
    match league {
        "nfl" => "NFL".into(),
        "mlb" => "MLB".into(),
        "nhl" => "NHL".into(),
        "wnba" => "WNBA".into(),
        "usa.1" => "MLS".into(),
        "usa.nwsl" => "NWSL".into(),
        "eng.1" => "Premier League".into(),
        "eng.2" => "EFL Championship".into(),
        "college-football" => "College football".into(),
        "mens-college-basketball" => "Men's college basketball".into(),
        "usa.ncaa.w.1" => "Women's college soccer".into(),
        "womens-college-volleyball" => "Women's college volleyball".into(),
        _ => format!("{sport} · {league}"),
    }
}

fn short_league_name(sport: &str, league: &str) -> String {
    match (sport, league) {
        ("football", "nfl") => "NFL".into(),
        ("football", "college-football") => "Football".into(),
        ("baseball", "mlb") => "MLB".into(),
        ("basketball", "nba") => "NBA".into(),
        ("basketball", "wnba") => "WNBA".into(),
        ("basketball", "mens-college-basketball") => "Men's Basketball".into(),
        ("volleyball", "womens-college-volleyball") => "Volleyball".into(),
        ("hockey", "nhl") => "NHL".into(),
        ("soccer", "usa.ncaa.w.1") => "Women's Soccer".into(),
        ("soccer", "usa.1") => "MLS".into(),
        ("soccer", "usa.nwsl") => "NWSL".into(),
        ("soccer", "eng.1") => "Premier League".into(),
        _ => league.into(),
    }
}

fn follow_label(f: &FollowRow) -> String {
    if f.kind == "league" {
        f.name.clone()
    } else {
        format!("{} · {}", f.name, short_league_name(&f.sport, &f.league))
    }
}

fn resolve_selection(
    show: &str,
    coverage: &[CoverageRow],
    follows: &[FollowRow],
) -> (Vec<ScoreboardChoice>, String, Selection) {
    let mut choices = vec![
        ScoreboardChoice { value: String::new(), label: "All sports".into(), group: String::new() },
        ScoreboardChoice { value: "top25".into(), label: "Top 25".into(), group: String::new() },
    ];

    // Leagues (deduplicated coverage keys, sorted by sport then league)
    let mut cov_keys: Vec<(String, String)> = coverage
        .iter()
        .map(|c| (c.sport.clone(), c.league.clone()))
        .collect();
    cov_keys.sort();
    cov_keys.dedup();
    for (sport, league) in &cov_keys {
        choices.push(ScoreboardChoice {
            value: format!("c:{sport}|{league}"),
            label: competition_label(sport, league),
            group: "Leagues".into(),
        });
    }

    // Conferences
    let mut conf_by_name: BTreeMap<String, Vec<FollowRow>> = BTreeMap::new();
    for f in follows.iter().filter(|f| f.kind == "conference") {
        conf_by_name.entry(f.name.clone()).or_default().push(f.clone());
    }
    for (name, group) in &conf_by_name {
        if group.len() >= 2 {
            choices.push(ScoreboardChoice {
                value: format!("g:{name}"),
                label: format!("{name} · All sports"),
                group: "Conferences".into(),
            });
        }
        for f in group {
            choices.push(ScoreboardChoice {
                value: format!("f:{}", f.id),
                label: follow_label(f),
                group: "Conferences".into(),
            });
        }
    }

    // Teams
    for f in follows.iter().filter(|f| f.kind == "team") {
        choices.push(ScoreboardChoice {
            value: format!("f:{}", f.id),
            label: follow_label(f),
            group: "Teams".into(),
        });
    }

    let trimmed = show.trim();
    if trimmed.eq_ignore_ascii_case("top25") {
        return (choices, "top25".into(), Selection::Top25);
    }
    if let Some(rest) = trimmed.strip_prefix("c:") {
        if let Some((sport, league)) = rest.split_once('|') {
            if cov_keys.iter().any(|(s, l)| s == sport && l == league) {
                return (
                    choices,
                    trimmed.to_string(),
                    Selection::Competition { sport: sport.to_string(), league: league.to_string() },
                );
            }
        }
    }
    if let Some(id_str) = trimmed.strip_prefix("f:") {
        if let Ok(fid) = id_str.parse::<i32>() {
            if let Some(f) = follows.iter().find(|f| f.id == fid) {
                if f.kind == "league" {
                    let c_val = format!("c:{}|{}", f.sport, f.league);
                    return (
                        choices,
                        c_val,
                        Selection::Competition { sport: f.sport.clone(), league: f.league.clone() },
                    );
                }
                return (choices, format!("f:{fid}"), Selection::Follow(f.clone()));
            }
        }
    }
    if let Some(gname) = trimmed.strip_prefix("g:") {
        let matching: Vec<FollowRow> = follows
            .iter()
            .filter(|f| f.kind == "conference" && f.name.eq_ignore_ascii_case(gname))
            .cloned()
            .collect();
        if !matching.is_empty() {
            let canonical = matching[0].name.clone();
            return (
                choices,
                format!("g:{canonical}"),
                Selection::ConferenceGroup { name: canonical, follows: matching },
            );
        }
    }

    (choices, String::new(), Selection::All)
}

fn matches_follow(g: &RawScoreboardGame, f: &FollowRow, conf_members: &HashMap<i32, HashSet<String>>) -> bool {
    if !g.provider.eq_ignore_ascii_case("espn") || g.sport != f.sport || g.league != f.league {
        return false;
    }
    match f.kind.as_str() {
        "league" => true,
        "team" => {
            let Some(tid) = f.team_id.as_deref().filter(|s| !s.is_empty()) else {
                return false;
            };
            g.home_team_id.as_deref() == Some(tid) || g.away_team_id.as_deref() == Some(tid)
        }
        "conference" => {
            let Some(members) = conf_members.get(&f.id) else {
                return false;
            };
            g.home_team_id.as_deref().is_some_and(|id| members.contains(id))
                || g.away_team_id.as_deref().is_some_and(|id| members.contains(id))
        }
        _ => false,
    }
}

fn matches_selection(
    g: &RawScoreboardGame,
    sel: &Selection,
    conf_members: &HashMap<i32, HashSet<String>>,
) -> bool {
    match sel {
        Selection::Follow(f) => matches_follow(g, f, conf_members),
        Selection::ConferenceGroup { follows, .. } => {
            follows.iter().any(|f| matches_follow(g, f, conf_members))
        }
        _ => true,
    }
}

fn compute_coverage_warning(coverage: &[CoverageRow], sel: &Selection) -> String {
    let relevant: Vec<&CoverageRow> = match sel {
        Selection::Competition { sport, league } => {
            coverage.iter().filter(|c| &c.sport == sport && &c.league == league).collect()
        }
        Selection::Follow(f) => {
            coverage.iter().filter(|c| c.sport == f.sport && c.league == f.league).collect()
        }
        Selection::ConferenceGroup { follows, .. } => coverage
            .iter()
            .filter(|c| follows.iter().any(|f| c.sport == f.sport && c.league == f.league))
            .collect(),
        _ => coverage.iter().collect(),
    };
    let degraded = relevant.iter().any(|c| {
        c.last_success_at.is_none() || matches!(c.availability.as_str(), "error" | "partial")
    });
    if degraded {
        "Some coverage is delayed or unavailable. Showing the latest stored scores.".into()
    } else {
        String::new()
    }
}

fn parse_clock_secs(clock: Option<&str>) -> Option<u32> {
    let s = clock?.trim();
    let (m, sec) = s.split_once(':')?;
    let mins: u32 = m.parse().ok()?;
    let secs: u32 = sec.split('.').next()?.parse().ok()?;
    Some(mins * 60 + secs)
}

fn build_groups(
    games: Vec<RawScoreboardGame>,
    top25_week: Vec<RawScoreboardGame>,
    sel: &Selection,
    today: NaiveDate,
    now_utc: NaiveDateTime,
) -> Vec<ScoreboardGroup> {
    let mut live = Vec::new();
    let mut upcoming = Vec::new();
    let mut final_games = Vec::new();

    for g in games {
        match g.status.as_str() {
            "live" | "delayed" | "suspended" => live.push(g),
            "upcoming" => upcoming.push(g),
            "final" => final_games.push(g),
            _ => {}
        }
    }

    live.sort_by_key(|g| {
        let p = g.period.unwrap_or(0);
        let clock = parse_clock_secs(g.display_clock.as_deref()).unwrap_or(u32::MAX);
        (std::cmp::Reverse(p), clock, g.id)
    });
    upcoming.sort_by_key(|g| (g.starts_at_utc.is_none(), g.starts_at_utc, g.id));
    final_games.sort_by_key(|g| (std::cmp::Reverse(g.last_observed_at_utc), g.id));

    let mut groups = Vec::new();
    if !live.is_empty() {
        let n = live.len();
        groups.push(ScoreboardGroup {
            title: "Live".into(),
            subtitle: format!("{n} {}", if n == 1 { "game" } else { "games" }),
            is_live: true,
            games: live.into_iter().map(|g| format_card(g, today, now_utc)).collect(),
        });
    }
    if !upcoming.is_empty() {
        let n = upcoming.len();
        groups.push(ScoreboardGroup {
            title: "Upcoming".into(),
            subtitle: format!("{n} {}", if n == 1 { "game" } else { "games" }),
            is_live: false,
            games: upcoming.into_iter().map(|g| format_card(g, today, now_utc)).collect(),
        });
    }
    if !final_games.is_empty() {
        let n = final_games.len();
        groups.push(ScoreboardGroup {
            title: "Final".into(),
            subtitle: format!("{n} {}", if n == 1 { "game" } else { "games" }),
            is_live: false,
            games: final_games.into_iter().map(|g| format_card(g, today, now_utc)).collect(),
        });
    }

    if matches!(sel, Selection::Top25) {
        let n = top25_week.len();
        groups.push(ScoreboardGroup {
            title: "Upcoming ranked games this week".into(),
            subtitle: if n == 0 {
                "No remaining ranked games this week.".into()
            } else {
                "Ranked matchups across the country this week (excluding Big 12 games).".into()
            },
            is_live: false,
            games: top25_week.into_iter().map(|g| format_card(g, today, now_utc)).collect(),
        });
    }

    groups
}

fn format_period(p: i32) -> String {
    match p {
        1 => "1st".into(),
        2 => "2nd".into(),
        3 => "3rd".into(),
        4 => "4th".into(),
        n if n >= 5 => format!("OT{}", n - 4),
        _ => String::new(),
    }
}

pub fn parse_live_details(json_str: Option<&str>) -> Vec<String> {
    let Some(raw) = json_str.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    for (key, unit) in [("outs", "outs"), ("balls", "balls"), ("strikes", "strikes")] {
        if let Some(n) = map.get(key).and_then(|v| v.as_i64()) {
            parts.push(format!("{n} {unit}"));
        }
    }
    for (key, label) in [
        ("onFirst", "1st occupied"),
        ("onSecond", "2nd occupied"),
        ("onThird", "3rd occupied"),
    ] {
        if map.get(key).and_then(|v| v.as_bool()) == Some(true) {
            parts.push(label.into());
        }
    }
    for (key, prefix) in [("awayPeriods", "Away periods"), ("homePeriods", "Home periods")] {
        if let Some(arr) = map.get(key).and_then(|v| v.as_array()) {
            let vals: Vec<String> = arr
                .iter()
                .take(40)
                .filter_map(|item| {
                    item.get("displayValue")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or_else(|| item.get("score").and_then(|v| v.as_f64()).map(|f| {
                            if f.fract() == 0.0 {
                                format!("{}", f as i64)
                            } else {
                                f.to_string()
                            }
                        }))
                })
                .collect();
            if !vals.is_empty() {
                parts.push(format!("{prefix}: {}", vals.join(" · ")));
            }
        }
    }
    parts
}

fn format_card(g: RawScoreboardGame, today: NaiveDate, now_utc: NaiveDateTime) -> ScoreboardGame {
    let header_text = match g.status.as_str() {
        "live" | "delayed" | "suspended" => {
            if let Some(clock) = g.display_clock.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                let p = g.period.map(format_period).unwrap_or_default();
                if p.is_empty() {
                    clock.to_string()
                } else {
                    format!("{p} · {clock}")
                }
            } else if let Some(st) = g.status_text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                st.to_string()
            } else {
                "Live".into()
            }
        }
        "upcoming" => {
            let (t, _) = format_event_time(None, g.time_tbd, g.starts_at_utc);
            let time_part = if t == "Time TBD" { "Start time TBD".into() } else { t };
            match g.broadcasts.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(b) => format!("{time_part} · {b}"),
                None => time_part,
            }
        }
        _ => g
            .status_text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Final")
            .to_string(),
    };

    let away_num = g.away_score.as_deref().and_then(|s| s.parse::<f64>().ok());
    let home_num = g.home_score.as_deref().and_then(|s| s.parse::<f64>().ok());

    let (away_winner, home_winner) = if g.status == "final" && g.completed {
        match g.winner_side.as_deref() {
            Some("away") => (true, false),
            Some("home") => (false, true),
            _ => match (away_num, home_num) {
                (Some(a), Some(h)) if a > h => (true, false),
                (Some(a), Some(h)) if h > a => (false, true),
                _ => (false, false),
            },
        }
    } else {
        (false, false)
    };

    let (away_leading, home_leading) = if g.status == "live" {
        match (away_num, home_num) {
            (Some(a), Some(h)) if a > h => (true, false),
            (Some(a), Some(h)) if h > a => (false, true),
            _ => (false, false),
        }
    } else {
        (false, false)
    };

    let mut details = parse_live_details(g.live_details_json.as_deref());
    if g.home_shootout.is_some() || g.away_shootout.is_some() {
        let a = g.away_shootout.map(|n| n.to_string()).unwrap_or_else(|| "—".into());
        let h = g.home_shootout.map(|n| n.to_string()).unwrap_or_else(|| "—".into());
        details.push(format!("Shootout · Away {a} – Home {h}"));
    }
    if g.scoreboard_date < today && matches!(g.status.as_str(), "live" | "delayed" | "suspended") {
        details.push(format!("Continuing from {}", format_day_heading(g.scoreboard_date)));
    }
    if let Some(v) = g.venue.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if g.neutral_site {
            details.push(format!("{v} · Neutral site"));
        } else {
            details.push(v.to_string());
        }
    }
    if let Some(b) = g.broadcasts.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        details.push(b.to_string());
    }

    let is_live_like = matches!(g.status.as_str(), "live" | "delayed" | "suspended");
    let stale_age = now_utc.signed_duration_since(g.last_observed_at_utc) > Duration::minutes(25);
    let mut warnings = Vec::new();
    if g.is_stale || (is_live_like && stale_age) {
        warnings.push(format!(
            "Scores may be outdated — last updated {}",
            format_last_updated(Some(g.last_observed_at_utc))
        ));
    }
    match g.request_availability.as_deref() {
        Some("error") => warnings.push("Latest collection is unavailable; showing the latest stored sample.".into()),
        Some("partial") => warnings.push("Latest collection is incomplete; showing the latest stored sample.".into()),
        _ => {}
    }

    ScoreboardGame {
        key: format!("{}|{}|{}|{}", g.provider, g.sport, g.league, g.event_id),
        league_label: competition_label(&g.sport, &g.league),
        status: g.status,
        header_text,
        away_initials: team_initials(&g.away_name),
        away_logo_url: safe_logo_url(g.away_logo_url.as_deref()),
        away_rank: g.away_rank.filter(|&r| (1..=25).contains(&r)),
        away_name: g.away_name,
        away_score: g.away_score.unwrap_or_default(),
        away_winner,
        away_leading,
        home_initials: team_initials(&g.home_name),
        home_logo_url: safe_logo_url(g.home_logo_url.as_deref()),
        home_rank: g.home_rank.filter(|&r| (1..=25).contains(&r)),
        home_name: g.home_name,
        home_score: g.home_score.unwrap_or_default(),
        home_winner,
        home_leading,
        details_text: details.join(" · "),
        warning_text: warnings.join(" · "),
        source_url: safe_url(g.source_url.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_details_parses_baseball_and_periods() {
        let json = r#"{"outs":1,"balls":2,"strikes":1,"onFirst":true,"onThird":true,"awayPeriods":[{"displayValue":"21","period":1,"score":21.0},{"displayValue":"20"}],"homePeriods":[{"score":14.0}]}"#;
        assert_eq!(
            parse_live_details(Some(json)),
            vec![
                "1 outs",
                "2 balls",
                "1 strikes",
                "1st occupied",
                "3rd occupied",
                "Away periods: 21 · 20",
                "Home periods: 14",
            ]
        );
    }

    #[test]
    fn selection_resolves_league_follow_to_competition() {
        let cov = vec![CoverageRow {
            sport: "hockey".into(),
            league: "nhl".into(),
            last_success_at: None,
            availability: "available".into(),
        }];
        let follows = vec![FollowRow {
            id: 5,
            kind: "league".into(),
            sport: "hockey".into(),
            league: "nhl".into(),
            team_id: None,
            name: "NHL".into(),
        }];
        let (_, resolved, sel) = resolve_selection("f:5", &cov, &follows);
        assert_eq!(resolved, "c:hockey|nhl");
        assert!(matches!(sel, Selection::Competition { .. }));
    }
}
