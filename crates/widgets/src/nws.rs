//! National Weather Service active alerts (`api.weather.gov`), US only.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
pub use tvdash_core::events::{AlertLevel, NwsAlert, nws_alert_level as level};

use crate::{Result, WidgetError};

pub const DEFAULT_BASE: &str = "https://api.weather.gov";
pub const REFRESH: Duration = Duration::from_secs(120);
pub const REFRESH_ACTIVE: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
struct AlertCollection {
    #[serde(default)]
    features: Vec<AlertFeature>,
}

#[derive(Debug, Deserialize)]
struct AlertFeature {
    properties: AlertProps,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertProps {
    #[serde(default)]
    id: String,
    #[serde(default)]
    event: String,
    headline: Option<String>,
    #[serde(default)]
    area_desc: String,
    #[serde(default)]
    severity: String,
    effective: Option<String>,
    expires: Option<String>,
    ends: Option<String>,
    instruction: Option<String>,
    description: Option<String>,
    #[serde(default)]
    message_type: String,
}

fn parse_ts(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn parse_alerts(body: &str, now: DateTime<Utc>) -> std::result::Result<Vec<NwsAlert>, serde_json::Error> {
    let col: AlertCollection = serde_json::from_str(body)?;
    Ok(col
        .features
        .into_iter()
        .filter_map(|f| {
            let p = f.properties;
            if p.message_type.eq_ignore_ascii_case("Cancel") || p.id.is_empty() || p.event.is_empty() {
                return None;
            }
            let expires = parse_ts(p.expires.as_deref());
            if expires.is_some_and(|ex| ex <= now) {
                return None;
            }
            Some(NwsAlert {
                id: p.id,
                event: p.event,
                headline: p.headline,
                area: p.area_desc,
                severity: p.severity,
                effective: parse_ts(p.effective.as_deref()),
                expires,
                ends: parse_ts(p.ends.as_deref()),
                instruction: p.instruction,
                description: p.description,
                message_type: p.message_type,
            })
        })
        .collect())
}

/// Active alerts at a point. `contact` goes in the User-Agent, as NWS asks.
pub async fn fetch(
    http: &reqwest::Client,
    base: &str,
    (lat, lon): (f64, f64),
    contact: &str,
) -> Result<Vec<NwsAlert>> {
    let url = format!("{}/alerts/active?point={lat:.4},{lon:.4}", base.trim_end_matches('/'));
    let who = if contact.trim().is_empty() { "no contact set" } else { contact.trim() };
    let ua = format!("tvdash/{} ({who})", env!("CARGO_PKG_VERSION"));
    let resp = http
        .get(&url)
        .header("User-Agent", ua)
        .header("Accept", "application/geo+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(WidgetError::Status(resp.status().as_u16()));
    }
    let text = resp.text().await?;
    let alerts = parse_alerts(&text, Utc::now()).map_err(|_| WidgetError::Status(502))?;
    Ok(alerts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alerts_drops_cancel_and_expired_and_classifies_levels() {
        let now = DateTime::parse_from_rfc3339("2026-11-25T20:00:00Z").unwrap().with_timezone(&Utc);
        let json = r#"{
            "features": [
                {
                    "properties": {
                        "id": "urn:oid:1",
                        "event": "Tornado Warning",
                        "headline": "Tornado Warning issued",
                        "severity": "Extreme",
                        "areaDesc": "Douglas County",
                        "effective": "2026-11-25T19:30:00Z",
                        "expires": "2026-11-25T21:00:00Z",
                        "ends": "2026-11-25T20:45:00Z",
                        "instruction": "Take cover now.",
                        "messageType": "Alert"
                    }
                },
                {
                    "properties": {
                        "id": "urn:oid:2",
                        "event": "Winter Storm Watch",
                        "areaDesc": "Douglas County",
                        "severity": "Moderate",
                        "expires": "2026-11-25T23:00:00Z",
                        "messageType": "Alert"
                    }
                },
                {
                    "properties": {
                        "id": "urn:oid:3",
                        "event": "Wind Advisory",
                        "areaDesc": "Douglas County",
                        "severity": "Minor",
                        "expires": "2026-11-25T23:00:00Z",
                        "messageType": "Alert"
                    }
                },
                {
                    "properties": {
                        "id": "urn:oid:4",
                        "event": "Tornado Warning",
                        "areaDesc": "Old",
                        "expires": "2026-11-25T19:00:00Z",
                        "messageType": "Alert"
                    }
                },
                {
                    "properties": {
                        "id": "urn:oid:5",
                        "event": "Flood Warning",
                        "areaDesc": "Cancelled",
                        "expires": "2026-11-25T23:00:00Z",
                        "messageType": "Cancel"
                    }
                }
            ]
        }"#;

        let parsed = parse_alerts(json, now).unwrap();
        assert_eq!(parsed.len(), 3);
        let takeover = vec!["Tornado Warning".to_string()];
        assert_eq!(level(&parsed[0].event, &takeover), AlertLevel::Takeover);
        assert_eq!(level(&parsed[1].event, &takeover), AlertLevel::Card);
        assert_eq!(level(&parsed[2].event, &takeover), AlertLevel::Advisory);
    }
}
