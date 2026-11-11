//! Background controller: owns media sources, sign-in flows, the mpv session
//! and the streaming browser. Receives `Command`s from the UI and emits
//! `AppEvent`s back. Only one thing is on the TV at a time: starting mpv closes
//! the browser and vice versa.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{debug, error, info, warn};
use tvdash_core::{
    AppEvent, Command, Config, SecretKey,
    config::{StreamingSite, encode_component, expand_home},
    events::{PlayerStatus, Service},
    models::{AccountStatus, Geometry, MediaItem, PlayState, RgbaImage, SignInState, SourceKind},
    secrets,
    state::AppState,
};
use tvdash_media::{
    MediaError, MediaSource,
    jellyfin::{Auth, JellyfinClient},
    plex::{DiscoveredServer, PlexAccount, PlexClient},
};
use tvdash_player::{
    browser::{Browser, BrowserOptions, is_brave},
    mpv::{Mpv, MpvEvent, MpvOptions},
};

use crate::{format, watch::Watch};

const REFRESH_EVERY: Duration = Duration::from_secs(120);
const PROGRESS_EVERY: Duration = Duration::from_secs(10);
const POSTER_HEIGHT: u32 = 480;
const QUICK_CONNECT_POLL: Duration = Duration::from_secs(2);
const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SESSIONS_EVERY: Duration = Duration::from_secs(15);
const PLEX_PIN_POLL: Duration = Duration::from_secs(2);
const RECENTLY_ADDED: u32 = 20;

/// Messages from tasks the controller spawned.
pub enum Internal {
    JellyfinSignedIn(Auth),
    PlexSignedIn {
        account_token: String,
        server: DiscoveredServer,
    },
    Refresh,
    /// TMDB provider IDs matched by name for sites that had none: (site name, IDs).
    FillProviders(Vec<(String, Vec<u32>)>),
}

struct Playback {
    status: PlayerStatus,
    mpv: Mpv,
    /// Sequential progress reports for this item (keeps start → progress → stop ordered).
    reporter: UnboundedSender<(u64, PlayState)>,
    last_emitted_sec: Option<u64>,
    /// The "can't decode" hint was already shown for this item (frame errors repeat).
    decode_hint_shown: bool,
}

pub struct Controller {
    config: Config,
    config_path: PathBuf,
    windowed: bool,
    events: UnboundedSender<AppEvent>,
    internal: UnboundedSender<Internal>,
    jellyfin: Option<Arc<JellyfinClient>>,
    plex: Option<Arc<PlexClient>>,
    sign_in: HashMap<SourceKind, JoinHandle<()>>,
    playback: Option<Playback>,
    browser: Option<Browser>,
    tv_geometry: Option<Geometry>,
    state: AppState,
    state_path: Option<PathBuf>,
    posters_requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    /// Where to watch (TMDB) and service logos.
    pub(crate) watch: Watch,
    /// Tells the widget pollers about display setting changes (units).
    general_tx: Option<tokio::sync::watch::Sender<tvdash_core::config::GeneralConfig>>,
}

enum Step {
    Command(Command),
    Mpv(Option<MpvEvent>),
    BrowserExited,
    Internal(Internal),
    Refresh,
    Sessions,
    Progress,
    Shutdown,
}

