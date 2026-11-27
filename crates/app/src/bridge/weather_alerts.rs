//! UI-thread NWS severe-weather alert state, dismissals, Watch card countdown,
//! and `/weather` HTTP sync for the Back pill extension.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Timer, TimerMode};
use tokio::sync::{mpsc::UnboundedSender, watch};
use tvdash_core::{
    Command,
    events::{AlertLevel, NwsAlert, nws_alert_level},
};

use super::{STATE, UiState};
use crate::{Dash, LaptopWindow, TvUi, WeatherAlertState, WeatherCardState, home_cmd::WeatherHttpState};

const CARD_SECS: i64 = 12;

#[derive(Default, Serialize, Deserialize)]
struct PersistedDismissals {
    /// alert id -> unix expiry timestamp
    dismissed: HashMap<String, i64>,
}

#[derive(Default)]
pub struct WeatherAlertsUi {
    pub takeover_list: Vec<String>,
    pub active_alerts: Vec<NwsAlert>,
    pub seen: HashMap<String, AlertLevel>,
    pub dismissed: HashMap<String, i64>,
    pub dismissed_loaded: bool,
    pub active_takeover: Option<NwsAlert>,
    pub details_mode: bool,
    pub card_queue: VecDeque<NwsAlert>,
    pub showing_card: Option<NwsAlert>,
    pub card_closes_in: i64,
    pub weather_http_tx: Option<watch::Sender<WeatherHttpState>>,
    pub cmd_tx: Option<UnboundedSender<Command>>,
    pub timer: Option<Timer>,
}

pub fn set_cmd_tx(cmd_tx: UnboundedSender<Command>) {
    STATE.with_borrow_mut(|s| s.weather_alerts.cmd_tx = Some(cmd_tx));
}

pub fn set_channel(tx: watch::Sender<WeatherHttpState>) {
    STATE.with_borrow_mut(|s| s.weather_alerts.weather_http_tx = Some(tx));
}

pub fn set_takeover_list(list: Vec<String>) {
    STATE.with_borrow_mut(|s| s.weather_alerts.takeover_list = list);
}

pub fn start_timer(laptop: slint::Weak<LaptopWindow>) {
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(l) = laptop.upgrade() else { return };
        STATE.with_borrow_mut(|s| on_tick(s, &l));
    });
    STATE.with_borrow_mut(|s| s.weather_alerts.timer = Some(timer));
}

fn store_path() -> PathBuf {
    crate::instance::runtime_state_path().with_file_name("weather-alerts.json")
}

fn load_dismissed(w: &mut WeatherAlertsUi) {
    if w.dismissed_loaded {
        return;
    }
    w.dismissed_loaded = true;
    let Ok(text) = std::fs::read_to_string(store_path()) else { return };
    if let Ok(p) = serde_json::from_str::<PersistedDismissals>(&text) {
        let now = Utc::now().timestamp();
        w.dismissed = p.dismissed.into_iter().filter(|&(_, exp)| exp > now).collect();
    }
}

fn save_dismissed(w: &WeatherAlertsUi) {
    let p = PersistedDismissals { dismissed: w.dismissed.clone() };
    if let Ok(json) = serde_json::to_string(&p) {
        let _ = std::fs::write(store_path(), json);
    }
}

fn until_dt(a: &NwsAlert) -> Option<DateTime<Utc>> {
    a.ends.or(a.expires)
}

fn fmt_time(dt: DateTime<Utc>, tz: Option<chrono_tz::Tz>) -> String {
    match tz {
        Some(z) => dt.with_timezone(&z).format("%-I:%M %p").to_string(),
        None => dt.with_timezone(&chrono::Local).format("%-I:%M %p").to_string(),
    }
}

fn format_area_until(a: &NwsAlert, tz: Option<chrono_tz::Tz>) -> String {
    let until = until_dt(a).map(|dt| format!("until {}", fmt_time(dt, tz)));
    match (a.area.trim().is_empty(), until) {
        (false, Some(u)) => format!("{} · {u}", a.area.trim()),
        (false, None) => a.area.trim().to_string(),
        (true, Some(u)) => u,
        (true, None) => String::new(),
    }
}

fn format_issued(a: &NwsAlert, tz: Option<chrono_tz::Tz>) -> String {
    match a.effective {
        Some(dt) => format!(
            "Issued {} · Source: weather.gov · Alerts for your saved location",
            fmt_time(dt, tz)
        ),
        None => "Source: weather.gov · Alerts for your saved location".into(),
    }
}

