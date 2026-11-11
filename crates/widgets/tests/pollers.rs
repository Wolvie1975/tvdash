//! Runs the real weather poller against a fake Open-Meteo.
//! Needs python3: `cargo test -p tvdash-widgets -- --ignored`

use std::{path::PathBuf, process::Stdio, time::Duration};

use tokio::{sync::mpsc, time::timeout};
use tvdash_core::{AppEvent, Config};
use tvdash_widgets::Endpoints;

struct Fake {
    _child: tokio::process::Child,
    port: String,
    log: PathBuf,
    dir: PathBuf,
}

async fn start_fake() -> Fake {
    let dir =
        std::env::temp_dir().join(format!("tvdash-widgets-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (port_file, log) = (dir.join("port"), dir.join("log"));
    let _ = std::fs::remove_file(&port_file);
    let child = tokio::process::Command::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes/fake_widgets.py"))
        .arg(&port_file)
        .arg(&log)
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("start fake");
    let port = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(p) = std::fs::read_to_string(&port_file)
                && !p.is_empty()
            {
                return p;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    Fake { _child: child, port, log, dir }
}

async fn first_weather(rx: &mut mpsc::UnboundedReceiver<AppEvent>) -> AppEvent {
    timeout(Duration::from_secs(10), async {
        loop {
            if let e @ AppEvent::Weather(_) = rx.recv().await.unwrap() {
                return e;
            }
        }
    })
    .await
    .expect("weather reports")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs python3; run with --ignored"]
async fn weather_poller_reports_forecast() {
    let fake = start_fake().await;
    let base = format!("http://127.0.0.1:{}", fake.port);
    let mut config = Config::default();
    config.general.latitude = 41.88;
    config.general.longitude = -87.63;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (general_tx, general) = tokio::sync::watch::channel(config.general.clone());
    let w = tvdash_widgets::spawn(&config, Endpoints { weather: base.clone() }, tx, general);

    let AppEvent::Weather(Some(weather)) = first_weather(&mut rx).await else { panic!() };
    assert_eq!(
        (weather.temperature, weather.condition.as_str(), weather.high, weather.low),
        (48.4, "Light rain", Some(55.1), Some(38.6))
    );
    let log: Vec<serde_json::Value> =
        std::fs::read_to_string(&fake.log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let forecast = log.iter().find(|r| r["path"] == "/v1/forecast").unwrap();
    assert_eq!(forecast["query"]["latitude"][0], "41.88");
    assert_eq!(forecast["query"]["temperature_unit"][0], "fahrenheit");
    assert_eq!(forecast["query"]["timezone"][0], "America/Chicago");

    // Switching to Celsius fetches again straight away.
    let mut celsius = config.general.clone();
    celsius.units = tvdash_core::config::Units::Celsius;
    general_tx.send(celsius).unwrap();
    first_weather(&mut rx).await;
    let log: Vec<serde_json::Value> =
        std::fs::read_to_string(&fake.log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(log.last().unwrap()["query"]["temperature_unit"][0], "celsius");
    w.abort();

    // No location configured: weather is off.
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (_g, general) = tokio::sync::watch::channel(Config::default().general);
    let w = tvdash_widgets::spawn(&Config::default(), Endpoints { weather: base }, tx, general);
    assert!(matches!(first_weather(&mut rx).await, AppEvent::Weather(None)));
    w.abort();
    std::fs::remove_dir_all(&fake.dir).ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs python3; run with --ignored"]
async fn geocoding_finds_places() {
    let fake = start_fake().await;
    let base = format!("http://127.0.0.1:{}", fake.port);
    let http = reqwest::Client::new();
    let found = tvdash_widgets::weather::geocode(&http, &base, "Chicago").await.unwrap();
    assert_eq!(found[0].label, "Chicago, Illinois, United States");
    assert_eq!((found[0].latitude, found[0].longitude, found[0].timezone.as_str()), (41.85, -87.65, "America/Chicago"));
    assert_eq!(found.len(), 2);
    assert!(tvdash_widgets::weather::geocode(&http, &base, "Nowhere").await.unwrap().is_empty());
    std::fs::remove_dir_all(&fake.dir).ok();
}
