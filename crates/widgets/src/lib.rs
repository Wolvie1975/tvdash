//! Header widgets: weather (Open-Meteo) and NWS active alerts, polled on their own tokio tasks.

pub mod nws;     // every 2 min (1 min while a takeover warning is active)
pub mod weather; // every 10 min

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WidgetError {
    #[error("{0}")]
    Http(#[from] reqwest::Error),
    #[error("HTTP {0}")]
    Status(u16),
}

pub type Result<T> = std::result::Result<T, WidgetError>;

// ── Pollers ─────────────────────────────────────────────────────────

use tokio::{
    sync::{mpsc::UnboundedSender, watch},
    task::JoinHandle,
};
use tracing::{info, warn};
use tvdash_core::{AppEvent, Config, config::GeneralConfig, events::Service};

/// Overridable service URLs (tests point these at fakes).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub weather: String,
    pub nws: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            weather: weather::DEFAULT_BASE.into(),
            nws: nws::DEFAULT_BASE.into(),
        }
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("tvdash/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client")
}

/// Running pollers.
pub struct Widgets {
    pub tasks: Vec<JoinHandle<()>>,
}

impl Widgets {
    pub fn abort(&self) {
        self.tasks.iter().for_each(|t| t.abort());
    }
}

/// Start the pollers. Each sends its first result right away. `general`
/// carries setting changes (units) so the weather is fetched again at once.
pub fn spawn(
    config: &Config,
    endpoints: Endpoints,
    events: UnboundedSender<AppEvent>,
    mut general: watch::Receiver<GeneralConfig>,
) -> Widgets {
    let mut tasks = Vec::new();
    let alerts_cfg = config.weather_alerts.clone();
    let mut general_nws = general.clone();
    let events_nws = events.clone();
    let nws_base = endpoints.nws.clone();

    // Weather
    tasks.push(tokio::spawn(async move {
        let http = client();
        loop {
            let g = general.borrow_and_update().clone();
            let wait = match weather::location(g.latitude, g.longitude) {
                None => {
                    info!("no latitude/longitude in config; weather disabled");
                    let _ = events.send(AppEvent::Weather(None));
                    None
                }
                Some(loc) => match weather::fetch(&http, &endpoints.weather, loc, g.units, &g.timezone).await {
                    Ok(w) => {
                        let note = format!("{} {}", w.temperature.round(), w.condition);
                        let _ = events.send(AppEvent::Weather(Some(w)));
                        let _ = events.send(AppEvent::Health { service: Service::Weather, result: Ok(note) });
                        Some(weather::REFRESH)
                    }
                    Err(e) => {
                        warn!(error = %e, "weather fetch failed; retrying in 1 min");
                        let _ = events.send(AppEvent::Health { service: Service::Weather, result: Err(e.to_string()) });
                        Some(std::time::Duration::from_secs(60))
                    }
                },
            };
            match wait {
                Some(d) => {
                    tokio::select! {
                        _ = tokio::time::sleep(d) => {}
                        changed = general.changed() => if changed.is_err() { return },
                    }
                }
                None => {
                    if general.changed().await.is_err() {
                        return;
                    }
                }
            }
        }
    }));

    // NWS active weather alerts
    if alerts_cfg.enabled {
        tasks.push(tokio::spawn(async move {
            let http = client();
            loop {
                let g = general_nws.borrow_and_update().clone();
                let wait = match weather::location(g.latitude, g.longitude) {
                    None => {
                        let _ = events_nws.send(AppEvent::WeatherAlerts(Vec::new()));
                        None
                    }
                    Some(loc) => match nws::fetch(&http, &nws_base, loc, &alerts_cfg.contact).await {
                        Ok(alerts) => {
                            let has_takeover = alerts
                                .iter()
                                .any(|a| nws::level(&a.event, &alerts_cfg.takeover) == nws::AlertLevel::Takeover);
                            let _ = events_nws.send(AppEvent::WeatherAlerts(alerts));
                            Some(if has_takeover { nws::REFRESH_ACTIVE } else { nws::REFRESH })
                        }
                        Err(e) => {
                            warn!(error = %e, "NWS alerts fetch failed; retrying in 5 min");
                            let _ = events_nws.send(AppEvent::Health {
                                service: Service::Weather,
                                result: Err(format!("alerts: {e}")),
                            });
                            Some(std::time::Duration::from_secs(300))
                        }
                    },
                };
                match wait {
                    Some(d) => {
                        tokio::select! {
                            _ = tokio::time::sleep(d) => {}
                            changed = general_nws.changed() => if changed.is_err() { return },
                        }
                    }
                    None => {
                        if general_nws.changed().await.is_err() {
                            return;
                        }
                    }
                }
            }
        }));
    }

    Widgets { tasks }
}
