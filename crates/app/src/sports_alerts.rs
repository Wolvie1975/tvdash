//! Sports alerts domain types, pure evaluation logic, and UI alert queue.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Soon,
    Final,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameStatus {
    Upcoming,
    Live,
    Final,
    Postponed,
    Cancelled,
    Suspended,
    Delayed,
    Unknown,
}

impl GameStatus {
    /// `SportsScoreboard.Status`, as written by Rss-Rust-Scrape `normalized_status`.
    pub fn from_db(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "upcoming" => Self::Upcoming,
            "live" => Self::Live,
            "final" => Self::Final,
            "postponed" => Self::Postponed,
            "cancelled" | "canceled" => Self::Cancelled,
            "suspended" => Self::Suspended,
            "delayed" => Self::Delayed,
            _ => Self::Unknown,
        }
    }
}

/// One row of `dbo.SportsScoreboard` (§4.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub event_id: String,
    /// `StartsAtUtc` as unix seconds; `None` when `TimeTbd = 1`.
    pub start: Option<i64>,
    pub status: GameStatus,
    /// `"Final"`, `"Final/OT"`, etc.
    pub status_text: String,
    /// Exact `League` string (part of the team key).
    pub league: String,
    pub sport: String,
    pub home: Side,
    pub away: Side,
    pub neutral: bool,
    /// Stadium name only.
    pub venue: String,
    pub broadcasts: Option<String>,
    /// ESPN game page (`SourceUrl`) → Recap ↗.
    pub source_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Side {
    pub team_id: String,
    pub name: String,
    pub score: Option<i32>,
}

/// A followed team: only unique as `(league, id)` (§0.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamKey {
    pub league: String,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Display name → `"Following Kansas"`.
    Team(String),
    Starred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub kind: AlertKind,
    pub game: Game,
    pub reason: Reason,
    pub key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub soon_s: i64,
    pub final_on: bool,
    pub final_grace_s: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            soon_s: 15 * 60,
            final_on: true,
            final_grace_s: 30 * 60,
        }
    }
}

/// Minimum gap in seconds between closing one alert card and showing the next.
pub const QUEUE_GAP_S: i64 = 3;

pub fn key(kind: AlertKind, g: &Game) -> String {
    match kind {
        // Include the start time so a rescheduled game alerts again at its new time.
        AlertKind::Soon => format!("soon:{}:{}", g.event_id, g.start.unwrap_or(0)),
        AlertKind::Final => format!("final:{}", g.event_id),
    }
}

pub fn reason(g: &Game, teams: &[TeamKey], starred: &HashSet<String>) -> Option<Reason> {
    // League AND id must both match: ids repeat across leagues (§0.4).
    // A followed team wins over a star for the chip text.
    for t in teams {
        if t.league == g.league && (g.home.team_id == t.id || g.away.team_id == t.id) {
            return Some(Reason::Team(t.name.clone()));
        }
    }
    starred.contains(&g.event_id).then_some(Reason::Starred)
}

/// Seed `final_seen_at` on the first fetch after startup so games that already
/// finished before tvdash started never alert.
pub fn seed_initial_finals(
    games: &[Game],
    final_seen_at: &mut HashMap<String, i64>,
    rules: &Rules,
    now: i64,
) {
    let expired = now - rules.final_grace_s - 1;
    for g in games {
        if g.status == GameStatus::Final {
            final_seen_at.entry(g.event_id.clone()).or_insert(expired);
        }
    }
}

/// Evaluate fetched games against rules and fired state.
///
/// `final_seen_at`: when this watcher first saw the game as `Final` (in-memory per `event_id`).
pub fn evaluate(
    games: &[Game],
    teams: &[TeamKey],
    starred: &HashSet<String>,
    fired: &HashSet<String>,
    final_seen_at: &mut HashMap<String, i64>,
    rules: &Rules,
    now: i64,
) -> Vec<Alert> {
    let mut out = Vec::new();
    for g in games {
        let Some(why) = reason(g, teams, starred) else {
            continue;
        };

        // Only `upcoming`: postponed / delayed / TBD start times aren't trustworthy.
        if rules.soon_s > 0 && g.status == GameStatus::Upcoming {
            if let Some(start) = g.start {
                let left = start - now;
                let k = key(AlertKind::Soon, g);
                if left > 0 && left <= rules.soon_s && !fired.contains(&k) {
                    out.push(Alert {
                        kind: AlertKind::Soon,
                        game: g.clone(),
                        reason: why.clone(),
                        key: k,
                    });
                }
            }
        }

        if rules.final_on && g.status == GameStatus::Final {
            let seen = *final_seen_at.entry(g.event_id.clone()).or_insert(now);
            let k = key(AlertKind::Final, g);
            if now - seen <= rules.final_grace_s && !fired.contains(&k) {
                out.push(Alert {
                    kind: AlertKind::Final,
                    game: g.clone(),
                    reason: why,
                    key: k,
                });
            }
        }
    }
    out.sort_by_key(|a| a.game.start.unwrap_or(i64::MAX));
    out
}

