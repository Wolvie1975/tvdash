//! Runs the real pollers against a fake Open-Meteo / calendar / Portainer /
//! Uptime Kuma server. Needs python3: `cargo test -p tvdash-widgets -- --ignored`

use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use tokio::{sync::mpsc, time::timeout};
use tvdash_core::{
    AppEvent, Config, SecretKey,
    models::{CalendarState, ContainerCounts, HomelabStatus, MonitorCounts, Probe},
};
use tvdash_widgets::{Endpoints, SecretReader};

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

fn secrets(map: HashMap<SecretKey, String>) -> SecretReader {
    Arc::new(move |k| map.get(&k).cloned())
}

/// Collect the first Weather, Calendar and Homelab events.
async fn first_three(rx: &mut mpsc::UnboundedReceiver<AppEvent>) -> (AppEvent, AppEvent, AppEvent) {
    let (mut w, mut c, mut h) = (None, None, None);
    timeout(Duration::from_secs(10), async {
        while w.is_none() || c.is_none() || h.is_none() {
            match rx.recv().await.unwrap() {
                e @ AppEvent::Weather(_) => w = Some(e),
                e @ AppEvent::Calendar(_) => c = Some(e),
                e @ AppEvent::Homelab(_) => h = Some(e),
                _ => {}
            }
        }
    })
    .await
    .expect("all three widgets report");
    (w.unwrap(), c.unwrap(), h.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs python3; run with --ignored"]
async fn pollers_report_weather_calendar_and_homelab() {
    let fake = start_fake().await;
    let base = format!("http://127.0.0.1:{}", fake.port);
    let mut config = Config::default();
    config.general.latitude = 41.88;
    config.general.longitude = -87.63;
    config.homelab.portainer_url = base.clone();
    config.homelab.portainer_endpoint_id = 2;
    config.homelab.uptime_kuma_url = base.clone();
    let reader = secrets(HashMap::from([
        (SecretKey::CalendarIcsUrl, format!("{base}/cal.ics")),
        (SecretKey::PortainerApiKey, "pk".into()),
    ]));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tasks = tvdash_widgets::spawn(&config, reader, Endpoints { weather: base.clone() }, tx);

    let (w, c, h) = first_three(&mut rx).await;
    let AppEvent::Weather(Some(w)) = w else { panic!("{w:?}") };
    assert_eq!((w.temperature, w.condition.as_str(), w.high, w.low), (48.4, "Light rain", Some(55.1), Some(38.6)));

    let AppEvent::Calendar(CalendarState::Events(evs)) = c else { panic!("{c:?}") };
    let next = tvdash_widgets::calendar::next(&evs, chrono::Utc::now()).expect("an upcoming event");
    assert_eq!(next.title, "Soon, really");
    assert_eq!(next.end - next.start, chrono::TimeDelta::minutes(30));

    let AppEvent::Homelab(h) = h else { panic!() };
    assert_eq!(
        h,
        HomelabStatus {
            monitors: Probe::Ok(MonitorCounts { up: 2, total: 3, down: vec!["NAS".into()] }),
            containers: Probe::Ok(ContainerCounts { running: 2, total: 3 }),
        }
    );

    // What the services were asked.
    let log: Vec<serde_json::Value> =
        std::fs::read_to_string(&fake.log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let forecast = log.iter().find(|r| r["path"] == "/v1/forecast").unwrap();
    assert_eq!(forecast["query"]["latitude"][0], "41.88");
    assert_eq!(forecast["query"]["temperature_unit"][0], "fahrenheit");
    assert_eq!(forecast["query"]["timezone"][0], "America/Chicago");
    let portainer = log.iter().find(|r| r["path"] == "/api/endpoints/2/docker/containers/json").unwrap();
    assert_eq!((portainer["api_key"].as_str(), portainer["query"]["all"][0].as_str()), (Some("pk"), Some("1")));

    tasks.iter().for_each(|t| t.abort());
    std::fs::remove_dir_all(&fake.dir).ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs python3; run with --ignored"]
async fn pollers_report_not_set_up_revoked_and_failures() {
    let fake = start_fake().await;
    let base = format!("http://127.0.0.1:{}", fake.port);

    // Nothing configured: no location, no calendar address, no homelab.
    let mut config = Config::default();
    config.homelab.portainer_url = String::new();
    config.homelab.uptime_kuma_url = String::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tasks = tvdash_widgets::spawn(&config, secrets(HashMap::new()), Endpoints { weather: base.clone() }, tx);
    let (w, c, h) = first_three(&mut rx).await;
    assert!(matches!(w, AppEvent::Weather(None)), "{w:?}");
    assert!(matches!(c, AppEvent::Calendar(CalendarState::NotSetUp)), "{c:?}");
    assert!(
        matches!(
            h,
            AppEvent::Homelab(HomelabStatus { monitors: Probe::NotConfigured, containers: Probe::NotConfigured })
        ),
        "{h:?}"
    );
    tasks.iter().for_each(|t| t.abort());

    // Revoked calendar address; wrong Portainer key; Kuma status page missing.
    let mut config = Config::default();
    config.general.latitude = 1.0;
    config.homelab.portainer_url = base.clone();
    config.homelab.portainer_endpoint_id = 2;
    config.homelab.uptime_kuma_url = base.clone();
    config.homelab.uptime_kuma_status_page = "missing".into();
    let reader = secrets(HashMap::from([
        (SecretKey::CalendarIcsUrl, format!("{base}/revoked.ics")),
        (SecretKey::PortainerApiKey, "wrong".into()),
    ]));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tasks = tvdash_widgets::spawn(&config, reader, Endpoints { weather: base.clone() }, tx);
    let (_, c, h) = first_three(&mut rx).await;
    assert!(matches!(c, AppEvent::Calendar(CalendarState::Revoked)), "{c:?}");
    let AppEvent::Homelab(h) = h else { panic!() };
    assert_eq!(h.containers, Probe::Failed("HTTP 401".into()));
    assert_eq!(h.monitors, Probe::Failed("HTTP 404".into()));
    tasks.iter().for_each(|t| t.abort());
    std::fs::remove_dir_all(&fake.dir).ok();
}
