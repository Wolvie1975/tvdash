//! Plex (M3): PIN sign-in and server discovery through plex.tv, then the
//! media server's own API behind [`MediaSource`].
//!
//! Every call sends `Accept: application/json`, `X-Plex-Token` (once known) and
//! `X-Plex-Client-Identifier`. Server responses are wrapped in `MediaContainer`.

use std::time::Duration;

use reqwest::{Method, RequestBuilder};
use serde::{Deserialize, de::DeserializeOwned};
use tracing::{debug, info, warn};
use tvdash_core::models::{
    CollectionInfo, EpisodeInfo, FinishSeasonItem, GenreTile, LibrarySort, MediaKind, SeasonInfo,
};

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

    pub fn admin(&self) -> Result<PlexAdmin> {
        Ok(PlexAdmin {
            http: self.http.clone(),
            base: self.base.clone(),
            token: self.token.clone(),
            client_id: self.client_id.clone(),
        })
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        identify(self.http.request(method, format!("{}{path}", self.base)), &self.client_id, Some(&self.token))
    }

    async fn metadata(&self, path: &str, query: &[(&str, String)], context: &str) -> Result<Vec<Metadata>> {
        let r: Container = send_json(self.request(Method::GET, path).query(query), context).await?;
        Ok(r.media_container.metadata)
    }

    pub async fn item(&self, rating_key: &str) -> Result<Option<MediaItem>> {
        Ok(self
            .metadata(&format!("/library/metadata/{rating_key}"), &[], "item")
            .await?
            .into_iter()
            .find_map(Metadata::into_media_item))
    }

    fn items(meta: Vec<Metadata>) -> Vec<MediaItem> {
        meta.into_iter().filter_map(Metadata::into_media_item).collect()
    }

    pub async fn sections(&self) -> Result<Vec<Section>> {
        self.admin()?.sections().await
    }

    pub async fn show_seasons(&self, show_key: &str) -> Result<Vec<SeasonInfo>> {
        let meta = self.metadata(&format!("/library/metadata/{show_key}/children"), &[], "show seasons").await?;
        let mut seasons: Vec<SeasonInfo> = meta
            .into_iter()
            .filter(|m| m.kind == "season")
            .map(|m| SeasonInfo {
                rating_key: m.rating_key,
                index: m.index.unwrap_or(0),
                title: if m.title.is_empty() { format!("Season {}", m.index.unwrap_or(0)) } else { m.title },
                leaf_count: m.leaf_count.unwrap_or(0),
                viewed_leaf_count: m.viewed_leaf_count.unwrap_or(0),
                thumb: m.thumb,
            })
            .collect();
        // Put Specials (index 0) last per spec §5.
        seasons.sort_by_key(|s| (s.index == 0, s.index));
        Ok(seasons)
    }

    pub async fn season_episodes(&self, season_key: &str) -> Result<Vec<EpisodeInfo>> {
        let meta = self.metadata(&format!("/library/metadata/{season_key}/children"), &[], "season episodes").await?;
        Ok(meta.into_iter().filter(|m| m.kind == "episode").map(to_episode).collect())
    }

    pub async fn show_on_deck(&self, show_key: &str) -> Result<Option<EpisodeInfo>> {
        let meta = self.metadata(&format!("/library/metadata/{show_key}/onDeck"), &[], "show onDeck").await?;
        Ok(meta.into_iter().find(|m| m.kind == "episode").map(to_episode))
    }

    /// Find the previous or next episode in the same series (crossing seasons when needed).
    pub async fn adjacent_episode(&self, episode_key: &str, next: bool) -> Result<Option<MediaItem>> {
        let meta = self.metadata(&format!("/library/metadata/{episode_key}"), &[], "episode meta").await?;
        let Some(ep) = meta.into_iter().find(|m| m.kind == "episode") else {
            return Ok(None);
        };
        let Some(season_key) = ep.parent_rating_key else {
            return Ok(None);
        };
        let eps = self.season_episodes(&season_key).await?;
        let Some(pos) = eps.iter().position(|e| e.rating_key == episode_key) else {
            return Ok(None);
        };
        if next {
            if let Some(target) = eps.get(pos + 1) {
                return self.item(&target.rating_key).await;
            }
            if let Some(show_key) = ep.grandparent_rating_key {
                let seasons: Vec<_> =
                    self.show_seasons(&show_key).await?.into_iter().filter(|s| s.index != 0).collect();
                if let Some(s_pos) = seasons.iter().position(|s| s.rating_key == season_key)
                    && let Some(next_season) = seasons.get(s_pos + 1)
                    && let Some(first) = self.season_episodes(&next_season.rating_key).await?.into_iter().next()
                {
                    return self.item(&first.rating_key).await;
                }
            }
        } else {
            if pos > 0 {
                return self.item(&eps[pos - 1].rating_key).await;
            }
            if let Some(show_key) = ep.grandparent_rating_key {
                let seasons: Vec<_> =
                    self.show_seasons(&show_key).await?.into_iter().filter(|s| s.index != 0).collect();
                if let Some(s_pos) = seasons.iter().position(|s| s.rating_key == season_key)
                    && s_pos > 0
                    && let Some(last) = self.season_episodes(&seasons[s_pos - 1].rating_key).await?.into_iter().last()
                {
                    return self.item(&last.rating_key).await;
                }
            }
        }
        Ok(None)
    }

    pub async fn collections(&self, section_key: &str) -> Result<Vec<CollectionInfo>> {
        let meta = self
            .metadata(&format!("/library/sections/{section_key}/collections"), &[], "collections")
            .await?;
        Ok(meta
            .into_iter()
            .map(|m| CollectionInfo {
                rating_key: m.rating_key,
                title: m.title,
                child_count: m.child_count.unwrap_or(0),
                watched_count: m.viewed_leaf_count.unwrap_or(0),
                smart: m.smart.unwrap_or(false),
                thumb: m.thumb,
                summary: m.summary.filter(|s| !s.is_empty()),
                min_year: m.year.map(|y| y as i32),
                max_year: None,
                total_duration_ms: 0,
            })
            .collect())
    }

    pub async fn collection_items(&self, collection_key: &str, sort: Option<&str>) -> Result<Vec<MediaItem>> {
        let q: Vec<(&str, String)> = sort.map(|s| vec![("sort", s.to_owned())]).unwrap_or_default();
        let meta = self
            .metadata(&format!("/library/collections/{collection_key}/children"), &q, "collection items")
            .await?;
        Ok(Self::items(meta))
    }

    /// Collection items plus the `ratingKey`s of items that have been watched.
    pub async fn collection_entries(&self, collection_key: &str) -> Result<(Vec<MediaItem>, Vec<String>)> {
        let meta = self
            .metadata(&format!("/library/collections/{collection_key}/children"), &[], "collection items")
            .await?;
        let watched_keys: Vec<String> = meta
            .iter()
            .filter(|m| m.view_count.unwrap_or(0) > 0 && m.view_offset.unwrap_or(0) == 0)
            .map(|m| m.rating_key.clone())
            .collect();
        Ok((Self::items(meta), watched_keys))
    }

    pub async fn section_genres(&self, section_key: &str) -> Result<Vec<GenreTile>> {
        let c: GenreContainer = send_json(
            self.request(Method::GET, &format!("/library/sections/{section_key}/genre")),
            "section genres",
        )
        .await?;
        Ok(c.media_container
            .directory
            .into_iter()
            .map(|d| GenreTile { key: d.key, title: d.title, count: d.count.unwrap_or(0) })
            .collect())
    }

    pub async fn section_items(
        &self,
        section_key: &str,
        sort: LibrarySort,
        genre_key: Option<&str>,
        limit: u32,
    ) -> Result<Vec<MediaItem>> {
        let mut q = vec![
            ("sort", sort.plex_sort().to_owned()),
            ("X-Plex-Container-Start", "0".into()),
            ("X-Plex-Container-Size", limit.to_string()),
        ];
        if let Some(g) = genre_key {
            q.push(("genre", g.to_owned()));
        }
        let meta = self.metadata(&format!("/library/sections/{section_key}/all"), &q, "section items").await?;
        Ok(Self::items(meta))
    }

    pub async fn added_since(
        &self,
        section_key: &str,
        since_epoch: i64,
        sort: LibrarySort,
        limit: u32,
    ) -> Result<Vec<MediaItem>> {
        let q = [
            ("addedAt>=", since_epoch.to_string()),
            ("sort", sort.plex_sort().to_owned()),
            ("X-Plex-Container-Start", "0".into()),
            ("X-Plex-Container-Size", limit.to_string()),
        ];
        let meta = self.metadata(&format!("/library/sections/{section_key}/all"), &q, "added since").await?;
        Ok(Self::items(meta))
    }

    pub async fn watch_again(
        &self,
        section_key: &str,
        is_tv: bool,
        sort: LibrarySort,
        limit: u32,
    ) -> Result<Vec<MediaItem>> {
        let filter = if is_tv { ("unwatchedLeaves", "0".into()) } else { ("viewCount>=", "2".into()) };
        let q = [
            filter,
            ("sort", sort.plex_sort().to_owned()),
            ("X-Plex-Container-Start", "0".into()),
            ("X-Plex-Container-Size", limit.to_string()),
        ];
        let meta = self.metadata(&format!("/library/sections/{section_key}/all"), &q, "watch again").await?;
        Ok(Self::items(meta))
    }

    pub async fn finish_a_season(&self, section_key: &str, limit: u32) -> Result<Vec<FinishSeasonItem>> {
        let q = [
            ("type", "3".into()),
            ("inProgress", "1".into()),
            ("X-Plex-Container-Start", "0".into()),
            ("X-Plex-Container-Size", "40".into()),
        ];
        let meta = self
            .metadata(&format!("/library/sections/{section_key}/all"), &q, "finish a season")
            .await?;
        let mut out = Vec::new();
        for m in meta {
            let total = m.leaf_count.unwrap_or(0);
            let viewed = m.viewed_leaf_count.unwrap_or(0);
            let remaining = total.saturating_sub(viewed);
            if remaining > 0 && remaining <= 3 {
                let season_title = m.title.clone();
                if let Some(item) = m.into_media_item() {
                    out.push(FinishSeasonItem {
                        item,
                        season_title,
                        total_episodes: total,
                        watched_episodes: viewed,
                    });
                    if out.len() >= limit as usize {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }
}

pub struct PlexAdmin {
    http: reqwest::Client,
    base: String,
    token: String,
    client_id: String,
}

impl PlexAdmin {
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

    pub async fn server_info(&self) -> Result<ServerInfo> {
        let response: ServerInfoContainer = send_json(self.request(Method::GET, "/"), "server info").await?;
        Ok(response.media_container)
    }

    pub async fn resources(&self) -> Result<Vec<ResourceSample>> {
        let r: ResourcesContainer = send_json(
            self.request(Method::GET, "/statistics/resources").query(&[("timespan", "6")]),
            "statistics/resources",
        )
        .await?;
        Ok(r.media_container.statistics_resources)
    }

    pub async fn bandwidth(&self) -> Result<Vec<BandwidthSample>> {
        let r: BandwidthContainer = send_json(
            self.request(Method::GET, "/statistics/bandwidth").query(&[("timespan", "6")]),
            "statistics/bandwidth",
        )
        .await?;
        Ok(r.media_container.statistics_bandwidth)
    }

    pub async fn activities(&self) -> Result<Vec<Activity>> {
        let r: ActivityContainer = send_json(self.request(Method::GET, "/activities"), "activities").await?;
        Ok(r.media_container.activities)
    }

    pub async fn cancel_activity(&self, uuid: &str) -> Result<()> {
        send(self.request(Method::DELETE, &format!("/activities/{uuid}")), "cancel activity").await?;
        Ok(())
    }

    pub async fn sections(&self) -> Result<Vec<Section>> {
        let r: SectionContainer = send_json(self.request(Method::GET, "/library/sections"), "library sections").await?;
        let mut out = Vec::with_capacity(r.media_container.directory.len());
        for dir in r.media_container.directory {
            let item_count = self.section_count(&dir.key).await?;
            out.push(Section {
                key: dir.key,
                title: dir.title,
                kind: dir.kind,
                scanned_at: dir.scanned_at,
                refreshing: dir.refreshing,
                item_count,
            });
        }
        Ok(out)
    }

    async fn section_count(&self, key: &str) -> Result<u32> {
        let r: CountContainer = send_json(
            self.request(Method::GET, &format!("/library/sections/{key}/all"))
                .query(&[("X-Plex-Container-Start", "0"), ("X-Plex-Container-Size", "0")]),
            "section count",
        )
        .await?;
        Ok(r.media_container.total_size)
    }

    pub async fn scan(&self, section_key: Option<&str>) -> Result<()> {
        let path = match section_key {
            Some(key) => format!("/library/sections/{key}/refresh"),
            None => "/library/sections/all/refresh".to_owned(),
        };
        send(self.request(Method::GET, &path), "scan library").await?;
        Ok(())
    }

    pub async fn refresh_item(&self, rating_key: &str) -> Result<()> {
        send(self.request(Method::PUT, &format!("/library/metadata/{rating_key}/refresh")), "refresh item").await?;
        Ok(())
    }

    pub async fn posters(&self, rating_key: &str) -> Result<Vec<ArtChoice>> {
        let r: PosterContainer =
            send_json(self.request(Method::GET, &format!("/library/metadata/{rating_key}/posters")), "posters").await?;
        Ok(r.media_container.metadata)
    }

    pub async fn backs(&self, rating_key: &str) -> Result<Vec<ArtChoice>> {
        let r: ArtContainer =
            send_json(self.request(Method::GET, &format!("/library/metadata/{rating_key}/arts")), "backdrops").await?;
        Ok(r.media_container.metadata)
    }

    pub async fn artwork_thumbnail(&self, key: &str, backdrop: bool) -> Result<Vec<u8>> {
        let (width, height) = if backdrop { ("360", "203") } else { ("160", "240") };
        let response = send(
            self.request(Method::GET, "/photo/:/transcode").query(&[
                ("url", key),
                ("width", width),
                ("height", height),
                ("minSize", "1"),
                ("upscale", "1"),
            ]),
            "artwork thumbnail",
        )
        .await?;
        Ok(response.bytes().await?.to_vec())
    }

    pub async fn set_poster(&self, rating_key: &str, key: &str) -> Result<()> {
        send(
            self.request(Method::PUT, &format!("/library/metadata/{rating_key}/poster")).query(&[("url", key)]),
            "set poster",
        )
        .await?;
        Ok(())
    }

    pub async fn set_backdrop(&self, rating_key: &str, key: &str) -> Result<()> {
        send(
            self.request(Method::PUT, &format!("/library/metadata/{rating_key}/art")).query(&[("url", key)]),
            "set backdrop",
        )
        .await?;
        Ok(())
    }

    pub async fn terminate(&self, session_id: &str, reason: &str) -> Result<()> {
        send(
            self.request(Method::GET, "/status/sessions/terminate")
                .query(&[("sessionId", session_id), ("reason", reason)]),
            "terminate stream",
        )
        .await?;
        Ok(())
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
        let meta = match self.metadata("/hubs/continueWatching/items", &[], "continue watching").await {
            Ok(items) if !items.is_empty() => items,
            _ => self.metadata("/library/onDeck", &[], "continue watching").await?,
        };
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

    async fn tmdb_library(&self) -> Result<Vec<(u32, MediaItem)>> {
        let r: Container = send_json(self.request(Method::GET, "/library/sections"), "library sections").await?;
        let mut out = Vec::new();
        for dir in r.media_container.directory.into_iter().filter(|d| d.kind == "movie" || d.kind == "show") {
            let q = [("includeGuids", "1".to_owned())];
            let meta = self.metadata(&format!("/library/sections/{}/all", dir.key), &q, "library").await?;
            for m in meta {
                let tmdb = m.guid.iter().find_map(|g| g.id.strip_prefix("tmdb://")?.parse::<u32>().ok());
                if let (Some(id), Some(item)) = (tmdb, m.into_media_item()) {
                    out.push((id, item));
                }
            }
        }
        Ok(out)
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
    directory: Vec<Directory>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ResourcesContainer {
    media_container: ResourcesMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ResourcesMediaContainer {
    #[serde(default)]
    statistics_resources: Vec<ResourceSample>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct BandwidthContainer {
    media_container: BandwidthMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct BandwidthMediaContainer {
    #[serde(default)]
    statistics_bandwidth: Vec<BandwidthSample>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ActivityContainer {
    media_container: ActivityMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ActivityMediaContainer {
    #[serde(rename = "Activity", default)]
    activities: Vec<Activity>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct SectionContainer {
    media_container: SectionMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct SectionMediaContainer {
    #[serde(default)]
    directory: Vec<Directory>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct CountContainer {
    media_container: CountMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct CountMediaContainer {
    #[serde(rename = "totalSize", default)]
    total_size: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct PosterContainer {
    media_container: PosterMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct PosterMediaContainer {
    #[serde(default)]
    metadata: Vec<ArtChoice>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ArtContainer {
    media_container: ArtMediaContainer,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ArtMediaContainer {
    #[serde(default)]
    metadata: Vec<ArtChoice>,
}

/// A library section (`/library/sections`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Directory {
    key: String,
    title: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    scanned_at: Option<u64>,
    #[serde(default)]
    refreshing: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResourceSample {
    at: i64,
    host_cpu_utilization: Option<f32>,
    process_cpu_utilization: Option<f32>,
    host_memory_utilization: Option<f32>,
    process_memory_utilization: Option<f32>,
}

impl ResourceSample {
    pub fn at(&self) -> i64 {
        self.at
    }

    pub fn host_cpu_utilization(&self) -> Option<f32> {
        self.host_cpu_utilization
    }

    pub fn process_cpu_utilization(&self) -> Option<f32> {
        self.process_cpu_utilization
    }

    pub fn host_memory_utilization(&self) -> Option<f32> {
        self.host_memory_utilization
    }

    pub fn process_memory_utilization(&self) -> Option<f32> {
        self.process_memory_utilization
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BandwidthSample {
    at: i64,
    lan: bool,
    bytes: u64,
    #[serde(rename = "deviceID")]
    device_id: Option<String>,
}

impl BandwidthSample {
    pub fn at(&self) -> i64 {
        self.at
    }

    pub fn lan(&self) -> bool {
        self.lan
    }

    pub fn bits_per_second(&self, window_seconds: u64) -> u64 {
        if window_seconds == 0 {
            return 0;
        }
        self.bytes.saturating_mul(8) / window_seconds
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Activity {
    uuid: String,
    #[serde(rename = "type")]
    kind: String,
    title: String,
    subtitle: String,
    progress: Option<f32>,
    cancellable: bool,
}

impl Activity {
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn subtitle(&self) -> &str {
        &self.subtitle
    }

    pub fn progress(&self) -> Option<f32> {
        self.progress
    }

    pub fn cancellable(&self) -> bool {
        self.cancellable
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerInfo {
    pub friendly_name: String,
    pub version: String,
    #[serde(deserialize_with = "string_from_value")]
    pub my_plex_subscription: Option<String>,
}

impl ServerInfo {
    pub fn has_plex_pass(&self) -> bool {
        self.my_plex_subscription.as_deref().is_some_and(|subscription| {
            !matches!(subscription.trim().to_ascii_lowercase().as_str(), "" | "0" | "false" | "none" | "no")
        })
    }
}

fn string_from_value<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.map(|value| match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => String::new(),
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct ServerInfoContainer {
    media_container: ServerInfo,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Section {
    pub key: String,
    pub title: String,
    pub kind: String,
    pub scanned_at: Option<u64>,
    pub refreshing: bool,
    pub item_count: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArtChoice {
    pub key: String,
    pub thumb: Option<String>,
    pub selected: bool,
    pub provider: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Guid {
    id: String,
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
    parent_rating_key: Option<String>,
    grandparent_rating_key: Option<String>,
    parent_index: Option<u32>,
    index: Option<u32>,
    year: Option<u32>,
    duration: Option<u64>,
    view_offset: Option<u64>,
    thumb: Option<String>,
    grandparent_thumb: Option<String>,
    summary: Option<String>,
    leaf_count: Option<u32>,
    viewed_leaf_count: Option<u32>,
    view_count: Option<u32>,
    child_count: Option<u32>,
    smart: Option<bool>,
    #[serde(rename = "Media")]
    media: Vec<Media>,
    #[serde(rename = "Player")]
    player: Option<Player>,
    #[serde(default, rename = "sessionKey", deserialize_with = "string_from_number")]
    session_key: Option<String>,
    #[serde(rename = "Guid")]
    guid: Vec<Guid>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct GenreContainer {
    media_container: GenreInner,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct GenreInner {
    directory: Vec<GenreDirectory>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct GenreDirectory {
    key: String,
    title: String,
    count: Option<u32>,
}

fn to_episode(m: Metadata) -> EpisodeInfo {
    EpisodeInfo {
        rating_key: m.rating_key,
        index: m.index.unwrap_or(0),
        season_index: m.parent_index.unwrap_or(0),
        title: m.title,
        summary: m.summary.filter(|s| !s.is_empty()),
        duration_ms: m.duration,
        view_offset_ms: m.view_offset,
        view_count: m.view_count.unwrap_or(0),
        thumb: m.thumb,
    }
}

fn string_from_number<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(u64),
    }
    Option::<StringOrNumber>::deserialize(deserializer).map(|value| {
        value.map(|value| match value {
            StringOrNumber::String(value) => value,
            StringOrNumber::Number(value) => value.to_string(),
        })
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Media {
    #[serde(rename = "Part")]
    part: Vec<Part>,
    video_decision: Option<String>,
    audio_decision: Option<String>,
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
        let session_id = self.session_key.clone();
        let transcoding = self.media.iter().any(|media| {
            media.video_decision.as_deref() == Some("transcode") || media.audio_decision.as_deref() == Some("transcode")
        });
        let item = self.into_media_item()?;
        Some(Session {
            source: SourceKind::Plex,
            item,
            device: player.title.or(player.product).unwrap_or_else(|| "Unknown device".into()),
            session_id,
            transcoding,
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
            "sessionKey": 123,
            "Media": [{ "videoDecision": "transcode" }],
            "Player": { "title": "Bedroom TV", "state": "paused", "machineIdentifier": "abc" }
        }))
        .unwrap();
        let s = m.into_session().unwrap();
        assert_eq!(s.device, "Bedroom TV");
        assert_eq!(s.session_id.as_deref(), Some("123"));
        assert!(s.transcoding);
        assert_eq!(s.state, PlayState::Paused);
        assert_eq!(s.position_ms, 60_000);
    }

    #[test]
    fn parses_server_subscription_values() {
        let info: ServerInfoContainer = serde_json::from_value(json!({
            "MediaContainer": {
                "friendlyName": "Plex box", "version": "1.2.3", "myPlexSubscription": "0"
            }
        }))
        .unwrap();
        assert_eq!(info.media_container.friendly_name, "Plex box");
        assert!(!info.media_container.has_plex_pass());

        let info: ServerInfoContainer = serde_json::from_value(json!({
            "MediaContainer": { "myPlexSubscription": true }
        }))
        .unwrap();
        assert!(info.media_container.has_plex_pass());
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
    fn parses_server_admin_payloads() {
        let r: ResourcesContainer = serde_json::from_value(json!({
            "MediaContainer": {
                "StatisticsResources": [{
                    "at": 1_700_000_000,
                    "hostCpuUtilization": 23.4,
                    "processCpuUtilization": 18.1,
                    "hostMemoryUtilization": 41.5,
                    "processMemoryUtilization": 29.2
                }]
            }
        }))
        .unwrap();
        let sample = &r.media_container.statistics_resources[0];
        assert_eq!(sample.host_cpu_utilization, Some(23.4));
        assert_eq!(sample.process_memory_utilization, Some(29.2));

        let b: BandwidthContainer = serde_json::from_value(json!({
            "MediaContainer": {
                "StatisticsBandwidth": [{
                    "at": 1_700_000_006,
                    "lan": true,
                    "bytes": 600,
                    "deviceID": "eth0"
                }]
            }
        }))
        .unwrap();
        let bw = &b.media_container.statistics_bandwidth[0];
        assert_eq!(bw.device_id.as_deref(), Some("eth0"));
        assert_eq!(bw.bits_per_second(6), 800);

        let a: ActivityContainer = serde_json::from_value(json!({
            "MediaContainer": {
                "Activity": [{
                    "uuid": "abc",
                    "type": "scan",
                    "title": "Scanning Movies",
                    "subtitle": "Server",
                    "progress": 72.0,
                    "cancellable": true
                }]
            }
        }))
        .unwrap();
        assert_eq!(a.media_container.activities[0].kind, "scan");
        assert!(a.media_container.activities[0].cancellable);
    }

    #[test]
    fn pin_response() {
        let p: PinResponse =
            serde_json::from_value(json!({ "id": 7, "code": "AB12", "authToken": null, "expiresIn": 1800 })).unwrap();
        assert_eq!((p.id, p.code.as_str(), p.auth_token, p.expires_in), (7, "AB12", None, Some(1800)));
    }
}
