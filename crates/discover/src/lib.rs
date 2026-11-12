//! Where to watch: TMDB search + US availability (JustWatch data via TMDB),
//! cached in SQLite and ranked by what you can watch right now.
//!
//! The availability data must be credited on screen:
//! "Availability in the US from JustWatch, via TMDB".

pub mod cache;

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tvdash_core::{
    config::StreamingSite,
    models::{MediaItem, WatchHit, WatchTier},
};

pub use cache::Cache;

pub const DEFAULT_API: &str = "https://api.themoviedb.org";
pub const DEFAULT_IMAGES: &str = "https://image.tmdb.org/t/p";
pub const CREDIT: &str = "Availability in the US from JustWatch, via TMDB";
/// Availability is cached a day; the provider list a week.
pub const AVAILABILITY_TTL: Duration = Duration::from_secs(24 * 3600);
pub const PROVIDERS_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// Parallel availability requests per search.
pub const CONCURRENCY: usize = 8;

#[derive(Debug, Error)]
pub enum TmdbError {
    #[error("TMDB rejected the token")]
    Unauthorized,
    #[error("TMDB: HTTP {0}")]
    Status(u16),
    #[error("can't reach TMDB: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, TmdbError>;

/// Overridable base URLs (tests point these at a fake).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub api: String,
    pub images: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self { api: DEFAULT_API.into(), images: DEFAULT_IMAGES.into() }
    }
}

/// One search result, in TMDB's relevance order (`rank`).
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub tmdb_id: u32,
    pub is_tv: bool,
    pub title: String,
    pub year: Option<i32>,
    pub overview: String,
    pub poster_path: Option<String>,
    pub rank: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub provider_id: u32,
    pub provider_name: String,
    #[serde(default)]
    pub logo_path: Option<String>,
}

/// US offers for one title (`results.US` of watch/providers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offers {
    #[serde(default)]
    pub flatrate: Vec<Offer>,
    #[serde(default)]
    pub free: Vec<Offer>,
    #[serde(default)]
    pub ads: Vec<Offer>,
    #[serde(default)]
    pub rent: Vec<Offer>,
    #[serde(default)]
    pub buy: Vec<Offer>,
    #[serde(default)]
    pub link: Option<String>,
}

impl Offers {
    fn included(&self) -> impl Iterator<Item = &Offer> {
        self.flatrate.iter().chain(&self.free).chain(&self.ads)
    }

    fn is_empty(&self) -> bool {
        self.flatrate.is_empty()
            && self.free.is_empty()
            && self.ads.is_empty()
            && self.rent.is_empty()
            && self.buy.is_empty()
    }
}

/// A streaming provider from TMDB's US list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    pub provider_id: u32,
    pub provider_name: String,
    #[serde(default)]
    pub logo_path: Option<String>,
}

pub struct Tmdb {
    http: reqwest::Client,
    endpoints: Endpoints,
    token: String,
}

impl Tmdb {
    pub fn new(token: &str, endpoints: Endpoints) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("tvdash/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Self { http, endpoints, token: token.trim().to_owned() }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let resp = self
            .http
            .get(format!("{}{path}", self.endpoints.api.trim_end_matches('/')))
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .query(query)
            .send()
            .await?;
        match resp.status().as_u16() {
            200..=299 => Ok(resp.json().await?),
            401 | 403 => Err(TmdbError::Unauthorized),
            s => Err(TmdbError::Status(s)),
        }
    }

    /// Prove the token works.
    pub async fn check(&self) -> Result<()> {
        let _: serde_json::Value = self.get("/3/configuration", &[]).await?;
        Ok(())
    }