impl Controller {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        windowed: bool,
        events: UnboundedSender<AppEvent>,
    ) -> (Self, UnboundedReceiver<Internal>) {
        let (internal, internal_rx) = unbounded_channel();
        let mut c = Self {
            config,
            config_path,
            windowed,
            events: events.clone(),
            internal,
            jellyfin: None,
            plex: None,
            sign_in: HashMap::new(),
            playback: None,
            browser: None,
            tv_geometry: None,
            state: AppState::default(),
            state_path: tvdash_core::state::default_path(),
            posters_requested: Arc::default(),
            general_tx: None,
            watch: Watch::new(events.clone(), tvdash_core::state::default_path().map(|p| p.with_file_name("cache.db"))),
        };
        c.watch.set_internal(c.internal.clone());
        (c, internal_rx)
    }

    pub fn set_general_tx(&mut self, tx: tokio::sync::watch::Sender<tvdash_core::config::GeneralConfig>) {
        self.general_tx = Some(tx);
    }

    fn emit(&self, event: AppEvent) {
        let _ = self.events.send(event);
    }

    fn health(&self, service: Service, result: Result<String, String>) {
        self.emit(AppEvent::Health { service, result });
    }

    fn toast(&self, msg: impl Into<String>) {
        self.emit(AppEvent::Toast(msg.into()));
    }

    pub async fn run(mut self, mut commands: UnboundedReceiver<Command>, mut internal: UnboundedReceiver<Internal>) {
        self.init().await;

        let mut refresh = interval(REFRESH_EVERY);
        refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
        refresh.tick().await; // first tick is immediate; init already refreshed
        let mut progress = interval(PROGRESS_EVERY);
        progress.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sessions = interval(SESSIONS_EVERY);
        sessions.set_missed_tick_behavior(MissedTickBehavior::Skip);
        sessions.tick().await; // init already polled once

        loop {
            let step = {
                let (playback, browser) = (self.playback.as_mut(), self.browser.as_mut());
                let mpv_next = async move {
                    match playback {
                        Some(p) => p.mpv.next_event().await,
                        None => std::future::pending().await,
                    }
                };
                let browser_exit = async move {
                    match browser {
                        Some(b) => b.wait().await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    c = commands.recv() => c.map_or(Step::Shutdown, Step::Command),
                    e = mpv_next => Step::Mpv(e),
                    _ = browser_exit => Step::BrowserExited,
                    Some(m) = internal.recv() => Step::Internal(m),
                    _ = refresh.tick() => Step::Refresh,
                    _ = progress.tick() => Step::Progress,
                    _ = sessions.tick() => Step::Sessions,
                }
            };
            match step {
                Step::Command(c) => self.on_command(c).await,
                Step::Mpv(e) => self.on_mpv(e).await,
                Step::BrowserExited => self.on_browser_exited(),
                Step::Internal(Internal::JellyfinSignedIn(auth)) => {
                    self.on_jellyfin_signed_in(auth).await;
                    self.watch.refresh_library(self.sources(), true);
                }
                Step::Internal(Internal::PlexSignedIn { account_token, server }) => {
                    self.on_plex_signed_in(account_token, server).await;
                    self.watch.refresh_library(self.sources(), true);
                }
                Step::Internal(Internal::FillProviders(fills)) => self.fill_providers(fills),
                Step::Internal(Internal::Refresh) | Step::Refresh => {
                    self.refresh();
                    self.watch.refresh_library(self.sources(), false);
                }
                Step::Sessions => self.poll_sessions(),
                Step::Progress => self.report_tick(),
                Step::Shutdown => break,
            }
        }
        self.stop_playback().await;
        self.close_browser().await;
    }

    async fn init(&mut self) {
        if let Some(path) = &self.state_path {
            self.state = AppState::load(path);
        }
        if self.config.jellyfin.enabled {
            self.init_jellyfin().await;
        }
        if self.config.plex.enabled {
            self.init_plex().await;
        }
        self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
        self.emit_sites();
        self.watch.init(&self.config.streaming).await;
        self.watch.refresh_library(self.sources(), true);
        if let Some(j) = &self.jellyfin {
            self.emit(AppEvent::Server(SourceKind::Jellyfin, Some(j.base_url().to_owned())));
        }
        if self.plex.is_some() {
            self.emit(AppEvent::Server(SourceKind::Plex, Some(self.config.plex.server_url.clone())));
        }
        // First run: nothing connected and setup never finished.
        if !self.state.setup_complete && self.jellyfin.is_none() && self.plex.is_none() {
            self.emit(AppEvent::ShowSetup);
        }
        self.refresh();
    }

    fn save_state(&self) {
        if let Some(path) = &self.state_path
            && let Err(e) = self.state.save(path)
        {
            warn!(error = %e, "couldn't save app state");
        }
    }

    fn update_config(&mut self, change: impl FnOnce(&mut Config)) {
        match Config::update_at(&self.config_path, change) {
            Ok(c) => self.config = c,
            Err(e) => {
                error!(error = %e, "couldn't save config");
                self.toast(format!("Couldn't save the config: {e}"));
            }
        }
    }

    async fn init_jellyfin(&mut self) {
        let cfg = &self.config.jellyfin;
        match (read_secret(SecretKey::JellyfinToken).await, cfg.user_id.is_empty()) {
            (Some(token), false) => {
                let auth = Auth { token, user_id: cfg.user_id.clone() };
                match JellyfinClient::new(&cfg.server_url, &cfg.device_id, Some(auth)) {
                    Ok(c) => self.jellyfin = Some(Arc::new(c)),
                    Err(e) => {
                        self.emit(AppEvent::Account(SourceKind::Jellyfin, AccountStatus::Unreachable(e.to_string())))
                    }
                }
            }
            _ => self.emit(AppEvent::Account(SourceKind::Jellyfin, AccountStatus::NotSetUp)),
        }
    }

    async fn init_plex(&mut self) {
        let cfg = &self.config.plex;
        match (read_secret(SecretKey::PlexToken).await, cfg.server_url.is_empty()) {
            (Some(token), false) => match PlexClient::new(&cfg.server_url, &token, &cfg.client_id) {
                Ok(c) => self.plex = Some(Arc::new(c)),
                Err(e) => self.emit(AppEvent::Account(SourceKind::Plex, AccountStatus::Unreachable(e.to_string()))),
            },
            _ => self.emit(AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp)),
        }
    }

    async fn on_command(&mut self, cmd: Command) {
        match cmd {
            Command::Play(item) => self.play(item).await,
            Command::Pause | Command::Resume => {
                let pause = matches!(cmd, Command::Pause);
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.mpv.set_pause(pause).await
                {
                    warn!(error = %e, "mpv pause failed");
                }
            }
            Command::Seek(secs) => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.mpv.seek(secs).await
                {
                    warn!(error = %e, "mpv seek failed");
                }
            }
            Command::Stop => {
                self.stop_playback().await;
                self.close_browser().await;
            }
            Command::SignIn(SourceKind::Jellyfin) => self.start_jellyfin_sign_in(),
            Command::SignIn(SourceKind::Plex) => self.start_plex_sign_in(),
            Command::CancelSignIn(kind) => {
                if let Some(task) = self.sign_in.remove(&kind) {
                    task.abort();
                }
                self.emit(AppEvent::SignIn(kind, SignInState::Cancelled));
            }
            Command::Refresh => self.refresh(),
            Command::OpenSite(i) => self.open_site(i).await,
            Command::SaveSite { index, site } => self.save_site(index, site),
            Command::RemoveSite { index, clear_cookies } => self.remove_site(index, clear_cookies).await,
            Command::MoveSite { from, to } => {
                let n = self.config.streaming.len();
                if from < n && to < n && from != to {
                    self.update_config(|c| {
                        let site = c.streaming.remove(from);
                        c.streaming.insert(to, site);
                    });
                    self.emit_sites();
                }
            }
            Command::SetSiteHidden(index, hidden) => {
                if index < self.config.streaming.len() {
                    self.update_config(|c| c.streaming[index].hidden = hidden);
                    self.emit_sites();
                }
            }
            Command::OpenUrl { label, url } => {
                self.stop_playback().await;
                self.close_browser().await;
                self.launch_browser(&label, &url).await;
            }
            Command::WhereToWatch(query) => self.watch.search(query, self.config.streaming.clone()),
            Command::OpenOnService { site, title } => {
                if let Some(s) = self.config.streaming.get(site).cloned() {
                    self.stop_playback().await;
                    self.close_browser().await;
                    if self.launch_browser(&s.name, &s.open_url(&title)).await
                        && self.state.mark_opened(&s.name, chrono::Utc::now())
                    {
                        self.save_state();
                        self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
                    }
                }
            }
            Command::SetTmdbToken(token) => {
                self.watch.set_token(token, &self.config.streaming).await;
                self.watch.refresh_library(self.sources(), true);
            }
            Command::FindProviders(name) => self.watch.find_providers(name),
            Command::CloseBrowser => self.close_browser().await,
            Command::TvGeometry(g) => {
                debug!(?g, "TV geometry");
                self.tv_geometry = Some(g);
            }
            Command::Search(query) => self.search(query),
            Command::SignOut(kind) => self.sign_out(kind).await,
            Command::JellyfinServer(url) => {
                let url = url.trim().trim_end_matches('/').to_owned();
                if !url.is_empty() && url != self.config.jellyfin.server_url {
                    info!(%url, "Jellyfin server address changed");
                    self.update_config(|c| c.jellyfin.server_url = url);
                }
            }
            Command::JellyfinPassword { username, password } => self.jellyfin_password(username, password),
            Command::ClearBrowserProfile => self.clear_browser_profile().await,
            Command::SetTheme(mode) => self.update_config(|c| c.general.theme = mode),
            Command::FindPlace(query) => {
                let events = self.events.clone();
                tokio::spawn(async move {
                    let http = reqwest::Client::builder()
                        .timeout(Duration::from_secs(15))
                        .user_agent(concat!("tvdash/", env!("CARGO_PKG_VERSION")))
                        .build()
                        .expect("HTTP client");
                    let base = std::env::var("TVDASH_GEOCODING_URL")
                        .unwrap_or_else(|_| tvdash_widgets::weather::GEOCODING_BASE.into());
                    let result =
                        tvdash_widgets::weather::geocode(&http, &base, &query).await.map_err(|e| e.to_string());
                    let _ = events.send(AppEvent::Places { query, result });
                });
            }
            Command::SetLocation(place) => {
                let tz = place.timezone.clone();
                self.update_config(|c| {
                    c.general.location = place.label.clone();
                    c.general.latitude = place.latitude;
                    c.general.longitude = place.longitude;
                    if !tz.is_empty() {
                        c.general.timezone = tz.clone();
                    }
                });
                if let Some(tx) = &self.general_tx {
                    let _ = tx.send(self.config.general.clone());
                }
                info!(location = %place.label, "weather location set");
                self.emit(AppEvent::Location {
                    name: place.label.clone(),
                    timezone: self.config.general.timezone.clone(),
                });
                self.toast(format!("Weather location: {}", place.label));
            }
            Command::Home => {
                self.stop_playback().await;
                self.close_browser().await;
            }
            Command::SetLayout(layout) => {
                self.update_config(|c| c.display.layout = layout);
                let name = match layout {
                    tvdash_core::config::Layout::Tv => "TV only",
                    tvdash_core::config::Layout::Dual => "two screens",
                };
                self.toast(format!("Layout: {name}"));
            }
            Command::SetDisplay { units, clock, seconds } => {
                self.update_config(|c| {
                    c.general.units = units;
                    c.general.clock = clock;
                    c.general.seconds = seconds;
                });
                if let Some(tx) = &self.general_tx {
                    let _ = tx.send(self.config.general.clone());
                }
            }
            Command::SetupFinished => {
                self.state.setup_complete = true;
                self.save_state();
            }
        }
    }

    // ── Accounts ────────────────────────────────────────────────────

    async fn sign_out(&mut self, kind: SourceKind) {
        if let Some(task) = self.sign_in.remove(&kind) {
            task.abort();
        }
        if self.playback.as_ref().is_some_and(|p| p.status.item.source == kind) {
            self.stop_playback().await;
        }
        let key = match kind {
            SourceKind::Jellyfin => SecretKey::JellyfinToken,
            SourceKind::Plex => SecretKey::PlexToken,
        };
        if let Err(e) = tokio::task::spawn_blocking(move || secrets::delete(key)).await.unwrap_or(Ok(())) {
            warn!(error = %e, "couldn't remove token from the keyring");
        }
        match kind {
            SourceKind::Jellyfin => {
                self.jellyfin = None;
                self.update_config(|c| c.jellyfin.user_id.clear());
            }
            SourceKind::Plex => {
                self.plex = None;
                self.update_config(|c| c.plex.server_url.clear());
            }
        }
        self.watch.forget(kind);
        info!(source = kind.label(), "signed out");
        self.emit(AppEvent::Server(kind, None));
        self.emit(AppEvent::ContinueWatching(kind, Vec::new()));
        self.emit(AppEvent::RecentlyAdded(kind, Vec::new()));
        self.emit(AppEvent::Account(kind, AccountStatus::NotSetUp));
        self.poll_sessions();
        self.toast(format!("Signed out of {}", kind.label()));
    }

    fn jellyfin_password(&mut self, username: String, password: String) {
        if let Some(old) = self.sign_in.remove(&SourceKind::Jellyfin) {
            old.abort();
        }
        let cfg = self.config.jellyfin.clone();
        let (events, internal) = (self.events.clone(), self.internal.clone());
        let task = tokio::spawn(async move {
            let send = |s| {
                let _ = events.send(AppEvent::SignIn(SourceKind::Jellyfin, s));
            };
            send(SignInState::Starting);
            let result = match JellyfinClient::new(&cfg.server_url, &cfg.device_id, None) {
                Ok(c) => c.authenticate_by_name(&username, &password).await,
                Err(e) => Err(e),
            };
            match result {
                Ok(auth) => {
                    let _ = internal.send(Internal::JellyfinSignedIn(auth));
                }
                Err(MediaError::Unauthorized) => send(SignInState::Failed("wrong username or password".into())),
                Err(e) => send(SignInState::Failed(e.to_string())),
            }
        });
        self.sign_in.insert(SourceKind::Jellyfin, task);
    }

    async fn clear_browser_profile(&mut self) {
        self.close_browser().await;
        let dir = expand_home(&self.config.browser.profile_dir);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if !safe_to_delete(&dir, home.as_deref()) {
            error!(dir = %dir.display(), "refusing to delete suspicious browser profile path");
            self.toast(format!("Not deleting {}: check [browser] profile_dir", dir.display()));
            return;
        }
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => info!(dir = %dir.display(), "browser profile deleted"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                self.toast(format!("Couldn't delete the browser profile: {e}"));
                return;
            }
        }
        self.state.streaming_opened.clear();
        self.save_state();
        self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
        self.toast("Browser profile cleared: streaming sites are signed out");
    }

    fn search(&self, query: String) {
        let sources = self.sources();
        let (events, requested) = (self.events.clone(), self.posters_requested.clone());
        tokio::spawn(async move {
            let q = query.trim().to_owned();
            if q.is_empty() {
                return;
            }
            let mut items = Vec::new();
            for source in sources {
                match source.search(&q).await {
                    Ok(found) => {
                        fetch_posters(source.clone(), found.clone(), requested.clone(), events.clone());
                        items.extend(found);
                    }
                    Err(e) => warn!(source = source.kind().label(), error = %e, "search failed"),
                }
            }
            let _ = events.send(AppEvent::SearchResults { query, items });
        });
    }

    // ── Library rows ────────────────────────────────────────────────

    fn sources(&self) -> Vec<Arc<dyn MediaSource>> {
        let mut v: Vec<Arc<dyn MediaSource>> = Vec::new();
        if let Some(j) = &self.jellyfin {
            v.push(j.clone());
        }
        if let Some(p) = &self.plex {
            v.push(p.clone());
        }
        v
    }

    fn source_for(&self, kind: SourceKind) -> Option<Arc<dyn MediaSource>> {
        self.sources().into_iter().find(|s| s.kind() == kind)
    }

    /// Active sessions on both servers, merged (other devices only; ours is the card itself).
    fn poll_sessions(&self) {
        let sources = self.sources();
        let events = self.events.clone();
        tokio::spawn(async move {
            let mut all = Vec::new();
            for source in sources {
                match source.now_playing().await {
                    Ok(s) => all.extend(s),
                    Err(e) => debug!(source = source.kind().label(), error = %e, "sessions unavailable"),
                }
            }
            let _ = events.send(AppEvent::NowPlaying(all));
        });
    }

    fn refresh(&self) {
        self.poll_sessions();
        for source in self.sources() {
            let events = self.events.clone();
            let requested = self.posters_requested.clone();
            tokio::spawn(async move {
                let kind = source.kind();
                match source.continue_watching().await {
                    Ok(items) => {
                        let note = format!("{} in Continue Watching", items.len());
                        let _ = events.send(AppEvent::Health { service: kind.into(), result: Ok(note) });
                        let _ = events.send(AppEvent::Account(kind, AccountStatus::Connected));
                        let _ = events.send(AppEvent::ContinueWatching(kind, items.clone()));
                        fetch_posters(source.clone(), items, requested.clone(), events.clone());
                    }
                    Err(e) => {
                        warn!(source = kind.label(), error = %e, "continue watching failed");
                        let _ = events.send(AppEvent::Health { service: kind.into(), result: Err(e.to_string()) });
                        let _ = events.send(AppEvent::Account(kind, account_status(&e)));
                        return;
                    }
                }
                match source.recently_added(RECENTLY_ADDED).await {
                    Ok(items) => {
                        let _ = events.send(AppEvent::RecentlyAdded(kind, items.clone()));
                        fetch_posters(source, items, requested, events);
                    }
                    Err(e) => warn!(source = kind.label(), error = %e, "recently added failed"),
                }
            });
        }
    }

    // ── Sign-in ─────────────────────────────────────────────────────

    fn start_jellyfin_sign_in(&mut self) {
        if let Some(old) = self.sign_in.remove(&SourceKind::Jellyfin) {
            old.abort();
        }
        let cfg = self.config.jellyfin.clone();
        let events = self.events.clone();
        let internal = self.internal.clone();
        let task = tokio::spawn(async move {
            let send = |s| {
                let _ = events.send(AppEvent::SignIn(SourceKind::Jellyfin, s));
            };
            send(SignInState::Starting);
            match jellyfin_quick_connect(&cfg.server_url, &cfg.device_id, |code| send(SignInState::Code(code))).await {
                Ok(auth) => {
                    let _ = internal.send(Internal::JellyfinSignedIn(auth));
                }
                Err(e) => {
                    warn!(error = %e, "Jellyfin sign-in failed");
                    send(SignInState::Failed(e.to_string()));
                }
            }
        });
        self.sign_in.insert(SourceKind::Jellyfin, task);
    }

    fn start_plex_sign_in(&mut self) {
        if let Some(old) = self.sign_in.remove(&SourceKind::Plex) {
            old.abort();
        }
        let cfg = self.config.plex.clone();
        let events = self.events.clone();
        let internal = self.internal.clone();
        let task = tokio::spawn(async move {
            let send = |s| {
                let _ = events.send(AppEvent::SignIn(SourceKind::Plex, s));
            };
            send(SignInState::Starting);
            match plex_pin_sign_in(&cfg.account_url, &cfg.client_id, |code| send(SignInState::Code(code))).await {
                Ok((account_token, server)) => {
                    let _ = internal.send(Internal::PlexSignedIn { account_token, server });
                }
                Err(e) => {
                    warn!(error = %e, "Plex sign-in failed");
                    send(SignInState::Failed(e.to_string()));
                }
            }
        });
        self.sign_in.insert(SourceKind::Plex, task);
    }

    async fn on_plex_signed_in(&mut self, account_token: String, server: DiscoveredServer) {
        self.sign_in.remove(&SourceKind::Plex);
        // Owned servers use the account token; shared ones get their own. Keep the
        // one that works against this server.
        let token = if server.access_token.is_empty() { account_token } else { server.access_token.clone() };
        let to_store = token.clone();
        if !matches!(
            tokio::task::spawn_blocking(move || secrets::set(SecretKey::PlexToken, &to_store)).await,
            Ok(Ok(()))
        ) {
            error!("couldn't store the Plex token in the keyring");
            self.toast("Signed in, but the token couldn't be saved to the keyring");
        }
        let url = server.url.clone();
        match Config::update_at(&self.config_path, |c| c.plex.server_url = url) {
            Ok(c) => self.config = c,
            Err(e) => error!(error = %e, "couldn't save Plex server URL to config"),
        }
        match PlexClient::new(&server.url, &token, &self.config.plex.client_id) {
            Ok(c) => {
                self.plex = Some(Arc::new(c));
                info!(server = %server.name, url = %server.url, "Plex signed in");
                self.emit(AppEvent::SignIn(SourceKind::Plex, SignInState::Done));
                self.emit(AppEvent::Server(SourceKind::Plex, Some(server.url.clone())));
                self.emit(AppEvent::Account(SourceKind::Plex, AccountStatus::Connected));
                self.toast(format!("Plex server: {}", server.name));
                self.refresh();
            }
            Err(e) => self.emit(AppEvent::SignIn(SourceKind::Plex, SignInState::Failed(e.to_string()))),
        }
    }

    async fn on_jellyfin_signed_in(&mut self, auth: Auth) {
        self.sign_in.remove(&SourceKind::Jellyfin);
        let token = auth.token.clone();
        let stored = tokio::task::spawn_blocking(move || secrets::set(SecretKey::JellyfinToken, &token)).await;
        if !matches!(stored, Ok(Ok(()))) {
            error!(?stored, "couldn't store the Jellyfin token in the keyring");
            self.toast("Signed in, but the token couldn't be saved to the keyring");
        }
        let user_id = auth.user_id.clone();
        match Config::update_at(&self.config_path, |c| c.jellyfin.user_id = user_id) {
            Ok(c) => self.config = c,
            Err(e) => error!(error = %e, "couldn't save Jellyfin user id to config"),
        }
        let cfg = &self.config.jellyfin;
        match JellyfinClient::new(&cfg.server_url, &cfg.device_id, Some(auth)) {
            Ok(c) => {
                self.jellyfin = Some(Arc::new(c));
                info!("Jellyfin signed in");
                self.emit(AppEvent::SignIn(SourceKind::Jellyfin, SignInState::Done));
                self.emit(AppEvent::Server(SourceKind::Jellyfin, Some(self.config.jellyfin.server_url.clone())));
                self.emit(AppEvent::Account(SourceKind::Jellyfin, AccountStatus::Connected));
                self.refresh();
            }
            Err(e) => self.emit(AppEvent::SignIn(SourceKind::Jellyfin, SignInState::Failed(e.to_string()))),
        }
    }

    // ── Playback ────────────────────────────────────────────────────

    async fn play(&mut self, item: MediaItem) {
        self.stop_playback().await;
        self.close_browser().await;
        let Some(source) = self.source_for(item.source) else {
            self.toast(format!("{} isn't connected", item.source.label()));
            return;
        };
        let url = match source.stream_url(&item).await {
            Ok(u) => u,
            Err(e) => {
                if matches!(e, MediaError::Unauthorized) {
                    self.emit(AppEvent::Account(item.source, AccountStatus::Expired));
                }
                self.toast(format!("Can't play: {e}"));
                return;
            }
        };
        let opts = MpvOptions {
            binary: self.config.player.mpv.clone(),
            screen_name: Some(self.config.display.tv_output.clone()).filter(|s| !s.is_empty()),
            title: format::item_line(&item),
            start_ms: item.resume_ms,
            fullscreen: !self.windowed,
        };
        let mpv = match Mpv::spawn(&opts, &url).await {
            Ok(m) => m,
            Err(e) => {
                error!(error = %e, "couldn't start mpv");
                self.health(Service::Player, Err(e.to_string()));
                self.toast(format!("Couldn't start mpv: {e}"));
                return;
            }
        };
        info!(title = %item.title, resume_ms = ?item.resume_ms, "playing on the TV");
        self.health(Service::Player, Ok(format!("playing {}", item.title)));

        let start = item.resume_ms.unwrap_or(0);
        let reporter = spawn_reporter(source, item.clone());
        let _ = reporter.send((start, PlayState::Playing));
        let status =
            PlayerStatus { duration_ms: item.duration_ms, item, state: PlayState::Playing, position_ms: start };
        self.emit(AppEvent::Player(Some(status.clone())));
        self.playback = Some(Playback { status, mpv, reporter, last_emitted_sec: None, decode_hint_shown: false });
    }

    // ── Streaming browser ───────────────────────────────────────────

    fn browser_geometry(&self) -> Option<Geometry> {
        match (self.config.browser.position_override(), self.tv_geometry) {
            (Some((x, y)), Some(g)) => Some(Geometry { x, y, ..g }),
            (Some((x, y)), None) => Some(Geometry { x, y, width: 1920, height: 1080 }),
            (None, g) => g,
        }
    }

    fn fill_providers(&mut self, fills: Vec<(String, Vec<u32>)>) {
        let mut named = Vec::new();
        self.update_config(|c| {
            for (name, ids) in fills {
                if let Some(site) = c.streaming.iter_mut().find(|s| s.name == name && s.tmdb_providers.is_empty()) {
                    site.tmdb_providers = ids;
                    named.push(name);
                }
            }
        });
        if !named.is_empty() {
            info!(sites = ?named, "matched TMDB providers by name");
            self.emit_sites();
        }
    }

    fn emit_sites(&self) {
        self.emit(AppEvent::Sites(self.config.streaming.clone()));
        self.watch.logos(&self.config.streaming);
    }

    fn save_site(&mut self, index: Option<usize>, site: StreamingSite) {
        let site = StreamingSite {
            name: site.name.trim().to_owned(),
            url: site.url.trim().to_owned(),
            search_url: site.search_url.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty()),
            ..site
        };
        let name = site.name.clone();
        match index {
            Some(i) if i < self.config.streaming.len() => {
                let old = self.config.streaming[i].name.clone();
                self.update_config(|c| c.streaming[i] = site);
                // Keep "opened N days ago" across a rename.
                if old != name
                    && let Some(t) = self.state.streaming_opened.remove(&old)
                {
                    self.state.streaming_opened.insert(name.clone(), t);
                    self.save_state();
                    self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
                }
                self.toast(format!("Saved {name}"));
            }
            Some(i) => warn!(index = i, "save site: no such entry"),
            None => {
                self.update_config(|c| c.streaming.push(site));
                self.toast(format!("Added {name}"));
            }
        }
        self.emit_sites();
    }

    async fn remove_site(&mut self, index: usize, clear_cookies: bool) {
        if index >= self.config.streaming.len() {
            return;
        }
        let site = self.config.streaming[index].clone();
        if self.browser.as_ref().is_some_and(|b| b.site == site.name) {
            self.close_browser().await;
        }
        self.update_config(|c| {
            c.streaming.remove(index);
        });
        if self.state.streaming_opened.remove(&site.name).is_some() {
            self.save_state();
            self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
        }
        self.emit_sites();
        match site.domain().filter(|_| clear_cookies) {
            // The cookie store is encrypted and locked while the browser runs, so
            // open the browser's own settings for that site and let you clear it there.
            Some(domain) => {
                self.stop_playback().await;
                self.close_browser().await;
                let url = format!("chrome://settings/content/all?searchSubpage={}", encode_component(domain));
                if self.launch_browser(&format!("{} cookies", site.name), &url).await {
                    self.emit(AppEvent::LongToast(format!(
                        "Removed {}. On the TV, delete its stored data, then press Close browser.",
                        site.name
                    )));
                }
            }
            None => self.toast(format!("Removed {}", site.name)),
        }
    }

    /// Start the TV browser on `url`. Returns false (with a toast) if it couldn't start.
    async fn launch_browser(&mut self, label: &str, url: &str) -> bool {
        let opts = self.browser_options();
        match Browser::launch(&opts, label, url).await {
            Ok(b) => {
                self.browser = Some(b);
                self.emit(AppEvent::Browser(Some(label.to_owned())));
                self.health(Service::Browser, Ok(format!("opened {label}")));
                true
            }
            Err(e) => {
                error!(error = %e, "couldn't start the browser");
                self.health(Service::Browser, Err(e.to_string()));
                self.toast(format!("Couldn't open {label}: {e}"));
                false
            }
        }
    }

    fn browser_options(&self) -> BrowserOptions {
        BrowserOptions {
            binary: self.config.browser.binary.clone(),
            profile_dir: expand_home(&self.config.browser.profile_dir),
            use_xwayland: self.config.browser.use_xwayland,
            geometry: self.browser_geometry(),
            fullscreen: !self.windowed,
        }
    }

    async fn open_site(&mut self, index: usize) {
        let Some(site) = self.config.streaming.get(index).cloned() else {
            warn!(index, "open site: no such streaming entry");
            return;
        };
        self.stop_playback().await;
        self.close_browser().await;
        if !self.launch_browser(&site.name, &site.url).await {
            return;
        }
        let opts = self.browser_options();

        if self.state.mark_opened(&site.name, chrono::Utc::now()) {
            let widevine = if is_brave(&opts.binary) {
                " If Brave asks to enable Widevine, choose Enable (once per profile)."
            } else {
                ""
            };
            self.emit(AppEvent::LongToast(format!(
                "First time on {}: sign in on the TV with this laptop's mouse and keyboard. It stays signed in.{widevine}",
                site.name
            )));
        }
        self.save_state();
        self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
    }

    async fn close_browser(&mut self) {
        if let Some(b) = self.browser.take() {
            b.close().await;
            self.emit(AppEvent::Browser(None));
        }
    }

    fn on_browser_exited(&mut self) {
        let Some(b) = self.browser.take() else { return };
        self.emit(AppEvent::Browser(None));
        if b.exited_immediately() {
            warn!(site = %b.site, "browser exited at once; another instance probably owns the profile");
            self.toast("The browser handed off to a window that was already open. Close it and try again.");
        } else {
            info!(site = %b.site, "browser closed on the TV");
        }
        drop(b); // signals any helpers left in its process group
    }

    async fn stop_playback(&mut self) {
        if let Some(p) = self.playback.take() {
            let _ = p.reporter.send((p.status.position_ms, PlayState::Stopped));
            p.mpv.quit().await;
            self.after_playback();
        }
    }

    fn after_playback(&self) {
        self.emit(AppEvent::Player(None));
        // Give the server a moment to record the stop before re-reading resume points.
        let internal = self.internal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = internal.send(Internal::Refresh);
        });
    }

    async fn on_mpv(&mut self, event: Option<MpvEvent>) {
        let Some(p) = self.playback.as_mut() else { return };
        let mut overlay: Option<Option<u32>> = None; // Some(duration) → show overlay
        match event {
            Some(MpvEvent::Position(secs)) => {
                p.status.position_ms = (secs.max(0.0) * 1000.0) as u64;
                let sec = p.status.position_ms / 1000;
                if p.last_emitted_sec == Some(sec) {
                    return;
                }
                p.last_emitted_sec = Some(sec);
            }
            Some(MpvEvent::Duration(secs)) => {
                let first = p.status.duration_ms.is_none() || p.last_emitted_sec.is_none();
                p.status.duration_ms = Some((secs * 1000.0) as u64);
                if first {
                    overlay = Some(Some(5000));
                }
            }
            Some(MpvEvent::Paused(paused)) => {
                let state = if paused { PlayState::Paused } else { PlayState::Playing };
                if state == p.status.state {
                    return;
                }
                p.status.state = state;
                let _ = p.reporter.send((p.status.position_ms, state));
                // Paused: keep the overlay up. Resumed: hide it after 5 s.
                overlay = Some(if paused { None } else { Some(5000) });
            }
            Some(MpvEvent::Ended { reason }) => {
                info!(reason, "mpv playback ended");
                return;
            }
            Some(MpvEvent::Error { prefix, text }) => {
                warn!(%prefix, %text, "mpv error");
                let _ = self
                    .events
                    .send(AppEvent::Health { service: Service::Player, result: Err(format!("{prefix}: {text}")) });
                if !p.decode_hint_shown
                    && let Some(hint) = decoder_hint(&prefix, &text)
                {
                    p.decode_hint_shown = true;
                    let _ = self.events.send(AppEvent::LongToast(hint));
                }
                return;
            }
            Some(MpvEvent::Exited) | None => {
                let p = self.playback.take().expect("checked above");
                let _ = p.reporter.send((p.status.position_ms, PlayState::Stopped));
                info!("mpv exited; TV back to idle");
                self.after_playback();
                return;
            }
        }
        let status = p.status.clone();
        if let Some(duration) = overlay {
            let text = format::overlay(&status, format::now_local(self.config.general.tz()), self.config.general.clock);
            if let Err(e) = p.mpv.show_text(&text, duration).await {
                warn!(error = %e, "mpv overlay failed");
            }
        }
        self.emit(AppEvent::Player(Some(status)));
    }

    fn report_tick(&self) {
        if let Some(p) = &self.playback
            && p.status.state == PlayState::Playing
        {
            let _ = p.reporter.send((p.status.position_ms, PlayState::Playing));
        }
    }
}

