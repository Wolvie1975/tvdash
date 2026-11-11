//! Plex (M3): PIN sign-in and server discovery through plex.tv, then the
//! media server's own API behind [`MediaSource`].
//!
//! Every call sends `Accept: application/json`, `X-Plex-Token` (once known) and
//! `X-Plex-Client-Identifier`. Server responses are wrapped in `MediaContainer`.

use std::time::Duration;

use reqwest::{Method, RequestBuilder};
use serde::{Deserialize, de::DeserializeOwned};
use tracing::{debug, info, warn};
use tvdash_core::models::MediaKind;

use crate::{MediaError, MediaItem, MediaSource, PlayState, Result, Session, SourceKind};

/// Product name sent as `X-Plex-Product`.
pub const PRODUCT: &str = tvdash_core::APP_NAME;
pub const DEFAULT_ACCOUNT_URL: &str = "https://plex.tv";
const DEVICE_NAME: &str = "tv-laptop";

fn identify(rb: RequestBuilder, client_id: &str, token: Option<&str>) -> RequestBuilder {
    let rb = rb
        .header("Accept", "application/json")
        .header("X-Plex-Product", PRODUCT)
        .header("X-Plex-Version", tvdash_core::APP_VERSION)
        .header("X-Plex-Client-Identifier", client_id)
        .header("X-Plex-Device-Name", DEVICE_NAME)
        .header("X-Plex-Platform", "Linux");
    match token {
        Some(t) => rb.header("X-Plex-Token", t),
        None => rb,
    }
}

async fn send(rb: RequestBuilder, context: &str) -> Result<reqwest::Response> {
    let resp = rb.send().await?;
    let status = resp.status();
    if status.is_success() { Ok(resp) } else { Err(MediaError::from_status(status, context)) }
}

async fn send_json<T: DeserializeOwned>(rb: RequestBuilder, context: &str) -> Result<T> {
    send(rb, context).await?.json().await.map_err(|e| MediaError::Protocol(format!("{context}: {e}")))
}

// ── plex.tv account ─────────────────────────────────────────────────

/// A PIN waiting to be linked at plex.tv/link.
#[derive(Debug, Clone)]
pub struct Pin {
    pub id: u64,
    /// 4 characters, shown to the user.
    pub code: String,
    /// How long the code stays valid.
    pub expires_in: Duration,
}

/// A media server the account can reach, with the connection we picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredServer {
    pub name: String,
    pub url: String,
    /// Server-specific token (same as the account token for owned servers).
    pub access_token: String,
}

pub struct PlexAccount {
    http: reqwest::Client,
    base: String,
    client_id: String,
}

impl PlexAccount {
    /// `base` is normally [`DEFAULT_ACCOUNT_URL`].
    pub fn new(base: &str, client_id: &str) -> Result<Self> {
        Ok(Self {
            http: crate::http_client()?,
            base: base.trim_end_matches('/').to_owned(),
            client_id: client_id.to_owned(),
        })
    }

    fn request(&self, method: Method, path: &str, token: Option<&str>) -> RequestBuilder {
        identify(self.http.request(method, format!("{}{path}", self.base)), &self.client_id, token)
    }

    pub async fn create_pin(&self) -> Result<Pin> {
        let r: PinResponse = send_json(self.request(Method::POST, "/api/v2/pins", None), "Plex PIN").await?;
        Ok(Pin { id: r.id, code: r.code, expires_in: Duration::from_secs(r.expires_in.unwrap_or(900)) })
    }

    /// The account token once the user has linked the PIN.
    pub async fn check_pin(&self, id: u64) -> Result<Option<String>> {
        let rb = self.request(Method::GET, &format!("/api/v2/pins/{id}"), None);
        match send_json::<PinResponse>(rb, "Plex PIN").await {
            Ok(r) => Ok(r.auth_token.filter(|t| !t.is_empty())),
            Err(MediaError::NotFound) => Err(MediaError::Protocol("the code expired — try again".into())),
            Err(e) => Err(e),
        }
    }

