//! Open-Meteo forecast (no API key). Refreshed every 10 minutes.

use std::time::Duration;

use serde::Deserialize;
use tvdash_core::{
    config::Units,
    models::{Place, Weather},
};

use crate::{Result, WidgetError};

pub const REFRESH: Duration = Duration::from_secs(10 * 60);
pub const DEFAULT_BASE: &str = "https://api.open-meteo.com";
pub const GEOCODING_BASE: &str = "https://geocoding-api.open-meteo.com";

/// `None` when no location is configured (0, 0 is the default placeholder).
pub fn location(lat: f64, lon: f64) -> Option<(f64, f64)> {
    (lat != 0.0 || lon != 0.0).then_some((lat, lon))
}

pub async fn fetch(
    http: &reqwest::Client,
    base: &str,
    (lat, lon): (f64, f64),
    units: Units,
    timezone: &str,
) -> Result<Weather> {
    let unit = match units {
        Units::Fahrenheit => "fahrenheit",
        Units::Celsius => "celsius",
    };
    let tz = if timezone.is_empty() { "auto" } else { timezone };
    let resp = http
        .get(format!("{}/v1/forecast", base.trim_end_matches('/')))
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("current", "temperature_2m,weather_code".into()),
            ("daily", "temperature_2m_max,temperature_2m_min".into()),
            ("temperature_unit", unit.into()),
            ("timezone", tz.into()),
            ("forecast_days", "1".into()),
        ])
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(WidgetError::Status(status.as_u16()));
    }
    let f: Forecast = resp.json().await?;
    Ok(Weather {
        temperature: f.current.temperature_2m,
        code: f.current.weather_code,
        condition: condition(f.current.weather_code).into(),
        high: f.daily.temperature_2m_max.first().copied().flatten(),
        low: f.daily.temperature_2m_min.first().copied().flatten(),
    })
}

/// Places matching a name (Open-Meteo geocoding, no key), best match first.
pub async fn geocode(http: &reqwest::Client, base: &str, name: &str) -> Result<Vec<Place>> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(default)]
        results: Vec<Raw>,
    }
    #[derive(Deserialize)]
    struct Raw {
        name: String,
        latitude: f64,
        longitude: f64,
        #[serde(default)]
        admin1: Option<String>,
        #[serde(default)]
        country: Option<String>,
        #[serde(default)]
        timezone: Option<String>,
    }
    let resp = http
        .get(format!("{}/v1/search", base.trim_end_matches('/')))
        .query(&[("name", name.trim()), ("count", "8"), ("language", "en"), ("format", "json")])
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(WidgetError::Status(status.as_u16()));
    }
    let r: Resp = resp.json().await?;
    Ok(r.results
        .into_iter()
        .map(|p| {
            let label = [Some(p.name.clone()), p.admin1, p.country]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            Place { label, latitude: p.latitude, longitude: p.longitude, timezone: p.timezone.unwrap_or_default() }
        })
        .collect())
}

/// WMO weather interpretation codes, as used by Open-Meteo.
pub fn condition(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80..=82 => "Showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm, hail",
        _ => "—",
    }
}

#[derive(Debug, Deserialize)]
struct Forecast {
    current: Current,
    #[serde(default)]
    daily: Daily,
}

#[derive(Debug, Deserialize)]
struct Current {
    temperature_2m: f64,
    weather_code: u8,
}

#[derive(Debug, Default, Deserialize)]
struct Daily {
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forecast() {
        let f: Forecast = serde_json::from_str(
            r#"{"latitude":41.9,"current":{"time":"2026-11-11T21:00","temperature_2m":48.3,"weather_code":2},
                "daily":{"time":["2026-11-11"],"temperature_2m_max":[55.1],"temperature_2m_min":[39.0]}}"#,
        )
        .unwrap();
        assert_eq!((f.current.temperature_2m, f.current.weather_code), (48.3, 2));
        assert_eq!(f.daily.temperature_2m_max, [Some(55.1)]);
    }

    #[test]
    fn wmo_codes() {
        assert_eq!(condition(0), "Clear");
        assert_eq!(condition(81), "Showers");
        assert_eq!(condition(99), "Thunderstorm, hail");
        assert_eq!(condition(200), "—");
    }

    #[test]
    fn unset_location() {
        assert_eq!(location(0.0, 0.0), None);
        assert_eq!(location(41.88, -87.63), Some((41.88, -87.63)));
    }
}
