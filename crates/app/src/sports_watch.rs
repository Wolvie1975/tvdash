//! Background Sports Alerts watcher:
//! - Refreshes followed teams from `dbo.ResultFollows` every 10 min (plus startup & Settings › Sports)
//! - Polls `dbo.SportsScoreboard` for followed/starred games (2 min when live/imminent, else 10 min; backoff on error)
//! - Evaluates alert rules every 30 s against cached games (clock-driven Starting soon)
//! - Persists fired keys (`sports-alerts.json`) and local starred games (`sports-starred.json`)

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Timelike, Utc};
use chrono_tz::America::Chicago;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, warn};
use tvdash_core::{
    AppEvent, SecretKey,
    config::{EventsConfig, SportsAlertsConfig},
    events::{
        FollowedTeamInfo, Service, SportsAlertDto, SportsAlertSideDto, SportsAlertsStatusSnapshot,
        StarredGameInfo,
    },
    secrets,
    sports_store::{self, FiredAlerts, StarredGame},
};
use tvdash_events::{
    alerts::{AlertGameRow, fetch_alert_games},
    format::team_initials,
};

use crate::{
    sports_alerts::{
        Alert, AlertKind, Game, GameStatus, Reason, Rules, Side, evaluate, seed_initial_finals,
    },
    sports_follows::{FollowsState, league_display_name},
};

const FOLLOW_REFRESH_SECS: u64 = 10 * 60;
const FETCH_NORMAL_SECS: u64 = 10 * 60;
const FETCH_ACTIVE_SECS: u64 = 2 * 60;
const EVAL_TICK_SECS: u64 = 30;
const FINAL_GRACE_SECS: i64 = 30 * 60;
const STALE_WARN_MINS: i64 = 25;

#[derive(Debug)]
pub enum WatcherMsg {
    UpdateEventsConfig(EventsConfig),
    UpdateAlertsConfig(SportsAlertsConfig),
    SetHeld(bool),
    RefreshFollows,
    AlertShown(String),
    ToggleStar(String),
    Unstar(String),
    EnqueueTest(String),
}

pub fn spawn(
    events_cfg: EventsConfig,
    alerts_cfg: SportsAlertsConfig,
    app_events: UnboundedSender<AppEvent>,
) -> UnboundedSender<WatcherMsg> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(run(events_cfg, alerts_cfg, app_events, rx));
    tx
}

struct Watcher {
    events_cfg: EventsConfig,
    alerts_cfg: SportsAlertsConfig,
    app_events: UnboundedSender<AppEvent>,
    follows: FollowsState,
    starred: Vec<StarredGame>,
    starred_path: Option<PathBuf>,
    fired: FiredAlerts,
    fired_path: Option<PathBuf>,
    cached_games: Vec<Game>,
    cached_rows: Vec<AlertGameRow>,
    final_seen_at: HashMap<String, i64>,
    first_fetch_done: bool,
    held: bool,
    last_follow_refresh: i64,
    last_fetch_at: Option<i64>,
    last_fetch_error: Option<String>,
    err_streak: u32,
    next_fetch_at: i64,
    stale_minutes: Option<i64>,
    last_next_game: Option<(tvdash_core::events::NextGameDto, i64)>,
}

