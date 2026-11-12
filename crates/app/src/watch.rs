//! Where to watch, owned by the controller: the TMDB client and token, the
//! availability cache, the library's TMDB IDs, service logos for the tiles.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::{
    sync::{Semaphore, mpsc::UnboundedSender},
    task::{JoinHandle, JoinSet},
};
use tracing::{debug, info, warn};
use tvdash_core::{
    AppEvent, SecretKey,
    config::{StreamingSite, expand_home},
    events::Service,
    models::{AccountStatus, MediaItem, MediaKind, RgbaImage, SourceKind},
    secrets,
};
use tvdash_discover::{Cache, Endpoints, LibraryKey, Offers, Tmdb, TmdbError};
use tvdash_media::MediaSource;

use crate::controller::{Internal, decode};

/// How often the library's TMDB IDs are re-read.
const LIBRARY_EVERY: Duration = Duration::from_secs(30 * 60);
const POSTER_SIZE: &str = "w185";
const LOGO_SIZE: &str = "w92";

type Library = Arc<Mutex<HashMap<SourceKind, HashMap<LibraryKey, MediaItem>>>>;

pub struct Watch {
    events: UnboundedSender<AppEvent>,
    /// Back to the controller: provider IDs matched for sites that had none.
    internal: Option<UnboundedSender<Internal>>,
    pub(crate) endpoints: Endpoints,
    /// Token to use instead of the keyring (tests).
    pub(crate) token_override: Option<String>,
    tmdb: Option<Arc<Tmdb>>,
    cache: Option<Arc<Cache>>,
    /// TMDB logo files, next to the cache (`~/.local/share/tvdash/logos`).
    logo_dir: Option<PathBuf>,
    library: Library,
    library_built: Option<Instant>,
    search: Option<JoinHandle<()>>,
}

