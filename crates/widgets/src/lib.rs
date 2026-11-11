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

use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};
use tracing::{info, warn};
use tvdash_core::{AppEvent, Config};

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

/// Start the pollers. Each sends its first result right away.
pub fn spawn(config: &Config, endpoints: Endpoints, events: UnboundedSender<AppEvent>) -> Widgets {
    let mut tasks = Vec::new();

    // Weather
    {
        let g = config.general.clone();
        let events = events.clone();
        tasks.push(tokio::spawn(async move {
            let http = client();
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

    Widgets { tasks }
}
