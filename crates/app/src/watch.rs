//! Where to watch, owned by the controller: the TMDB client and token, the
//! availability cache and the library's TMDB IDs.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tokio::{
    sync::{Semaphore, mpsc::UnboundedSender},
    task::{JoinHandle, JoinSet},
};
use tracing::{debug, info, warn};
use tvdash_core::{
    AppEvent, SecretKey,
    config::{APPS, StreamingSite},
    events::Service,
    models::{AccountStatus, LibraryData, MediaItem, MediaKind, SourceKind},
    secrets,
};
use tvdash_discover::{Cache, Endpoints, LibraryKey, Offers, Tmdb, TmdbError};

use crate::controller::decode;

const POSTER_SIZE: &str = "w185";

type Library = Arc<Mutex<HashMap<SourceKind, HashMap<LibraryKey, MediaItem>>>>;

pub struct Watch {
    events: UnboundedSender<AppEvent>,
    pub(crate) endpoints: Endpoints,
    /// Token to use instead of the keyring (tests).
    pub(crate) token_override: Option<String>,
    tmdb: Option<Arc<Tmdb>>,
    cache: Option<Arc<Cache>>,
    library: Library,
    search: Option<JoinHandle<()>>,
}

impl Watch {
    pub fn new(events: UnboundedSender<AppEvent>, cache_path: Option<PathBuf>) -> Self {
        let cache = cache_path.and_then(|p| match Cache::open(&p) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                warn!(path = %p.display(), error = %e, "no Where to watch cache");
                None
            }
        });
        Self {
            events,
            endpoints: Endpoints::default(),
            token_override: None,
            tmdb: None,
            cache,
            library: Library::default(),
            search: None,
        }
    }

    fn emit(&self, e: AppEvent) {
        let _ = self.events.send(e);
    }

    /// Pick up the stored token.
    pub async fn init(&mut self) {
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
            }
            None => self.emit(AppEvent::Tmdb(AccountStatus::NotSetUp)),
        }
    }

    /// Check a token with TMDB, then store it (empty = remove).
    pub async fn set_token(&mut self, token: String) {
        let token = token.trim().to_owned();
        if token.is_empty() {
            let _ = tokio::task::spawn_blocking(|| secrets::delete(SecretKey::TmdbToken)).await;
            self.tmdb = None;
            self.emit(AppEvent::Tmdb(AccountStatus::NotSetUp));
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
        self.emit(AppEvent::Toast("TMDB connected: search is ready".into()));
    }

    /// A source's library was (re-)read: keep its movies and shows by TMDB ID.
    pub fn set_library(&self, kind: SourceKind, data: &LibraryData) {
        let map: HashMap<LibraryKey, MediaItem> = data
            .movies
            .iter()
            .chain(&data.shows)
            .filter(|i| matches!(i.kind, MediaKind::Movie | MediaKind::Series))
            .filter_map(|i| Some(((i.kind == MediaKind::Series, i.tmdb_id?), i.clone())))
            .collect();
        debug!(source = kind.label(), titles = map.len(), "library TMDB IDs");
        self.library.lock().unwrap().insert(kind, map);
    }

    /// Forget a signed-out source's titles.
    pub fn forget(&mut self, kind: SourceKind) {
        self.library.lock().unwrap().remove(&kind);
    }

    fn library_snapshot(&self) -> HashMap<LibraryKey, MediaItem> {
        let lib = self.library.lock().unwrap();
        lib.get(&SourceKind::Plex).cloned().unwrap_or_default()
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
            let error = Some("Add your TMDB token in Settings › Accounts to search streaming services.".into());
            self.emit(AppEvent::WatchResults { query, hits: Vec::new(), error });
            return;
        };
        let (events, cache, library) = (self.events.clone(), self.cache.clone(), self.library_snapshot());
        self.search = Some(tokio::spawn(async move {
            let sites = with_named_providers(&tmdb, cache.as_deref(), sites).await;
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
}

/// Add the TMDB providers each app is known by name (see `App::tmdb_names`),
/// for services whose IDs aren't fixed in the code. The list is cached a week.
async fn with_named_providers(tmdb: &Tmdb, cache: Option<&Cache>, mut sites: Vec<StreamingSite>) -> Vec<StreamingSite> {
    let providers = match tmdb.providers(cache).await {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "couldn't load the TMDB provider list");
            return sites;
        }
    };
    for (site, app) in sites.iter_mut().zip(APPS.iter()) {
        for p in providers.iter().filter(|p| app.has_tmdb_name(&p.provider_name)) {
            if !site.tmdb_providers.contains(&p.provider_id) {
                site.tmdb_providers.push(p.provider_id);
            }
        }
    }
    sites
}