impl Watch {
    pub fn new(events: UnboundedSender<AppEvent>, cache_path: Option<PathBuf>) -> Self {
        let logo_dir = cache_path.as_deref().and_then(Path::parent).map(|d| d.join("logos"));
        let cache = cache_path.and_then(|p| match Cache::open(&p) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                warn!(path = %p.display(), error = %e, "no Where to watch cache");
                None
            }
        });
        Self {
            events,
            internal: None,
            endpoints: Endpoints::default(),
            token_override: None,
            tmdb: None,
            cache,
            logo_dir,
            library: Library::default(),
            library_built: None,
            search: None,
        }
    }

    pub fn set_internal(&mut self, internal: UnboundedSender<Internal>) {
        self.internal = Some(internal);
    }

    fn emit(&self, e: AppEvent) {
        let _ = self.events.send(e);
    }

    /// Pick up the stored token; fetch tile logos if there is one.
    pub async fn init(&mut self, sites: &[StreamingSite]) {
        let token = match self.token_override.clone() {
            Some(t) => Some(t),
            None => {
                tokio::task::spawn_blocking(|| secrets::get(SecretKey::TmdbToken).ok().flatten()).await.ok().flatten()
            }
        };
        match token.filter(|t| !t.trim().is_empty()) {
            Some(t) => {
                self.tmdb = Some(Arc::new(Tmdb::new(&t, self.endpoints.clone())));
                self.emit(AppEvent::Tmdb(AccountStatus::Connected));
                self.logos(sites);
            }
            None => {
                self.emit(AppEvent::Tmdb(AccountStatus::NotSetUp));
                self.logos(sites); // logo overrides only
            }
        }
    }

    /// Check a token with TMDB, then store it (empty = remove).
    pub async fn set_token(&mut self, token: String, sites: &[StreamingSite]) {
        let token = token.trim().to_owned();
        if token.is_empty() {
            let _ = tokio::task::spawn_blocking(|| secrets::delete(SecretKey::TmdbToken)).await;
            self.tmdb = None;
            self.emit(AppEvent::Tmdb(AccountStatus::NotSetUp));
            self.logos(sites); // only the overrides are left
            return;
        }
        let tmdb = Tmdb::new(&token, self.endpoints.clone());
        if let Err(e) = tmdb.check().await {
            let why = match e {
                TmdbError::Unauthorized => "TMDB didn't accept that token. Use the API Read Access Token.".into(),
                e => e.to_string(),
            };
            self.emit(AppEvent::Tmdb(AccountStatus::Unreachable(why)));
            return;
        }
        if self.token_override.is_none() {
            let t = token.clone();
            match tokio::task::spawn_blocking(move || secrets::set(SecretKey::TmdbToken, &t)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    self.emit(AppEvent::Tmdb(AccountStatus::Unreachable(format!("couldn't save it: {e}"))));
                    return;
                }
                Err(e) => warn!(error = %e, "keyring task failed"),
            }
        }
        info!("TMDB token saved");
        self.tmdb = Some(Arc::new(tmdb));
        self.emit(AppEvent::Tmdb(AccountStatus::Connected));
        self.emit(AppEvent::Toast("TMDB connected: Where to watch is ready".into()));
        self.logos(sites);
    }

    /// Re-read every source's TMDB IDs (at most every 30 min unless `force`).
    pub fn refresh_library(&mut self, sources: Vec<Arc<dyn MediaSource>>, force: bool) {
        if self.tmdb.is_none() || (!force && self.library_built.is_some_and(|t| t.elapsed() < LIBRARY_EVERY)) {
            return;
        }
        self.library_built = Some(Instant::now());
        for source in sources {
            let library = self.library.clone();
            tokio::spawn(async move {
                let kind = source.kind();
                match source.tmdb_library().await {
                    Ok(items) => {
                        let map: HashMap<LibraryKey, MediaItem> = items
                            .into_iter()
                            .filter(|(_, i)| matches!(i.kind, MediaKind::Movie | MediaKind::Series))
                            .map(|(id, i)| ((i.kind == MediaKind::Series, id), i))
                            .collect();
                        debug!(source = kind.label(), titles = map.len(), "library TMDB IDs");
                        library.lock().unwrap().insert(kind, map);
                    }
                    Err(e) => warn!(source = kind.label(), error = %e, "couldn't read library TMDB IDs"),
                }
            });
        }
    }

    /// Forget a signed-out source's titles.
    pub fn forget(&mut self, kind: SourceKind) {
        self.library.lock().unwrap().remove(&kind);
    }

    fn library_snapshot(&self) -> HashMap<LibraryKey, MediaItem> {
        let lib = self.library.lock().unwrap();
        // Jellyfin first, then Plex, so a title in both resolves the same way each time.
        let mut out = HashMap::new();
        for kind in [SourceKind::Plex, SourceKind::Jellyfin] {
            if let Some(m) = lib.get(&kind) {
                out.extend(m.iter().map(|(k, v)| (*k, v.clone())));
            }
        }
        out
    }

    /// Search and rank; results are re-sent as availability arrives.
    pub fn search(&mut self, query: String, sites: Vec<StreamingSite>) {
        if let Some(old) = self.search.take() {
            old.abort();
        }
        let query = query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        let Some(tmdb) = self.tmdb.clone() else {
            let error = Some("Add your TMDB token in Settings › Accounts to use Where to watch.".into());
            self.emit(AppEvent::WatchResults { query, hits: Vec::new(), error });
            return;
        };
        let (events, cache, library) = (self.events.clone(), self.cache.clone(), self.library_snapshot());
        self.search = Some(tokio::spawn(async move {
            let hits = match tmdb.search(&query).await {
                Ok(h) => {
                    let note = format!("search “{query}”: {} results", h.len());
                    let _ = events.send(AppEvent::Health { service: Service::Tmdb, result: Ok(note) });
                    h
                }
                Err(e) => {
                    let _ = events.send(AppEvent::Health { service: Service::Tmdb, result: Err(e.to_string()) });
                    let status = if matches!(e, TmdbError::Unauthorized) {
                        AccountStatus::Expired
                    } else {
                        AccountStatus::Connected
                    };
                    let _ = events.send(AppEvent::Tmdb(status));
                    let _ = events.send(AppEvent::WatchResults { query, hits: Vec::new(), error: Some(e.to_string()) });
                    return;
                }
            };
            let mut offers: Vec<Option<Offers>> = vec![None; hits.len()];
            let send = |offers: &[Option<Offers>]| {
                let mut ranked: Vec<_> = hits
                    .iter()
                    .zip(offers)
                    .map(|(h, o)| (h.rank, tvdash_discover::watch_hit(h, o.as_ref(), &sites, &library)))
                    .collect();
                tvdash_discover::rank(&mut ranked);
                let hits = ranked.into_iter().map(|(_, h)| h).collect();
                let _ = events.send(AppEvent::WatchResults { query: query.clone(), hits, error: None });
            };
            send(&offers);

            // Posters, one after another in the background.
            {
                let (tmdb, events) = (tmdb.clone(), events.clone());
                let wanted: Vec<_> =
                    hits.iter().filter_map(|h| Some((h.tmdb_id, h.is_tv, h.poster_path.clone()?))).collect();
                tokio::spawn(async move {
                    for (tmdb_id, is_tv, path) in wanted {
                        if let Some(image) = tmdb.image(POSTER_SIZE, &path).await.ok().and_then(|b| decode(&b)) {
                            let _ = events.send(AppEvent::WatchPoster { tmdb_id, is_tv, image });
                        }
                    }
                });
            }

            // Availability, a few at a time; each answer moves its title into a group.
            let limit = Arc::new(Semaphore::new(tvdash_discover::CONCURRENCY));
            let mut set = JoinSet::new();
            for (i, h) in hits.iter().enumerate() {
                let (tmdb, cache, limit) = (tmdb.clone(), cache.clone(), limit.clone());
                let (is_tv, id) = (h.is_tv, h.tmdb_id);
                set.spawn(async move {
                    let _permit = limit.acquire_owned().await;
                    (i, tmdb.availability(cache.as_deref(), is_tv, id).await)
                });
            }
            while let Some(Ok((i, result))) = set.join_next().await {
                offers[i] = Some(result.unwrap_or_else(|e| {
                    warn!(error = %e, "availability lookup failed");
                    Offers::default()
                }));
                send(&offers);
            }
        }));
    }

    /// Load each site's tile logo: its `logo` file if set, else the logo of its
    /// first TMDB provider that has one (kept on disk after the first download).
    /// Sites without provider IDs get them matched by name first.
    pub fn logos(&self, sites: &[StreamingSite]) {
        let (events, cache, sites) = (self.events.clone(), self.cache.clone(), sites.to_vec());
        let (tmdb, internal, logo_dir) = (self.tmdb.clone(), self.internal.clone(), self.logo_dir.clone());
        tokio::spawn(async move {
            let providers = match &tmdb {
                None => Vec::new(),
                Some(tmdb) => match tmdb.providers(cache.as_deref()).await {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(error = %e, "couldn't load the TMDB provider list");
                        let _ = events.send(AppEvent::Health { service: Service::Tmdb, result: Err(e.to_string()) });
                        Vec::new()
                    }
                },
            };
            let fills: Vec<(String, Vec<u32>)> = sites
                .iter()
                .filter(|s| s.tmdb_providers.is_empty())
                .filter_map(|s| {
                    let ids: Vec<u32> =
                        tvdash_discover::match_providers(&s.name, &providers).iter().map(|p| p.provider_id).collect();
                    (!ids.is_empty()).then(|| (s.name.clone(), ids))
                })
                .collect();
            if !fills.is_empty()
                && let Some(tx) = &internal
            {
                // The controller saves them and asks for logos again.
                let _ = tx.send(Internal::FillProviders(fills));
                return;
            }
            let mut logos = Vec::new();
            for site in sites {
                let img = match (&site.logo, &tmdb) {
                    (Some(file), _) => logo_file(&expand_home(file)).await,
                    (None, Some(tmdb)) => {
                        let path = site.tmdb_providers.iter().find_map(|id| {
                            providers.iter().find(|p| p.provider_id == *id).and_then(|p| p.logo_path.clone())
                        });
                        match path {
                            Some(path) => tmdb_logo(tmdb, logo_dir.as_deref(), &path).await,
                            None => None,
                        }
                    }
                    (None, None) => None,
                };
                if let Some(img) = img {
                    logos.push((site.name, img));
                }
            }
            let _ = events.send(AppEvent::SiteLogos(logos));
        });
    }

    /// Provider IDs for a site name (the editor's Find button).
    pub fn find_providers(&self, name: String) {
        let Some(tmdb) = self.tmdb.clone() else {
            self.emit(AppEvent::Toast("Add your TMDB token in Settings › Accounts first.".into()));
            return;
        };
        let (events, cache) = (self.events.clone(), self.cache.clone());
        tokio::spawn(async move {
            match tmdb.providers(cache.as_deref()).await {
                Ok(list) => {
                    let found = tvdash_discover::match_providers(&name, &list);
                    let ids = found.iter().map(|p| p.provider_id).collect();
                    let names = found.iter().map(|p| p.provider_name.clone()).collect();
                    let _ = events.send(AppEvent::ProviderMatches { name, ids, names });
                }
                Err(e) => {
                    let _ = events.send(AppEvent::Toast(format!("Couldn't load TMDB's providers: {e}")));
                }
            }
        });
    }
}