/// Turn mpv's "can't decode" errors into something actionable. Fedora's own
/// ffmpeg ("ffmpeg-free") leaves out H.264/HEVC, so audio plays with no picture.
fn decoder_hint(prefix: &str, text: &str) -> Option<String> {
    let video = prefix == "vd" || prefix.starts_with("ffmpeg/video");
    let audio = prefix == "ad" || prefix.starts_with("ffmpeg/audio");
    // Either the decoder can't start, or (OpenH264 with most H.264 movies) it starts
    // and then fails on every frame.
    let codec = text.strip_prefix("Failed to initialize a decoder for codec '").and_then(|r| r.split('\'').next());
    let decode_error = codec.is_some()
        || text.starts_with("Error while decoding frame")
        || text.contains("DecodeFrame")
        || text.starts_with("libopenh264");
    if !(decode_error && (video || audio)) {
        return None;
    }
    let what = match (codec, video) {
        (Some(c), true) => format!("{c} video"),
        (Some(c), false) => format!("{c} audio"),
        (None, true) => "this video".into(),
        (None, false) => "this audio".into(),
    };
    Some(format!(
        "mpv can't decode {what}, so there's no {}. On Fedora, install the full codecs from \
         RPM Fusion: sudo dnf swap ffmpeg-free ffmpeg --allowerasing",
        if video { "picture" } else { "sound" }
    ))
}

