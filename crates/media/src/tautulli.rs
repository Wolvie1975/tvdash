//! Tautulli API client for "Popular on Republic City" (`get_home_stats`).

use serde::Deserialize;
use tvdash_core::models::PopularItem;

use crate::{MediaError, Result};

#[derive(Clone)]
pub struct TautulliClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl TautulliClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim().trim_end_matches('/').to_owned();
        let api_key = api_key.into().trim().to_owned();
        if base_url.is_empty() || api_key.is_empty() {
            return Err(MediaError::NotConfigured);
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| MediaError::Protocol(e.to_string()))?;
        Ok(Self { http, base_url, api_key })
    }

    /// Fetch `top_tv` or `top_movies` from Tautulli's `get_home_stats`.
    pub async fn home_stats(&self, stat_id: &str, time_range_days: u32, count: u32) -> Result<Vec<PopularItem>> {
        let resp = self
            .http
            .get(format!("{}/api/v2", self.base_url))
            .query(&[
                ("apikey", self.api_key.as_str()),
                ("cmd", "get_home_stats"),
                ("stat_id", stat_id),
                ("stats_type", "plays"),
                ("time_range", &time_range_days.to_string()),
                ("stats_count", &count.to_string()),
            ])
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(MediaError::from_status(status, "tautulli get_home_stats"));
        }
        let body: ApiResponse = resp.json().await?;
        if body.response.result != "success" {
            return Err(MediaError::Protocol(
                body.response.message.unwrap_or_else(|| "tautulli returned error".into()),
            ));
        }

        let stat = body
            .response
            .data
            .into_iter()
            .find(|s| s.stat_id == stat_id)
            .unwrap_or_default();

        Ok(stat
            .rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| PopularItem {
                rank: (i + 1) as u32,
                rating_key: r.rating_key.as_string(),
                title: r.title,
                plays: r.total_plays.unwrap_or(0),
                thumb: r.thumb.filter(|t| !t.is_empty()),
            })
            .collect())
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    response: InnerResponse,
}

#[derive(Deserialize)]
struct InnerResponse {
    result: String,
    message: Option<String>,
    #[serde(default)]
    data: Vec<StatBlock>,
}

#[derive(Default, Deserialize)]
struct StatBlock {
    #[serde(default)]
    stat_id: String,
    #[serde(default)]
    rows: Vec<StatRow>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrInt {
    Str(String),
    Int(i64),
}

impl StringOrInt {
    fn as_string(&self) -> String {
        match self {
            Self::Str(s) => s.clone(),
            Self::Int(n) => n.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct StatRow {
    rating_key: StringOrInt,
    title: String,
    total_plays: Option<u32>,
    thumb: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_home_stats() {
        let payload = json!({
            "response": {
                "result": "success",
                "data": [{
                    "stat_id": "top_tv",
                    "rows": [
                        { "rating_key": 101, "title": "Harbor Lights", "total_plays": 23, "thumb": "/library/metadata/101/thumb/1" },
                        { "rating_key": "202", "title": "Deep Water", "total_plays": 15, "thumb": "" }
                    ]
                }]
            }
        });
        let parsed: ApiResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.response.result, "success");
        assert_eq!(parsed.response.data[0].rows.len(), 2);
        assert_eq!(parsed.response.data[0].rows[0].rating_key.as_string(), "101");
        assert_eq!(parsed.response.data[0].rows[0].total_plays, Some(23));
        assert_eq!(parsed.response.data[0].rows[1].rating_key.as_string(), "202");
    }
}