async fn run(
    events_cfg: EventsConfig,
    alerts_cfg: SportsAlertsConfig,
    app_events: UnboundedSender<AppEvent>,
    mut rx: UnboundedReceiver<WatcherMsg>,
) {
    let now = Utc::now().timestamp();
    let starred_path = sports_store::default_starred_path();
    let starred = starred_path
        .as_deref()
        .map(sports_store::load_starred)
        .unwrap_or_default();
    let fired_path = sports_store::default_fired_path();
    let fired = fired_path
        .as_deref()
        .map(|p| sports_store::load_fired(p, now))
        .unwrap_or_default();

    let mut w = Watcher {
        events_cfg,
        alerts_cfg,
        app_events,
        follows: FollowsState::load_cached(),
        starred,
        starred_path,
        fired,
        fired_path,
        cached_games: Vec::new(),
        cached_rows: Vec::new(),
        final_seen_at: HashMap::new(),
        first_fetch_done: false,
        held: false,
        last_follow_refresh: 0,
        last_fetch_at: None,
        last_fetch_error: None,
        err_streak: 0,
        next_fetch_at: 0,
        stale_minutes: None,
        last_next_game: None,
    };

    // Initial follow refresh + game fetch.
    w.refresh_follows().await;
    w.fetch_and_eval().await;
    w.emit_status();

    let mut tick = tokio::time::interval(Duration::from_secs(EVAL_TICK_SECS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await; // consume immediate first tick

    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                match msg {
                    WatcherMsg::UpdateEventsConfig(cfg) => {
                        w.events_cfg = cfg;
                        w.next_fetch_at = 0;
                        w.refresh_follows().await;
                        w.fetch_and_eval().await;
                        w.emit_status();
                    }
                    WatcherMsg::UpdateAlertsConfig(cfg) => {
                        w.alerts_cfg = cfg;
                        w.eval_cached();
                        w.emit_status();
                    }
                    WatcherMsg::SetHeld(held) => {
                        if w.held != held {
                            w.held = held;
                            w.emit_status();
                        }
                    }
                    WatcherMsg::RefreshFollows => {
                        w.refresh_follows().await;
                        w.fetch_and_eval().await;
                        w.emit_status();
                    }
                    WatcherMsg::AlertShown(key) => {
                        let ts = Utc::now().timestamp();
                        w.fired.keys.insert(key, ts);
                        if let Some(p) = &w.fired_path {
                            if let Err(e) = sports_store::save_fired(p, &w.fired) {
                                warn!(error = %e, "failed to save sports-alerts.json");
                            }
                        }
                    }
                    WatcherMsg::ToggleStar(event_id) => {
                        w.toggle_star(&event_id);
                        w.emit_status();
                        w.fetch_and_eval().await;
                        w.emit_status();
                    }
                    WatcherMsg::Unstar(event_id) => {
                        w.unstar(&event_id);
                        w.emit_status();
                    }
                    WatcherMsg::EnqueueTest(kind) => {
                        let dto = sample_alert(&kind);
                        let _ = w.app_events.send(AppEvent::SportsAlerts(vec![dto]));
                    }
                }
            }
            _ = tick.tick() => {
                let now = Utc::now().timestamp();
                if now - w.last_follow_refresh >= FOLLOW_REFRESH_SECS as i64 {
                    w.refresh_follows().await;
                }
                if now >= w.next_fetch_at {
                    w.fetch_and_eval().await;
                } else {
                    w.eval_cached();
                }
                w.emit_status();
            }
            else => break,
        }
    }
}

impl Watcher {
    async fn read_password() -> String {
        tokio::task::spawn_blocking(|| secrets::get(SecretKey::EventsSqlPassword))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .unwrap_or_default()
    }

    async fn refresh_follows(&mut self) {
        let pw = Self::read_password().await;
        if pw.trim().is_empty() {
            return;
        }
        self.follows.refresh(&self.events_cfg, &pw).await;
        self.last_follow_refresh = Utc::now().timestamp();
    }