fn minutes_left(a: &NwsAlert) -> String {
    let Some(dt) = until_dt(a) else { return "—".into() };
    let mins = ((dt - Utc::now()).num_seconds().max(0) + 59) / 60;
    mins.to_string()
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "Extreme" => 4,
        "Severe" => 3,
        "Moderate" => 2,
        "Minor" => 1,
        _ => 0,
    }
}

pub fn on_alerts(s: &mut UiState, laptop: &LaptopWindow, alerts: Vec<NwsAlert>) {
    load_dismissed(&mut s.weather_alerts);
    let now = Utc::now().timestamp();
    s.weather_alerts.dismissed.retain(|_, exp| *exp > now);
    s.weather_alerts.active_alerts = alerts;

    let takeover_list = s.weather_alerts.takeover_list.clone();
    let mut new_takeover: Option<NwsAlert> = None;
    let mut banner_alert: Option<NwsAlert> = None;
    let mut best_advisory: Option<NwsAlert> = None;

    for a in &s.weather_alerts.active_alerts {
        let lvl = nws_alert_level(&a.event, &takeover_list);
        match lvl {
            AlertLevel::Takeover => {
                let exp_ts = until_dt(a).map(|d| d.timestamp()).unwrap_or(now + 3600);
                let prev_exp = s.weather_alerts.dismissed.get(&a.id).copied();
                let is_update = a.message_type.eq_ignore_ascii_case("Update")
                    && prev_exp.is_some_and(|pe| exp_ts > pe + 60);
                if prev_exp.is_none() || is_update {
                    if is_update {
                        s.weather_alerts.dismissed.remove(&a.id);
                    }
                    if new_takeover
                        .as_ref()
                        .is_none_or(|cur| severity_rank(&a.severity) > severity_rank(&cur.severity))
                    {
                        new_takeover = Some(a.clone());
                    }
                } else if banner_alert
                    .as_ref()
                    .is_none_or(|cur| severity_rank(&a.severity) > severity_rank(&cur.severity))
                {
                    banner_alert = Some(a.clone());
                }
            }
            AlertLevel::Card => {
                let exp_ts = until_dt(a).map(|d| d.timestamp()).unwrap_or(now + 3600);
                if !s.weather_alerts.seen.contains_key(&a.id)
                    && !s.weather_alerts.dismissed.contains_key(&a.id)
                    && exp_ts > now
                {
                    s.weather_alerts.card_queue.push_back(a.clone());
                }
            }
            AlertLevel::Advisory => {
                if best_advisory
                    .as_ref()
                    .is_none_or(|cur| severity_rank(&a.severity) > severity_rank(&cur.severity))
                {
                    best_advisory = Some(a.clone());
                }
            }
        }
        s.weather_alerts.seen.insert(a.id.clone(), lvl);
    }

    // Advisory line on Home
    let adv_line = best_advisory
        .map(|a| match until_dt(&a) {
            Some(dt) => format!("{} until {}", a.event, fmt_time(dt, s.tz)),
            None => a.event,
        })
        .unwrap_or_default();
    laptop.global::<Dash>().set_weather_alert_line(adv_line.into());

    // Banner
    let banner = banner_alert
        .map(|a| match until_dt(&a) {
            Some(dt) => format!("⚠ {} · until {}", a.event, fmt_time(dt, s.tz)),
            None => format!("⚠ {}", a.event),
        })
        .unwrap_or_default();
    laptop.global::<WeatherAlertState>().set_banner(banner.into());

    // Takeover
    if let Some(a) = new_takeover {
        let already_showing = s
            .weather_alerts
            .active_takeover
            .as_ref()
            .is_some_and(|cur| cur.id == a.id && laptop.global::<WeatherAlertState>().get_visible());
        if !already_showing {
            show_takeover(s, laptop, a, false);
        }
    }

    promote_card(s, laptop);
    publish_http(s, laptop);
}

fn show_takeover(s: &mut UiState, laptop: &LaptopWindow, a: NwsAlert, details_mode: bool) {
    let can_resume = !details_mode && s.player.is_some();
    let wa = laptop.global::<WeatherAlertState>();
    wa.set_alert_id(a.id.as_str().into());
    wa.set_event(a.event.as_str().into());
    wa.set_area_until(format_area_until(&a, s.tz).into());
    wa.set_instruction(
        a.instruction
            .as_deref()
            .or(a.headline.as_deref())
            .or(a.description.as_deref())
            .unwrap_or("Monitor local forecasts and follow official instructions.")
            .into(),
    );
    wa.set_issued_line(format_issued(&a, s.tz).into());
    wa.set_expires_in(minutes_left(&a).into());
    wa.set_can_resume(can_resume);
    wa.set_details_mode(details_mode);
    wa.set_focus(0);
    wa.set_visible(true);

    // Close screensaver & HUD so takeover is unobstructed
    let tv = laptop.global::<TvUi>();
    tv.set_ambient(false);
    tv.set_hud_open(false);
    laptop.global::<Dash>().set_screensaver_open(false);

    s.weather_alerts.active_takeover = Some(a);
    s.weather_alerts.details_mode = details_mode;
    if !details_mode && let Some(tx) = &s.weather_alerts.cmd_tx {
        let _ = tx.send(Command::WeatherTakeover(true));
    }
}