    /// Movies and shows matching `query`, in TMDB's relevance order.
    pub async fn search(&self, query: &str) -> Result<Vec<Hit>> {
        #[derive(Deserialize)]
        struct Page {
            #[serde(default)]
            results: Vec<Raw>,
        }
        #[derive(Deserialize)]
        struct Raw {
            id: u32,
            #[serde(default)]
            media_type: String,
            title: Option<String>,
            name: Option<String>,
            release_date: Option<String>,
            first_air_date: Option<String>,
            #[serde(default)]
            overview: String,
            poster_path: Option<String>,
        }
        let page: Page = self
            .get(
                "/3/search/multi",
                &[("query", query), ("include_adult", "false"), ("language", "en-US"), ("page", "1")],
            )
            .await?;
        let year = |d: &Option<String>| d.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok());
        Ok(page
            .results
            .into_iter()
            .filter(|r| r.media_type == "movie" || r.media_type == "tv")
            .enumerate()
            .map(|(rank, r)| {
                let is_tv = r.media_type == "tv";
                Hit {
                    tmdb_id: r.id,
                    is_tv,
                    title: if is_tv { r.name } else { r.title }.unwrap_or_default(),
                    year: if is_tv { year(&r.first_air_date) } else { year(&r.release_date) },
                    overview: r.overview,
                    poster_path: r.poster_path,
                    rank,
                }
            })
            .collect())
    }

    /// US offers for a title, from the cache when fresh.
    pub async fn availability(&self, cache: Option<&Cache>, is_tv: bool, id: u32) -> Result<Offers> {
        let kind = if is_tv { "tv" } else { "movie" };
        let key = format!("offers:{kind}:{id}");
        if let Some(hit) = cache.and_then(|c| c.get::<Offers>(&key, AVAILABILITY_TTL)) {
            return Ok(hit);
        }
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            results: HashMap<String, Offers>,
        }
        let r: Resp = self.get(&format!("/3/{kind}/{id}/watch/providers"), &[]).await?;
        let offers = r.results.get("US").cloned().unwrap_or_default();
        if let Some(c) = cache {
            c.put(&key, &offers);
        }
        Ok(offers)
    }

    /// TMDB's US provider list (movies and TV merged), from the cache when fresh.
    pub async fn providers(&self, cache: Option<&Cache>) -> Result<Vec<Provider>> {
        const KEY: &str = "providers:US";
        if let Some(hit) = cache.and_then(|c| c.get::<Vec<Provider>>(KEY, PROVIDERS_TTL)) {
            return Ok(hit);
        }
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            results: Vec<Provider>,
        }
        let mut seen = HashSet::new();
        let mut all = Vec::new();
        for kind in ["movie", "tv"] {
            let r: Resp = self.get(&format!("/3/watch/providers/{kind}"), &[("watch_region", "US")]).await?;
            all.extend(r.results.into_iter().filter(|p| seen.insert(p.provider_id)));
        }
        if let Some(c) = cache {
            c.put(KEY, &all);
        }
        Ok(all)
    }

    /// An image from TMDB's CDN, e.g. `image("w185", "/abc.jpg")`.
    pub async fn image(&self, size: &str, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}/{size}{path}", self.endpoints.images.trim_end_matches('/'));
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(TmdbError::Status(resp.status().as_u16()));
        }
        Ok(resp.bytes().await?.to_vec())
    }
}

/// TMDB ID and kind of a library item, for matching.
pub type LibraryKey = (bool, u32);

/// Which group a title lands in (plan §3, "Ranking").
pub fn tier(in_library: bool, offers: Option<&Offers>, mine: &HashSet<u32>) -> WatchTier {
    if in_library {
        return WatchTier::Library;
    }
    match offers {
        None => WatchTier::Checking,
        Some(o) if o.included().any(|x| mine.contains(&x.provider_id)) => WatchTier::Included,
        Some(o) if !o.is_empty() => WatchTier::CostsExtra,
        _ => WatchTier::NotStreaming,
    }
}

/// Build the UI's result for a hit. `offers: None` = still checking.
pub fn watch_hit(
    hit: &Hit,
    offers: Option<&Offers>,
    sites: &[StreamingSite],
    library: &HashMap<LibraryKey, MediaItem>,
) -> WatchHit {
    let mine: HashSet<u32> = sites.iter().flat_map(|s| s.tmdb_providers.iter().copied()).collect();
    let item = library.get(&(hit.is_tv, hit.tmdb_id)).cloned();
    let tier = tier(item.is_some(), offers, &mine);
    let mut services: Vec<(usize, String)> = Vec::new();
    let mut elsewhere: Vec<String> = Vec::new();
    if let Some(o) = offers {
        for offer in o.included() {
            match sites.iter().position(|s| s.tmdb_providers.contains(&offer.provider_id)) {
                Some(i) if !services.iter().any(|(j, _)| *j == i) => services.push((i, sites[i].name.clone())),
                Some(_) => {}
                None => {
                    let line = format!("{} (not one of yours)", offer.provider_name);
                    if !elsewhere.contains(&line) {
                        elsewhere.push(line);
                    }
                }
            }
        }
        for (what, list) in [("Rent", &o.rent), ("Buy", &o.buy)] {
            for offer in list {
                let line = format!("{what} · {}", offer.provider_name);
                if !elsewhere.contains(&line) {
                    elsewhere.push(line);
                }
            }
        }
    }
    WatchHit {
        tmdb_id: hit.tmdb_id,
        is_tv: hit.is_tv,
        title: hit.title.clone(),
        year: hit.year,
        overview: hit.overview.clone(),
        tier,
        services,
        elsewhere,
        library: item,
        link: offers.and_then(|o| o.link.clone()),
    }
}

