//! Header widgets: weather (Open-Meteo), polled on its own tokio task.

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
}

impl Default for Endpoints {
    fn default() -> Self {
        Self { weather: weather::DEFAULT_BASE.into() }
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
    let _ = config; // per-widget settings come from `general`

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
    Widgets { tasks }
}