fn promote_card(s: &mut UiState, laptop: &LaptopWindow) {
    if s.weather_alerts.showing_card.is_some()
        || laptop.global::<WeatherAlertState>().get_visible()
    {
        return;
    }
    let Some(a) = s.weather_alerts.card_queue.pop_front() else {
        return;
    };
    let wc = laptop.global::<WeatherCardState>();
    wc.set_alert_id(a.id.as_str().into());
    wc.set_title(a.event.as_str().into());
    let sub = match (until_dt(&a), a.headline.as_deref()) {
        (Some(dt), Some(h)) => format!("Until {} · {h}", fmt_time(dt, s.tz)),
        (Some(dt), None) => format!("Until {} · {}", fmt_time(dt, s.tz), a.area),
        (None, Some(h)) => h.to_string(),
        (None, None) => a.area.clone(),
    };
    wc.set_sub(sub.into());
    wc.set_focus(0);
    wc.set_closes_in(CARD_SECS as i32);
    wc.set_visible(true);
    s.weather_alerts.showing_card = Some(a);
    s.weather_alerts.card_closes_in = CARD_SECS;
}

pub fn got_it(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        let details = s.weather_alerts.details_mode;
        if !details && let Some(a) = s.weather_alerts.active_takeover.take() {
            let exp = until_dt(&a).map(|d| d.timestamp()).unwrap_or_else(|| Utc::now().timestamp() + 3600);
            s.weather_alerts.dismissed.insert(a.id.clone(), exp);
            save_dismissed(&s.weather_alerts);
            let banner = match until_dt(&a) {
                Some(dt) => format!("⚠ {} · until {}", a.event, fmt_time(dt, s.tz)),
                None => format!("⚠ {}", a.event),
            };
            laptop.global::<WeatherAlertState>().set_banner(banner.into());
        } else {
            s.weather_alerts.active_takeover = None;
        }
        s.weather_alerts.details_mode = false;
        laptop.global::<WeatherAlertState>().set_visible(false);
        promote_card(s, laptop);
        publish_http(s, laptop);
    });
}

pub fn resume(laptop: &LaptopWindow) {
    got_it(laptop);
    STATE.with_borrow(|s| {
        if let Some(tx) = &s.weather_alerts.cmd_tx {
            let _ = tx.send(Command::WeatherTakeover(false));
        }
    });
}

pub fn dismiss_card(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if let Some(a) = s.weather_alerts.showing_card.take() {
            let exp = until_dt(&a).map(|d| d.timestamp()).unwrap_or_else(|| Utc::now().timestamp() + 3600);
            s.weather_alerts.dismissed.insert(a.id, exp);
            save_dismissed(&s.weather_alerts);
        }
        laptop.global::<WeatherCardState>().set_visible(false);
        promote_card(s, laptop);
        publish_http(s, laptop);
    });
}

pub fn open_details(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if let Some(a) = s.weather_alerts.showing_card.take() {
            laptop.global::<WeatherCardState>().set_visible(false);
            show_takeover(s, laptop, a, true);
            publish_http(s, laptop);
        }
    });
}

pub fn on_http_action(laptop: &LaptopWindow, id: &str, action: &str) {
    let matches_takeover = laptop.global::<WeatherAlertState>().get_visible()
        && laptop.global::<WeatherAlertState>().get_alert_id() == id;
    let matches_card = laptop.global::<WeatherCardState>().get_visible()
        && laptop.global::<WeatherCardState>().get_alert_id() == id;
    match action {
        "got-it" if matches_takeover => got_it(laptop),
        "resume" if matches_takeover => resume(laptop),
        "dismiss" if matches_card => dismiss_card(laptop),
        "details" if matches_card => open_details(laptop),
        _ => {}
    }
}

fn on_tick(s: &mut UiState, laptop: &LaptopWindow) {
    if laptop.global::<WeatherAlertState>().get_visible()
        && let Some(a) = &s.weather_alerts.active_takeover
    {
        laptop.global::<WeatherAlertState>().set_expires_in(minutes_left(a).into());
    }
    if laptop.global::<WeatherCardState>().get_visible() {
        s.weather_alerts.card_closes_in -= 1;
        if s.weather_alerts.card_closes_in <= 0 {
            dismiss_card_inner(s, laptop);
        } else {
            laptop
                .global::<WeatherCardState>()
                .set_closes_in(s.weather_alerts.card_closes_in as i32);
        }
    }
}