/// A site's own logo file (`logo = "…"` in the config).
async fn logo_file(path: &Path) -> Option<RgbaImage> {
    match tokio::fs::read(path).await {
        Ok(bytes) => decode(&bytes),
        Err(e) => {
            warn!(path = %path.display(), error = %e, "couldn't read the logo file");
            None
        }
    }
}

/// A TMDB logo from `dir` if it was downloaded before, else from TMDB (then saved to `dir`).
async fn tmdb_logo(tmdb: &Tmdb, dir: Option<&Path>, logo_path: &str) -> Option<RgbaImage> {
    let file = dir.map(|d| d.join(logo_file_name(logo_path)));
    if let Some(f) = &file
        && let Ok(bytes) = tokio::fs::read(f).await
    {
        match decode(&bytes) {
            Some(img) => return Some(img),
            None => {
                let _ = tokio::fs::remove_file(f).await; // damaged; download it again
            }
        }
    }
    let bytes = match tmdb.image(LOGO_SIZE, logo_path).await {
        Ok(b) => b,
        Err(e) => {
            debug!(path = logo_path, error = %e, "logo download failed");
            return None;
        }
    };
    let img = decode(&bytes)?;
    if let Some(f) = &file {
        let saved = match f.parent() {
            Some(d) => tokio::fs::create_dir_all(d).await,
            None => Ok(()),
        };
        if let Err(e) = saved.and(tokio::fs::write(f, &bytes).await) {
            warn!(file = %f.display(), error = %e, "couldn't save the logo");
        }
    }
    Some(img)
}

/// File name for a TMDB `logo_path` ("/abc.jpg" → "w92-abc.jpg"). TMDB paths
/// change when the image does, so a saved file never goes stale.
fn logo_file_name(logo_path: &str) -> String {
    let name: String = logo_path
        .trim_start_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    format!("{LOGO_SIZE}-{name}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn logo_file_names_stay_in_the_folder() {
        assert_eq!(super::logo_file_name("/pbpMk2Jmc.jpg"), "w92-pbpMk2Jmc.jpg");
        assert_eq!(super::logo_file_name("/../../etc/passwd"), "w92-.._.._etc_passwd");
        assert!(!super::logo_file_name("/a/b c?.png").contains(['/', ' ', '?']));
    }
}