    /// Find the account's media servers and pick a reachable connection on one.
    /// Owned servers first; per server: local → remote → relay.
    pub async fn discover(&self, token: &str) -> Result<DiscoveredServer> {
        let rb = self
            .request(Method::GET, "/api/v2/resources", Some(token))
            .query(&[("includeHttps", "1"), ("includeRelay", "1")]);
        let mut resources: Vec<Resource> = send_json(rb, "Plex servers").await?;
        resources.retain(|r| r.provides.split(',').any(|p| p == "server"));
        resources.sort_by_key(|r| !r.owned);
        if resources.is_empty() {
            return Err(MediaError::Protocol("no Plex Media Server found on this account".into()));
        }
        for res in resources {
            let server_token = res.access_token.clone().unwrap_or_else(|| token.to_owned());
            for uri in res.candidate_uris() {
                debug!(server = %res.name, %uri, "trying Plex connection");
                match self.reachable(&uri, &server_token).await {
                    Ok(()) => {
                        if uri.starts_with("http://") {
                            warn!(server = %res.name, %uri, "using a plain-HTTP local address (plex.direct didn't work)");
                        }
                        info!(server = %res.name, %uri, "using Plex connection");
                        return Ok(DiscoveredServer {
                            name: res.name,
                            url: uri.trim_end_matches('/').to_owned(),
                            access_token: server_token,
                        });
                    }
                    Err(why) => debug!(%uri, %why, "Plex connection failed"),
                }
            }
        }
        Err(MediaError::Unreachable("none of the Plex server's addresses answered".into()))
    }