fn account_status(e: &MediaError) -> AccountStatus {
    match e {
        MediaError::Unauthorized => AccountStatus::Expired,
        other => AccountStatus::Unreachable(other.to_string()),
    }
}

async fn jellyfin_quick_connect(
    server_url: &str,
    device_id: &str,
    show_code: impl Fn(String),
) -> tvdash_media::Result<Auth> {
    let client = JellyfinClient::new(server_url, device_id, None)?;
    let qc = client.quick_connect_initiate().await?;
    show_code(qc.code.clone());
    let deadline = tokio::time::Instant::now() + QUICK_CONNECT_TIMEOUT;
    loop {
        tokio::time::sleep(QUICK_CONNECT_POLL).await;
        if client.quick_connect_approved(&qc.secret).await? {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            return Err(MediaError::Protocol("the code expired — try again".into()));
        }
    }
    client.authenticate_with_quick_connect(&qc.secret).await
}

async fn plex_pin_sign_in(
    account_url: &str,
    client_id: &str,
    show_code: impl Fn(String),
) -> tvdash_media::Result<(String, DiscoveredServer)> {
    let account = PlexAccount::new(account_url, client_id)?;
    let pin = account.create_pin().await?;
    show_code(pin.code.clone());
    let deadline = tokio::time::Instant::now() + pin.expires_in;
    let token = loop {
        tokio::time::sleep(PLEX_PIN_POLL).await;
        if let Some(token) = account.check_pin(pin.id).await? {
            break token;
        }
        if tokio::time::Instant::now() > deadline {
            return Err(MediaError::Protocol("the code expired — try again".into()));
        }
    };
    let server = account.discover(&token).await?;
    Ok((token, server))
}