#[derive(Clone, Debug, Default)]
pub struct AlertQueue {
    items: VecDeque<QueuedAlert>,
    showing: Option<Alert>,
    held: bool,
    last_closed_at: Option<i64>,
}

#[derive(Clone, Debug)]
struct QueuedAlert {
    alert: Alert,
    queued_at: i64,
}

impl AlertQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_held(&mut self, held: bool) {
        self.held = held;
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub fn showing(&self) -> Option<&Alert> {
        self.showing.as_ref()
    }

    pub fn pending_len(&self) -> usize {
        self.items.len()
    }

    /// Enqueue an alert unless its key is already showing or queued.
    pub fn push(&mut self, alert: Alert, now: i64) -> bool {
        if self.showing.as_ref().is_some_and(|a| a.key == alert.key) {
            return false;
        }
        if self.items.iter().any(|q| q.alert.key == alert.key) {
            return false;
        }
        self.items.push_back(QueuedAlert { alert, queued_at: now });
        true
    }

    /// Pop and promote the next eligible alert to `showing` if nothing is showing,
    /// not held, not blocked by a higher-priority card, and >= 3s after last close.
    pub fn next(&mut self, now: i64, blocked_by_priority: bool, final_grace_s: i64) -> Option<Alert> {
        if self.showing.is_some() || self.held || blocked_by_priority {
            return None;
        }
        if let Some(closed_at) = self.last_closed_at {
            if now - closed_at < QUEUE_GAP_S {
                return None;
            }
        }
        while let Some(q) = self.items.pop_front() {
            let stale = match q.alert.kind {
                AlertKind::Soon => q.alert.game.start.is_none_or(|s| s <= now),
                AlertKind::Final => now - q.queued_at > final_grace_s,
            };
            if stale {
                continue;
            }
            self.showing = Some(q.alert.clone());
            return Some(q.alert);
        }
        None
    }

    /// Close the currently showing alert and record the close time for the 3s gap.
    pub fn close(&mut self, now: i64) -> Option<Alert> {
        let closed = self.showing.take();
        if closed.is_some() {
            self.last_closed_at = Some(now);
        }
        closed
    }

