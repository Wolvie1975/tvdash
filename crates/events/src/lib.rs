//! Read-only SQL Server client for the Sports / Competitions page.

pub mod alerts;
pub mod format;
pub mod scoreboard;

use chrono::{NaiveDate, NaiveDateTime};
use thiserror::Error;
use tiberius::{AuthMethod, Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tvdash_core::{config::EventsConfig, models::SportsEventsPage};

use crate::format::{
    RawGameRow, build_schedule_choices, format_last_updated, group_games, today_central,
};

pub const PAGE_SIZE: u32 = 200;

#[derive(Debug, Error)]
pub enum EventsError {
    #[error("SQL Server password is not set (configure it in Settings → Accounts or --set-secret events_sql_password)")]
    MissingPassword,
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("query failed: {0}")]
    Query(#[from] tiberius::error::Error),
}

const UPCOMING_FROM_WHERE: &str = r#"
FROM dbo.SportsEvents e
LEFT JOIN dbo.SportsEventsType t ON t.ID = e.SportsEventsTypeId
WHERE e.EventDate BETWEEN @P1 AND DATEADD(day, 3, @P1)
  AND (e.Sport IS NULL OR e.Sport <> N'Women''s Basketball')
  AND (e.ScheduleStatus IS NULL OR (
        e.ScheduleStatus NOT IN ('STATUS_CANCELED','STATUS_CANCELLED','STATUS_UNNECESSARY','STATUS_FULL_TIME')
        AND e.ScheduleStatus NOT LIKE 'STATUS[_]FINAL%'))
  AND (
        e.StartsAtUtc IS NULL
     OR e.StartsAtUtc >= DATEADD(hour, -4, SYSUTCDATETIME())
     OR e.ScheduleStatus IN ('STATUS_IN_PROGRESS','STATUS_HALFTIME','STATUS_END_PERIOD','STATUS_DELAYED','STATUS_RAIN_DELAY')
      )
  AND (
        (e.ProviderKey IN ('espn-nwsl','espn-wnba','espn-mlb','espn-championship','espn-epl','espn-mls')
          AND t.Enabled = 1 AND t.ProviderKey = e.ProviderKey
          AND (   (e.ProviderKey = 'espn-nwsl' AND t.ProviderSchoolId = 20907)
               OR  e.ProviderKey IN ('espn-championship','espn-epl','espn-mls')
               OR (e.ProviderKey IN ('espn-wnba','espn-mlb') AND t.ProviderSchoolId = 0)))
     OR
        ((e.ProviderKey IS NULL OR e.ProviderKey NOT IN ('espn-nwsl','espn-wnba','espn-mlb','espn-championship','espn-epl','espn-mls'))
          AND (EXISTS (SELECT 1 FROM dbo.SportsEventSources s
                       JOIN dbo.SportsEventsType f ON f.ID = s.SportsEventsTypeId
                       WHERE s.SportsEventId = e.ID AND f.Enabled = 1)
               OR (NOT EXISTS (SELECT 1 FROM dbo.SportsEventSources s WHERE s.SportsEventId = e.ID)
                   AND (t.ID IS NULL OR t.Enabled = 1))))
      )
"#;

const CHOICES_FROM_WHERE: &str = r#"
FROM dbo.SportsEvents e
LEFT JOIN dbo.SportsEventsType t ON t.ID = e.SportsEventsTypeId
WHERE e.EventDate >= @P1
  AND (e.Sport IS NULL OR e.Sport <> N'Women''s Basketball')
  AND (e.ScheduleStatus IS NULL OR (
        e.ScheduleStatus NOT IN ('STATUS_CANCELED','STATUS_CANCELLED','STATUS_UNNECESSARY','STATUS_FULL_TIME')
        AND e.ScheduleStatus NOT LIKE 'STATUS[_]FINAL%'))
  AND (
        e.StartsAtUtc IS NULL
     OR e.StartsAtUtc >= DATEADD(hour, -4, SYSUTCDATETIME())
     OR e.ScheduleStatus IN ('STATUS_IN_PROGRESS','STATUS_HALFTIME','STATUS_END_PERIOD','STATUS_DELAYED','STATUS_RAIN_DELAY')
      )
  AND (
        (e.ProviderKey IN ('espn-nwsl','espn-wnba','espn-mlb','espn-championship','espn-epl','espn-mls')
          AND t.Enabled = 1 AND t.ProviderKey = e.ProviderKey
          AND (   (e.ProviderKey = 'espn-nwsl' AND t.ProviderSchoolId = 20907)
               OR  e.ProviderKey IN ('espn-championship','espn-epl','espn-mls')
               OR (e.ProviderKey IN ('espn-wnba','espn-mlb') AND t.ProviderSchoolId = 0)))
     OR
        ((e.ProviderKey IS NULL OR e.ProviderKey NOT IN ('espn-nwsl','espn-wnba','espn-mlb','espn-championship','espn-epl','espn-mls'))
          AND (EXISTS (SELECT 1 FROM dbo.SportsEventSources s
                       JOIN dbo.SportsEventsType f ON f.ID = s.SportsEventsTypeId
                       WHERE s.SportsEventId = e.ID AND f.Enabled = 1)
               OR (NOT EXISTS (SELECT 1 FROM dbo.SportsEventSources s WHERE s.SportsEventId = e.ID)
                   AND (t.ID IS NULL OR t.Enabled = 1))))
      )
"#;

async fn connect(cfg: &EventsConfig, password: &str) -> Result<Client<Compat<TcpStream>>, EventsError> {
    let mut tcfg = Config::new();
    tcfg.host(cfg.host.trim());
    tcfg.port(cfg.port);
    tcfg.database(cfg.database.trim());
    tcfg.authentication(AuthMethod::sql_server(cfg.user.trim(), password));
    if cfg.trust_cert {
        tcfg.trust_cert();
    }

    let addr = format!("{}:{}", cfg.host.trim(), cfg.port);
    let tcp = tokio::time::timeout(std::time::Duration::from_secs(10), TcpStream::connect(&addr))
        .await
        .map_err(|_| EventsError::Connect(format!("timed out connecting to {addr}")))?
        .map_err(|e| EventsError::Connect(format!("{addr}: {e}")))?;
    tcp.set_nodelay(true).ok();

    Client::connect(tcfg, tcp.compat_write())
        .await
        .map_err(|e| EventsError::Connect(e.to_string()))
}

/// Fetch one page of upcoming games, schedule choices, and last-updated timestamp.
pub async fn fetch_page(
    cfg: &EventsConfig,
    password: &str,
    schedule: &str,
    page: u32,
) -> Result<SportsEventsPage, EventsError> {
    if password.trim().is_empty() {
        return Err(EventsError::MissingPassword);
    }

    let mut client = connect(cfg, password).await?;
    let today: NaiveDate = today_central();

    // 1. Schedule last updated (unfiltered across all rows).
    let last_seen_row = client
        .query("SELECT MAX(LastSeenAt) FROM dbo.SportsEvents", &[])
        .await?
        .into_row()
        .await?;
    let last_seen: Option<NaiveDateTime> = last_seen_row.and_then(|r| r.get::<NaiveDateTime, _>(0));

    // 2. Schedule dropdown choices.
    let choices_sql = format!(
        "SELECT DISTINCT COALESCE(e.ProviderKey, t.ProviderKey) AS Provider, e.Sport {CHOICES_FROM_WHERE}"
    );
    let choice_rows = client.query(&choices_sql, &[&today]).await?.into_first_result().await?;
    let pairs: Vec<(Option<String>, Option<String>)> = choice_rows
        .into_iter()
        .map(|r| (r.get::<&str, _>(0).map(str::to_string), r.get::<&str, _>(1).map(str::to_string)))
        .collect();
    let choices = build_schedule_choices(&pairs);

    // Validate requested schedule against available choices; ignore unknown values.
    let active_schedule = if !schedule.is_empty() && choices.iter().any(|c| c.value == schedule) {
        schedule.to_string()
    } else {
        String::new()
    };

    let (filter_clause, prov_param, sport_param) = parse_schedule_filter(&active_schedule);

    // 3. Total count for paging.
    let count_sql = format!("SELECT COUNT(*) {UPCOMING_FROM_WHERE} {filter_clause}");
    let count_row = match (prov_param.as_deref(), sport_param.as_deref()) {
        (Some(p), Some(s)) => client.query(&count_sql, &[&today, &p, &s]).await?.into_row().await?,
        (Some(p), None) => client.query(&count_sql, &[&today, &p]).await?.into_row().await?,
        _ => client.query(&count_sql, &[&today]).await?.into_row().await?,
    };
    let total_count = count_row.and_then(|r| r.get::<i32, _>(0)).unwrap_or(0).max(0) as u32;
    let total_pages = total_count.div_ceil(PAGE_SIZE).max(1);
    let clamped_page = page.min(10_000).min(total_pages - 1);
    let offset = (clamped_page * PAGE_SIZE) as i64;

    // 4. Upcoming games page.
    let games_sql = format!(
        r#"
SELECT e.ID, e.Url, e.Title, e.Sport, e.Opponent,
       CASE WHEN e.ProviderKey IN ('espn-championship','espn-epl','espn-mls') THEN CAST(0 AS bit) ELSE e.IsAway END AS IsAway,
       e.Location, e.EventDate, e.StartsAtUtc, e.TimeTbd, e.Tv, e.StreamUrl, e.LiveStatsUrl,
       e.TeamLogoUrl, e.OpponentLogoUrl,
       COALESCE(e.SchoolName, t.SchoolName)   AS SchoolName,
       e.IsNeutral, e.Radio, e.AudioUrl, e.ScheduleStatus,
       COALESCE(e.ProviderKey, t.ProviderKey) AS ProviderKey
{UPCOMING_FROM_WHERE}
{filter_clause}
ORDER BY e.EventDate,
         CASE WHEN e.StartsAtUtc IS NULL THEN 1 ELSE 0 END, e.StartsAtUtc,
         e.Sport, e.ID
OFFSET @P{offset_param} ROWS FETCH NEXT 200 ROWS ONLY
"#,
        offset_param = match (prov_param.is_some(), sport_param.is_some()) {
            (true, true) => 4,
            (true, false) => 3,
            _ => 2,
        }
    );

    let rows = match (prov_param.as_deref(), sport_param.as_deref()) {
        (Some(p), Some(s)) => client.query(&games_sql, &[&today, &p, &s, &offset]).await?.into_first_result().await?,
        (Some(p), None) => client.query(&games_sql, &[&today, &p, &offset]).await?.into_first_result().await?,
        _ => client.query(&games_sql, &[&today, &offset]).await?.into_first_result().await?,
    };

    let raw_games: Vec<RawGameRow> = rows
        .into_iter()
        .map(|r| RawGameRow {
            id: r.get::<i32, _>(0).unwrap_or_default(),
            url: r.get::<&str, _>(1).unwrap_or_default().to_string(),
            title: r.get::<&str, _>(2).unwrap_or_default().to_string(),
            sport: r.get::<&str, _>(3).map(str::to_string),
            opponent: r.get::<&str, _>(4).map(str::to_string),
            is_away: r.get::<bool, _>(5),
            location: r.get::<&str, _>(6).map(str::to_string),
            event_date: r.get::<NaiveDate, _>(7).unwrap_or(today),
            starts_at_utc: r.get::<NaiveDateTime, _>(8),
            time_tbd: r.get::<bool, _>(9).unwrap_or(false),
            tv: r.get::<&str, _>(10).map(str::to_string),
            stream_url: r.get::<&str, _>(11).map(str::to_string),
            live_stats_url: r.get::<&str, _>(12).map(str::to_string),
            team_logo_url: r.get::<&str, _>(13).map(str::to_string),
            opponent_logo_url: r.get::<&str, _>(14).map(str::to_string),
            school_name: r.get::<&str, _>(15).map(str::to_string),
            is_neutral: r.get::<bool, _>(16),
            radio: r.get::<&str, _>(17).map(str::to_string),
            audio_url: r.get::<&str, _>(18).map(str::to_string),
            schedule_status: r.get::<&str, _>(19).map(str::to_string),
            provider_key: r.get::<&str, _>(20).map(str::to_string),
        })
        .collect();

    Ok(SportsEventsPage {
        schedule: active_schedule,
        choices,
        total_count,
        page: clamped_page,
        total_pages,
        last_updated: format_last_updated(last_seen),
        days: group_games(raw_games),
    })
}

fn parse_schedule_filter(schedule: &str) -> (&'static str, Option<String>, Option<String>) {
    if schedule.is_empty() {
        return ("", None, None);
    }
    let Some((prov, sport)) = schedule.split_once('|') else {
        return ("", None, None);
    };
    if sport.is_empty() {
        (
            "AND COALESCE(e.ProviderKey, t.ProviderKey, '') = @P2",
            Some(prov.to_string()),
            None,
        )
    } else {
        (
            "AND COALESCE(e.ProviderKey, t.ProviderKey, '') = @P2 AND e.Sport = @P3",
            Some(prov.to_string()),
            Some(sport.to_string()),
        )
    }
}