/// Refuse to delete anything that could be a real directory rather than our
/// browser profile: `/`, `/home`, `$HOME` itself, or anything shallower.
fn safe_to_delete(dir: &std::path::Path, home: Option<&std::path::Path>) -> bool {
    let deep_enough = dir.is_absolute() && dir.components().count() >= 4;
    let is_home_or_parent = home.is_some_and(|h| h.starts_with(dir));
    deep_enough && !is_home_or_parent
}

#[cfg(test)]
mod tests {
    use super::{decoder_hint, safe_to_delete};

    #[test]
    fn decoder_failure_hint() {
        let h = decoder_hint("vd", "Failed to initialize a decoder for codec 'hevc'.").unwrap();
        assert!(h.starts_with("mpv can't decode hevc video, so there's no picture."), "{h}");
        assert!(h.contains("ffmpeg-free"));
        // OpenH264 starts, then fails on each frame of a High-profile file.
        let h = decoder_hint("ffmpeg/video", "libopenh264: DecodeFrame failed").unwrap();
        assert!(h.starts_with("mpv can't decode this video"), "{h}");
        assert!(decoder_hint("vd", "Error while decoding frame (hardware decoding)!").is_some());
        assert!(decoder_hint("ad", "Failed to initialize a decoder for codec 'eac3'.").unwrap().contains("no sound"));
        assert_eq!(decoder_hint("vd", "Something else"), None);
        assert_eq!(decoder_hint("cplayer", "Error while decoding frame"), None);
    }
    use std::path::Path;

    #[test]
    fn profile_delete_guard() {
        let home = Some(Path::new("/home/me"));
        assert!(safe_to_delete(Path::new("/home/me/.local/share/tvdash/browser"), home));
        assert!(safe_to_delete(Path::new("/tmp/tvdash-e2e/x/profile"), home));
        for bad in ["/", "/tmp", "/home", "/home/me", "relative/path/that/is/deep"] {
            assert!(!safe_to_delete(Path::new(bad), home), "{bad}");
        }
    }
}

async fn read_secret(key: SecretKey) -> Option<String> {
    match tokio::task::spawn_blocking(move || secrets::get(key)).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            warn!(key = key.as_str(), error = %e, "couldn't read from the keyring");
            None
        }
        Err(e) => {
            warn!(error = %e, "keyring task failed");
            None
        }
    }
}

fn spawn_reporter(source: Arc<dyn MediaSource>, item: MediaItem) -> UnboundedSender<(u64, PlayState)> {
    let (tx, mut rx) = unbounded_channel::<(u64, PlayState)>();
    tokio::spawn(async move {
        while let Some((pos, state)) = rx.recv().await {
            if let Err(e) = source.report_progress(&item, pos, state).await {
                warn!(source = source.kind().label(), ?state, error = %e, "progress report failed");
            }
        }
    });
    tx
}

fn fetch_posters(
    source: Arc<dyn MediaSource>,
    items: Vec<MediaItem>,
    requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    events: UnboundedSender<AppEvent>,
) {
    for item in items.into_iter().filter(|i| i.poster.is_some()) {
        let key = (item.source, item.id.clone());
        if !requested.lock().unwrap().insert(key.clone()) {
            continue;
        }
        let (source, requested, events) = (source.clone(), requested.clone(), events.clone());
        tokio::spawn(async move {
            let decoded = match source.poster(&item, POSTER_HEIGHT).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || decode(&bytes)).await.ok().flatten(),
                Err(e) => {
                    warn!(title = %item.title, error = %e, "poster fetch failed");
                    None
                }
            };
            match decoded {
                Some(image) => {
                    let _ = events.send(AppEvent::Poster { source: item.source, item_id: item.id, image });
                }
                None => {
                    requested.lock().unwrap().remove(&key); // retry on next refresh
                }
            }
        });
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            Some(RgbaImage { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() })
        }
        Err(e) => {
            warn!(error = %e, "poster decode failed");
            None
        }
    }
}

#[cfg(test)]
mod e2e {
    //! Drives the real controller against fake servers + fake mpv (Python).
    //! Run with: `cargo test -p tvdash -- --ignored`

    use super::*;
    use serde_json::{Value, json};
    use std::{path::Path, process::Stdio};
    use tokio::time::{sleep, timeout};

    const FAKES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes");

    /// The tests share env vars and the mpv socket path; run them one at a time.
    static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    struct Harness {
        dir: PathBuf,
        port: String,
        server_log: PathBuf,
        mpv_log: PathBuf,
        _server: tokio::process::Child,
        events: UnboundedReceiver<AppEvent>,
        cmd: UnboundedSender<Command>,
        config_path: PathBuf,
        /// Every event seen so far, whichever wait it arrived during.
        seen: Vec<AppEvent>,
    }

    impl Harness {
        /// Start `fake` and a controller whose config is tweaked by `configure(config, port)`.
        async fn start(fake: &str, configure: impl FnOnce(&mut Config, &str)) -> Self {
            Self::start_with(fake, configure, |_, _| {}).await
        }

        /// Like `start`, and `tweak(controller, dir)` runs before the controller does.
        async fn start_with(
            fake: &str,
            configure: impl FnOnce(&mut Config, &str),
            tweak: impl FnOnce(&mut Controller, &Path),
        ) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "tvdash-e2e-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let (port_file, server_log, mpv_log) = (dir.join("port"), dir.join("server.log"), dir.join("mpv.log"));
            let server = tokio::process::Command::new(format!("{FAKES}/{fake}"))
                .arg(&port_file)
                .arg(&server_log)
                .stdout(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .expect("start fake server");
            let port = timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(p) = std::fs::read_to_string(&port_file)
                        && !p.is_empty()
                    {
                        return p;
                    }
                    sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .expect("fake server port");

            // SAFETY: test-only, serialized by SERIAL.
            unsafe {
                std::env::set_var("FAKE_MPV_LOG", &mpv_log);
                std::env::set_var("FAKE_BROWSER_LOG", dir.join("browser.log"));
                std::env::remove_var("FAKE_BROWSER_MODE");
                std::env::set_var("XDG_RUNTIME_DIR", &dir);
            }
            let mut config = Config::default();
            config.player.mpv = format!("{FAKES}/fake_mpv.py");
            config.browser.binary = format!("{FAKES}/fake_browser.py");
            config.browser.profile_dir = dir.join("profile").display().to_string();
            configure(&mut config, &port);
            let config_path = dir.join("config.toml");
            config.save_to(&config_path).unwrap();

            let (ev_tx, events) = unbounded_channel();
            let (cmd, cmd_rx) = unbounded_channel();
            let (mut controller, internal_rx) = Controller::new(config, config_path.clone(), false, ev_tx);
            controller.state_path = Some(dir.join("state.toml"));
            // Never touch the real cache in tests.
            controller.watch = Watch::new(controller.events.clone(), Some(dir.join("cache.db")));
            controller.watch.set_internal(controller.internal.clone());
            tweak(&mut controller, &dir);
            tokio::spawn(controller.run(cmd_rx, internal_rx));
            Self { dir, port, server_log, mpv_log, _server: server, events, cmd, config_path, seen: Vec::new() }
        }

        fn send(&self, c: Command) {
            self.cmd.send(c).unwrap();
        }

        async fn wait_for<T>(&mut self, mut pick: impl FnMut(&AppEvent) -> Option<T>) -> T {
            timeout(Duration::from_secs(15), async {
                loop {
                    let ev = self.events.recv().await.expect("controller stopped");
                    if std::env::var_os("E2E_DEBUG").is_some() {
                        eprintln!("event: {ev:?}");
                    }
                    self.seen.push(ev.clone());
                    if let Some(v) = pick(&ev) {
                        return v;
                    }
                }
            })
            .await
            .expect("timed out waiting for event")
        }

        /// Like `wait_for`, but also matches events that already arrived (latest first).
        async fn find_or_wait<T>(&mut self, mut pick: impl FnMut(&AppEvent) -> Option<T>) -> T {
            if let Some(v) = self.seen.iter().rev().find_map(&mut pick) {
                return v;
            }
            self.wait_for(pick).await
        }