    async fn fetch_and_eval(&mut self) {
        let now = Utc::now().timestamp();
        self.prune_starred(now);

        let pw = Self::read_password().await;
        if pw.trim().is_empty() {
            self.next_fetch_at = now + FETCH_NORMAL_SECS as i64;
            return;
        }

        let teams: Vec<(String, String)> = self
            .follows
            .teams
            .iter()
            .map(|t| (t.league.clone(), t.id.clone()))
            .collect();
        let starred_ids: Vec<String> = self.starred.iter().map(|s| s.event_id.clone()).collect();
        let now_utc = Utc::now();

        if !self.alerts_cfg.enabled || (self.follows.teams.is_empty() && self.starred.is_empty()) {
            self.cached_games.clear();
            self.cached_rows.clear();
            self.last_fetch_error = None;
            self.err_streak = 0;
            self.stale_minutes = None;
            self.next_fetch_at = now + FETCH_NORMAL_SECS as i64;
        } else {
            let from_utc = now_utc - ChronoDuration::hours(12);
            let lookahead_mins = self.alerts_cfg.soon_minutes as i64 + 30;
            let to_utc = now_utc + ChronoDuration::minutes(lookahead_mins);

            match fetch_alert_games(&self.events_cfg, &pw, &teams, &starred_ids, from_utc, to_utc).await {
                Ok(res) => {
                    self.last_fetch_at = Some(now);
                    self.last_fetch_error = None;
                    self.err_streak = 0;
                    self.stale_minutes = res.max_observed_at_utc.and_then(|obs| {
                        let obs_ts = DateTime::<Utc>::from_naive_utc_and_offset(obs, Utc).timestamp();
                        let mins = (now - obs_ts) / 60;
                        (mins >= STALE_WARN_MINS).then_some(mins)
                    });
                    self.follows.enrich_names_from_games(&res.games);
                    self.cached_rows = res.games.clone();
                    self.cached_games = res.games.into_iter().map(row_to_game).collect();

                    if !self.first_fetch_done {
                        let rules = self.rules();
                        seed_initial_finals(&self.cached_games, &mut self.final_seen_at, &rules, now);
                        self.first_fetch_done = true;
                    }

                    let interval = if self.has_active_or_imminent_game(now) {
                        FETCH_ACTIVE_SECS
                    } else {
                        FETCH_NORMAL_SECS
                    };
                    self.next_fetch_at = now + interval as i64;

                    let health_res = if let Some(mins) = self.stale_minutes {
                        Err(format!("Scoreboard data is {mins} min old"))
                    } else {
                        Ok(self.diagnostics_detail())
                    };
                    let _ = self.app_events.send(AppEvent::Health {
                        service: Service::Events,
                        result: health_res,
                    });

                    self.eval_cached();
                }
                Err(e) => {
                    let msg = e.to_string();
                    warn!(error = %msg, "sports alert poll failed");
                    self.last_fetch_error = Some(msg.clone());
                    self.err_streak = self.err_streak.saturating_add(1);
                    let backoff_secs = match self.err_streak {
                        1 => 60,
                        2 => 120,
                        _ => 300,
                    };
                    self.next_fetch_at = now + backoff_secs;
                    let _ = self.app_events.send(AppEvent::Health {
                        service: Service::Events,
                        result: Err(format!("Alerts: {msg}")),
                    });
                }
            }
        }

        // Next Game card (§5.2): all games from now-2h to end of tomorrow (local Chicago).
        let local_now = now_utc.with_timezone(&Chicago);
        let today_local = local_now.date_naive();
        let win_from = now_utc - ChronoDuration::hours(2);
        let tomorrow_end_local = (today_local + ChronoDuration::days(2))
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let win_to = Chicago
            .from_local_datetime(&tomorrow_end_local)
            .earliest()
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| now_utc + ChronoDuration::hours(48));
        match tvdash_events::alerts::fetch_window_games(&self.events_cfg, &pw, win_from, win_to).await {
            Ok(rows) => {
                let starred_set: HashSet<String> = starred_ids.iter().cloned().collect();
                let picked = crate::next_game::pick_next_game(
                    &rows,
                    &teams,
                    &starred_set,
                    now_utc.naive_utc(),
                    today_local,
                    Chicago,
                );
                let dto = picked.map(|g| crate::next_game::to_dto(g, today_local, Chicago));
                if dto.as_ref().is_some_and(|d| d.state == "live") {
                    self.next_fetch_at = self.next_fetch_at.min(now + FETCH_ACTIVE_SECS as i64);
                }
                self.last_next_game = dto.as_ref().map(|d| (d.clone(), now));
                let _ = self.app_events.send(AppEvent::NextGame(dto));
            }
            Err(e) => {
                warn!(error = %e, "next game poll failed");
                if self.last_next_game.as_ref().is_some_and(|(_, ts)| now - ts >= 30 * 60) {
                    self.last_next_game = None;
                    let _ = self.app_events.send(AppEvent::NextGame(None));
                }
            }
        }
    }

    fn rules(&self) -> Rules {
        Rules {
            soon_s: (self.alerts_cfg.soon_minutes as i64) * 60,
            final_on: self.alerts_cfg.final_on,
            final_grace_s: FINAL_GRACE_SECS,
        }
    }

    fn has_active_or_imminent_game(&self, now: i64) -> bool {
        let window = (self.alerts_cfg.soon_minutes as i64 + 30) * 60;
        self.cached_games.iter().any(|g| {
            matches!(g.status, GameStatus::Live | GameStatus::Delayed | GameStatus::Suspended)
                || (g.status == GameStatus::Upcoming
                    && g.start.is_some_and(|s| s >= now - 600 && s <= now + window))
        })
    }

    fn eval_cached(&mut self) {
        if !self.alerts_cfg.enabled || self.cached_games.is_empty() {
            return;
        }
        let now = Utc::now().timestamp();
        let rules = self.rules();
        let starred: HashSet<String> = self.starred.iter().map(|s| s.event_id.clone()).collect();
        let fired: HashSet<String> = self.fired.keys.keys().cloned().collect();

        let alerts = evaluate(
            &self.cached_games,
            &self.follows.teams,
            &starred,
            &fired,
            &mut self.final_seen_at,
            &rules,
            now,
        );
        if !alerts.is_empty() {
            debug!(count = alerts.len(), "enqueuing sports alerts");
            let dtos: Vec<SportsAlertDto> = alerts.into_iter().map(|a| to_dto(&a, now)).collect();
            let _ = self.app_events.send(AppEvent::SportsAlerts(dtos));
        }
    }

    fn toggle_star(&mut self, event_id: &str) {
        if let Some(pos) = self.starred.iter().position(|s| s.event_id == event_id) {
            self.starred.remove(pos);
            info!(event_id, "unstarred game");
        } else {
            self.starred.push(StarredGame {
                event_id: event_id.to_string(),
                starred_at: Utc::now().timestamp(),
            });
            info!(event_id, "starred game");
        }
        self.save_starred();
    }

    fn unstar(&mut self, event_id: &str) {
        let before = self.starred.len();
        self.starred.retain(|s| s.event_id != event_id);
        if self.starred.len() != before {
            self.save_starred();
        }
    }

    fn prune_starred(&mut self, now: i64) {
        let before = self.starred.len();
        self.starred.retain(|s| {
            if let Some(g) = self.cached_games.iter().find(|g| g.event_id == s.event_id) {
                if g.status == GameStatus::Final {
                    if let Some(&seen) = self.final_seen_at.get(&g.event_id) {
                        return now - seen <= 86_400;
                    }
                }
                if let Some(start) = g.start {
                    if now - start > 86_400 && now - s.starred_at > 2 * 86_400 {
                        return false;
                    }
                }
            } else if now - s.starred_at > 8 * 86_400 {
                return false;
            }
            true
        });
        if self.starred.len() != before {
            self.save_starred();
        }
    }

    fn save_starred(&self) {
        if let Some(p) = &self.starred_path {
            if let Err(e) = sports_store::save_starred(p, &self.starred) {
                warn!(error = %e, "failed to save sports-starred.json");
            }
        }
    }

    fn diagnostics_detail(&self) -> String {
        let checked = self
            .last_fetch_at
            .map(format_short_time)
            .unwrap_or_else(|| "never".into());
        format!(
            "Alerts: {} teams · {} starred · checked {checked}",
            self.follows.teams.len(),
            self.starred.len()
        )
    }

    fn emit_status(&self) {
        let followed: Vec<FollowedTeamInfo> = self
            .follows
            .teams
            .iter()
            .map(|t| FollowedTeamInfo {
                league: t.league.clone(),
                id: t.id.clone(),
                name: t.name.clone(),
                league_label: league_display_name(&t.league).to_string(),
            })
            .collect();

        let starred: Vec<StarredGameInfo> = self
            .starred
            .iter()
            .map(|s| {
                if let Some(g) = self.cached_games.iter().find(|g| g.event_id == s.event_id) {
                    let time = g.start.map(format_day_and_time).unwrap_or_else(|| "Time TBD".into());
                    let tv = g.broadcasts.as_deref().unwrap_or("");
                    let mut parts = vec![time, league_display_name(&g.league).to_string()];
                    if !tv.is_empty() {
                        parts.push(tv.to_string());
                    }
                    StarredGameInfo {
                        event_id: s.event_id.clone(),
                        matchup: format!("{} at {}", g.away.name, g.home.name),
                        meta: parts.join(" · "),
                    }
                } else {
                    StarredGameInfo {
                        event_id: s.event_id.clone(),
                        matchup: format!("Game {}", s.event_id),
                        meta: "Waiting for scoreboard window".into(),
                    }
                }
            })
            .collect();

        let next_eligible = self
            .cached_games
            .iter()
            .filter(|g| g.status == GameStatus::Upcoming && g.start.is_some())
            .min_by_key(|g| g.start.unwrap_or(i64::MAX))
            .map(|g| {
                format!(
                    "{} at {} ({})",
                    g.away.name,
                    g.home.name,
                    format_day_and_time(g.start.unwrap())
                )
            });

        let _ = self.app_events.send(AppEvent::SportsAlertsStatus(
            SportsAlertsStatusSnapshot {
                config: self.alerts_cfg.clone(),
                followed,
                follows_updated_at: self.follows.updated_at,
                follows_error: self.follows.error.clone(),
                unmapped_count: self.follows.unmapped_count,
                starred,
                held: self.held,
                last_poll_at: self.last_fetch_at,
                last_poll_error: self.last_fetch_error.clone(),
                stale_minutes: self.stale_minutes,
                next_eligible,
            },
        ));
    }
}

