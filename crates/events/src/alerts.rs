//! Read-only SQL Server queries for Sports Alerts (`dbo.ResultFollows` and `dbo.SportsScoreboard`).

use chrono::{DateTime, NaiveDateTime, Utc};
use tiberius::ToSql;
use tvdash_core::config::EventsConfig;

use crate::{
    EventsError, connect,
    format::safe_logo_url,
    format::safe_url,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowedTeamRow {
    pub league: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct FollowFetchResult {
    pub teams: Vec<FollowedTeamRow>,
    pub unmapped_count: usize,
}

#[derive(Debug, Clone)]
pub struct AlertGameRow {
    pub event_id: String,
    pub starts_at_utc: Option<NaiveDateTime>,
    pub time_tbd: bool,
    pub status: String,
    pub status_text: String,
    pub league: String,
    pub sport: String,
    pub home_team_id: String,
    pub home_name: String,
    pub home_score: Option<i32>,
    pub home_logo_url: Option<String>,
    pub away_team_id: String,
    pub away_name: String,
    pub away_score: Option<i32>,
    pub away_logo_url: Option<String>,
    pub neutral_site: bool,
    pub venue: String,
    pub broadcasts: Option<String>,
    pub source_url: Option<String>,
    pub last_observed_at_utc: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Default)]
pub struct AlertGamesResult {
    pub games: Vec<AlertGameRow>,
    pub max_observed_at_utc: Option<NaiveDateTime>,
}

/// Read enabled team follows from `dbo.ResultFollows` (read-only).
pub async fn fetch_followed_teams(
    cfg: &EventsConfig,
    password: &str,
) -> Result<FollowFetchResult, EventsError> {
    if password.trim().is_empty() {
        return Err(EventsError::MissingPassword);
    }

    let mut client = connect(cfg, password).await?;
    let rows = client
        .query(
            r#"
SELECT f.League, f.TeamId, f.Name
FROM dbo.ResultFollows f
WHERE f.Enabled = 1 AND f.Kind = 'team'
ORDER BY f.Name, f.League
"#,
            &[],
        )
        .await?
        .into_first_result()
        .await?;

    let mut out = FollowFetchResult::default();
    for r in rows {
        let league = r.get::<&str, _>(0).unwrap_or_default().trim().to_string();
        let team_id = r.get::<&str, _>(1).unwrap_or_default().trim().to_string();
        let name = r.get::<&str, _>(2).unwrap_or_default().trim().to_string();
        if league.is_empty() || team_id.is_empty() {
            out.unmapped_count += 1;
            continue;
        }
        out.teams.push(FollowedTeamRow {
            league,
            id: team_id,
            name,
        });
    }
    Ok(out)
}

/// Build the parameterized `WHERE` clause and parameter list for `fetch_alert_games`.
///
/// Exposed for unit testing of SQL parameter indexing.
fn build_alert_games_sql(team_count: usize, starred_count: usize) -> String {
    let mut or_parts = Vec::new();
    let mut idx = 3usize; // @P1 = from_utc, @P2 = to_utc
    for _ in 0..team_count {
        let p_league = idx;
        let p_team = idx + 1;
        idx += 2;
        or_parts.push(format!(
            "(League = @P{p_league} AND (HomeTeamId = @P{p_team} OR AwayTeamId = @P{p_team}))"
        ));
    }
    if starred_count > 0 {
        let placeholders: Vec<String> = (0..starred_count)
            .map(|offset| format!("@P{}", idx + offset))
            .collect();
        or_parts.push(format!("EventId IN ({})", placeholders.join(", ")));
    }
    let filter = or_parts.join(" OR ");
    format!(
        r#"
SELECT EventId, StartsAtUtc, TimeTbd, Status, StatusText,
       League, Sport, HomeTeamId, HomeName, HomeScore,
       AwayTeamId, AwayName, AwayScore, NeutralSite,
       Venue, Broadcasts, SourceUrl, LastObservedAtUtc,
       HomeLogoUrl, AwayLogoUrl
FROM dbo.SportsScoreboard
WHERE ({filter})
  AND (StartsAtUtc IS NULL OR StartsAtUtc BETWEEN @P1 AND @P2)
"#
    )
}

fn parse_score(raw: Option<&str>) -> Option<i32> {
    raw.and_then(|s| s.trim().parse::<i32>().ok())
}

/// Fetch `dbo.SportsScoreboard` rows for followed teams and starred games in `[from_utc, to_utc]`.
pub async fn fetch_alert_games(
    cfg: &EventsConfig,
    password: &str,
    teams: &[(String, String)],
    starred: &[String],
    from_utc: DateTime<Utc>,
    to_utc: DateTime<Utc>,
) -> Result<AlertGamesResult, EventsError> {
    if teams.is_empty() && starred.is_empty() {
        return Ok(AlertGamesResult::default());
    }
    if password.trim().is_empty() {
        return Err(EventsError::MissingPassword);
    }

    let sql = build_alert_games_sql(teams.len(), starred.len());
    let from_naive = from_utc.naive_utc();
    let to_naive = to_utc.naive_utc();

    let mut params: Vec<&dyn ToSql> = Vec::with_capacity(2 + teams.len() * 2 + starred.len());
    params.push(&from_naive);
    params.push(&to_naive);
    for (league, team_id) in teams {
        params.push(league);
        params.push(team_id);
    }
    for eid in starred {
        params.push(eid);
    }

    let mut client = connect(cfg, password).await?;
    let rows = client.query(&sql, &params).await?.into_first_result().await?;

    let mut games = Vec::with_capacity(rows.len());
    let mut max_obs: Option<NaiveDateTime> = None;

    for r in rows {
        let obs = r.get::<NaiveDateTime, _>(17);
        if let Some(dt) = obs {
            if max_obs.is_none_or(|cur| dt > cur) {
                max_obs = Some(dt);
            }
        }
        games.push(AlertGameRow {
            event_id: r.get::<&str, _>(0).unwrap_or_default().to_string(),
            starts_at_utc: r.get::<NaiveDateTime, _>(1),
            time_tbd: r.get::<bool, _>(2).unwrap_or(false),
            status: r.get::<&str, _>(3).unwrap_or_default().to_string(),
            status_text: r.get::<&str, _>(4).unwrap_or_default().to_string(),
            league: r.get::<&str, _>(5).unwrap_or_default().to_string(),
            sport: r.get::<&str, _>(6).unwrap_or_default().to_string(),
            home_team_id: r.get::<&str, _>(7).unwrap_or_default().to_string(),
            home_name: r.get::<&str, _>(8).unwrap_or_default().to_string(),
            home_score: parse_score(r.get::<&str, _>(9)),
            away_team_id: r.get::<&str, _>(10).unwrap_or_default().to_string(),
            away_name: r.get::<&str, _>(11).unwrap_or_default().to_string(),
            away_score: parse_score(r.get::<&str, _>(12)),
            neutral_site: r.get::<bool, _>(13).unwrap_or(false),
            venue: r.get::<&str, _>(14).unwrap_or_default().to_string(),
            broadcasts: r.get::<&str, _>(15).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            source_url: safe_url(r.get::<&str, _>(16)),
            last_observed_at_utc: obs,
            home_logo_url: safe_logo_url(r.get::<&str, _>(18)),
            away_logo_url: safe_logo_url(r.get::<&str, _>(19)),
        });
    }

    Ok(AlertGamesResult {
        games,
        max_observed_at_utc: max_obs,
    })
}

/// Scoreboard row for Home's Next Game card (§5.2).
#[derive(Debug, Clone)]
pub struct NextGameRow {
    pub event_id: String,
    pub scoreboard_date: chrono::NaiveDate,
    pub starts_at_utc: Option<NaiveDateTime>,
    pub time_tbd: bool,
    pub status: String,
    pub status_text: String,
    pub period: Option<i32>,
    pub display_clock: Option<String>,
    pub league: String,
    pub home_team_id: String,
    pub home_name: String,
    pub home_score: Option<i32>,
    pub home_logo_url: Option<String>,
    pub home_rank: Option<u8>,
    pub away_team_id: String,
    pub away_name: String,
    pub away_score: Option<i32>,
    pub away_logo_url: Option<String>,
    pub away_rank: Option<u8>,
    pub venue: String,
    pub broadcasts: Option<String>,
}

/// Fetch all `dbo.SportsScoreboard` games in `[from_utc, to_utc]` for Home's Next Game card.
pub async fn fetch_window_games(
    cfg: &EventsConfig,
    password: &str,
    from_utc: DateTime<Utc>,
    to_utc: DateTime<Utc>,
) -> Result<Vec<NextGameRow>, EventsError> {
    if password.trim().is_empty() {
        return Err(EventsError::MissingPassword);
    }
    let from_naive = from_utc.naive_utc();
    let to_naive = to_utc.naive_utc();
    let mut client = connect(cfg, password).await?;
    let rows = client
        .query(
            r#"
SELECT EventId, ScoreboardDate, StartsAtUtc, TimeTbd, Status, StatusText,
       Period, DisplayClock, League,
       HomeTeamId, HomeName, HomeScore, HomeLogoUrl, HomeRank,
       AwayTeamId, AwayName, AwayScore, AwayLogoUrl, AwayRank,
       Venue, Broadcasts
FROM dbo.SportsScoreboard
WHERE StartsAtUtc BETWEEN @P1 AND @P2
"#,
            &[&from_naive, &to_naive],
        )
        .await?
        .into_first_result()
        .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let Some(date) = r.get::<chrono::NaiveDate, _>(1) else { continue };
        out.push(NextGameRow {
            event_id: r.get::<&str, _>(0).unwrap_or_default().to_string(),
            scoreboard_date: date,
            starts_at_utc: r.get::<NaiveDateTime, _>(2),
            time_tbd: r.get::<bool, _>(3).unwrap_or(false),
            status: r.get::<&str, _>(4).unwrap_or_default().to_string(),
            status_text: r.get::<&str, _>(5).unwrap_or_default().to_string(),
            period: r.get::<i32, _>(6),
            display_clock: r.get::<&str, _>(7).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            league: r.get::<&str, _>(8).unwrap_or_default().to_string(),
            home_team_id: r.get::<&str, _>(9).unwrap_or_default().to_string(),
            home_name: r.get::<&str, _>(10).unwrap_or_default().to_string(),
            home_score: parse_score(r.get::<&str, _>(11)),
            home_logo_url: safe_logo_url(r.get::<&str, _>(12)),
            home_rank: r.get::<u8, _>(13).filter(|&rk| (1..=25).contains(&rk)),
            away_team_id: r.get::<&str, _>(14).unwrap_or_default().to_string(),
            away_name: r.get::<&str, _>(15).unwrap_or_default().to_string(),
            away_score: parse_score(r.get::<&str, _>(16)),
            away_logo_url: safe_logo_url(r.get::<&str, _>(17)),
            away_rank: r.get::<u8, _>(18).filter(|&rk| (1..=25).contains(&rk)),
            venue: r.get::<&str, _>(19).unwrap_or_default().trim().to_string(),
            broadcasts: r.get::<&str, _>(20).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_builds_parameterized_where_clause() {
        let sql = build_alert_games_sql(2, 2);
        assert!(sql.contains("(League = @P3 AND (HomeTeamId = @P4 OR AwayTeamId = @P4))"));
        assert!(sql.contains("(League = @P5 AND (HomeTeamId = @P6 OR AwayTeamId = @P6))"));
        assert!(sql.contains("EventId IN (@P7, @P8)"));
        assert!(sql.contains("StartsAtUtc BETWEEN @P1 AND @P2"));
    }
}