        async fn wait_posters(&mut self, n: usize) {
            let mut seen = HashSet::new();
            while seen.len() < n {
                let id = self
                    .wait_for(|e| match e {
                        AppEvent::Poster { item_id, image, .. } => {
                            assert_eq!((image.width, image.height), (4, 6));
                            Some(item_id.clone())
                        }
                        _ => None,
                    })
                    .await;
                seen.insert(id);
            }
        }

        /// Play → pause → seek → resume → stop, checking the events along the way.
        async fn play_pause_stop(&mut self, item: &MediaItem) {
            let start = item.resume_ms.unwrap_or(0);
            self.send(Command::Play(item.clone()));
            self.wait_for(|e| match e {
                AppEvent::Player(Some(s)) if s.position_ms >= start + 2_000 && s.duration_ms.is_some() => Some(()),
                _ => None,
            })
            .await;
            self.send(Command::Pause);
            self.wait_for(|e| matches!(e, AppEvent::Player(Some(s)) if s.state == PlayState::Paused).then_some(()))
                .await;
            self.send(Command::Seek(30));
            self.send(Command::Resume);
            self.wait_for(|e| matches!(e, AppEvent::Player(Some(s)) if s.state == PlayState::Playing).then_some(()))
                .await;
            self.send(Command::Stop);
            self.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;
            sleep(Duration::from_millis(500)).await; // let the Stopped report land
        }

        fn server_log(&self) -> Vec<Value> {
            read_log(&self.server_log)
        }

        fn mpv_log(&self) -> (Vec<String>, Vec<Value>) {
            let log = read_log(&self.mpv_log);
            let argv = serde_json::from_value(log[0]["argv"].clone()).unwrap();
            let cmds = log.iter().filter_map(|l| l.get("command").cloned()).collect();
            (argv, cmds)
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn read_log(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    fn assert_mpv_transport(cmds: &[Value]) {
        for want in [json!(["set_property", "pause", true]), json!(["seek", 30, "relative"]), json!(["quit"])] {
            assert!(cmds.contains(&want), "mpv never got {want}: {cmds:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn jellyfin_sign_in_continue_watching_play_pause_stop() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_jellyfin.py", |c, port| {
            c.jellyfin.server_url = format!("http://127.0.0.1:{port}");
            c.jellyfin.device_id = "dev-e2e".into();
            c.plex.enabled = false;
        })
        .await;

        // Not signed in yet (and no keyring in CI) → sign-in prompt.
        h.wait_for(|e| matches!(e, AppEvent::Account(SourceKind::Jellyfin, AccountStatus::NotSetUp)).then_some(()))
            .await;

        // Quick Connect: code shown, approved on second poll, token exchanged.
        h.send(Command::SignIn(SourceKind::Jellyfin));
        let code = h
            .wait_for(|e| match e {
                AppEvent::SignIn(_, SignInState::Code(c)) => Some(c.clone()),
                _ => None,
            })
            .await;
        assert_eq!(code, "424242");
        h.wait_for(|e| matches!(e, AppEvent::SignIn(_, SignInState::Done)).then_some(())).await;
        let saved = Config::load_or_create_at(&h.config_path).unwrap().config;
        assert_eq!(saved.jellyfin.user_id, "u1", "user id persisted to config");

        // Continue Watching (via the pre-10.9 fallback route) and posters.
        let items = h
            .wait_for(|e| match e {
                AppEvent::ContinueWatching(SourceKind::Jellyfin, items) => Some(items.clone()),
                _ => None,
            })
            .await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].resume_ms, Some(600_000));
        h.wait_posters(2).await;

        h.play_pause_stop(&items[0]).await;

        // What Jellyfin saw.
        let jf = h.server_log();
        let paths: Vec<&str> = jf.iter().map(|r| r["path"].as_str().unwrap()).collect();
        let pos = |p: &str| paths.iter().position(|x| *x == p).unwrap_or_else(|| panic!("{p} not called: {paths:?}"));
        assert!(pos("/UserItems/Resume") < pos("/Users/u1/Items/Resume"), "10.9 route tried first");
        assert!(pos("/Sessions/Playing") < pos("/Sessions/Playing/Progress"));
        assert!(pos("/Sessions/Playing/Progress") < pos("/Sessions/Playing/Stopped"));
        let paused_report = jf
            .iter()
            .find(|r| r["path"] == "/Sessions/Playing/Progress" && r["body"]["IsPaused"] == true)
            .expect("pause reported to Jellyfin");
        assert_eq!(paused_report["body"]["ItemId"], "ep1");
        let stopped = jf.iter().find(|r| r["path"] == "/Sessions/Playing/Stopped").unwrap();
        assert!(stopped["body"]["PositionTicks"].as_u64().unwrap() > 6_000_000_000, "stop position past resume point");
        assert!(jf.iter().all(|r| r["auth"].as_str().unwrap().contains(r#"DeviceId="dev-e2e""#)));

        // What mpv was told.
        let (argv, cmds) = h.mpv_log();
        assert!(argv.contains(&"--start=600.000".into()));
        assert!(argv.contains(&"--fs-screen-name=HDMI-A-1".into()));
        assert!(argv.last().unwrap().contains("/Videos/ep1/stream?static=true&api_key=tok"));
        assert_mpv_transport(&cmds);
        let overlay = cmds.iter().find(|c| c[0] == "show-text" && c[1].as_str().unwrap().contains("Paused")).unwrap();
        assert!(overlay[1].as_str().unwrap().starts_with("Some Show · S1 · E1 · The Pilot\nPaused · "));

        // One thing on the TV at a time: opening a site stops mpv first.
        h.send(Command::Play(items[1].clone()));
        h.wait_for(|e| matches!(e, AppEvent::Player(Some(s)) if s.item.id == "mov1").then_some(())).await;
        h.send(Command::OpenSite(0));
        h.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(_))).then_some(())).await;
        sleep(Duration::from_millis(300)).await; // progress reports are async
        let (_, cmds) = h.mpv_log();
        assert_eq!(cmds.iter().filter(|c| **c == json!(["quit"])).count(), 2, "second mpv was quit");
        let stops = h.server_log().iter().filter(|r| r["path"] == "/Sessions/Playing/Stopped").count();
        assert_eq!(stops, 2, "second playback reported stopped");
        // ...and playing something closes the browser.
        h.send(Command::Play(items[0].clone()));
        h.wait_for(|e| matches!(e, AppEvent::Browser(None)).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::Player(Some(_))).then_some(())).await;
        h.send(Command::Stop);
        h.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;

        // Library: Recently Added (10.9 route) and search.
        let recent = h
            .find_or_wait(|e| match e {
                AppEvent::RecentlyAdded(SourceKind::Jellyfin, items) if !items.is_empty() => Some(items.clone()),
                _ => None,
            })
            .await;
        assert_eq!(recent[0].title, "New Film");
        h.send(Command::Search("film".into()));
        let (query, found) = h
            .wait_for(|e| match e {
                AppEvent::SearchResults { query, items } => Some((query.clone(), items.clone())),
                _ => None,
            })
            .await;
        assert_eq!((query.as_str(), found.len(), found[0].title.as_str()), ("film", 1, "A Film"));
        let search = h.server_log().into_iter().rfind(|r| r["path"] == "/Items").unwrap();
        assert_eq!(search["query"]["includeItemTypes"][0], "Movie,Series,Episode");

        // Sign out: rows cleared, account back to "not connected", config forgets the user.
        h.send(Command::SignOut(SourceKind::Jellyfin));
        h.wait_for(|e| matches!(e, AppEvent::Server(SourceKind::Jellyfin, None)).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::ContinueWatching(SourceKind::Jellyfin, v) if v.is_empty()).then_some(()))
            .await;
        h.wait_for(|e| matches!(e, AppEvent::Account(SourceKind::Jellyfin, AccountStatus::NotSetUp)).then_some(()))
            .await;
        assert_eq!(Config::load_or_create_at(&h.config_path).unwrap().config.jellyfin.user_id, "");