fn row_to_game(r: AlertGameRow) -> Game {
    let start = if r.time_tbd {
        None
    } else {
        r.starts_at_utc
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp())
    };
    Game {
        event_id: r.event_id,
        start,
        status: GameStatus::from_db(&r.status),
        status_text: r.status_text,
        league: r.league,
        sport: r.sport,
        home: Side {
            team_id: r.home_team_id,
            name: r.home_name,
            score: r.home_score,
        },
        away: Side {
            team_id: r.away_team_id,
            name: r.away_name,
            score: r.away_score,
        },
        neutral: r.neutral_site,
        venue: r.venue,
        broadcasts: r.broadcasts,
        source_url: r.source_url,
    }
}

pub(crate) fn team_badge_color(name: &str) -> String {
    const PALETTE: [&str; 8] = [
        "#0051BA", "#C8102E", "#1D428A", "#006BB6", "#154734", "#512888", "#CC0000", "#002D62",
    ];
    let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    PALETTE[hash % PALETTE.len()].to_string()
}

fn format_short_time(ts: i64) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp(ts, 0) else {
        return String::new();
    };
    let local = utc.with_timezone(&Chicago);
    let (_, h12) = local.hour12();
    let ampm = if local.hour() < 12 { "AM" } else { "PM" };
    format!("{}:{:02} {}", h12, local.minute(), ampm)
}

