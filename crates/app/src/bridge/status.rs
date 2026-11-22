//! The header's weather, the weather location (Settings › Display › Location),
//! and Settings › Diagnostics.

use chrono::{DateTime, Utc};
use slint::{ComponentHandle, ModelRc, VecModel};
use tvdash_core::{
    config::{ClockFormat, Units},
    events::Service,
    models::{Place, Weather},
};

use super::{UiState, ago, home};
use crate::{Dash, Diagnostics, Display, HealthRow, LaptopWindow, Theme, format};

/// Weather older than this is shown dimmed with its age (it refreshes every 10 min).
const WEATHER_STALE: chrono::TimeDelta = chrono::TimeDelta::minutes(20);

/// Last success and failure of a service, with when.
#[derive(Default, Clone)]
pub(super) struct Health {
    ok: Option<(DateTime<Utc>, String)>,
    err: Option<(DateTime<Utc>, String)>,
}

// ── Diagnostics ─────────────────────────────────────────────────────

/// A service's latest result, for Diagnostics (and the weather chip's warning).
pub(super) fn record(s: &mut UiState, laptop: &LaptopWindow, service: Service, result: Result<String, String>) {
    let h = s.health.entry(service).or_default();
    match result {
        Ok(note) => h.ok = Some((Utc::now(), note)),
        Err(e) => h.err = Some((Utc::now(), e)),
    }
    if service == Service::Weather {
        let h = &s.health[&service];
        s.weather_err = h.err.as_ref().and_then(|(t, e)| {
            let newer = h.ok.as_ref().is_none_or(|(ok, _)| t > ok);
            newer.then(|| e.clone())
        });
        refresh_weather(s, laptop);
    }
    refresh_diagnostics(s, laptop);
}

/// The startup keyring self-test: Diagnostics plus the footer's status line.
pub(super) fn on_keyring(s: &mut UiState, laptop: &LaptopWindow, result: Result<(), String>) {
    let (theme, dash) = (laptop.global::<Theme>(), laptop.global::<Dash>());
    match &result {
        Ok(()) => {
            dash.set_keyring_status("GNOME Keyring connected".into());
            dash.set_keyring_dot(theme.get_ok());
        }
        Err(e) => {
            dash.set_keyring_status(format!("Keyring unavailable: {e}").into());
            dash.set_keyring_dot(theme.get_alert());
        }
    }
    record(s, laptop, Service::Keyring, result.map(|()| "read/write OK".into()));
}

pub(super) fn refresh_diagnostics(s: &UiState, laptop: &LaptopWindow) {
    let clock = s.clock.unwrap_or(ClockFormat::H24);
    let at = |t: &DateTime<Utc>| {
        let local = match s.tz {
            Some(tz) => t.with_timezone(&tz).naive_local(),
            None => t.with_timezone(&chrono::Local).naive_local(),
        };
        format!("{} · {}", format::hm(local, clock), ago(*t))
    };
    let rows: Vec<HealthRow> = Service::ALL
        .iter()
        .map(|svc| {
            let h = s.health.get(svc).cloned().unwrap_or_default();
            let failing = match (&h.ok, &h.err) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some((ok, _)), Some((err, _))) => err > ok,
            };
            HealthRow {
                name: svc.label().into(),
                state: if failing {
                    "warn"
                } else if h.ok.is_some() {
                    "ok"
                } else {
                    "off"
                }
                .into(),
                last_ok: h.ok.as_ref().map(|(t, note)| format!("{} · {note}", at(t))).unwrap_or_default().into(),
                last_error: h.err.as_ref().map(|(t, e)| format!("{} · {e}", at(t))).unwrap_or_default().into(),
            }
        })
        .collect();
    laptop.global::<Diagnostics>().set_rows(ModelRc::new(VecModel::from(rows)));
}

// ── Weather ─────────────────────────────────────────────────────────

pub(super) fn on_weather(s: &mut UiState, laptop: &LaptopWindow, weather: Option<Weather>) {
    match weather {
        Some(w) => {
            s.weather = Some((w, Utc::now()));
            s.weather_err = None;
            refresh_weather(s, laptop);
        }
        None => {
            s.weather = None;
            laptop.global::<Dash>().set_weather_on(false);
        }
    }
}

pub(super) fn refresh_weather(s: &UiState, laptop: &LaptopWindow) {
    let dash = laptop.global::<Dash>();
    dash.set_weather_warn(s.weather_err.is_some());
    match &s.weather {
        None => {
            if let Some(e) = &s.weather_err {
                dash.set_weather_on(true);
                dash.set_weather_temp("—".into());
                dash.set_weather_stale(true);
                dash.set_weather_note(format!("Unavailable: {e}").into());
            }
        }
        Some((w, at)) => {
            let c = format::weather_chip(w, s.units.unwrap_or(Units::Fahrenheit));
            let cond = c.detail.split(" · ").next().unwrap_or("").to_lowercase();
            let hl = c.detail.split_once(" · ").map_or(c.detail.as_str(), |(_, rest)| rest);
            dash.set_weather_on(true);
            dash.set_weather_temp(c.value.into());
            dash.set_weather_detail(hl.into());
            dash.set_weather_condition(cond.into());
            dash.set_weather_icon(format::weather_icon(w.code).into());
            let stale = Utc::now() - *at > WEATHER_STALE;
            dash.set_weather_stale(stale);
            dash.set_weather_note(if stale { format!("Updated {}", ago(*at)) } else { String::new() }.into());
        }
    }
}

// ── Location ────────────────────────────────────────────────────────

/// Place search results (Settings › Display › Location).
pub(super) fn on_places(s: &mut UiState, laptop: &LaptopWindow, query: &str, result: Result<Vec<Place>, String>) {
    let d = laptop.global::<Display>();
    if d.get_place_query().trim() != query.trim() {
        return; // an older search
    }
    d.set_place_searching(false);
    let (places, message) = match result {
        Ok(p) if p.is_empty() => (p, format!("No place called “{}”. Try a city name.", query.trim())),
        Ok(p) => (p, String::new()),
        Err(e) => (Vec::new(), format!("Couldn't look it up: {e}")),
    };
    let labels: Vec<slint::SharedString> = places.iter().map(|p| p.label.as_str().into()).collect();
    d.set_places(ModelRc::new(VecModel::from(labels)));
    d.set_place_message(message.into());
    s.places = places;
}

/// A place was picked: show it, follow its time zone, drop the old place's weather.
pub(super) fn on_location(s: &mut UiState, laptop: &LaptopWindow, name: String, timezone: &str) {
    let d = laptop.global::<Display>();
    d.set_location_name(name.into());
    d.set_places(ModelRc::default());
    d.set_place_query("".into());
    s.places.clear();
    let tz = (!timezone.is_empty()).then(|| timezone.parse().ok()).flatten();
    if tz.is_some() {
        s.tz = tz;
        crate::clock::set_tz(tz);
    }
    s.weather = None;
    laptop.global::<Dash>().set_weather_on(false);
    home::refresh_tv_card(s, laptop);
}
