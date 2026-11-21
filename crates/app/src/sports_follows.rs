//! Read-only followed teams from `dbo.ResultFollows`, with offline cache.

use std::path::PathBuf;

use chrono::Utc;
use tracing::{info, warn};
use tvdash_core::{
    config::EventsConfig,
    sports_store::{self, CachedTeam, FollowCache},
};
use tvdash_events::alerts::{AlertGameRow, FollowedTeamRow, fetch_followed_teams};

use crate::sports_alerts::TeamKey;

#[derive(Debug, Clone)]
pub struct FollowsState {
    pub teams: Vec<TeamKey>,
    pub updated_at: Option<i64>,
    pub error: Option<String>,
    pub unmapped_count: usize,
    cache_path: Option<PathBuf>,
}

impl FollowsState {
    pub fn load_cached() -> Self {
        let cache_path = sports_store::default_follow_cache_path();
        let cache = cache_path
            .as_deref()
            .map(sports_store::load_follow_cache)
            .unwrap_or_default();
        let teams = cache
            .teams
            .into_iter()
            .map(|t| TeamKey {
                league: t.league,
                id: t.id,
                name: t.name,
            })
            .collect();
        let updated_at = (cache.updated_at > 0).then_some(cache.updated_at);
        Self {
            teams,
            updated_at,
            error: None,
            unmapped_count: 0,
            cache_path,
        }
    }

    /// Refresh from `dbo.ResultFollows`. On DB failure, keeps the cached list and records the error.
    pub async fn refresh(&mut self, cfg: &EventsConfig, password: &str) {
        match fetch_followed_teams(cfg, password).await {
            Ok(res) => {
                let now = Utc::now().timestamp();
                let mapped: Vec<TeamKey> = res.teams.into_iter().map(map_row).collect();
                info!(
                    count = mapped.len(),
                    unmapped = res.unmapped_count,
                    "refreshed followed teams from ResultFollows"
                );
                self.teams = mapped;
                self.updated_at = Some(now);
                self.error = None;
                self.unmapped_count = res.unmapped_count;
                self.save_cache(now);
            }
            Err(e) => {
                let msg = e.to_string();
                warn!(error = %msg, "failed to refresh followed teams; keeping cached list");
                self.error = Some(msg);
            }
        }
    }

    /// Fill any placeholder `"Team <id>"` names from recently fetched scoreboard games.
    pub fn enrich_names_from_games(&mut self, games: &[AlertGameRow]) {
        let mut changed = false;
        for t in &mut self.teams {
            if !t.name.starts_with("Team ") {
                continue;
            }
            for g in games {
                if g.league != t.league {
                    continue;
                }
                if g.home_team_id == t.id && !g.home_name.is_empty() {
                    t.name = g.home_name.clone();
                    changed = true;
                    break;
                }
                if g.away_team_id == t.id && !g.away_name.is_empty() {
                    t.name = g.away_name.clone();
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            if let Some(ts) = self.updated_at {
                self.save_cache(ts);
            }
        }
    }

    fn save_cache(&self, updated_at: i64) {
        let Some(path) = &self.cache_path else {
            return;
        };
        let cache = FollowCache {
            updated_at,
            teams: self
                .teams
                .iter()
                .map(|t| CachedTeam {
                    league: t.league.clone(),
                    id: t.id.clone(),
                    name: t.name.clone(),
                })
                .collect(),
        };
        if let Err(e) = sports_store::save_follow_cache(path, &cache) {
            warn!(error = %e, "failed to save sports follow cache");
        }
    }
}

pub fn map_row(r: FollowedTeamRow) -> TeamKey {
    let name = if r.name.trim().is_empty() {
        format!("Team {}", r.id)
    } else {
        r.name.trim().to_string()
    };
    TeamKey {
        league: r.league,
        id: r.id,
        name,
    }
}

pub fn league_display_name(league: &str) -> &str {
    match league {
        "nfl" => "NFL",
        "mlb" => "MLB",
        "nhl" => "NHL",
        "nba" => "NBA",
        "wnba" => "WNBA",
        "usa.1" => "MLS",
        "usa.nwsl" => "NWSL",
        "eng.1" => "Premier League",
        "eng.2" => "EFL Championship",
        "college-football" => "College Football",
        "mens-college-basketball" => "Men's College Basketball",
        "womens-college-basketball" => "Women's College Basketball",
        "usa.ncaa.w.1" => "Women's College Soccer",
        "womens-college-volleyball" => "Women's College Volleyball",
        other => other,
    }
}
