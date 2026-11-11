//! Homelab status for the "N / N up" header chip, polled every minute:
//! Portainer containers and an Uptime Kuma status page (both optional).

use std::{collections::HashMap, time::Duration};

use serde::Deserialize;
use tvdash_core::models::{ContainerCounts, MonitorCounts, Probe};

use crate::{Result, WidgetError};

pub const REFRESH: Duration = Duration::from_secs(60);

/// `GET {url}/api/endpoints/{id}/docker/containers/json?all=1` with `X-API-Key`.
pub async fn portainer(http: &reqwest::Client, url: &str, endpoint: u32, api_key: &str) -> Result<ContainerCounts> {
    let resp = http
        .get(format!("{}/api/endpoints/{endpoint}/docker/containers/json", url.trim_end_matches('/')))
        .query(&[("all", "1")])
        .header("X-API-Key", api_key)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(WidgetError::Status(resp.status().as_u16()));
    }
    let containers: Vec<Container> = resp.json().await?;
    Ok(count_containers(&containers))
}

/// `GET {url}/api/status-page/{slug}` + `/api/status-page/heartbeat/{slug}`.
pub async fn uptime_kuma(http: &reqwest::Client, url: &str, slug: &str) -> Result<MonitorCounts> {
    let base = url.trim_end_matches('/');
    let get = |path: String| async move {
        let resp = http.get(path).send().await?;
        if resp.status().is_success() { Ok(resp) } else { Err(WidgetError::Status(resp.status().as_u16())) }
    };
    let page: StatusPage = get(format!("{base}/api/status-page/{slug}")).await?.json().await?;
    let beats: Heartbeats = get(format!("{base}/api/status-page/heartbeat/{slug}")).await?.json().await?;
    Ok(count_monitors(&page, &beats))
}

/// Turn a fetch result into a probe, labelling failures for the logs/UI.
pub fn probe<T>(r: Result<T>) -> Probe<T> {
    match r {
        Ok(v) => Probe::Ok(v),
        Err(e) => Probe::Failed(e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Container {
    #[serde(default)]
    state: String,
}

fn count_containers(c: &[Container]) -> ContainerCounts {
    ContainerCounts { running: c.iter().filter(|c| c.state == "running").count() as u32, total: c.len() as u32 }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusPage {
    #[serde(default)]
    public_group_list: Vec<Group>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Group {
    #[serde(default)]
    monitor_list: Vec<Monitor>,
}

#[derive(Debug, Deserialize)]
struct Monitor {
    id: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Heartbeats {
    #[serde(default)]
    heartbeat_list: HashMap<String, Vec<Beat>>,
}

#[derive(Debug, Deserialize)]
struct Beat {
    /// 0 down, 1 up, 2 pending, 3 maintenance.
    status: u8,
}

/// Up = latest heartbeat is up or in maintenance. No heartbeat yet counts as down.
fn count_monitors(page: &StatusPage, beats: &Heartbeats) -> MonitorCounts {
    let mut seen = std::collections::HashSet::new();
    let monitors: Vec<&Monitor> =
        page.public_group_list.iter().flat_map(|g| &g.monitor_list).filter(|m| seen.insert(m.id)).collect();
    let mut down = Vec::new();
    for m in &monitors {
        let latest = beats.heartbeat_list.get(&m.id.to_string()).and_then(|b| b.last()).map(|b| b.status);
        if !matches!(latest, Some(1 | 3)) {
            down.push(m.name.clone());
        }
    }
    MonitorCounts { up: (monitors.len() - down.len()) as u32, total: monitors.len() as u32, down }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_containers() {
        let c: Vec<Container> = serde_json::from_str(
            r#"[{"Id":"a","Names":["/plex"],"State":"running"},{"Id":"b","State":"exited"},{"Id":"c","State":"running"}]"#,
        )
        .unwrap();
        assert_eq!(count_containers(&c), ContainerCounts { running: 2, total: 3 });
    }

    #[test]
    fn counts_monitors_from_latest_beat() {
        let page: StatusPage = serde_json::from_str(
            r#"{"config":{"slug":"homelab"},"incident":null,"publicGroupList":[
                {"id":1,"name":"Media","monitorList":[{"id":1,"name":"Plex"},{"id":2,"name":"Jellyfin"}]},
                {"id":2,"name":"Infra","monitorList":[{"id":3,"name":"NAS"},{"id":4,"name":"Router"},{"id":1,"name":"Plex"}]}
            ],"maintenanceList":[]}"#,
        )
        .unwrap();
        let beats: Heartbeats = serde_json::from_str(
            r#"{"heartbeatList":{
                "1":[{"status":0,"time":"t1"},{"status":1,"time":"t2"}],
                "2":[{"status":1,"time":"t1"},{"status":0,"time":"t2"}],
                "3":[{"status":3,"time":"t1"}]
            },"uptimeList":{"1_24":0.99}}"#,
        )
        .unwrap();
        let m = count_monitors(&page, &beats);
        assert_eq!((m.up, m.total), (2, 4), "Plex up, NAS in maintenance; Jellyfin down; Router never reported");
        assert_eq!(m.down, ["Jellyfin", "Router"]);
    }
}