    pub fn clear_pending(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_game(event_id: &str, league: &str, away_id: &str, home_id: &str) -> Game {
        Game {
            event_id: event_id.into(),
            start: Some(1_000_000),
            status: GameStatus::Upcoming,
            status_text: String::new(),
            league: league.into(),
            sport: "football".into(),
            home: Side {
                team_id: home_id.into(),
                name: "Kansas".into(),
                score: None,
            },
            away: Side {
                team_id: away_id.into(),
                name: "Iowa State".into(),
                score: None,
            },
            neutral: false,
            venue: "David Booth Memorial Stadium".into(),
            broadcasts: Some("FOX".into()),
            source_url: Some("https://www.espn.com/college-football/game/_/gameId/401".into()),
        }
    }

    fn team(league: &str, id: &str, name: &str) -> TeamKey {
        TeamKey {
            league: league.into(),
            id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn soon_fires_inside_window_for_followed_team() {
        let g = sample_game("401", "college-football", "66", "2305");
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 14 * 60;

        let alerts = evaluate(&[g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Soon);
        assert_eq!(alerts[0].reason, Reason::Team("Kansas".into()));
    }

    #[test]
    fn soon_does_not_fire_outside_window_or_after_start() {
        let g = sample_game("401", "college-football", "66", "2305");
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();

        // 16 min before start -> none
        let too_early = 1_000_000 - 16 * 60;
        assert!(evaluate(&[g.clone()], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, too_early).is_empty());

        // Start already passed (even if status is still upcoming) -> none
        let passed = 1_000_000;
        assert!(evaluate(&[g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, passed).is_empty());
    }

    #[test]
    fn soon_fired_key_suppresses_repeat_until_rescheduled() {
        let mut g = sample_game("401", "college-football", "66", "2305");
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        let mut fired = HashSet::new();
        fired.insert(key(AlertKind::Soon, &g));
        assert!(evaluate(&[g.clone()], &teams, &HashSet::new(), &fired, &mut seen, &rules, now).is_empty());

        // Moved 1h later -> new key, alerts again inside the new window
        g.start = Some(1_000_000 + 3600);
        let new_now = 1_000_000 + 3600 - 10 * 60;
        let alerts = evaluate(&[g], &teams, &HashSet::new(), &fired, &mut seen, &rules, new_now);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Soon);
    }

    #[test]
    fn tbd_postponed_delayed_do_not_fire_soon() {
        let teams = vec![team("college-football", "2305", "Kansas")];
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        let mut tbd = sample_game("1", "college-football", "66", "2305");
        tbd.start = None;

        let mut postponed = sample_game("2", "college-football", "66", "2305");
        postponed.status = GameStatus::Postponed;

        let mut delayed = sample_game("3", "college-football", "66", "2305");
        delayed.status = GameStatus::Delayed;

        let mut seen = HashMap::new();
        let alerts = evaluate(
            &[tbd, postponed, delayed],
            &teams,
            &HashSet::new(),
            &HashSet::new(),
            &mut seen,
            &rules,
            now,
        );
        assert!(alerts.is_empty());
    }

    #[test]
    fn final_fires_once_when_status_turns_final() {
        let mut g = sample_game("401", "college-football", "66", "2305");
        g.status = GameStatus::Final;
        g.status_text = "Final".into();
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 + 10_800;

        let alerts = evaluate(&[g.clone()], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Final);

        let mut fired = HashSet::new();
        fired.insert(alerts[0].key.clone());
        assert!(evaluate(&[g], &teams, &HashSet::new(), &fired, &mut seen, &rules, now + 60).is_empty());
    }

    #[test]
    fn first_fetch_seeds_already_final_games_so_they_never_alert() {
        let mut g = sample_game("401", "college-football", "66", "2305");
        g.status = GameStatus::Final;
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 + 10_800;

        seed_initial_finals(&[g.clone()], &mut seen, &rules, now);
        let alerts = evaluate(&[g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now);
        assert!(alerts.is_empty());
    }

    #[test]
    fn suspended_does_not_alert_but_later_final_does() {
        let mut g = sample_game("401", "college-football", "66", "2305");
        g.status = GameStatus::Suspended;
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        assert!(evaluate(&[g.clone()], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now).is_empty());

        g.status = GameStatus::Final;
        let alerts = evaluate(&[g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now + 7200);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Final);
    }

    #[test]
    fn cancelled_never_alerts() {
        let mut g = sample_game("401", "college-football", "66", "2305");
        g.status = GameStatus::Cancelled;
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        assert!(evaluate(&[g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now).is_empty());
    }

    #[test]
    fn team_id_collision_across_leagues_does_not_alert() {
        // Following Chiefs (nfl, "12") must not alert for Arizona (mens-college-basketball, "12")
        // or Islanders (nhl, "12").
        let teams = vec![team("nfl", "12", "Chiefs")];
        let arizona = sample_game("1", "mens-college-basketball", "12", "2305");
        let islanders = sample_game("2", "nhl", "12", "2");
        let chiefs = sample_game("3", "nfl", "12", "13");

        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        let alerts = evaluate(
            &[arizona, islanders, chiefs],
            &teams,
            &HashSet::new(),
            &HashSet::new(),
            &mut seen,
            &rules,
            now,
        );
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].game.event_id, "3");
        assert_eq!(alerts[0].reason, Reason::Team("Chiefs".into()));
    }

    #[test]
    fn mens_vs_womens_basketball_id_197_separated_by_league() {
        let teams = vec![team("mens-college-basketball", "197", "Oklahoma State")];
        let womens = sample_game("1", "womens-college-basketball", "197", "2305");
        let mens = sample_game("2", "mens-college-basketball", "197", "2305");

        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        let alerts = evaluate(&[womens, mens], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].game.event_id, "2");
    }

    #[test]
    fn starred_reason_and_team_wins_over_starred() {
        let g = sample_game("401", "college-football", "66", "2305");
        let mut starred = HashSet::new();
        starred.insert("401".into());
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        // Starred only
        let a1 = evaluate(&[g.clone()], &[], &starred, &HashSet::new(), &mut seen, &rules, now);
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].reason, Reason::Starred);

        // Starred + followed team -> Team wins
        let teams = vec![team("college-football", "2305", "Kansas")];
        let a2 = evaluate(&[g], &teams, &starred, &HashSet::new(), &mut seen, &rules, now);
        assert_eq!(a2.len(), 1);
        assert_eq!(a2[0].reason, Reason::Team("Kansas".into()));
    }

    #[test]
    fn unfollowed_unstarred_game_never_alerts() {
        let g = sample_game("401", "college-football", "66", "2305");
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;
        assert!(evaluate(&[g], &[], &HashSet::new(), &HashSet::new(), &mut seen, &rules, now).is_empty());
    }

    #[test]
    fn disabled_rules_suppress_soon_or_final() {
        let upcoming = sample_game("1", "college-football", "66", "2305");
        let mut final_g = sample_game("2", "college-football", "66", "2305");
        final_g.status = GameStatus::Final;
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let now = 1_000_000 - 10 * 60;

        let no_soon = Rules { soon_s: 0, final_on: true, final_grace_s: 1800 };
        let a1 = evaluate(&[upcoming.clone(), final_g.clone()], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &no_soon, now);
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].kind, AlertKind::Final);