    async fn reachable(&self, uri: &str, token: &str) -> std::result::Result<(), String> {
        let rb = identify(
            self.http.get(format!("{}/identity", uri.trim_end_matches('/'))).timeout(Duration::from_secs(4)),
            &self.client_id,
            Some(token),
        );
        match rb.send().await {
            Ok(r) if r.status().is_success() => Ok(()),
            Ok(r) => Err(format!("HTTP {}", r.status())),
            Err(e) => Err(error_chain(&e)),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PinResponse {
    id: u64,
    code: String,
    #[serde(default)]
    auth_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Resource {
    name: String,
    provides: String,
    owned: bool,
    access_token: Option<String>,
    connections: Vec<Connection>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Connection {
    uri: String,
    /// Raw IP or host, e.g. `192.168.1.100`.
    address: String,
    port: u16,
    local: bool,
    relay: bool,
}

/// reqwest's top-level message hides the cause ("error sending request");
/// include the sources so DNS/TLS failures are visible in the log.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut src = e.source();
    while let Some(inner) = src {
        s.push_str(": ");
        s.push_str(&inner.to_string());
        src = inner.source();
    }
    s
}

impl Resource {
    /// Addresses to try, best first. Each local `*.plex.direct` address is followed
    /// by its plain `http://IP:port` form: many routers block DNS answers that point
    /// at private IPs ("DNS rebinding protection"), which breaks plex.direct on the
    /// LAN. Plex's own apps fall back the same way.
    fn candidate_uris(&self) -> Vec<String> {
        let mut out = Vec::new();
        for c in self.ordered_connections() {
            out.push(c.uri.clone());
            if c.local && !c.relay && !c.address.is_empty() && c.port != 0 {
                let plain = format!("http://{}:{}", c.address, c.port);
                if !out.contains(&plain) {
                    out.push(plain);
                }
            }
        }
        out
    }

    fn ordered_connections(&self) -> Vec<Connection> {
        let mut c = self.connections.clone();
        // Stable sort keeps plex.tv's order within each group.
        c.sort_by_key(|c| match (c.relay, c.local) {
            (false, true) => 0,
            (false, false) => 1,
            (true, _) => 2,
        });
        c
    }
}

// ── Media server ────────────────────────────────────────────────────

pub struct PlexClient {
    http: reqwest::Client,
    base: String,
    token: String,
    client_id: String,
}

impl PlexClient {
    pub fn new(server_url: &str, token: &str, client_id: &str) -> Result<Self> {
        let base = server_url.trim().trim_end_matches('/').to_owned();
        if base.is_empty() || token.is_empty() {
            return Err(MediaError::NotConfigured);
        }
        Ok(Self { http: crate::http_client()?, base, token: token.to_owned(), client_id: client_id.to_owned() })
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        identify(self.http.request(method, format!("{}{path}", self.base)), &self.client_id, Some(&self.token))
    }

    async fn metadata(&self, path: &str, query: &[(&str, String)], context: &str) -> Result<Vec<Metadata>> {
        let r: Container = send_json(self.request(Method::GET, path).query(query), context).await?;
        Ok(r.media_container.metadata)
    }

    fn items(meta: Vec<Metadata>) -> Vec<MediaItem> {
        meta.into_iter().filter_map(Metadata::into_media_item).collect()
    }
}

#[async_trait::async_trait]
impl MediaSource for PlexClient {
    fn kind(&self) -> SourceKind {
        SourceKind::Plex
    }

    async fn now_playing(&self) -> Result<Vec<Session>> {
        let meta = self.metadata("/status/sessions", &[], "sessions").await?;
        Ok(meta
            .into_iter()
            // Our own mpv playback reports as a session too; the card already shows it.
            .filter(|m| m.player.as_ref().is_none_or(|p| p.machine_identifier != self.client_id))
            .filter_map(Metadata::into_session)
            .collect())
    }

    async fn continue_watching(&self) -> Result<Vec<MediaItem>> {
        let meta = self.metadata("/library/onDeck", &[], "continue watching").await?;
        Ok(Self::items(meta))
    }

    async fn recently_added(&self, limit: u32) -> Result<Vec<MediaItem>> {
        let q = [("X-Plex-Container-Start", "0".to_owned()), ("X-Plex-Container-Size", limit.to_string())];
        Ok(Self::items(self.metadata("/library/recentlyAdded", &q, "recently added").await?))
    }

    async fn search(&self, query: &str) -> Result<Vec<MediaItem>> {
        let rb = self.request(Method::GET, "/hubs/search").query(&[("query", query), ("limit", "20")]);
        let r: Container = send_json(rb, "search").await?;
        Ok(r.media_container.hub.into_iter().flat_map(|h| h.metadata).filter_map(Metadata::into_media_item).collect())
    }

    async fn poster(&self, item: &MediaItem, max_h: u32) -> Result<Vec<u8>> {
        let thumb = item.poster.as_deref().ok_or(MediaError::NotFound)?;
        let rb = self.request(Method::GET, "/photo/:/transcode").query(&[
            ("url", thumb.to_owned()),
            ("height", max_h.to_string()),
            ("width", (max_h * 2 / 3).to_string()),
            ("minSize", "1".into()),
            ("upscale", "1".into()),
        ]);
        Ok(send(rb, "poster").await?.bytes().await?.to_vec())
    }

    async fn stream_url(&self, item: &MediaItem) -> Result<String> {
        // Rows don't carry part keys; look the item up for its first file.
        let meta = self.metadata(&format!("/library/metadata/{}", item.id), &[], "item").await?;
        let key = meta
            .into_iter()
            .flat_map(|m| m.media)
            .flat_map(|m| m.part)
            .map(|p| p.key)
            .find(|k| !k.is_empty())
            .ok_or_else(|| MediaError::Protocol("item has no playable file".into()))?;
        Ok(format!("{}{key}?X-Plex-Token={}", self.base, self.token))
    }

    async fn report_progress(&self, item: &MediaItem, pos_ms: u64, state: PlayState) -> Result<()> {
        let state = match state {
            PlayState::Playing => "playing",
            PlayState::Paused => "paused",
            PlayState::Stopped => "stopped",
        };
        let mut q = vec![
            ("ratingKey", item.id.clone()),
            ("key", format!("/library/metadata/{}", item.id)),
            ("state", state.to_owned()),
            ("time", pos_ms.to_string()),
        ];
        if let Some(d) = item.duration_ms {
            q.push(("duration", d.to_string()));
        }
        send(self.request(Method::GET, "/:/timeline").query(&q), "progress").await?;
        Ok(())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct Container {
    media_container: MediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct MediaContainer {
    metadata: Vec<Metadata>,
    hub: Vec<Hub>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct Hub {
    metadata: Vec<Metadata>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Metadata {
    rating_key: String,
    #[serde(rename = "type")]
    kind: String,
    title: String,
    grandparent_title: Option<String>,
    parent_index: Option<u32>,
    index: Option<u32>,
    year: Option<u32>,
    duration: Option<u64>,
    view_offset: Option<u64>,
    thumb: Option<String>,
    grandparent_thumb: Option<String>,
    #[serde(rename = "Media")]
    media: Vec<Media>,
    #[serde(rename = "Player")]
    player: Option<Player>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Media {
    #[serde(rename = "Part")]
    part: Vec<Part>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Part {
    key: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Player {
    title: Option<String>,
    product: Option<String>,
    state: String,
    machine_identifier: String,
}

impl Metadata {
    fn into_media_item(self) -> Option<MediaItem> {
        if self.rating_key.is_empty() {
            return None;
        }
        let kind = match self.kind.as_str() {
            "movie" => MediaKind::Movie,
            "show" => MediaKind::Series,
            "episode" => MediaKind::Episode,
            "season" | "artist" | "album" | "track" | "photo" => return None,
            _ => MediaKind::Other,
        };
        let (title, subtitle, episode_label, poster) = match kind {
            MediaKind::Episode => {
                let label = match (self.parent_index, self.index) {
                    (Some(s), Some(e)) => Some(format!("S{s} · E{e}")),
                    (None, Some(e)) => Some(format!("E{e}")),
                    _ => None,
                };
                (
                    self.grandparent_title.unwrap_or_else(|| self.title.clone()),
                    Some(self.title),
                    label,
                    self.grandparent_thumb.or(self.thumb),
                )
            }
            _ => (self.title, self.year.map(|y| y.to_string()), None, self.thumb),
        };
        Some(MediaItem {
            source: SourceKind::Plex,
            id: self.rating_key,
            kind,
            title,
            subtitle,
            episode_label,
            duration_ms: self.duration,
            resume_ms: self.view_offset.filter(|&v| v > 0),
            poster,
        })
    }

    fn into_session(mut self) -> Option<Session> {
        let player = self.player.take().unwrap_or_default();
        let position_ms = self.view_offset.unwrap_or(0);
        let item = self.into_media_item()?;
        Some(Session {
            source: SourceKind::Plex,
            item,
            device: player.title.or(player.product).unwrap_or_else(|| "Unknown device".into()),
            state: if player.state == "paused" { PlayState::Paused } else { PlayState::Playing },
            position_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_episode_and_movie() {
        let c: Container = serde_json::from_value(json!({ "MediaContainer": { "size": 3, "Metadata": [
            { "ratingKey": "101", "type": "episode", "title": "The Pilot", "grandparentTitle": "Some Show",
              "parentIndex": 2, "index": 5, "duration": 2_700_000, "viewOffset": 900_000,
              "thumb": "/library/metadata/101/thumb/1", "grandparentThumb": "/library/metadata/100/thumb/1" },
            { "ratingKey": "200", "type": "movie", "title": "A Film", "year": 1999, "thumb": "/t/200" },
            { "ratingKey": "300", "type": "track", "title": "Song" }
        ]}}))
        .unwrap();
        let items = PlexClient::items(c.media_container.metadata);
        assert_eq!(items.len(), 2, "music skipped");
        assert_eq!(items[0].title, "Some Show");
        assert_eq!(items[0].episode_label.as_deref(), Some("S2 · E5"));
        assert_eq!(items[0].poster.as_deref(), Some("/library/metadata/100/thumb/1"), "series poster");
        assert_eq!(items[0].resume_ms, Some(900_000));
        assert_eq!(items[1].subtitle.as_deref(), Some("1999"));
        assert_eq!(items[1].resume_ms, None);
        assert!(items.iter().all(|i| i.source == SourceKind::Plex));
    }

    #[test]
    fn empty_container_has_no_metadata_key() {
        let c: Container = serde_json::from_value(json!({ "MediaContainer": { "size": 0 } })).unwrap();
        assert!(c.media_container.metadata.is_empty());
    }

    #[test]
    fn maps_session() {
        let m: Metadata = serde_json::from_value(json!({
            "ratingKey": "200", "type": "movie", "title": "A Film", "viewOffset": 60_000,
            "Player": { "title": "Bedroom TV", "state": "paused", "machineIdentifier": "abc" }
        }))
        .unwrap();
        let s = m.into_session().unwrap();
        assert_eq!(s.device, "Bedroom TV");
        assert_eq!(s.state, PlayState::Paused);
        assert_eq!(s.position_ms, 60_000);
    }

    #[test]
    fn connection_preference() {
        let r: Resource = serde_json::from_value(json!({
            "name": "s", "provides": "server", "connections": [
                { "uri": "https://relay", "local": false, "relay": true },
                { "uri": "https://remote", "local": false, "relay": false },
                { "uri": "https://local-a", "local": true, "relay": false },
                { "uri": "https://local-b", "local": true, "relay": false }
            ]
        }))
        .unwrap();
        let order: Vec<String> = r.ordered_connections().into_iter().map(|c| c.uri).collect();
        assert_eq!(order, ["https://local-a", "https://local-b", "https://remote", "https://relay"]);
    }

    #[test]
    fn plain_local_fallback_after_plex_direct() {
        let r: Resource = serde_json::from_value(json!({
            "name": "s", "provides": "server", "connections": [
                { "uri": "https://1-2-3-4.abc.plex.direct:26163", "address": "1.2.3.4", "port": 26163,
                  "local": false, "relay": false },
                { "uri": "https://192-168-1-100.abc.plex.direct:32400", "address": "192.168.1.100", "port": 32400,
                  "local": true, "relay": false },
                { "uri": "https://relay.plex.direct:8443", "address": "45.0.0.1", "port": 8443,
                  "local": false, "relay": true }
            ]
        }))
        .unwrap();
        assert_eq!(
            r.candidate_uris(),
            [
                "https://192-168-1-100.abc.plex.direct:32400",
                "http://192.168.1.100:32400",
                "https://1-2-3-4.abc.plex.direct:26163",
                "https://relay.plex.direct:8443",
            ]
        );
    }

    #[test]
    fn pin_response() {
        let p: PinResponse =
            serde_json::from_value(json!({ "id": 7, "code": "AB12", "authToken": null, "expiresIn": 1800 })).unwrap();
        assert_eq!((p.id, p.code.as_str(), p.auth_token, p.expires_in), (7, "AB12", None, Some(1800)));
    }
}