fn format_time_ct(ts: i64) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp(ts, 0) else {
        return String::new();
    };
    let local = utc.with_timezone(&Chicago);
    let (_, h12) = local.hour12();
    let ampm = if local.hour() < 12 { "A.M." } else { "P.M." };
    format!("{}:{:02} {} CT", h12, local.minute(), ampm)
}

fn format_day_and_time(ts: i64) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp(ts, 0) else {
        return String::new();
    };
    let local = utc.with_timezone(&Chicago);
    let (_, h12) = local.hour12();
    let ampm = if local.hour() < 12 { "A.M." } else { "P.M." };
    format!("{} {}:{:02} {} CT", local.format("%a"), h12, local.minute(), ampm)
}

pub fn to_dto(a: &Alert, now: i64) -> SportsAlertDto {
    let g = &a.game;
    let why = match &a.reason {
        Reason::Team(name) => format!("Following {name}"),
        Reason::Starred => "Starred game".into(),
    };
    match a.kind {
        AlertKind::Soon => {
            let mins = g.start.map(|s| ((s - now + 59) / 60).max(1)).unwrap_or(15);
            let kicker = format!("STARTING IN {mins} MIN");
            let mut meta_parts = Vec::new();
            if let Some(s) = g.start {
                meta_parts.push(format_time_ct(s));
            }
            if let Some(tv) = g.broadcasts.as_deref().filter(|s| !s.is_empty()) {
                meta_parts.push(format!("TV: {tv}"));
            }
            if !g.venue.is_empty() {
                meta_parts.push(g.venue.clone());
            }
            let role_away = if g.neutral { "Team" } else { "Away" };
            let role_home = if g.neutral { "Team" } else { "Home" };
            SportsAlertDto {
                key: a.key.clone(),
                kind: "soon".into(),
                event_id: g.event_id.clone(),
                start: g.start,
                queued_at: now,
                kicker,
                kicker_final: false,
                why,
                away: SportsAlertSideDto {
                    name: g.away.name.clone(),
                    initials: team_initials(&g.away.name),
                    color: team_badge_color(&g.away.name),
                    logo_url: None,
                    right: role_away.into(),
                    winner: false,
                    muted: false,
                },
                home: SportsAlertSideDto {
                    name: g.home.name.clone(),
                    initials: team_initials(&g.home.name),
                    color: team_badge_color(&g.home.name),
                    logo_url: None,
                    right: role_home.into(),
                    winner: false,
                    muted: false,
                },
                meta: meta_parts.join(" · "),
                primary: Some("Watch on YouTube TV".into()),
                source_url: None,
            }
        }
        AlertKind::Final => {
            let kicker = if g.status_text.trim().is_empty() {
                "FINAL".into()
            } else {
                g.status_text.trim().to_uppercase()
            };
            let away_s = g.away.score.unwrap_or(0);
            let home_s = g.home.score.unwrap_or(0);
            let away_win = away_s > home_s;
            let home_win = home_s > away_s;
            let mut meta_parts = vec![league_display_name(&g.league).to_string()];
            if !g.venue.is_empty() {
                meta_parts.push(g.venue.clone());
            }
            SportsAlertDto {
                key: a.key.clone(),
                kind: "final".into(),
                event_id: g.event_id.clone(),
                start: g.start,
                queued_at: now,
                kicker,
                kicker_final: true,
                why,
                away: SportsAlertSideDto {
                    name: g.away.name.clone(),
                    initials: team_initials(&g.away.name),
                    color: team_badge_color(&g.away.name),
                    logo_url: None,
                    right: g.away.score.map(|s| s.to_string()).unwrap_or_else(|| "—".into()),
                    winner: away_win,
                    muted: home_win,
                },
                home: SportsAlertSideDto {
                    name: g.home.name.clone(),
                    initials: team_initials(&g.home.name),
                    color: team_badge_color(&g.home.name),
                    logo_url: None,
                    right: g.home.score.map(|s| s.to_string()).unwrap_or_else(|| "—".into()),
                    winner: home_win,
                    muted: away_win,
                },
                meta: meta_parts.join(" · "),
                primary: g.source_url.as_ref().map(|_| "Recap ↗".into()),
                source_url: g.source_url.clone(),
            }
        }
    }
}

