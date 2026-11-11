//! Jellyfin client (M2).
//!
//! Sign-in: Quick Connect (`/QuickConnect/Initiate` → poll `/QuickConnect/Connect`
//! → `/Users/AuthenticateWithQuickConnect`), with username/password as fallback.
//! Every request carries:
//!
//! ```text
//! Authorization: MediaBrowser Client="tvdash", Device="tv-laptop", DeviceId="<uuid>",
//!                Version="0.1.0", Token="<token>"
//! ```
//!
//! Jellyfin 10.9 moved some per-user routes (`/Users/{id}/Items/Resume` →
//! `/UserItems/Resume?userId=`); the client tries the new route first and falls
//! back to the old one on 404.

use std::{collections::HashMap, sync::Mutex};

use reqwest::{Method, RequestBuilder};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use tvdash_core::models::MediaKind;

use crate::{MediaError, MediaItem, MediaSource, PlayState, Result, Session, SourceKind, ms_to_ticks, ticks_to_ms};

/// Build the `Authorization` header value Jellyfin expects.
pub fn auth_header(device_id: &str, token: Option<&str>) -> String {
    let mut h = format!(
        r#"MediaBrowser Client="{}", Device="tv-laptop", DeviceId="{device_id}", Version="{}""#,
        tvdash_core::APP_NAME,
        tvdash_core::APP_VERSION,
    );
    if let Some(t) = token {
        h.push_str(&format!(r#", Token="{t}""#));
    }
    h
}

/// A signed-in Jellyfin user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Auth {
    pub token: String,
    pub user_id: String,
}

/// A pending Quick Connect request.
#[derive(Debug, Clone)]
pub struct QuickConnect {
    /// Show this to the user; they enter it on a signed-in Jellyfin device.
    pub code: String,
    /// Keep private; used to poll and to exchange for a token.
    pub secret: String,
}

pub struct JellyfinClient {
    http: reqwest::Client,
    base: String,
    device_id: String,
    auth: Option<Auth>,
    /// Item for which `/Sessions/Playing` (playback start) has been sent.
    started: Mutex<Option<String>>,
}

impl JellyfinClient {
    /// `auth: None` gives a client that can only sign in.
    pub fn new(server_url: &str, device_id: &str, auth: Option<Auth>) -> Result<Self> {
        let base = server_url.trim().trim_end_matches('/').to_owned();
        if base.is_empty() {
            return Err(MediaError::NotConfigured);
        }
        Ok(Self {
            http: crate::http_client()?,
            base,
            device_id: device_id.to_owned(),
            auth,
            started: Mutex::new(None),
        })
    }

    pub fn auth(&self) -> Option<&Auth> {
        self.auth.as_ref()
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let token = self.auth.as_ref().map(|a| a.token.as_str());
        self.http
            .request(method, format!("{}{path}", self.base))
            .header("Authorization", auth_header(&self.device_id, token))
            .header("Accept", "application/json")
    }

    fn user(&self) -> Result<&Auth> {
        self.auth.as_ref().ok_or(MediaError::Unauthorized)
    }

    async fn send(rb: RequestBuilder, context: &str) -> Result<reqwest::Response> {
        let resp = rb.send().await?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else {
            Err(MediaError::from_status(status, context))
        }
    }

    async fn send_json<T: DeserializeOwned>(rb: RequestBuilder, context: &str) -> Result<T> {
        Self::send(rb, context)
            .await?
            .json()
            .await
            .map_err(|e| MediaError::Protocol(format!("{context}: {e}")))
    }

    // ── Sign-in ─────────────────────────────────────────────────────

    pub async fn quick_connect_initiate(&self) -> Result<QuickConnect> {
        let rb = self.request(Method::POST, "/QuickConnect/Initiate");
        match Self::send_json::<QuickConnectResult>(rb, "Quick Connect").await {
            Ok(r) => Ok(QuickConnect {
                code: r.code,
                secret: r.secret,
            }),
            // Jellyfin answers 401 when Quick Connect is switched off.
            Err(MediaError::Unauthorized) => Err(MediaError::Protocol(
                "Quick Connect is disabled — enable it in Jellyfin's Dashboard › General".into(),
            )),
            Err(e) => Err(e),
        }
    }

    /// True once the user has approved the code.
    pub async fn quick_connect_approved(&self, secret: &str) -> Result<bool> {
        let rb = self.request(Method::GET, "/QuickConnect/Connect").query(&[("secret", secret)]);
        match Self::send_json::<QuickConnectResult>(rb, "Quick Connect").await {
            Ok(r) => Ok(r.authenticated),
            Err(MediaError::NotFound) => Err(MediaError::Protocol("the code expired — try again".into())),
            Err(e) => Err(e),
        }
    }

    pub async fn authenticate_with_quick_connect(&self, secret: &str) -> Result<Auth> {
        let rb = self
            .request(Method::POST, "/Users/AuthenticateWithQuickConnect")
            .json(&json!({ "Secret": secret }));
        Ok(Self::send_json::<AuthResult>(rb, "Quick Connect sign-in").await?.into())
    }

    pub async fn authenticate_by_name(&self, username: &str, password: &str) -> Result<Auth> {
        let rb = self
            .request(Method::POST, "/Users/AuthenticateByName")
            .json(&json!({ "Username": username, "Pw": password }));
        Ok(Self::send_json::<AuthResult>(rb, "sign-in").await?.into())
    }

    // ── Library ─────────────────────────────────────────────────────

    /// GET `new_path`; on 404 retry the pre-10.9 `old_path`.
    async fn get_with_fallback<T: DeserializeOwned>(
        &self,
        new_path: &str,
        old_path: &str,
        query: &[(&str, String)],
        context: &str,
    ) -> Result<T> {
        let user = self.user()?;
        let mut q = query.to_vec();
        q.push(("userId", user.user_id.clone()));
        match Self::send_json(self.request(Method::GET, new_path).query(&q), context).await {
            Err(MediaError::NotFound) => {
                Self::send_json(self.request(Method::GET, old_path).query(query), context).await
            }
            other => other,
        }
    }

    fn base_item_query(limit: u32) -> Vec<(&'static str, String)> {
        vec![
            ("limit", limit.to_string()),
            ("enableUserData", "true".into()),
            ("enableImageTypes", "Primary".into()),
            ("imageTypeLimit", "1".into()),
        ]
    }
}

#[async_trait::async_trait]
impl MediaSource for JellyfinClient {
    fn kind(&self) -> SourceKind {
        SourceKind::Jellyfin
    }

    async fn now_playing(&self) -> Result<Vec<Session>> {
        self.user()?;
        let rb = self
            .request(Method::GET, "/Sessions")
            .query(&[("activeWithinSeconds", "60")]);
        let sessions: Vec<SessionInfo> = Self::send_json(rb, "sessions").await?;
        Ok(sessions.into_iter().filter_map(SessionInfo::into_session).collect())
    }

    async fn continue_watching(&self) -> Result<Vec<MediaItem>> {
        let user_id = self.user()?.user_id.clone();
        let mut q = Self::base_item_query(20);
        q.push(("mediaTypes", "Video".into()));
        let result: ItemsResult = self
            .get_with_fallback(
                "/UserItems/Resume",
                &format!("/Users/{user_id}/Items/Resume"),
                &q,
                "continue watching",
            )
            .await?;
        Ok(result.items.into_iter().map(BaseItemDto::into_media_item).collect())
    }

    async fn recently_added(&self, limit: u32) -> Result<Vec<MediaItem>> {
        let user_id = self.user()?.user_id.clone();
        let q = Self::base_item_query(limit);
        let items: Vec<BaseItemDto> = self
            .get_with_fallback(
                "/Items/Latest",
                &format!("/Users/{user_id}/Items/Latest"),
                &q,
                "recently added",
            )
            .await?;
        Ok(items.into_iter().map(BaseItemDto::into_media_item).collect())
    }

    async fn search(&self, query: &str) -> Result<Vec<MediaItem>> {
        let user = self.user()?;
        let mut q = Self::base_item_query(50);
        q.extend([
            ("searchTerm", query.to_owned()),
            ("includeItemTypes", "Movie,Series,Episode".into()),
            ("recursive", "true".into()),
            ("userId", user.user_id.clone()),
        ]);
        let result: ItemsResult = Self::send_json(self.request(Method::GET, "/Items").query(&q), "search").await?;
        Ok(result.items.into_iter().map(BaseItemDto::into_media_item).collect())
    }

    async fn poster(&self, item: &MediaItem, max_h: u32) -> Result<Vec<u8>> {
        let id = item.poster.as_deref().ok_or(MediaError::NotFound)?;
        let rb = self
            .request(Method::GET, &format!("/Items/{id}/Images/Primary"))
            .query(&[("maxHeight", max_h.to_string()), ("quality", "90".into())]);
        Ok(Self::send(rb, "poster").await?.bytes().await?.to_vec())
    }

    async fn stream_url(&self, item: &MediaItem) -> Result<String> {
        let token = &self.user()?.token;
        Ok(format!("{}/Videos/{}/stream?static=true&api_key={token}", self.base, item.id))
    }

    async fn report_progress(&self, item: &MediaItem, pos_ms: u64, state: PlayState) -> Result<()> {
        self.user()?;
        let body = progress_body(&item.id, pos_ms, state);
        let path = {
            let mut started = self.started.lock().unwrap();
            match state {
                PlayState::Stopped => {
                    *started = None;
                    "/Sessions/Playing/Stopped"
                }
                _ if started.as_deref() != Some(item.id.as_str()) => {
                    *started = Some(item.id.clone());
                    "/Sessions/Playing"
                }
                _ => "/Sessions/Playing/Progress",
            }
        };
        Self::send(self.request(Method::POST, path).json(&body), "progress").await?;
        Ok(())
    }
}

fn progress_body(item_id: &str, pos_ms: u64, state: PlayState) -> serde_json::Value {
    json!({
        "ItemId": item_id,
        "PositionTicks": ms_to_ticks(pos_ms),
        "IsPaused": state == PlayState::Paused,
        "CanSeek": true,
        "PlayMethod": "DirectStream",
    })
}

// ── Wire types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectResult {
    #[serde(default)]
    authenticated: bool,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthResult {
    access_token: String,
    user: AuthUser,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthUser {
    id: String,
}

impl From<AuthResult> for Auth {
    fn from(r: AuthResult) -> Self {
        Auth {
            token: r.access_token,
            user_id: r.user.id,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemsResult {
    #[serde(default)]
    items: Vec<BaseItemDto>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct BaseItemDto {
    id: String,
    name: Option<String>,
    #[serde(rename = "Type")]
    item_type: Option<String>,
    series_name: Option<String>,
    series_id: Option<String>,
    series_primary_image_tag: Option<String>,
    parent_index_number: Option<u32>,
    index_number: Option<u32>,
    production_year: Option<u32>,
    run_time_ticks: Option<u64>,
    user_data: Option<UserData>,
    image_tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct UserData {
    playback_position_ticks: Option<u64>,
}

impl BaseItemDto {
    fn into_media_item(self) -> MediaItem {
        let kind = match self.item_type.as_deref() {
            Some("Movie") => MediaKind::Movie,
            Some("Series") => MediaKind::Series,
            Some("Episode") => MediaKind::Episode,
            _ => MediaKind::Other,
        };
        let has_primary = self.image_tags.as_ref().is_some_and(|t| t.contains_key("Primary"));
        // Episodes use the series poster (portrait) rather than the 16:9 still.
        let poster = match (&kind, &self.series_id, &self.series_primary_image_tag) {
            (MediaKind::Episode, Some(sid), Some(_)) => Some(sid.clone()),
            _ if has_primary => Some(self.id.clone()),
            _ => None,
        };
        let name = self.name.unwrap_or_default();
        let (title, subtitle, episode_label) = match kind {
            MediaKind::Episode => {
                let label = match (self.parent_index_number, self.index_number) {
                    (Some(s), Some(e)) => Some(format!("S{s} · E{e}")),
                    (None, Some(e)) => Some(format!("E{e}")),
                    _ => None,
                };
                (self.series_name.unwrap_or_else(|| name.clone()), Some(name), label)
            }
            _ => (name, self.production_year.map(|y| y.to_string()), None),
        };
        let resume_ms = self
            .user_data
            .and_then(|u| u.playback_position_ticks)
            .filter(|&t| t > 0)
            .map(ticks_to_ms);
        MediaItem {
            source: SourceKind::Jellyfin,
            id: self.id,
            kind,
            title,
            subtitle,
            episode_label,
            duration_ms: self.run_time_ticks.map(ticks_to_ms),
            resume_ms,
            poster,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct SessionInfo {
    device_name: Option<String>,
    client: Option<String>,
    now_playing_item: Option<BaseItemDto>,
    play_state: Option<SessionPlayState>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct SessionPlayState {
    position_ticks: Option<u64>,
    is_paused: bool,
}

impl SessionInfo {
    fn into_session(self) -> Option<Session> {
        let item = self.now_playing_item?.into_media_item();
        let ps = self.play_state.unwrap_or_default();
        Some(Session {
            source: SourceKind::Jellyfin,
            item,
            device: self.device_name.or(self.client).unwrap_or_else(|| "Unknown device".into()),
            state: if ps.is_paused { PlayState::Paused } else { PlayState::Playing },
            position_ms: ps.position_ticks.map(ticks_to_ms).unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_header_format() {
        let h = auth_header("dev-1", Some("tok"));
        assert!(h.starts_with(r#"MediaBrowser Client="tvdash", Device="tv-laptop", DeviceId="dev-1""#));
        assert!(h.ends_with(r#", Token="tok""#));
        assert!(!auth_header("dev-1", None).contains("Token"));
    }

    #[test]
    fn maps_episode() {
        let dto: BaseItemDto = serde_json::from_value(json!({
            "Id": "ep1", "Name": "The Pilot", "Type": "Episode",
            "SeriesName": "Some Show", "SeriesId": "ser1", "SeriesPrimaryImageTag": "abc",
            "ParentIndexNumber": 2, "IndexNumber": 5,
            "RunTimeTicks": 27_000_000_000u64,
            "UserData": { "PlaybackPositionTicks": 9_000_000_000u64 },
            "ImageTags": { "Primary": "def" }
        }))
        .unwrap();
        let item = dto.into_media_item();
        assert_eq!(item.kind, MediaKind::Episode);
        assert_eq!(item.title, "Some Show");
        assert_eq!(item.subtitle.as_deref(), Some("The Pilot"));
        assert_eq!(item.episode_label.as_deref(), Some("S2 · E5"));
        assert_eq!(item.poster.as_deref(), Some("ser1"));
        assert_eq!(item.duration_ms, Some(2_700_000));
        assert_eq!(item.resume_ms, Some(900_000));
        assert!((item.progress().unwrap() - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn maps_movie_without_image() {
        let dto: BaseItemDto =
            serde_json::from_value(json!({ "Id": "m1", "Name": "A Film", "Type": "Movie", "ProductionYear": 1999 }))
                .unwrap();
        let item = dto.into_media_item();
        assert_eq!(item.kind, MediaKind::Movie);
        assert_eq!(item.subtitle.as_deref(), Some("1999"));
        assert_eq!(item.poster, None);
        assert_eq!(item.resume_ms, None);
    }

    #[test]
    fn parses_resume_result_and_sessions() {
        let r: ItemsResult = serde_json::from_value(json!({
            "Items": [{ "Id": "a", "Name": "x", "Type": "Movie" }], "TotalRecordCount": 1
        }))
        .unwrap();
        assert_eq!(r.items.len(), 1);

        let sessions: Vec<SessionInfo> = serde_json::from_value(json!([
            { "DeviceName": "Living Room", "NowPlayingItem": { "Id": "a", "Name": "x", "Type": "Movie" },
              "PlayState": { "PositionTicks": 600_000_000u64, "IsPaused": true } },
            { "DeviceName": "Idle phone" }
        ]))
        .unwrap();
        let s: Vec<Session> = sessions.into_iter().filter_map(SessionInfo::into_session).collect();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].device, "Living Room");
        assert_eq!(s[0].state, PlayState::Paused);
        assert_eq!(s[0].position_ms, 60_000);
    }

    #[test]
    fn progress_body_uses_ticks() {
        let b = progress_body("a", 1_000, PlayState::Paused);
        assert_eq!(b["PositionTicks"], 10_000_000u64);
        assert_eq!(b["IsPaused"], true);
    }

    #[test]
    fn empty_url_is_not_configured() {
        assert!(matches!(JellyfinClient::new("  ", "d", None), Err(MediaError::NotConfigured)));
    }
}