/// Sort by group, keeping TMDB's relevance order inside each group.
pub fn rank(hits: &mut [(usize, WatchHit)]) {
    hits.sort_by_key(|(rank, h)| (h.tier, *rank));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvdash_core::models::{MediaKind, SourceKind};

    fn offer(id: u32, name: &str) -> Offer {
        Offer { provider_id: id, provider_name: name.into(), logo_path: None }
    }

    fn site(name: &str, ids: &[u32]) -> StreamingSite {
        StreamingSite { name: name.into(), url: "https://x".into(), tmdb_providers: ids.to_vec(), ..Default::default() }
    }

    fn hit(id: u32, rank: usize) -> Hit {
        Hit {
            tmdb_id: id,
            is_tv: false,
            title: format!("T{id}"),
            year: None,
            overview: String::new(),
            poster_path: None,
            rank,
        }
    }

    #[test]
    fn tiers() {
        let mine: HashSet<u32> = [8].into();
        let included = Offers { flatrate: vec![offer(8, "Netflix")], ..Default::default() };
        let rent_only = Offers { rent: vec![offer(2, "Apple TV")], ..Default::default() };
        let other = Offers { ads: vec![offer(99, "Tubi")], ..Default::default() };
        assert_eq!(tier(true, None, &mine), WatchTier::Library);
        assert_eq!(tier(false, Some(&included), &mine), WatchTier::Included);
        assert_eq!(tier(false, Some(&rent_only), &mine), WatchTier::CostsExtra);
        assert_eq!(tier(false, Some(&other), &mine), WatchTier::CostsExtra);
        assert_eq!(tier(false, Some(&Offers::default()), &mine), WatchTier::NotStreaming);
        assert_eq!(tier(false, None, &mine), WatchTier::Checking);
    }

    #[test]
    fn hits_name_services_and_keep_relevance_inside_groups() {
        let sites = [site("Netflix", &[8]), site("Hulu", &[15])];
        let offers = Offers {
            flatrate: vec![offer(15, "Hulu"), offer(1899, "Max")],
            rent: vec![offer(2, "Apple TV")],
            link: Some("https://tmdb/watch".into()),
            ..Default::default()
        };
        let h = watch_hit(&hit(1, 0), Some(&offers), &sites, &HashMap::new());
        assert_eq!(h.tier, WatchTier::Included);
        assert_eq!(h.services, [(1, "Hulu".to_string())]);
        assert_eq!(h.elsewhere, ["Max (not one of yours)", "Rent · Apple TV"]);
        assert_eq!(h.link.as_deref(), Some("https://tmdb/watch"));

        let item = MediaItem {
            source: SourceKind::Plex,
            id: "9".into(),
            kind: MediaKind::Movie,
            title: "T2".into(),
            subtitle: None,
            episode_label: None,
            duration_ms: None,
            resume_ms: None,
            poster: None,
            ..Default::default()
        };
        let library = HashMap::from([((false, 2), item)]);
        let mut all = vec![
            (0, watch_hit(&hit(1, 0), Some(&Offers::default()), &sites, &library)),
            (1, watch_hit(&hit(2, 1), None, &sites, &library)),
            (2, watch_hit(&hit(3, 2), Some(&offers), &sites, &library)),
            (3, watch_hit(&hit(4, 3), None, &sites, &library)),
        ];
        rank(&mut all);
        let order: Vec<(u32, WatchTier)> = all.iter().map(|(_, h)| (h.tmdb_id, h.tier)).collect();
        assert_eq!(
            order,
            [(2, WatchTier::Library), (3, WatchTier::Included), (1, WatchTier::NotStreaming), (4, WatchTier::Checking)]
        );
        // A TV show with the same number isn't a library match for the movie.
        let tv = Hit { is_tv: true, ..hit(2, 0) };
        assert!(watch_hit(&tv, None, &sites, &library).library.is_none());
    }
}