/// Sample card for `tvdash --alert-test soon|final` (§7).
pub fn sample_alert(kind: &str) -> SportsAlertDto {
    let now = Utc::now().timestamp();
    let is_final = kind.eq_ignore_ascii_case("final");
    if is_final {
        SportsAlertDto {
            key: format!("test:final:{now}"),
            kind: "final".into(),
            event_id: "sample-final".into(),
            start: None,
            queued_at: now,
            kicker: "FINAL".into(),
            kicker_final: true,
            why: "Following Kansas".into(),
            away: SportsAlertSideDto {
                name: "Iowa State".into(),
                initials: "ISU".into(),
                color: "#C8102E".into(),
                logo_url: None,
                right: "17".into(),
                winner: false,
                muted: true,
            },
            home: SportsAlertSideDto {
                name: "Kansas".into(),
                initials: "KU".into(),
                color: "#0051BA".into(),
                logo_url: None,
                right: "31".into(),
                winner: true,
                muted: false,
            },
            meta: "College Football · David Booth Memorial Stadium".into(),
            primary: Some("Recap ↗".into()),
            source_url: Some("https://www.espn.com/college-football/".into()),
        }
    } else {
        SportsAlertDto {
            key: format!("test:soon:{now}"),
            kind: "soon".into(),
            event_id: "sample-soon".into(),
            start: Some(now + 15 * 60),
            queued_at: now,
            kicker: "STARTING IN 15 MIN".into(),
            kicker_final: false,
            why: "Following Kansas".into(),
            away: SportsAlertSideDto {
                name: "Iowa State".into(),
                initials: "ISU".into(),
                color: "#C8102E".into(),
                logo_url: None,
                right: "Away".into(),
                winner: false,
                muted: false,
            },
            home: SportsAlertSideDto {
                name: "Kansas".into(),
                initials: "KU".into(),
                color: "#0051BA".into(),
                logo_url: None,
                right: "Home".into(),
                winner: false,
                muted: false,
            },
            meta: "11:00 A.M. CT · TV: FOX · David Booth Memorial Stadium".into(),
            primary: Some("Watch on YouTube TV".into()),
            source_url: None,
        }
    }
}