fn dismiss_card_inner(s: &mut UiState, laptop: &LaptopWindow) {
    if let Some(a) = s.weather_alerts.showing_card.take() {
        let exp = until_dt(&a).map(|d| d.timestamp()).unwrap_or_else(|| Utc::now().timestamp() + 3600);
        s.weather_alerts.dismissed.insert(a.id, exp);
        save_dismissed(&s.weather_alerts);
    }
    laptop.global::<WeatherCardState>().set_visible(false);
    promote_card(s, laptop);
    publish_http(s, laptop);
}

fn publish_http(s: &UiState, laptop: &LaptopWindow) {
    let Some(tx) = &s.weather_alerts.weather_http_tx else { return };
    let wa = laptop.global::<WeatherAlertState>();
    let wc = laptop.global::<WeatherCardState>();
    let st = if wa.get_visible() && !s.weather_alerts.details_mode {
        WeatherHttpState {
            level: "takeover".into(),
            id: wa.get_alert_id().to_string(),
            event: wa.get_event().to_string(),
            area_until: wa.get_area_until().to_string(),
            instruction: wa.get_instruction().to_string(),
            issued_line: wa.get_issued_line().to_string(),
            expires_in: wa.get_expires_in().to_string(),
            closes_in: 0,
        }
    } else if wc.get_visible() {
        WeatherHttpState {
            level: "card".into(),
            id: wc.get_alert_id().to_string(),
            event: wc.get_title().to_string(),
            area_until: wc.get_sub().to_string(),
            instruction: String::new(),
            issued_line: String::new(),
            expires_in: String::new(),
            closes_in: wc.get_closes_in() as i64,
        }
    } else {
        WeatherHttpState::default()
    };
    tx.send_replace(st);
}

/// Inject a synthetic NWS alert state for `tvdash --weather-test takeover|card|advisory|clear`.
pub fn inject_test(laptop: &LaptopWindow, kind: &str) {
    let now = Utc::now();
    let alerts = match kind {
        "clear" => {
            STATE.with_borrow_mut(|s| {
                s.weather_alerts.dismissed.clear();
                s.weather_alerts.seen.clear();
                s.weather_alerts.card_queue.clear();
                s.weather_alerts.showing_card = None;
                s.weather_alerts.active_takeover = None;
                save_dismissed(&s.weather_alerts);
            });
            laptop.global::<WeatherAlertState>().set_visible(false);
            laptop.global::<WeatherAlertState>().set_banner("".into());
            laptop.global::<WeatherCardState>().set_visible(false);
            Vec::new()
        }
        "card" => vec![NwsAlert {
            id: format!("test-card-{}", now.timestamp()),
            event: "Winter Storm Watch".into(),
            headline: Some("Heavy snow possible late tonight through Thursday.".into()),
            area: "Douglas County, KS".into(),
            severity: "Moderate".into(),
            effective: Some(now - chrono::Duration::minutes(10)),
            expires: Some(now + chrono::Duration::hours(6)),
            ends: Some(now + chrono::Duration::hours(6)),
            instruction: Some("Plan ahead for hazardous travel conditions.".into()),
            description: None,
            message_type: "Alert".into(),
        }],
        "advisory" => vec![NwsAlert {
            id: format!("test-adv-{}", now.timestamp()),
            event: "Wind Advisory".into(),
            headline: Some("Gusts up to 45 mph expected.".into()),
            area: "Douglas County, KS".into(),
            severity: "Minor".into(),
            effective: Some(now - chrono::Duration::minutes(10)),
            expires: Some(now + chrono::Duration::hours(4)),
            ends: Some(now + chrono::Duration::hours(4)),
            instruction: None,
            description: None,
            message_type: "Alert".into(),
        }],
        _ => vec![NwsAlert {
            id: format!("test-takeover-{}", now.timestamp()),
            event: "Tornado Warning".into(),
            headline: Some("Tornado Warning issued for Douglas County.".into()),
            area: "Douglas County, KS".into(),
            severity: "Extreme".into(),
            effective: Some(now - chrono::Duration::minutes(3)),
            expires: Some(now + chrono::Duration::minutes(33)),
            ends: Some(now + chrono::Duration::minutes(33)),
            instruction: Some(
                "TAKE COVER NOW! Move to a basement or an interior room on the lowest floor of a sturdy building."
                    .into(),
            ),
            description: None,
            message_type: "Alert".into(),
        }],
    };
    STATE.with_borrow_mut(|s| on_alerts(s, laptop, alerts));
}