        let no_final = Rules { soon_s: 900, final_on: false, final_grace_s: 1800 };
        let a2 = evaluate(&[upcoming, final_g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &no_final, now);
        assert_eq!(a2.len(), 1);
        assert_eq!(a2[0].kind, AlertKind::Soon);
    }

    #[test]
    fn multiple_games_sorted_by_start_time() {
        let mut g_later = sample_game("2", "college-football", "66", "2305");
        g_later.start = Some(1_000_300);
        let mut g_earlier = sample_game("1", "college-football", "66", "2305");
        g_earlier.start = Some(1_000_000);
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();
        let now = 1_000_000 - 10 * 60;

        let alerts = evaluate(&[g_later, g_earlier], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, now);
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].game.event_id, "1");
        assert_eq!(alerts[1].game.event_id, "2");
    }

    #[test]
    fn game_status_from_db_maps_all_f5_values() {
        assert_eq!(GameStatus::from_db("upcoming"), GameStatus::Upcoming);
        assert_eq!(GameStatus::from_db("LIVE"), GameStatus::Live);
        assert_eq!(GameStatus::from_db("final"), GameStatus::Final);
        assert_eq!(GameStatus::from_db("postponed"), GameStatus::Postponed);
        assert_eq!(GameStatus::from_db("cancelled"), GameStatus::Cancelled);
        assert_eq!(GameStatus::from_db("canceled"), GameStatus::Cancelled);
        assert_eq!(GameStatus::from_db("suspended"), GameStatus::Suspended);
        assert_eq!(GameStatus::from_db("delayed"), GameStatus::Delayed);
        assert_eq!(GameStatus::from_db("unknown"), GameStatus::Unknown);
        assert_eq!(GameStatus::from_db("anything_else"), GameStatus::Unknown);
    }

    #[test]
    fn clock_driven_evaluation_fires_soon_without_refetch() {
        let g = sample_game("401", "college-football", "66", "2305");
        let teams = vec![team("college-football", "2305", "Kansas")];
        let mut seen = HashMap::new();
        let rules = Rules::default();

        // At -16 min: nothing
        let t0 = 1_000_000 - 16 * 60;
        assert!(evaluate(&[g.clone()], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, t0).is_empty());

        // 90s later on the 30s tick (same cached `g`): now -14m30s -> fires!
        let t1 = t0 + 90;
        let alerts = evaluate(&[g], &teams, &HashSet::new(), &HashSet::new(), &mut seen, &rules, t1);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Soon);
    }

    #[test]
    fn queue_enforces_gap_hold_priority_and_stale_drop() {
        let mut q = AlertQueue::new();
        let g1 = sample_game("1", "college-football", "66", "2305");
        let g2 = sample_game("2", "college-football", "66", "2305");
        let a1 = Alert {
            kind: AlertKind::Soon,
            key: key(AlertKind::Soon, &g1),
            game: g1,
            reason: Reason::Starred,
        };
        let a2 = Alert {
            kind: AlertKind::Soon,
            key: key(AlertKind::Soon, &g2),
            game: g2,
            reason: Reason::Starred,
        };

        let t0 = 1_000_000 - 600;
        assert!(q.push(a1.clone(), t0));
        assert!(!q.push(a1, t0)); // duplicate key ignored
        assert!(q.push(a2, t0));

        // Blocked by higher priority card
        assert!(q.next(t0, true, 1800).is_none());
        // Held (Meet or during_video=hold)
        q.set_held(true);
        assert!(q.next(t0, false, 1800).is_none());
        q.set_held(false);

        // First shows
        assert_eq!(q.next(t0, false, 1800).unwrap().game.event_id, "1");
        assert!(q.next(t0 + 1, false, 1800).is_none());

        // Close at t0 + 5 -> 3s gap before second
        q.close(t0 + 5);
        assert!(q.next(t0 + 7, false, 1800).is_none());
        assert_eq!(q.next(t0 + 8, false, 1800).unwrap().game.event_id, "2");
    }
}
