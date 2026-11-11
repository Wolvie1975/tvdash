//! Header widgets: weather, next calendar event, homelab status.
//! Each runs as its own tokio poller and emits `AppEvent`s.

pub mod calendar; // every 5 min
pub mod homelab; // every minute
pub mod weather; // every 10 min

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WidgetError {
    #[error("{0}")]
    Http(#[from] reqwest::Error),
    #[error("HTTP {0}")]
    Status(u16),
    #[error("the address no longer works")]
    Revoked,
}

pub type Result<T> = std::result::Result<T, WidgetError>;

// ── Pollers ─────────────────────────────────────────────────────────

use std::sync::Arc;

use tokio::{
    sync::{Notify, mpsc::UnboundedSender},
    task::JoinHandle,
};
use tracing::{debug, info, warn};
use tvdash_core::{
    AppEvent, Config, SecretKey,
    models::{CalendarState, HomelabStatus, Probe},
};

/// Reads a secret (blocking; called off the async threads).
pub type SecretReader = Arc<dyn Fn(SecretKey) -> Option<String> + Send + Sync>;

/// Overridable service URLs (tests point these at fakes).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub weather: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self { weather: weather::DEFAULT_BASE.into() }
    }
}

fn client(verify_tls: bool) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("tvdash/", env!("CARGO_PKG_VERSION")))
        .danger_accept_invalid_certs(!verify_tls)
        .build()
        .expect("HTTP client")
}

async fn read_secret(secrets: &SecretReader, key: SecretKey) -> Option<String> {
    let s = secrets.clone();
    tokio::task::spawn_blocking(move || s(key)).await.ok().flatten().filter(|v| !v.trim().is_empty())
}

/// Running pollers. Notify a handle to poll again now (e.g. after a secret changed).
pub struct Widgets {
    pub tasks: Vec<JoinHandle<()>>,
    pub refresh_calendar: Arc<Notify>,
    pub refresh_homelab: Arc<Notify>,
}

impl Widgets {
    pub fn abort(&self) {
        self.tasks.iter().for_each(|t| t.abort());
    }
}

/// Sleep for `d`, or less if `wake` is notified.
async fn sleep_or(d: std::time::Duration, wake: &Notify) {
    tokio::select! {
        _ = tokio::time::sleep(d) => {}
        _ = wake.notified() => {}
    }
}

/// Start the three pollers. Each sends its first result right away.
pub fn spawn(
    config: &Config,
    secrets: SecretReader,
    endpoints: Endpoints,
    events: UnboundedSender<AppEvent>,
) -> Widgets {
    let tz = config.general.tz().unwrap_or(chrono_tz::UTC);
    let mut tasks = Vec::new();
    let refresh_calendar = Arc::new(Notify::new());
    let refresh_homelab = Arc::new(Notify::new());

    // Weather
    {
        let g = config.general.clone();
        let events = events.clone();
        tasks.push(tokio::spawn(async move {
            let http = client(true);
            let Some(loc) = weather::location(g.latitude, g.longitude) else {
                info!("no latitude/longitude in config; weather disabled");
                let _ = events.send(AppEvent::Weather(None));
                return;
            };
            loop {
                let wait = match weather::fetch(&http, &endpoints.weather, loc, g.units, &g.timezone).await {
                    Ok(w) => {
                        let _ = events.send(AppEvent::Weather(Some(w)));
                        weather::REFRESH
                    }
                    Err(e) => {
                        warn!(error = %e, "weather fetch failed; retrying in 1 min");
                        std::time::Duration::from_secs(60)
                    }
                };
                tokio::time::sleep(wait).await;
            }
        }));
    }

    // Calendar (the secret is re-read each cycle so a newly set address is picked up)
    if config.calendar.enabled {
        let (events, secrets, wake) = (events.clone(), secrets.clone(), refresh_calendar.clone());
        tasks.push(tokio::spawn(async move {
            let http = client(true);
            loop {
                let state = match read_secret(&secrets, SecretKey::CalendarIcsUrl).await {
                    None => Some(CalendarState::NotSetUp),
                    Some(url) => match calendar::fetch(&http, &url).await {
                        Ok(ics) => {
                            let now = chrono::Utc::now();
                            let (from, to) = calendar::window(now, tz);
                            let evs = calendar::occurrences(&ics, tz, from, to);
                            debug!(count = evs.len(), "calendar events today/tomorrow");
                            Some(CalendarState::Events(evs))
                        }
                        Err(WidgetError::Revoked) => Some(CalendarState::Revoked),
                        Err(e) => {
                            warn!(error = %e, "calendar fetch failed; keeping the last result");
                            None
                        }
                    },
                };
                if let Some(state) = state {
                    let _ = events.send(AppEvent::Calendar(state));
                }
                sleep_or(calendar::REFRESH, &wake).await;
            }
        }));
    }

    // Homelab
    {
        let h = config.homelab.clone();
        let wake = refresh_homelab.clone();
        tasks.push(tokio::spawn(async move {
            let portainer_http = client(h.portainer_verify_tls);
            let http = client(true);
            loop {
                let containers = match (h.portainer_url.trim(), read_secret(&secrets, SecretKey::PortainerApiKey).await)
                {
                    ("", _) | (_, None) => Probe::NotConfigured,
                    (url, Some(key)) => {
                        homelab::probe(homelab::portainer(&portainer_http, url, h.portainer_endpoint_id, &key).await)
                    }
                };
                let monitors = match (h.uptime_kuma_url.trim(), h.uptime_kuma_status_page.trim()) {
                    ("", _) | (_, "") => Probe::NotConfigured,
                    (url, slug) => homelab::probe(homelab::uptime_kuma(&http, url, slug).await),
                };
                for (what, err) in [("portainer", failure(&containers)), ("uptime kuma", failure(&monitors))] {
                    if let Some(e) = err {
                        warn!(source = what, error = %e, "homelab poll failed");
                    }
                }
                let _ = events.send(AppEvent::Homelab(HomelabStatus { monitors, containers }));
                sleep_or(homelab::REFRESH, &wake).await;
            }
        }));
    }
    Widgets { tasks, refresh_calendar, refresh_homelab }
}

fn failure<T>(p: &Probe<T>) -> Option<&str> {
    match p {
        Probe::Failed(e) => Some(e),
        _ => None,
    }
}