        // Password fallback: wrong, then right.
        h.send(Command::JellyfinPassword { username: "me".into(), password: "nope".into() });
        let failed = h
            .wait_for(|e| match e {
                AppEvent::SignIn(SourceKind::Jellyfin, SignInState::Failed(m)) => Some(m.clone()),
                _ => None,
            })
            .await;
        assert_eq!(failed, "wrong username or password");
        h.send(Command::JellyfinPassword { username: "me".into(), password: "pw".into() });
        h.wait_for(|e| matches!(e, AppEvent::SignIn(SourceKind::Jellyfin, SignInState::Done)).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::ContinueWatching(SourceKind::Jellyfin, v) if !v.is_empty()).then_some(()))
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn plex_pin_discovery_sessions_play_pause_stop() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_plex.py", |c, port| {
            c.jellyfin.enabled = false;
            c.plex.account_url = format!("http://127.0.0.1:{port}");
            c.plex.client_id = "plex-client-e2e".into();
        })
        .await;

        h.wait_for(|e| matches!(e, AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp)).then_some(())).await;

        // PIN: 4-char code, linked on second poll, then server discovery.
        h.send(Command::SignIn(SourceKind::Plex));
        let code = h
            .wait_for(|e| match e {
                AppEvent::SignIn(SourceKind::Plex, SignInState::Code(c)) => Some(c.clone()),
                _ => None,
            })
            .await;
        assert_eq!(code, "WXYZ");
        h.wait_for(|e| matches!(e, AppEvent::SignIn(SourceKind::Plex, SignInState::Done)).then_some(())).await;
        let server_url = format!("http://127.0.0.1:{}", h.port);
        let saved = Config::load_or_create_at(&h.config_path).unwrap().config;
        assert_eq!(saved.plex.server_url, server_url, "skipped the dead address, saved the live one");

        // On Deck → rows with the Plex badge, posters via the transcoder.
        let items = h
            .wait_for(|e| match e {
                AppEvent::ContinueWatching(SourceKind::Plex, items) => Some(items.clone()),
                _ => None,
            })
            .await;
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.source == SourceKind::Plex));
        assert_eq!((items[0].title.as_str(), items[0].resume_ms), ("Plex Show", Some(1_200_000)));
        h.wait_posters(2).await;

        // Sessions: the other device only; our own client id is filtered out.
        let sessions = h
            .find_or_wait(|e| match e {
                AppEvent::NowPlaying(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            })
            .await;
        assert_eq!(sessions.len(), 1);
        assert_eq!((sessions[0].device.as_str(), sessions[0].state), ("Bedroom TV", PlayState::Paused));

        h.play_pause_stop(&items[0]).await;

        // What Plex saw.
        let log = h.server_log();
        let server_calls: Vec<&Value> = log.iter().filter(|r| r["path"] != "/api/v2/pins/77").collect();
        assert!(
            server_calls.iter().all(|r| r["client"] == "plex-client-e2e" && r["product"] == "tvdash"),
            "identifying headers on every call"
        );
        let timeline: Vec<&str> = log
            .iter()
            .filter(|r| r["path"] == "/:/timeline")
            .map(|r| r["query"]["state"][0].as_str().unwrap())
            .collect();
        assert_eq!(timeline.first(), Some(&"playing"));
        assert!(timeline.contains(&"paused"));
        assert_eq!(timeline.last(), Some(&"stopped"));
        let stopped = log.iter().rfind(|r| r["path"] == "/:/timeline").unwrap();
        assert_eq!(stopped["query"]["ratingKey"][0], "101");
        assert_eq!(stopped["query"]["key"][0], "/library/metadata/101");
        assert_eq!(stopped["query"]["duration"][0], "3000000");
        assert!(stopped["query"]["time"][0].as_str().unwrap().parse::<u64>().unwrap() > 1_200_000);

        // Library: Recently Added and search (music hubs skipped).
        let recent = h
            .find_or_wait(|e| match e {
                AppEvent::RecentlyAdded(SourceKind::Plex, items) if !items.is_empty() => Some(items.clone()),
                _ => None,
            })
            .await;
        assert_eq!(recent[0].title, "Fresh Plex Film");
        h.send(Command::Search("film".into()));
        let found = h
            .wait_for(|e| match e {
                AppEvent::SearchResults { items, .. } => Some(items.clone()),
                _ => None,
            })
            .await;
        assert_eq!(found.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Plex Film"]);

        let (argv, cmds) = h.mpv_log();
        assert!(argv.contains(&"--start=1200.000".into()));
        assert_eq!(argv.last().unwrap(), &format!("{server_url}/library/parts/9101/file.mkv?X-Plex-Token=ptok"));
        assert_mpv_transport(&cmds);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn streaming_sites_open_switch_stop_and_handoff() {
        let _serial = SERIAL.lock().await;
        // Any fake works as the "server"; this test only uses the browser.
        let mut h = Harness::start("fake_jellyfin.py", |c, _| {
            c.jellyfin.enabled = false;
            c.plex.enabled = false;
        })
        .await;
        let browser_log = h.dir.join("browser.log");
        let profile = h.dir.join("profile");

        // Nothing connected and setup never finished → first-run setup.
        h.find_or_wait(|e| matches!(e, AppEvent::ShowSetup).then_some(())).await;

        h.send(Command::TvGeometry(Geometry { x: 1920, y: 0, width: 3840, height: 2160 }));

        // First open: right site, profile, placement; first-visit hint.
        h.send(Command::OpenSite(0));
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(s)) if s == "Netflix").then_some(())).await;
        let toast = h
            .wait_for(|e| match e {
                AppEvent::LongToast(t) => Some(t.clone()),
                _ => None,
            })
            .await;
        assert!(toast.starts_with("First time on Netflix"), "{toast}");
        assert!(profile.is_dir(), "profile dir created");
        sleep(Duration::from_millis(300)).await;

        // Switching sites closes the old browser (main + helper) first.
        h.send(Command::OpenSite(2));
        h.wait_for(|e| matches!(e, AppEvent::Browser(None)).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(s)) if s == "Hulu").then_some(())).await;
        sleep(Duration::from_millis(300)).await;

        // Stop closes it and the TV goes idle.
        h.send(Command::Stop);
        h.wait_for(|e| matches!(e, AppEvent::Browser(None)).then_some(())).await;

        let log = read_log(&browser_log);
        let starts: Vec<&Value> = log.iter().filter(|l| l["event"] == "start").collect();
        assert_eq!(starts.len(), 2);
        let argv: Vec<String> = serde_json::from_value(starts[0]["argv"].clone()).unwrap();
        for want in [
            "--app=https://www.netflix.com".to_string(),
            format!("--user-data-dir={}", profile.display()),
            "--start-fullscreen".into(),
            "--ozone-platform=x11".into(),
            "--window-position=1920,0".into(),
            "--window-size=3840,2160".into(),
        ] {
            assert!(argv.contains(&want), "missing {want}: {argv:?}");
        }
        assert!(serde_json::to_string(&starts[1]["argv"]).unwrap().contains("--app=https://www.hulu.com"));
        let terms = |role: &str| log.iter().filter(|l| l["event"] == "term" && l["role"] == role).count();
        assert_eq!(terms("main"), 2, "both main processes got SIGTERM: {log:?}");
        assert_eq!(terms("helper"), 2, "both helpers got SIGTERM: {log:?}");
        for s in &starts {
            assert_eq!(s["pid"], s["pgid"], "browser leads its own process group");
        }

        // Re-opening a known site: no first-visit hint; state remembers both.
        h.send(Command::OpenSite(0));
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(_))).then_some(())).await;
        h.send(Command::CloseBrowser);
        let next = h
            .wait_for(|e| match e {
                AppEvent::Toast(t) | AppEvent::LongToast(t) => Some(Some(t.clone())),
                AppEvent::Browser(None) => Some(None),
                _ => None,
            })
            .await;
        assert_eq!(next, None, "no first-visit toast for a known site");
        let state = AppState::load(&h.dir.join("state.toml"));
        assert!(state.streaming_opened.contains_key("Netflix") && state.streaming_opened.contains_key("Hulu"));

        // Hand-off: the browser exits at once → TV back to idle with an explanation.
        unsafe {
            std::env::set_var("FAKE_BROWSER_MODE", "exit-now");
        }
        h.send(Command::OpenSite(0));
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(_))).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::Browser(None)).then_some(())).await;
        let toast = h
            .wait_for(|e| match e {
                AppEvent::Toast(t) => Some(t.clone()),
                _ => None,
            })
            .await;
        assert!(toast.contains("handed off"), "{toast}");
        unsafe {
            std::env::remove_var("FAKE_BROWSER_MODE");
        }

        // Clear browser profile: directory gone, every site back to "not signed in".
        std::fs::write(profile.join("Cookies"), b"x").unwrap();
        h.send(Command::ClearBrowserProfile);
        h.wait_for(|e| matches!(e, AppEvent::Streaming(m) if m.is_empty()).then_some(())).await;
        assert!(!profile.exists(), "profile deleted");
        assert!(AppState::load(&h.dir.join("state.toml")).streaming_opened.is_empty());

        // Finishing setup is remembered.
        h.send(Command::SetupFinished);
        sleep(Duration::from_millis(200)).await;
        assert!(AppState::load(&h.dir.join("state.toml")).setup_complete);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn streaming_site_management() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_jellyfin.py", |c, _| {
            c.jellyfin.enabled = false;
            c.plex.enabled = false;
        })
        .await;
        let browser_log = h.dir.join("browser.log");
        let sites = |h: &Harness| Config::load_or_create_at(&h.config_path).unwrap().config.streaming;
        let names = |v: &[StreamingSite]| v.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        let initial = h
            .find_or_wait(|e| match e {
                AppEvent::Sites(v) => Some(v.clone()),
                _ => None,
            })
            .await;
        assert_eq!(initial.len(), 6);

        // Add: appended, saved to the config, the UI gets the new list.
        let peacock = StreamingSite {
            name: "Peacock".into(),
            url: "https://www.peacocktv.com".into(),
            search_url: Some("https://www.peacocktv.com/search?q={title}".into()),
            tmdb_providers: vec![386, 387],
            hidden: false,
        };
        h.send(Command::SaveSite { index: None, site: peacock.clone() });
        let v = h
            .wait_for(|e| match e {
                AppEvent::Sites(v) => Some(v.clone()),
                _ => None,
            })
            .await;
        assert_eq!(v.last(), Some(&peacock));
        assert_eq!(sites(&h).last(), Some(&peacock));

        // Open it, then rename it: "opened" history follows the new name.
        h.send(Command::OpenSite(6));
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(s)) if s == "Peacock").then_some(())).await;
        h.send(Command::CloseBrowser);
        h.wait_for(|e| matches!(e, AppEvent::Browser(None)).then_some(())).await;
        h.send(Command::SaveSite {
            index: Some(6),
            site: StreamingSite { name: "Peacock TV".into(), ..peacock.clone() },
        });
        h.wait_for(|e| matches!(e, AppEvent::Streaming(m) if m.contains_key("Peacock TV")).then_some(())).await;
        assert!(AppState::load(&h.dir.join("state.toml")).streaming_opened.contains_key("Peacock TV"));

        // Move to the front; hide it.
        h.send(Command::MoveSite { from: 6, to: 0 });
        h.wait_for(|e| matches!(e, AppEvent::Sites(v) if v[0].name == "Peacock TV").then_some(())).await;
        assert_eq!(names(&sites(&h))[..2], ["Peacock TV".to_string(), "Netflix".into()]);
        h.send(Command::SetSiteHidden(0, true));
        h.wait_for(|e| matches!(e, AppEvent::Sites(v) if v[0].hidden).then_some(())).await;
        assert!(sites(&h)[0].hidden);
        assert!(std::fs::read_to_string(&h.config_path).unwrap().contains("hidden = true"));

        // Test on TV opens any address.
        h.send(Command::OpenUrl { label: "Test".into(), url: "https://example.com/try".into() });
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(s)) if s == "Test").then_some(())).await;

        // Remove with "clear cookies": gone from config and state; the browser
        // opens its settings for that domain.
        h.send(Command::RemoveSite { index: 0, clear_cookies: true });
        h.wait_for(|e| matches!(e, AppEvent::Sites(v) if v.len() == 6).then_some(())).await;
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(s)) if s == "Peacock TV cookies").then_some(())).await;
        assert_eq!(names(&sites(&h)), names(&initial));
        assert!(!AppState::load(&h.dir.join("state.toml")).streaming_opened.contains_key("Peacock TV"));
        sleep(Duration::from_millis(300)).await;
        let log = read_log(&browser_log);
        let last = log.iter().rfind(|l| l["event"] == "start").unwrap();
        assert!(
            serde_json::to_string(&last["argv"])
                .unwrap()
                .contains("--app=chrome://settings/content/all?searchSubpage=www.peacocktv.com"),
            "{last:?}"
        );
        h.send(Command::CloseBrowser);
    }

    /// Start an extra fake server (besides the harness's own); returns (process, base URL, log path).
    async fn spawn_fake(script: &str, dir: &Path) -> (tokio::process::Child, String, PathBuf) {
        let stem = script.trim_end_matches(".py");
        let (port_file, log) = (dir.join(format!("{stem}.port")), dir.join(format!("{stem}.log")));
        let child = tokio::process::Command::new(format!("{FAKES}/{script}"))
            .arg(&port_file)
            .arg(&log)
            .stdout(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("start fake");
        let port = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(p) = std::fs::read_to_string(&port_file)
                    && !p.is_empty()
                {
                    return p;
                }
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("fake port");
        (child, format!("http://127.0.0.1:{port}"), log)
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn where_to_watch_ranks_by_library_and_services() {
        use tvdash_core::models::WatchTier;
        let _serial = SERIAL.lock().await;
        let tmp = std::env::temp_dir().join(format!("tvdash-tmdb-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (_tmdb, tmdb_url, tmdb_log) = spawn_fake("fake_tmdb.py", &tmp).await;
        let site = |name: &str, url: &str, ids: &[u32], hidden| StreamingSite {
            name: name.into(),
            url: url.into(),
            search_url: (name == "Netflix").then(|| "https://www.netflix.com/search?q={title}".into()),
            tmdb_providers: ids.to_vec(),
            hidden,
        };
        let endpoints = tvdash_discover::Endpoints { api: tmdb_url.clone(), images: format!("{tmdb_url}/t/p") };
        let mut h = Harness::start_with(
            "fake_jellyfin.py",
            |c, port| {
                c.jellyfin.server_url = format!("http://127.0.0.1:{port}");
                c.jellyfin.device_id = "dev-e2e".into();
                c.plex.enabled = false;
                c.streaming = vec![
                    site("Netflix", "https://www.netflix.com", &[8, 1796], false),
                    site("Hulu", "https://www.hulu.com", &[15], true),
                    // No IDs yet: matched by name once TMDB is connected.
                    site("Max", "https://www.max.com", &[], false),
                ];
            },
            move |c, _| {
                c.watch.endpoints = endpoints;
                c.watch.token_override = Some("good".into());
            },
        )
        .await;
        let tmdb_calls =
            |pat: &str| read_log(&tmdb_log).iter().filter(|r| r["path"].as_str().unwrap().contains(pat)).count();

        h.find_or_wait(|e| matches!(e, AppEvent::Tmdb(AccountStatus::Connected)).then_some(())).await;
        // Tile logos: Netflix has a TMDB logo; Hulu (hidden) too.
        let logos = h
            .find_or_wait(|e| match e {
                AppEvent::SiteLogos(l) if !l.is_empty() => Some(l.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .await;
        assert_eq!(logos, ["Netflix", "Hulu", "Max"]);
        let saved = Config::load_or_create_at(&h.config_path).unwrap().config.streaming;
        assert_eq!(saved[2].tmdb_providers, [1899], "Max matched by name and saved");

        // Sign in to Jellyfin so its library's TMDB IDs are known.
        h.send(Command::SignIn(SourceKind::Jellyfin));
        h.wait_for(|e| matches!(e, AppEvent::SignIn(_, SignInState::Done)).then_some(())).await;
        sleep(Duration::from_millis(500)).await;
        assert!(
            h.server_log().iter().any(|r| r["path"] == "/Items" && r["query"]["fields"][0] == "ProviderIds"),
            "library listed with provider IDs"
        );

        // Search: everything starts as "checking", then lands in its group.
        h.send(Command::WhereToWatch("matrix".into()));
        let first = h
            .wait_for(|e| match e {
                AppEvent::WatchResults { hits, .. } => Some(hits.clone()),
                _ => None,
            })
            .await;
        assert_eq!(first.len(), 5, "people are left out");
        assert!(first.iter().all(|h| h.tier == WatchTier::Checking || h.tier == WatchTier::Library));
        let done = h
            .wait_for(|e| match e {
                AppEvent::WatchResults { hits, .. } if hits.iter().all(|h| h.tier != WatchTier::Checking) => {
                    Some(hits.clone())
                }
                _ => None,
            })
            .await;
        let order: Vec<(u32, WatchTier)> = done.iter().map(|h| (h.tmdb_id, h.tier)).collect();
        assert_eq!(
            order,
            [
                (604, WatchTier::Library),
                (603, WatchTier::Included),
                (1399, WatchTier::Included),
                (606, WatchTier::CostsExtra),
                (605, WatchTier::NotStreaming),
            ]
        );
        assert_eq!(done[0].library.as_ref().map(|i| i.id.as_str()), Some("m604"));
        assert_eq!(done[1].services, [(0, "Netflix".to_string())]);
        assert_eq!(done[1].elsewhere, ["Rent · Apple TV"]);
        assert_eq!(done[1].link.as_deref(), Some("https://tmdb.example/603/watch"));
        // Max is one of yours now, so the show is included.
        assert_eq!(done[2].services, [(2, "Max".to_string())]);
        assert!(done[2].is_tv && done[2].year == Some(2011));
        h.find_or_wait(|e| matches!(e, AppEvent::WatchPoster { tmdb_id: 603, .. }).then_some(())).await;

        // Same search again within a day: answered from the cache.
        let before = tmdb_calls("/watch/providers");
        h.send(Command::WhereToWatch("matrix".into()));
        h.wait_for(|e| match e {
            AppEvent::WatchResults { hits, .. } if hits.iter().all(|h| h.tier != WatchTier::Checking) => Some(()),
            _ => None,
        })
        .await;
        assert_eq!(tmdb_calls("/watch/providers"), before, "no availability calls on a cache hit");

        // Open on a service: its search page with the title filled in.
        h.send(Command::OpenOnService { site: 0, title: "The Matrix".into() });
        h.wait_for(|e| matches!(e, AppEvent::Browser(Some(s)) if s == "Netflix").then_some(())).await;
        sleep(Duration::from_millis(300)).await;
        let log = read_log(&h.dir.join("browser.log"));
        let start = log.iter().rfind(|l| l["event"] == "start").unwrap();
        assert!(
            serde_json::to_string(&start["argv"])
                .unwrap()
                .contains("--app=https://www.netflix.com/search?q=The%20Matrix")
        );
        h.send(Command::CloseBrowser);

        // The editor's Find button.
        h.send(Command::FindProviders("Netflix".into()));
        let ids = h
            .wait_for(|e| match e {
                AppEvent::ProviderMatches { ids, .. } => Some(ids.clone()),
                _ => None,
            })
            .await;
        assert_eq!(ids, [8, 1796]);

        // A bad token is reported, not stored.
        h.send(Command::SetTmdbToken("bad".into()));
        let status = h
            .wait_for(|e| match e {
                AppEvent::Tmdb(s) => Some(s.clone()),
                _ => None,
            })
            .await;
        assert!(matches!(status, AccountStatus::Unreachable(ref e) if e.contains("didn't accept")), "{status:?}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn setting_the_location_saves_it() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_jellyfin.py", |c, _| {
            c.jellyfin.enabled = false;
            c.plex.enabled = false;
        })
        .await;
        let place = tvdash_core::models::Place {
            label: "Chicago, Illinois, United States".into(),
            latitude: 41.85,
            longitude: -87.65,
            timezone: "America/Chicago".into(),
        };
        h.send(Command::SetLocation(place));
        let (name, tz) = h
            .wait_for(|e| match e {
                AppEvent::Location { name, timezone } => Some((name.clone(), timezone.clone())),
                _ => None,
            })
            .await;
        assert_eq!((name.as_str(), tz.as_str()), ("Chicago, Illinois, United States", "America/Chicago"));
        let g = Config::load_or_create_at(&h.config_path).unwrap().config.general;
        assert_eq!((g.latitude, g.longitude, g.location.as_str()), (41.85, -87.65, "Chicago, Illinois, United States"));
        assert_eq!(g.timezone, "America/Chicago");
    }
}
