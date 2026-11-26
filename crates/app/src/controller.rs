//! Background controller: owns media sources, sign-in flows, the mpv session
//! and the streaming browser. Receives `Command`s from the UI and emits
//! `AppEvent`s back. Only one thing is on the TV at a time: starting mpv closes
//! the browser and vice versa.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Datelike;
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{debug, error, info, warn};
use tvdash_core::{
    AppEvent, Command, Config, SecretKey,
    config::{Layout, StreamingSite, encode_component, expand_home},
    events::{PlayerStatus, Service},
    models::{
        AccountStatus, Geometry, MediaItem, PlayState, RgbaImage, ServerActivity, ServerLibrary, ServerStream,
        ServerSummary, SignInState, SourceKind,
    },
    secrets,
    state::AppState,
};
use tvdash_media::{
    MediaError, MediaSource,
    jellyfin::{Auth, JellyfinClient},
    plex::{DiscoveredServer, PlexAccount, PlexAdmin, PlexClient},
};
use tvdash_player::{
    browser::{Browser, BrowserOptions, is_brave},
    libmpv::{Embedded, VideoSlot},
    mpv::{Mpv, MpvEvent, MpvOptions},
};

use crate::{format, watch::Watch};

const REFRESH_EVERY: Duration = Duration::from_secs(120);
const PROGRESS_EVERY: Duration = Duration::from_secs(10);
const POSTER_HEIGHT: u32 = 480;
const QUICK_CONNECT_POLL: Duration = Duration::from_secs(2);
const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SESSIONS_EVERY: Duration = Duration::from_secs(10);
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
    SaveBrowseRotation {
        key: String,
        rotation: tvdash_core::state::BrowseRotation,
    },
    SaveStartWatching(tvdash_core::state::StartWatchingCache),
    Markers {
        item_id: String,
        markers: Vec<tvdash_media::plex::Marker>,
    },
    NextAtCredits {
        item_id: String,
        next_ep: MediaItem,
    },
    ShowExtras {
        show_key: String,
        extras: Vec<tvdash_core::events::ExtraItem>,
    },
    MovieExtras {
        movie_key: String,
        extras: Vec<tvdash_core::events::ExtraItem>,
    },
    MovieCollection {
        movie_key: String,
        link: Option<tvdash_core::events::CollectionLink>,
    },
}

/// What plays a Plex item: mpv in its own window, or libmpv drawing
/// inside the tvdash window (TV-only layout).
enum Player {
    Window(Mpv),
    InWindow(Embedded),
}

impl Player {
    async fn next_event(&mut self) -> Option<MpvEvent> {
        match self {
            Self::Window(m) => m.next_event().await,
            Self::InWindow(e) => e.next_event().await,
        }
    }

    async fn set_pause(&mut self, paused: bool) -> Result<(), String> {
        match self {
            Self::Window(m) => m.set_pause(paused).await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.set_pause(paused).map_err(|e| e.to_string()),
        }
    }

    async fn seek(&mut self, secs: i64) -> Result<(), String> {
        match self {
            Self::Window(m) => m.seek(secs).await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.seek(secs).map_err(|e| e.to_string()),
        }
    }

    async fn seek_to(&mut self, secs: u64) -> Result<(), String> {
        match self {
            Self::Window(m) => m.seek_to(secs).await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.seek_to(secs).map_err(|e| e.to_string()),
        }
    }

    async fn cycle_audio(&mut self) -> Result<(), String> {
        match self {
            Self::Window(m) => m.cycle_audio().await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.cycle_audio().map_err(|e| e.to_string()),
        }
    }

    async fn cycle_sub(&mut self) -> Result<(), String> {
        match self {
            Self::Window(m) => m.cycle_sub().await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.cycle_sub().map_err(|e| e.to_string()),
        }
    }

    async fn add_volume(&mut self, delta: i64) -> Result<(), String> {
        match self {
            Self::Window(m) => m.add_volume(delta).await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.add_volume(delta).map_err(|e| e.to_string()),
        }
    }

    async fn set_volume(&mut self, volume: f64) -> Result<(), String> {
        match self {
            Self::Window(m) => m.set_volume(volume).await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.set_volume(volume).map_err(|e| e.to_string()),
        }
    }

    async fn show_text(&mut self, text: &str, duration_ms: Option<u32>) -> Result<(), String> {
        match self {
            Self::Window(m) => m.show_text(text, duration_ms).await.map_err(|e| e.to_string()),
            Self::InWindow(e) => e.show_text(text, duration_ms).map_err(|e| e.to_string()),
        }
    }

    async fn quit(self) {
        match self {
            Self::Window(m) => m.quit().await,
            Self::InWindow(e) => e.quit().await,
        }
    }

    fn in_window(&self) -> bool {
        matches!(self, Self::InWindow(_))
    }
}

struct Playback {
    status: PlayerStatus,
    player: Player,
    /// Sequential progress reports for this item (keeps start → progress → stop ordered).
    reporter: UnboundedSender<(u64, PlayState)>,
    last_emitted_sec: Option<u64>,
    /// The "can't decode" hint was already shown for this item (frame errors repeat).
    decode_hint_shown: bool,
    /// True when playback reached natural end-of-file (rather than user stop).
    ended_eof: bool,
    /// Intro/credits markers for this item (Plex), loaded after playback starts.
    markers: Vec<tvdash_media::plex::Marker>,
    credits_prompted: bool,
    credits_dismissed: bool,
    playing_extra: bool,
}

pub struct Controller {
    config: Config,
    config_path: PathBuf,
    windowed: bool,
    events: UnboundedSender<AppEvent>,
    internal: UnboundedSender<Internal>,
    jellyfin: Option<Arc<JellyfinClient>>,
    plex: Option<Arc<PlexClient>>,
    tautulli: Option<tvdash_media::tautulli::TautulliClient>,
    tautulli_cache: Option<Arc<tvdash_discover::cache::Cache>>,
    sign_in: HashMap<SourceKind, JoinHandle<()>>,
    playback: Option<Playback>,
    pending_next_ep: Option<MediaItem>,
    show_extras: Vec<tvdash_core::events::ExtraItem>,
    movie_extras: Vec<tvdash_core::events::ExtraItem>,
    open_movie: Option<String>,
    browser: Option<Browser>,
    inhibit: Option<crate::portal::IdleInhibit>,
    tv_geometry: Option<Geometry>,
    tv_output: Option<String>,
    state: AppState,
    state_path: Option<PathBuf>,
    posters_requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    backdrops_requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    sports_logos_requested: Arc<Mutex<HashSet<String>>>,
    server_sections: Arc<Mutex<Vec<ServerLibrary>>>,
    /// Where to watch (TMDB) and service logos.
    pub(crate) watch: Watch,
    /// Tells the widget pollers about display setting changes (units).
    general_tx: Option<tokio::sync::watch::Sender<tvdash_core::config::GeneralConfig>>,
    /// Where in-window video goes (None: libmpv isn't available).
    video: Option<Arc<VideoSlot>>,
    /// The layout on screen now (in-window video is for TV only).
    layout: Layout,
    /// In-window drawing failed this run: use mpv's window from now on.
    video_failed: bool,
    paused_for_weather: bool,
    sleep: crate::sleep::Sleep,
    sleep_tx: Option<tokio::sync::watch::Sender<crate::home_cmd::SleepHttpState>>,
    quick_tx: Option<tokio::sync::watch::Sender<crate::home_cmd::QuickHttpState>>,
    sleep_episode_finished: bool,
    sleep_finished_key: Option<String>,
    sleep_held: bool,
    sleep_last_key: String,
    sports_tx: Option<UnboundedSender<crate::sports_watch::WatcherMsg>>,
}

enum Step {
    Command(Command),
    Mpv(Option<MpvEvent>),
    BrowserExited,
    Internal(Internal),
    Refresh,
    Sessions,
    Progress,
    SleepTick,
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
        let state_path = tvdash_core::state::default_path();
        let cache_path = state_path.as_ref().map(|p| p.with_file_name("cache.db"));
        let tautulli_cache = cache_path
            .as_deref()
            .and_then(|p| tvdash_discover::cache::Cache::open(p).ok())
            .map(Arc::new);
        let mut c = Self {
            config,
            config_path,
            windowed,
            events: events.clone(),
            internal,
            jellyfin: None,
            plex: None,
            tautulli: None,
            tautulli_cache,
            sign_in: HashMap::new(),
            playback: None,
            pending_next_ep: None,
            show_extras: Vec::new(),
            movie_extras: Vec::new(),
            open_movie: None,
            browser: None,
            inhibit: None,
            tv_geometry: None,
            tv_output: None,
            state: AppState::default(),
            state_path,
            posters_requested: Arc::default(),
            backdrops_requested: Arc::default(),
            sports_logos_requested: Arc::default(),
            server_sections: Arc::default(),
            general_tx: None,
            video: None,
            layout: Layout::Dual,
            video_failed: false,
            paused_for_weather: false,
            watch: Watch::new(events.clone(), cache_path),
            sleep: crate::sleep::Sleep::default(),
            sleep_tx: None,
            quick_tx: None,
            sleep_episode_finished: false,
            sleep_finished_key: None,
            sleep_held: false,
            sleep_last_key: String::new(),
            sports_tx: None,
        };
        c.watch.set_internal(c.internal.clone());
        (c, internal_rx)
    }

    pub fn set_sleep_tx(&mut self, tx: tokio::sync::watch::Sender<crate::home_cmd::SleepHttpState>) {
        self.sleep_tx = Some(tx);
        self.emit_sleep();
    }

    pub fn set_quick_tx(&mut self, tx: tokio::sync::watch::Sender<crate::home_cmd::QuickHttpState>) {
        self.quick_tx = Some(tx);
        self.publish_quick();
        self.emit(AppEvent::QuickSwitch(self.config.tv.quick_switch.clone()));
    }

    fn publish_quick(&self) {
        let Some(tx) = &self.quick_tx else { return };
        let all: Vec<String> = self
            .config
            .streaming
            .iter()
            .map(|s| crate::brand::service_id(&s.name))
            .collect();
        let recent: Vec<String> = self.config.tv.recent.iter().map(|r| r.id.clone()).collect();
        let set = crate::apps::quick_switch_set(&self.config.tv.quick_switch, &recent, &all);
        let tiles = set
            .iter()
            .filter_map(|id| {
                let site = self
                    .config
                    .streaming
                    .iter()
                    .find(|s| crate::brand::service_id(&s.name) == *id)?;
                let (fill, ink) = crate::brand::colors(id);
                Some(crate::home_cmd::QuickHttpTile {
                    id: id.clone(),
                    name: site.name.clone(),
                    fill: crate::brand::hex(fill),
                    ink: crate::brand::hex(ink),
                    logo_svg: crate::brand::logo_svg(id).map(str::to_owned),
                })
            })
            .collect();
        let current = self.browser.as_ref().map(|b| crate::brand::service_id(&b.site));
        tx.send_replace(crate::home_cmd::QuickHttpState {
            key: self.config.tv.quick_switch_key.clone(),
            current,
            tiles,
        });
    }

    /// Play inside the tvdash window (through `slot`) whenever the layout is TV only.
    pub fn set_video(&mut self, slot: Arc<VideoSlot>) {
        self.video = Some(slot);
    }

    /// The layout the window starts in (`--layout` can differ from the config).
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
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

        let mut sleep_tick = interval(Duration::from_secs(1));
        sleep_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        sleep_tick.tick().await;

        loop {
            let step = {
                let (playback, browser) = (self.playback.as_mut(), self.browser.as_mut());
                let mpv_next = async move {
                    match playback {
                        Some(p) => p.player.next_event().await,
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
                    _ = sleep_tick.tick() => Step::SleepTick,
                }
            };
            match step {
                Step::Command(c) => self.on_command(c).await,
                Step::Mpv(e) => self.on_mpv(e).await,
                Step::BrowserExited => self.on_browser_exited().await,
                Step::Internal(Internal::JellyfinSignedIn(auth)) => {
                    self.on_jellyfin_signed_in(auth).await;
                    self.watch.refresh_library(self.sources(), true);
                }
                Step::Internal(Internal::PlexSignedIn { account_token, server }) => {
                    self.on_plex_signed_in(account_token, server).await;
                    self.watch.refresh_library(self.sources(), true);
                }
                Step::Internal(Internal::FillProviders(fills)) => self.fill_providers(fills),
                Step::Internal(Internal::SaveBrowseRotation { key, rotation }) => {
                    self.state.browse_rotations.insert(key, rotation);
                    self.save_state();
                }
                Step::Internal(Internal::SaveStartWatching(cache)) => {
                    self.state.start_watching = Some(cache);
                    self.save_state();
                }
                Step::Internal(Internal::Markers { item_id, markers }) => {
                    if let Some(p) = self.playback.as_mut()
                        && p.status.item.id == item_id
                    {
                        p.markers = markers;
                    }
                }
                Step::Internal(Internal::NextAtCredits { item_id, next_ep }) => {
                    if let Some(p) = self.playback.as_ref()
                        && p.status.item.id == item_id
                        && !p.credits_dismissed
                    {
                        self.pending_next_ep = Some(next_ep.clone());
                        self.emit(AppEvent::NextEpisodeAtCredits(next_ep));
                    }
                }
                Step::Internal(Internal::ShowExtras { show_key, extras }) => {
                    self.show_extras = extras.clone();
                    self.emit(AppEvent::ShowExtras { show_key, extras });
                }
                Step::Internal(Internal::MovieExtras { movie_key, extras }) => {
                    if self.open_movie.as_deref() == Some(movie_key.as_str()) {
                        self.movie_extras = extras.clone();
                        self.emit(AppEvent::MovieExtras { movie_key, extras });
                    }
                }
                Step::Internal(Internal::MovieCollection { movie_key, link }) => {
                    if self.open_movie.as_deref() == Some(movie_key.as_str()) {
                        self.emit(AppEvent::MovieCollection { movie_key, link });
                    }
                }
                Step::Internal(Internal::Refresh) | Step::Refresh => {
                    self.refresh();
                    self.watch.refresh_library(self.sources(), false);
                }
                Step::Sessions => self.poll_sessions(),
                Step::Progress => self.report_tick(),
                Step::SleepTick => self.on_sleep_tick().await,
                Step::Shutdown => break,
            }
        }
        self.clear_tv().await;
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
        self.init_tautulli().await;
        self.init_events().await;
        self.sports_tx = Some(crate::sports_watch::spawn(
            self.config.events.clone(),
            self.config.sports.alerts.clone(),
            self.events.clone(),
        ));
        self.watch.refresh_library(self.sources(), true);
        if !self.config.jellyfin.server_url.is_empty() {
            self.emit(AppEvent::Server(SourceKind::Jellyfin, Some(self.config.jellyfin.server_url.clone())));
        }
        if self.plex.is_some() {
            self.emit(AppEvent::Server(SourceKind::Plex, Some(self.config.plex.server_url.clone())));
        }
        // First run: nothing connected and setup never finished.
        if !self.state.setup_complete && self.jellyfin.is_none() && self.plex.is_none() {
            self.emit(AppEvent::ShowSetup);
        }
        self.reconnect_runtime_state().await;
        self.refresh();
    }

    fn save_runtime_state(&self) {
        let path = crate::instance::runtime_state_path();
        let mut rt = crate::instance::RuntimeState::default();
        if let Some(p) = &self.playback
            && !p.player.in_window()
        {
            rt.mpv_socket = Some(tvdash_player::mpv::socket_path().to_string_lossy().into_owned());
            if p.status.item.source == SourceKind::Plex {
                rt.plex_rating_key = Some(p.status.item.id.clone());
            }
            rt.source = Some(p.status.item.source);
            rt.item_id = Some(p.status.item.id.clone());
            rt.item_title = Some(p.status.item.title.clone());
        }
        if let Some(b) = &self.browser {
            rt.browser_pid = b.pid();
            rt.browser_site = Some(b.site.clone());
        }
        rt.sleep = self.sleep.clone();
        if rt.mpv_socket.is_none() && rt.browser_pid.is_none() && !rt.sleep.is_active() {
            crate::instance::RuntimeState::clear(&path);
        } else {
            rt.save(&path);
        }
    }

    async fn reconnect_runtime_state(&mut self) {
        let path = crate::instance::runtime_state_path();
        let Some(rt) = crate::instance::RuntimeState::load(&path) else {
            return;
        };

        if let Some(sock) = &rt.mpv_socket {
            match Mpv::reconnect(std::path::Path::new(sock)).await {
                Ok(mpv) => {
                    let source_kind = rt.source.unwrap_or(SourceKind::Plex);
                    let item_id = rt.item_id.clone().or_else(|| rt.plex_rating_key.clone()).unwrap_or_default();
                    let fetched = if source_kind == SourceKind::Plex
                        && let Some(plex) = &self.plex
                        && !item_id.is_empty()
                    {
                        plex.item(&item_id).await.ok().flatten()
                    } else {
                        None
                    };
                    let item = fetched.unwrap_or_else(|| MediaItem {
                        source: source_kind,
                        id: item_id,
                        kind: tvdash_core::models::MediaKind::Movie,
                        title: rt.item_title.clone().unwrap_or_else(|| "Playing on TV".into()),
                        subtitle: None,
                        episode_label: None,
                        duration_ms: None,
                        resume_ms: None,
                        poster: None,
                        summary: None,
                        extra: Default::default(),
                    });
                    let (reporter, _) = match self.source_for(item.source) {
                        Some(src) => (spawn_reporter(src, item.clone()), true),
                        None => (unbounded_channel().0, false),
                    };
                    let status = PlayerStatus {
                        duration_ms: item.duration_ms,
                        item,
                        state: PlayState::Playing,
                        position_ms: 0,
                    };
                    self.emit(AppEvent::Player(Some(status.clone())));
                    self.playback = Some(Playback {
                        status,
                        player: Player::Window(mpv),
                        reporter,
                        last_emitted_sec: None,
                        decode_hint_shown: false,
                        ended_eof: false,
                        markers: Vec::new(),
                        credits_prompted: false,
                        credits_dismissed: false,
                        playing_extra: false,
                    });
                    info!(socket = %sock, "reconnected to running mpv");
                }
                Err(e) => {
                    warn!(socket = %sock, error = %e, "couldn't reconnect to mpv socket");
                }
            }
        }

        if let Some(pid) = rt.browser_pid {
            let site = rt.browser_site.clone().unwrap_or_else(|| "Streaming".into());
            if let Some(b) = Browser::reconnect(pid, site.clone()) {
                self.browser = Some(b);
                self.emit(AppEvent::Browser(Some(site)));
                info!(pid, "reattached to running browser");
            }
        }

        if rt.sleep.is_active() {
            self.sleep = rt.sleep;
            self.emit_sleep();
        }

        self.save_runtime_state();
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
                    && let Err(e) = p.player.set_pause(pause).await
                {
                    warn!(error = %e, "mpv pause failed");
                }
            }
            Command::Seek(secs) => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.player.seek(secs).await
                {
                    warn!(error = %e, "mpv seek failed");
                }
            }
            Command::SeekTo(secs) => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.player.seek_to(secs).await
                {
                    warn!(error = %e, "mpv seek_to failed");
                }
            }
            Command::CycleAudio => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.player.cycle_audio().await
                {
                    warn!(error = %e, "mpv cycle audio failed");
                }
            }
            Command::CycleSub => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.player.cycle_sub().await
                {
                    warn!(error = %e, "mpv cycle sub failed");
                }
            }
            Command::VolumeDelta(delta) => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.player.add_volume(delta).await
                {
                    warn!(error = %e, "mpv volume failed");
                }
            }
            Command::PreviousEpisode | Command::NextEpisode => {
                let next = matches!(cmd, Command::NextEpisode);
                if let Some(item) = self.playback.as_ref().map(|p| p.status.item.clone())
                    && item.kind == tvdash_core::models::MediaKind::Episode
                    && item.source == SourceKind::Plex
                    && let Some(plex) = &self.plex
                {
                    match plex.adjacent_episode(&item.id, next).await {
                        Ok(Some(mut ep)) => {
                            ep.resume_ms = None;
                            self.play(ep).await;
                        }
                        Ok(None) => self.emit(AppEvent::Toast(if next { "No next episode." } else { "No previous episode." }.into())),
                        Err(e) => warn!(error = %e, "adjacent episode lookup failed"),
                    }
                }
            }
            Command::Stop => self.clear_tv().await,
            Command::Home => {
                // Video inside the window keeps playing behind Home; mpv's own window would cover it.
                if !self.playback.as_ref().is_some_and(|p| p.player.in_window()) {
                    self.stop_playback().await;
                }
                if let Some(site) = self.browser.as_ref().map(|b| b.site.clone()) {
                    self.close_browser().await;
                    self.toast(format!("Closed {site} · back on Home"));
                }
            }
            Command::Overlay => self.show_overlay(Some(5000)).await,
            Command::VideoFailed(reason) => self.video_failed(reason).await,
            Command::SignIn(SourceKind::Jellyfin) => {
                let cfg = self.config.jellyfin.clone();
                self.spawn_sign_in(
                    SourceKind::Jellyfin,
                    |show_code| async move {
                        jellyfin_quick_connect(&cfg.server_url, &cfg.device_id, show_code)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Internal::JellyfinSignedIn,
                );
            }
            Command::SignIn(SourceKind::Plex) => {
                let cfg = self.config.plex.clone();
                self.spawn_sign_in(
                    SourceKind::Plex,
                    |show_code| async move {
                        plex_pin_sign_in(&cfg.account_url, &cfg.client_id, show_code).await.map_err(|e| e.to_string())
                    },
                    |(account_token, server)| Internal::PlexSignedIn { account_token, server },
                );
            }
            Command::CancelSignIn(kind) => {
                if let Some(task) = self.sign_in.remove(&kind) {
                    task.abort();
                }
                self.emit(AppEvent::SignIn(kind, SignInState::Cancelled));
            }
            Command::Refresh => self.refresh(),
            Command::OpenSite(i) => self.open_site(i).await,
            Command::ClearRecentServices => {
                self.update_config(|c| c.tv.recent.clear());
                self.emit(AppEvent::RecentServices(Vec::new()));
                self.publish_quick();
                self.toast("Recent services cleared");
            }
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
                self.clear_tv().await;
                self.launch_browser(&label, &url).await;
            }
            Command::WhereToWatch(query) => self.watch.search(query, self.config.streaming.clone()),
            Command::OpenOnService { site, title } => {
                if let Some(s) = self.config.streaming.get(site).cloned() {
                    self.clear_tv().await;
                    if self.launch_browser(&s.name, &s.open_url(&title)).await
                        && self.state.mark_opened(&s.name, chrono::Utc::now())
                    {
                        self.streaming_changed();
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
            Command::TvOutput(name) => {
                debug!(%name, "TV output");
                self.tv_output = (!name.is_empty()).then_some(name);
            }
            Command::Search(query) => self.search(query),
            Command::ServerScan { index } => self.server_scan(index).await,
            Command::ServerRefresh { refresh_libraries } => self.poll_server_summary(refresh_libraries),
            Command::ServerRefreshActivities => self.poll_server_activities(),
            Command::RefreshSessions => self.poll_sessions(),
            Command::PrefetchBackdrop(item) => {
                if let Some(source) = self.source_for(item.source) {
                    fetch_backdrops(source, vec![item], self.backdrops_requested.clone(), self.events.clone());
                }
            }
            Command::ServerCancelActivity(uuid) => self.server_cancel_activity(&uuid).await,
            Command::ServerTerminate { session_id, reason } => self.server_terminate(&session_id, &reason).await,
            Command::RefreshMetadata(item) => self.refresh_metadata(item).await,
            Command::FetchArtwork { item, backdrop } => self.fetch_artwork(item, backdrop),
            Command::SetArtwork { item, key, backdrop } => self.set_artwork(item, &key, backdrop).await,
            Command::SignOut(kind) => self.sign_out(kind).await,
            Command::JellyfinServer(url) => {
                let url = url.trim().trim_end_matches('/').to_owned();
                if !url.is_empty() && url != self.config.jellyfin.server_url {
                    info!(%url, "Jellyfin server address changed");
                    self.update_config(|c| c.jellyfin.server_url = url.clone());
                    self.emit(AppEvent::Server(SourceKind::Jellyfin, Some(url)));
                    if self.jellyfin.is_some() {
                        self.init_jellyfin().await;
                        self.refresh();
                    }
                }
            }
            Command::JellyfinPassword { username, password } => self.jellyfin_password(username, password),
            Command::ClearBrowserProfile => self.clear_browser_profile().await,
            Command::SetTheme(mode) => self.update_config(|c| c.general.theme = mode),
            Command::SetDashboardUi(ui) => self.update_config(|c| c.display.dashboard_ui = ui),
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
            Command::SetLayout(layout) => {
                self.layout = layout;
                // The window in-window video was drawing into is changing over.
                if self.playback.as_ref().is_some_and(|p| p.player.in_window()) {
                    self.stop_playback().await;
                }
            }
            Command::SetLayoutSetting(setting) => {
                self.update_config(|c| c.display.layout = setting);
                let name = match setting {
                    tvdash_core::LayoutSetting::Auto => "Automatic",
                    tvdash_core::LayoutSetting::Dual => "two screens",
                    tvdash_core::LayoutSetting::Tv => "TV only",
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
            Command::SetTautulli { url, api_key } => {
                self.set_tautulli(url, api_key).await;
            }
            Command::ClearTautulli => {
                self.clear_tautulli().await;
            }
            Command::LoadSportsEvents { schedule, page } => {
                self.load_sports_events(schedule, page);
            }
            Command::SetEventsPassword(pw) => {
                self.set_events_password(pw).await;
            }
            Command::LoadScoreboard { day, show, final_only } => {
                self.load_scoreboard(day, show, final_only);
            }
            Command::LoadLibraryShelves { is_tv, sort, popular_days } => {
                self.refresh();
                self.load_library_shelves(is_tv, sort, popular_days);
            }
            Command::BrowseLetter { request_id, is_tv, letter, sort, genre } => {
                self.browse_letter(request_id, is_tv, letter, sort, genre);
            }
            Command::SetBrowseSort { is_tv, sort } => {
                self.update_config(|c| {
                    if is_tv {
                        c.general.browse_sort_tv = sort.clone();
                    } else {
                        c.general.browse_sort_movies = sort.clone();
                    }
                });
            }
            Command::OpenShow(rating_key) => {
                self.open_show(rating_key);
            }
            Command::LoadSeasonEpisodes(season_key) => {
                self.load_season_episodes(season_key);
            }
            Command::OpenCollection(rating_key) => {
                self.open_collection(rating_key);
            }
            Command::PlayRatingKey { rating_key, resume } => {
                if let Some(plex) = &self.plex
                    && let Ok(Some(mut item)) = plex.item(&rating_key).await
                {
                    if !resume {
                        item.resume_ms = None;
                    }
                    self.play(item).await;
                }
            }
            Command::SleepSetMinutes(mins) => {
                let mins = mins.clamp(1, 480);
                let now = crate::sleep::now_unix();
                self.sleep.set_minutes(mins, now);
                if !self.config.sleep.remember_last {
                    self.sleep.last_minutes = None;
                }
                self.sleep_episode_finished = false;
                self.sleep_finished_key = None;
                self.save_runtime_state();
                self.emit_sleep();
                let use_24h = self.config.general.clock == tvdash_core::config::ClockFormat::H24;
                let ends = crate::sleep::format_ends_at(now + mins * 60, use_24h);
                let dur = crate::sleep::label(mins * 60);
                self.toast(format!("Sleep in {dur} · ends {ends}"));
            }
            Command::SleepSetEndOfItem => {
                if let Some(p) = &self.playback
                    && p.status.item.source == SourceKind::Plex
                {
                    let key = p.status.item.id.clone();
                    let is_movie = p.status.item.kind == tvdash_core::models::MediaKind::Movie;
                    self.sleep.set_end_of_item(&key);
                    self.sleep_episode_finished = false;
                    self.sleep_finished_key = None;
                    self.save_runtime_state();
                    self.emit_sleep();
                    self.toast(if is_movie { "Sleeping after this movie" } else { "Sleeping after this episode" });
                } else {
                    self.toast("End of episode is only available during Plex playback");
                }
            }
            Command::SleepOff => {
                self.sleep.off();
                self.sleep_episode_finished = false;
                self.sleep_finished_key = None;
                self.save_runtime_state();
                self.emit_sleep();
            }
            Command::SleepExtend => {
                let ext = self.config.sleep.extend_minutes();
                let now = crate::sleep::now_unix();
                self.sleep.set_minutes(ext, now);
                self.sleep_episode_finished = false;
                self.sleep_finished_key = None;
                self.save_runtime_state();
                self.emit_sleep();
                let use_24h = self.config.general.clock == tvdash_core::config::ClockFormat::H24;
                let ends = crate::sleep::format_ends_at(now + ext * 60, use_24h);
                self.toast(format!("Sleep extended · ends {ends}"));
            }
            Command::SleepNow => {
                self.sleep_now().await;
            }
            Command::Quit => {
                info!("quit requested: closing everything on the TV");
                self.release_tv().await;
                quit_ui();
            }
            Command::SleepPlayNext => {
                let key = self.sleep_finished_key.take();
                self.sleep.off();
                self.sleep_episode_finished = false;
                self.save_runtime_state();
                self.emit_sleep();
                if let Some(id) = key
                    && let Some(plex) = &self.plex
                    && let Ok(Some(mut ep)) = plex.adjacent_episode(&id, true).await
                {
                    ep.resume_ms = None;
                    self.play(ep).await;
                }
            }
            Command::SetSleepWarnBefore(val) => {
                self.update_config(|c| c.sleep.warn_before = val);
                self.emit_sleep();
            }
            Command::FetchPosters(items) => {
                let (plex_items, jf_items): (Vec<_>, Vec<_>) =
                    items.into_iter().partition(|i| i.source == SourceKind::Plex);
                if !plex_items.is_empty()
                    && let Some(plex) = &self.plex
                {
                    fetch_posters(plex.clone(), plex_items, self.posters_requested.clone(), self.events.clone());
                }
                if !jf_items.is_empty()
                    && let Some(jf) = &self.jellyfin
                {
                    fetch_posters(jf.clone(), jf_items, self.posters_requested.clone(), self.events.clone());
                }
            }
            Command::ForgetImages { posters, backdrops, logos } => {
                if !posters.is_empty() {
                    let mut guard = self.posters_requested.lock().unwrap();
                    for k in posters {
                        guard.remove(&k);
                    }
                }
                if !backdrops.is_empty() {
                    let mut guard = self.backdrops_requested.lock().unwrap();
                    for k in backdrops {
                        guard.remove(&k);
                    }
                }
                if !logos.is_empty() {
                    let mut guard = self.sports_logos_requested.lock().unwrap();
                    for k in logos {
                        guard.remove(&k);
                    }
                }
            }
            Command::SportsAlertTest(kind) => {
                if let Some(tx) = &self.sports_tx {
                    let _ = tx.send(crate::sports_watch::WatcherMsg::EnqueueTest(kind));
                }
            }
            Command::SportsAlertShown(key) => {
                if let Some(tx) = &self.sports_tx {
                    let _ = tx.send(crate::sports_watch::WatcherMsg::AlertShown(key));
                }
            }
            Command::SportsAlertPrimary { kind, source_url } => {
                if kind == "soon" {
                    if let Some(idx) = self
                        .config
                        .streaming
                        .iter()
                        .position(|s| s.name.to_ascii_lowercase().contains("youtube tv"))
                    {
                        self.open_site(idx).await;
                    } else {
                        self.toast("YouTube TV isn't configured in Sites");
                    }
                } else if kind == "final"
                    && let Some(url) = source_url
                {
                    self.clear_tv().await;
                    self.launch_browser("ESPN Recap", &url).await;
                }
            }
            Command::SetSportsAlertsConfig(cfg) => {
                self.update_config(|c| c.sports.alerts = cfg.clone());
                if let Some(tx) = &self.sports_tx {
                    let _ = tx.send(crate::sports_watch::WatcherMsg::UpdateAlertsConfig(cfg));
                }
            }
            Command::ToggleStarGame(event_id) => {
                if let Some(tx) = &self.sports_tx {
                    let _ = tx.send(crate::sports_watch::WatcherMsg::ToggleStar(event_id));
                }
            }
            Command::UnstarGame(event_id) => {
                if let Some(tx) = &self.sports_tx {
                    let _ = tx.send(crate::sports_watch::WatcherMsg::Unstar(event_id));
                }
            }
            Command::RefreshSportsFollows => {
                if let Some(tx) = &self.sports_tx {
                    let _ = tx.send(crate::sports_watch::WatcherMsg::RefreshFollows);
                }
            }
            Command::SetStartupIntro(on) => {
                self.update_config(|c| c.startup.intro = if on { "on".into() } else { "off".into() });
            }
            Command::OpenService(id) => {
                match self
                    .config
                    .streaming
                    .iter()
                    .position(|s| crate::brand::service_id(&s.name) == id)
                {
                    Some(i) => {
                        let name = self.config.streaming[i].name.clone();
                        self.emit(AppEvent::Toast(format!("Opening {name}…")));
                        self.open_site(i).await;
                    }
                    None => warn!(%id, "quick switch: no such app"),
                }
            }
            Command::SetQuickSwitch(ids) => {
                self.update_config(|c| c.tv.quick_switch = ids);
                self.publish_quick();
                self.emit(AppEvent::QuickSwitch(self.config.tv.quick_switch.clone()));
            }
            Command::PlayNextNow => {
                if let Some(next_ep) = self.pending_next_ep.take() {
                    self.stop_playback().await;
                    self.play(next_ep).await;
                }
            }
            Command::WatchCredits => {
                if let Some(p) = self.playback.as_mut() {
                    p.credits_dismissed = true;
                }
                self.pending_next_ep = None;
            }
            Command::WeatherTakeover(open) => {
                if open {
                    if let Some(p) = self.playback.as_mut()
                        && p.status.state == PlayState::Playing
                        && p.player.set_pause(true).await.is_ok()
                    {
                        self.paused_for_weather = true;
                    }
                } else if self.paused_for_weather {
                    self.paused_for_weather = false;
                    if let Some(p) = self.playback.as_mut()
                        && p.status.state == PlayState::Paused
                    {
                        let _ = p.player.set_pause(false).await;
                    }
                }
            }
            Command::PlayExtra(key) => {
                if let Some(ex) =
                    self.show_extras.iter().chain(self.movie_extras.iter()).find(|e| e.item.id == key).cloned()
                {
                    let mut item = ex.item;
                    item.resume_ms = None;
                    self.play_item(item, true).await;
                }
            }
            Command::OpenMovie(key) => {
                self.open_movie = Some(key.clone());
                self.movie_extras.clear();
                self.open_movie(key);
            }
            Command::SetMovieWatched { rating_key, watched } => {
                if let Some(plex) = self.plex.clone() {
                    let internal = self.internal.clone();
                    let events = self.events.clone();
                    let requested = self.posters_requested.clone();
                    let backdrops_req = self.backdrops_requested.clone();
                    tokio::spawn(async move {
                        if let Err(e) = plex.scrobble(&rating_key, watched).await {
                            warn!(key = %rating_key, error = %e, "scrobble failed");
                            return;
                        }
                        let _ = internal.send(Internal::Refresh);
                        if let Ok(detail) = plex.movie_detail(&rating_key).await {
                            let source: Arc<dyn MediaSource> = plex.clone();
                            fetch_backdrops(
                                source.clone(),
                                vec![detail.item.clone()],
                                backdrops_req,
                                events.clone(),
                            );
                            fetch_posters(source, vec![detail.item.clone()], requested, events.clone());
                            let _ = events.send(AppEvent::MovieDetail(Box::new(detail)));
                        }
                    });
                }
            }
        }
    }

    async fn init_tautulli(&mut self) {
        let url = self.config.tautulli.url.trim().to_owned();
        if url.is_empty() {
            self.tautulli = None;
            self.emit(AppEvent::Tautulli { configured: false, url: String::new() });
            return;
        }
        let key = tokio::task::spawn_blocking(|| secrets::get(SecretKey::TautulliApiKey))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .unwrap_or_default();
        self.tautulli = tvdash_media::tautulli::TautulliClient::new(&url, &key).ok();
        self.emit(AppEvent::Tautulli { configured: self.tautulli.is_some(), url });
    }

    async fn set_tautulli(&mut self, url: String, api_key: String) {
        let url = url.trim().trim_end_matches('/').to_owned();
        let api_key = api_key.trim().to_owned();
        self.update_config(|c| c.tautulli.url = url.clone());
        if !api_key.is_empty() {
            let k = api_key.clone();
            let _ = tokio::task::spawn_blocking(move || secrets::set(SecretKey::TautulliApiKey, &k)).await;
        }
        self.init_tautulli().await;
        self.init_events().await;
        self.sports_tx = Some(crate::sports_watch::spawn(
            self.config.events.clone(),
            self.config.sports.alerts.clone(),
            self.events.clone(),
        ));
        self.toast(if self.tautulli.is_some() { "Tautulli saved" } else { "Tautulli URL saved (API key needed)" });
    }

    async fn clear_tautulli(&mut self) {
        self.tautulli = None;
        self.update_config(|c| c.tautulli.url.clear());
        let _ = tokio::task::spawn_blocking(|| secrets::delete(SecretKey::TautulliApiKey)).await;
        self.emit(AppEvent::Tautulli { configured: false, url: String::new() });
        self.toast("Tautulli disconnected");
    }

    async fn init_events(&self) {
        let configured = tokio::task::spawn_blocking(|| secrets::get(SecretKey::EventsSqlPassword))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .is_some_and(|s| !s.trim().is_empty());
        self.emit(AppEvent::EventsPasswordConfigured(configured));
    }

    async fn set_events_password(&self, pw: Option<String>) {
        match pw {
            Some(password) => {
                let res = tokio::task::spawn_blocking(move || {
                    secrets::set(SecretKey::EventsSqlPassword, &password)
                })
                .await;
                match res {
                    Ok(Ok(())) => {
                        self.emit(AppEvent::EventsPasswordConfigured(true));
                        self.toast("Sports SQL password saved");
                    }
                    Ok(Err(e)) => self.toast(format!("Couldn't save password: {e}")),
                    Err(e) => self.toast(format!("Keyring task failed: {e}")),
                }
            }
            None => {
                let _ = tokio::task::spawn_blocking(|| secrets::delete(SecretKey::EventsSqlPassword)).await;
                self.emit(AppEvent::EventsPasswordConfigured(false));
                self.toast("Sports SQL password removed");
            }
        }
    }

    fn load_sports_events(&self, schedule: String, page: u32) {
        let cfg = self.config.events.clone();
        let events = self.events.clone();
        let logos_req = self.sports_logos_requested.clone();
        tokio::spawn(async move {
            let pw = tokio::task::spawn_blocking(|| secrets::get(SecretKey::EventsSqlPassword))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                .unwrap_or_default();
            let res = tvdash_events::fetch_page(&cfg, &pw, &schedule, page).await;
            match res {
                Ok(data) => {
                    let count = data.total_count;
                    let _ = events.send(AppEvent::Health {
                        service: Service::Events,
                        result: Ok(format!("{count} upcoming")),
                    });
                    let mut logo_urls = Vec::new();
                    for c in &data.choices {
                        if let Some(u) = crate::bridge::league_logo_url(&c.value) {
                            logo_urls.push(u.to_string());
                        }
                    }
                    for day in &data.days {
                        for lg in &day.leagues {
                            for g in &lg.games {
                                if let Some(u) = &g.away_logo_url {
                                    logo_urls.push(u.clone());
                                }
                                if let Some(u) = &g.home_logo_url {
                                    logo_urls.push(u.clone());
                                }
                            }
                        }
                    }
                    let _ = events.send(AppEvent::SportsEvents(Ok(data)));
                    Self::spawn_sports_logos(logo_urls, logos_req, events);
                }
                Err(e) => {
                    let msg = e.to_string();
                    let _ = events.send(AppEvent::Health {
                        service: Service::Events,
                        result: Err(msg.clone()),
                    });
                    let _ = events.send(AppEvent::SportsEvents(Err(msg)));
                }
            }
        });
    }

    fn load_scoreboard(&self, day: String, show: String, final_only: bool) {
        let cfg = self.config.events.clone();
        let events = self.events.clone();
        let logos_req = self.sports_logos_requested.clone();
        tokio::spawn(async move {
            let pw = tokio::task::spawn_blocking(|| secrets::get(SecretKey::EventsSqlPassword))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                .unwrap_or_default();
            let res = tvdash_events::scoreboard::fetch_scoreboard(&cfg, &pw, &day, &show, final_only).await;
            match res {
                Ok(data) => {
                    let count: usize = data.groups.iter().map(|g| g.games.len()).sum();
                    let _ = events.send(AppEvent::Health {
                        service: Service::Events,
                        result: Ok(format!("{count} scoreboard games")),
                    });
                    let mut logo_urls = Vec::new();
                    for c in &data.choices {
                        if let Some(u) = crate::bridge::league_logo_url(&c.value) {
                            logo_urls.push(u.to_string());
                        }
                    }
                    for grp in &data.groups {
                        for g in &grp.games {
                            if let Some(u) = &g.away_logo_url {
                                logo_urls.push(u.clone());
                            }
                            if let Some(u) = &g.home_logo_url {
                                logo_urls.push(u.clone());
                            }
                        }
                    }
                    let _ = events.send(AppEvent::Scoreboard(Ok(data)));
                    Self::spawn_sports_logos(logo_urls, logos_req, events);
                }
                Err(e) => {
                    let msg = e.to_string();
                    let _ = events.send(AppEvent::Health {
                        service: Service::Events,
                        result: Err(msg.clone()),
                    });
                    let _ = events.send(AppEvent::Scoreboard(Err(msg)));
                }
            }
        });
    }

    fn spawn_sports_logos(
        urls: Vec<String>,
        requested: Arc<Mutex<HashSet<String>>>,
        events: UnboundedSender<AppEvent>,
    ) {
        let todo: Vec<String> = {
            let mut guard = requested.lock().unwrap();
            urls.into_iter().filter(|u| guard.insert(u.clone())).collect()
        };
        if todo.is_empty() {
            return;
        }
        tokio::spawn(async move {
            let client = match reqwest::Client::builder().timeout(Duration::from_secs(8)).build() {
                Ok(c) => c,
                Err(_) => return,
            };
            for url in todo {
                let Ok(resp) = client.get(&url).send().await else {
                    requested.lock().unwrap().remove(&url);
                    continue;
                };
                if !resp.status().is_success() {
                    requested.lock().unwrap().remove(&url);
                    continue;
                }
                let Ok(bytes) = resp.bytes().await else {
                    requested.lock().unwrap().remove(&url);
                    continue;
                };
                if let Some(image) = tokio::task::spawn_blocking(move || decode_sports_logo(&bytes))
                    .await
                    .ok()
                    .flatten()
                {
                    let _ = events.send(AppEvent::SportsLogo { url, image });
                } else {
                    requested.lock().unwrap().remove(&url);
                }
            }
        });
    }

    fn load_library_shelves(&self, is_tv: bool, sort: tvdash_core::models::LibrarySort, popular_days: u32) {
        let Some(plex) = self.plex.clone() else { return };
        let tautulli = self.tautulli.clone();
        let cache = self.tautulli_cache.clone();
        let events = self.events.clone();
        let internal = self.internal.clone();
        let requested = self.posters_requested.clone();
        let backdrops_req = self.backdrops_requested.clone();
        let saved_sw = self.state.start_watching.clone();
        let days = if popular_days == 7 { 7 } else { 30 };

        tokio::spawn(async move {
            let shelves_key = format!("plex:shelves:v6:{is_tv}:{}:{days}", sort.plex_sort());
            if let Some(cached_shelves) =
                cache.as_ref().and_then(|c| c.get::<tvdash_core::models::LibraryShelves>(&shelves_key, Duration::from_secs(300)))
            {
                let source: Arc<dyn MediaSource> = plex;
                fetch_posters(source.clone(), cached_shelves.added_this_week.clone(), requested.clone(), events.clone());
                fetch_posters(source.clone(), cached_shelves.recently_released.clone(), requested.clone(), events.clone());
                fetch_posters(source.clone(), cached_shelves.watch_again.clone(), requested.clone(), events.clone());
                fetch_posters(source.clone(), collection_poster_items(&cached_shelves.collections), requested.clone(), events.clone());
                fetch_posters(source.clone(), cached_shelves.start_watching.clone(), requested, events.clone());
                let bd_items: Vec<_> = cached_shelves
                    .finish_season
                    .iter()
                    .map(|f| f.item.clone())
                    .chain(cached_shelves.continue_watching.iter().cloned())
                    .chain(cached_shelves.added_this_week.iter().cloned())
                    .chain(cached_shelves.recently_released.iter().cloned())
                    .take(16)
                    .collect();
                fetch_backdrops(source, bd_items, backdrops_req, events.clone());
                let _ = events.send(AppEvent::LibraryShelves(cached_shelves));
                return;
            }

            let mut shelves = tvdash_core::models::LibraryShelves { is_tv, ..Default::default() };
            let target_type = if is_tv { "show" } else { "movie" };
            let sections = plex.sections().await.unwrap_or_default();
            let movie_sections: Vec<_> = sections.iter().filter(|s| s.kind == "movie").cloned().collect();
            let section = sections.into_iter().find(|s| s.kind == target_type);

            if let Some(sec) = section {
                shelves.total_count = sec.item_count;
                // Start of current week (Monday 00:00 local).
                let today = chrono::Local::now().date_naive();
                let days_from_mon = today.weekday().num_days_from_monday() as i64;
                let monday = today - chrono::TimeDelta::days(days_from_mon);
                let since_epoch = monday
                    .and_hms_opt(0, 0, 0)
                    .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
                    .map(|dt| dt.timestamp())
                    .unwrap_or(0);

                if is_tv {
                    shelves.genres = plex.section_genres(&sec.key).await.unwrap_or_default();
                    shelves.added_this_week = plex.added_since(&sec.key, since_epoch, sort, 6).await.unwrap_or_default();

                    let iso = chrono::Local::now().iso_week();
                    let (cur_year, cur_week) = (iso.year(), iso.week());
                    let cached_keys = saved_sw
                        .as_ref()
                        .filter(|sw| sw.year == cur_year && sw.week == cur_week && !sw.keys.is_empty())
                        .map(|sw| sw.keys.clone());
                    let mut sw_items = if let Some(keys) = cached_keys {
                        plex.items_by_keys(&keys).await.unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    if sw_items.is_empty() {
                        sw_items = plex.unstarted_random_shows(&sec.key, 40).await.unwrap_or_default();
                        if !sw_items.is_empty() {
                            let keys = sw_items.iter().map(|i| i.id.clone()).collect();
                            let _ = internal.send(Internal::SaveStartWatching(
                                tvdash_core::state::StartWatchingCache {
                                    year: cur_year,
                                    week: cur_week,
                                    keys,
                                },
                            ));
                        }
                    }
                    shelves.start_watching = sw_items;
                } else {
                    for m_sec in &movie_sections {
                        match plex.collections(&m_sec.key).await {
                            Ok(cols) => shelves.collections.extend(cols),
                            Err(e) => warn!(section = %m_sec.key, error = %e, "collections fetch failed"),
                        }
                    }
                    shelves.added_this_week = plex
                        .section_items(&sec.key, tvdash_core::models::LibrarySort::RecentlyAdded, None, 6)
                        .await
                        .unwrap_or_default();
                }
                shelves.recently_released = plex.recently_released(&sec.key, is_tv, 6).await.unwrap_or_default();
                shelves.watch_again = plex.watch_again(&sec.key, is_tv, sort, 6).await.unwrap_or_default();
                let source: Arc<dyn MediaSource> = plex;
                fetch_posters(source.clone(), shelves.added_this_week.clone(), requested.clone(), events.clone());
                fetch_posters(source.clone(), shelves.recently_released.clone(), requested.clone(), events.clone());
                fetch_posters(source.clone(), shelves.watch_again.clone(), requested.clone(), events.clone());
                fetch_posters(source.clone(), collection_poster_items(&shelves.collections), requested.clone(), events.clone());
                fetch_posters(source.clone(), shelves.start_watching.clone(), requested, events.clone());
                let bd_items: Vec<_> = shelves
                    .finish_season
                    .iter()
                    .map(|f| f.item.clone())
                    .chain(shelves.continue_watching.iter().cloned())
                    .chain(shelves.added_this_week.iter().cloned())
                    .chain(shelves.recently_released.iter().cloned())
                    .take(16)
                    .collect();
                fetch_backdrops(source, bd_items, backdrops_req, events.clone());
            }

            let stat_id = if is_tv { "top_tv" } else { "top_movies" };
            let cache_key = format!("tautulli:{stat_id}:{days}");
            let cached = cache
                .as_ref()
                .and_then(|c| c.get::<Vec<tvdash_core::models::PopularItem>>(&cache_key, Duration::from_secs(3600)));
            if let Some(items) = cached {
                shelves.popular = items;
            } else if let Some(t) = tautulli
                && let Ok(items) = t.home_stats(stat_id, days, 12).await
            {
                if let Some(c) = &cache {
                    c.put(&cache_key, &items);
                }
                shelves.popular = items;
            }

            if let Some(c) = &cache {
                c.put(&shelves_key, &shelves);
            }
            let _ = events.send(AppEvent::LibraryShelves(shelves));
        });
    }

    fn browse_letter(
        &self,
        request_id: u64,
        is_tv: bool,
        letter: String,
        sort: tvdash_core::models::LibrarySort,
        genre: Option<String>,
    ) {
        let Some(plex) = self.plex.clone() else { return };
        let events = self.events.clone();
        let internal = self.internal.clone();
        let requested = self.posters_requested.clone();
        let rotations = self.state.browse_rotations.clone();

        tokio::spawn(async move {
            let target_type = if is_tv { "show" } else { "movie" };
            let sections = plex.sections().await.unwrap_or_default();
            let Some(sec) = sections.into_iter().find(|s| s.kind == target_type) else {
                let _ = events.send(AppEvent::BrowseItems {
                    request_id,
                    is_tv,
                    letter,
                    sort,
                    genre,
                    items: Vec::new(),
                    total: 0,
                    letter_counts: Vec::new(),
                });
                return;
            };

            let genre_ref = genre.as_deref().filter(|g| !g.is_empty());
            let (items, total, letter_counts) = if letter.is_empty() {
                let total = plex
                    .section_count_filtered(&sec.key, genre_ref)
                    .await
                    .unwrap_or(0);
                let rot_key = format!("{}:{}", sec.key, genre_ref.unwrap_or(""));
                let mut rot = rotations
                    .get(&rot_key)
                    .cloned()
                    .map(crate::browse_rotation::Rotation::from);
                let positions = {
                    let mut rng = rand::thread_rng();
                    crate::browse_rotation::take_visit_positions(&mut rot, total, 10, &mut rng)
                };
                if let Some(updated) = rot.as_ref() {
                    let _ = internal.send(Internal::SaveBrowseRotation {
                        key: rot_key,
                        rotation: updated.into(),
                    });
                }

                use futures_util::stream::{self, StreamExt};
                let fetched: Vec<tvdash_core::models::MediaItem> = stream::iter(positions)
                    .map(|pos| {
                        let plex = plex.clone();
                        let sec_key = sec.key.clone();
                        let g = genre_ref.map(str::to_owned);
                        async move {
                            plex.section_item_at(&sec_key, pos, g.as_deref())
                                .await
                                .ok()
                                .flatten()
                        }
                    })
                    .buffer_unordered(4)
                    .filter_map(|x| async move { x })
                    .collect()
                    .await;

                let counts = plex
                    .section_letter_counts(&sec.key, genre_ref)
                    .await
                    .unwrap_or_default();
                (fetched, total, counts)
            } else {
                let items = plex
                    .section_letter(&sec.key, &letter, sort, genre_ref)
                    .await
                    .unwrap_or_default();
                let total = items.len() as u32;
                (items, total, Vec::new())
            };

            let prefetch: Vec<_> = items.iter().take(48).cloned().collect();
            let source: Arc<dyn MediaSource> = plex;
            fetch_posters(source, prefetch, requested, events.clone());

            let _ = events.send(AppEvent::BrowseItems {
                request_id,
                is_tv,
                letter,
                sort,
                genre,
                items,
                total,
                letter_counts,
            });
        });
    }

    fn open_show(&self, rating_key: String) {
        let Some(plex) = self.plex.clone() else { return };
        let events = self.events.clone();
        let requested = self.posters_requested.clone();
        let backdrops_req = self.backdrops_requested.clone();
        let internal = self.internal.clone();
        tokio::spawn(async move {
            let res = async {
                let show_key = plex.show_key_for(&rating_key).await.unwrap_or(rating_key);
                let item = plex
                    .item(&show_key)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "Show not found".to_string())?;
                let seasons = plex.show_seasons(&show_key).await.map_err(|e| e.to_string())?;
                let on_deck = plex.show_on_deck(&show_key).await.ok().flatten();
                let up_next_season_key = on_deck
                    .as_ref()
                    .and_then(|ep| seasons.iter().find(|s| s.index == ep.season_index))
                    .or_else(|| seasons.first())
                    .map(|s| s.rating_key.clone());
                Ok(tvdash_core::models::ShowDetail {
                    item,
                    seasons,
                    up_next_season_key,
                    up_next_episode: on_deck,
                })
            }
            .await;
            let initial_season = res.as_ref().ok().and_then(|d| d.up_next_season_key.clone());
            let show_id_for_extras = res.as_ref().ok().map(|d| d.item.id.clone());
            if let Ok(detail) = &res {
                let source: Arc<dyn MediaSource> = plex.clone();
                fetch_backdrops(source.clone(), vec![detail.item.clone()], backdrops_req, events.clone());
                let season_items: Vec<MediaItem> = detail
                    .seasons
                    .iter()
                    .filter(|s| s.thumb.is_some())
                    .map(|s| MediaItem {
                        source: SourceKind::Plex,
                        id: s.rating_key.clone(),
                        kind: tvdash_core::models::MediaKind::Other,
                        title: s.title.clone(),
                        subtitle: None,
                        episode_label: None,
                        duration_ms: None,
                        resume_ms: None,
                        poster: s.thumb.clone(),
                        summary: None,
                        extra: Default::default(),
                    })
                    .collect();
                fetch_posters(source, season_items, requested.clone(), events.clone());
            }
            let _ = events.send(AppEvent::ShowDetail(res));
            if let Some(show_key) = show_id_for_extras {
                let plex_ex = plex.clone();
                let req_ex = requested.clone();
                let ev_ex = events.clone();
                let int_ex = internal.clone();
                tokio::spawn(async move {
                    if let Ok(extras) = plex_ex.extras(&show_key).await {
                        let poster_items: Vec<MediaItem> = extras.iter().map(|e| e.item.clone()).collect();
                        if !poster_items.is_empty() {
                            fetch_posters(plex_ex, poster_items, req_ex, ev_ex);
                        }
                        let _ = int_ex.send(Internal::ShowExtras { show_key, extras });
                    }
                });
            }
            if let Some(season_key) = initial_season {
                fetch_season_episodes(plex, season_key, requested, events).await;
            }
        });
    }

    fn load_season_episodes(&self, season_key: String) {
        let Some(plex) = self.plex.clone() else { return };
        let events = self.events.clone();
        let requested = self.posters_requested.clone();
        tokio::spawn(fetch_season_episodes(plex, season_key, requested, events));
    }

    fn open_collection(&self, rating_key: String) {
        let Some(plex) = self.plex.clone() else { return };
        let events = self.events.clone();
        let requested = self.posters_requested.clone();
        tokio::spawn(async move {
            let res = async {
                let sections = plex.sections().await.unwrap_or_default();
                let mut info = None;
                for sec in sections.into_iter().filter(|s| s.kind == "movie") {
                    if let Ok(cols) = plex.collections(&sec.key).await
                        && let Some(found) = cols.into_iter().find(|c| c.rating_key == rating_key)
                    {
                        info = Some(found);
                        break;
                    }
                }
                let collection = info.unwrap_or(tvdash_core::models::CollectionInfo {
                    rating_key: rating_key.clone(),
                    title: "Collection".into(),
                    child_count: 0,
                    watched_count: 0,
                    smart: false,
                    thumb: None,
                    summary: None,
                    min_year: None,
                    max_year: None,
                    total_duration_ms: 0,
                });
                let (items, watched_keys) =
                    plex.collection_entries(&rating_key).await.map_err(|e| e.to_string())?;
                fetch_posters(plex.clone(), items.clone(), requested, events.clone());
                let up_next_key = items
                    .iter()
                    .find(|i| i.resume_ms.is_some())
                    .or_else(|| items.iter().find(|i| !watched_keys.contains(&i.id)))
                    .or_else(|| items.first())
                    .map(|i| i.id.clone());
                Ok(tvdash_core::models::CollectionDetail { collection, items, watched_keys, up_next_key })
            }
            .await;
            let _ = events.send(AppEvent::CollectionDetail(res));
        });
    }

    fn open_movie(&self, rating_key: String) {
        let Some(plex) = self.plex.clone() else { return };
        let events = self.events.clone();
        let requested = self.posters_requested.clone();
        let backdrops_req = self.backdrops_requested.clone();
        let internal = self.internal.clone();
        tokio::spawn(async move {
            let detail = match plex.movie_detail(&rating_key).await {
                Ok(d) => d,
                Err(e) => {
                    warn!(key = %rating_key, error = %e, "movie_detail failed");
                    return;
                }
            };
            let source: Arc<dyn MediaSource> = plex.clone();
            fetch_backdrops(source.clone(), vec![detail.item.clone()], backdrops_req, events.clone());
            let mut poster_items = vec![detail.item.clone()];
            for p in detail.crew.iter().chain(detail.cast.iter()) {
                if let Some(url) = &p.thumb {
                    poster_items.push(MediaItem {
                        source: SourceKind::Plex,
                        id: format!("person:{url}"),
                        kind: tvdash_core::models::MediaKind::Other,
                        title: p.name.clone(),
                        subtitle: None,
                        episode_label: None,
                        duration_ms: None,
                        resume_ms: None,
                        poster: Some(url.clone()),
                        summary: None,
                        extra: Default::default(),
                    });
                }
            }
            fetch_posters(source, poster_items, requested.clone(), events.clone());
            let _ = events.send(AppEvent::MovieDetail(Box::new(detail.clone())));

            // Extras (async, page doesn't wait).
            {
                let plex_ex = plex.clone();
                let req_ex = requested.clone();
                let ev_ex = events.clone();
                let int_ex = internal.clone();
                let key_ex = rating_key.clone();
                tokio::spawn(async move {
                    let extras = plex_ex.extras(&key_ex).await.unwrap_or_default();
                    let items: Vec<MediaItem> = extras.iter().map(|e| e.item.clone()).collect();
                    if !items.is_empty() {
                        fetch_posters(plex_ex, items, req_ex, ev_ex);
                    }
                    let _ = int_ex.send(Internal::MovieExtras { movie_key: key_ex, extras });
                });
            }

            // Collection link (async, page doesn't wait).
            {
                let plex_col = plex.clone();
                let req_col = requested.clone();
                let ev_col = events.clone();
                let int_col = internal.clone();
                let key_col = rating_key.clone();
                tokio::spawn(async move {
                    if detail.collections.is_empty() {
                        let _ = int_col.send(Internal::MovieCollection { movie_key: key_col, link: None });
                        return;
                    }
                    let sections = plex_col.sections().await.unwrap_or_default();
                    let mut all_cols = Vec::new();
                    for sec in sections.into_iter().filter(|s| s.kind == "movie") {
                        if let Ok(cols) = plex_col.collections(&sec.key).await {
                            all_cols.extend(cols);
                        }
                    }
                    let Some(col) = crate::movie::pick_collection(&detail, &all_cols).cloned() else {
                        let _ = int_col.send(Internal::MovieCollection { movie_key: key_col, link: None });
                        return;
                    };
                    if let Some(thumb) = &col.thumb {
                        fetch_posters(
                            plex_col.clone(),
                            vec![MediaItem {
                                source: SourceKind::Plex,
                                id: col.rating_key.clone(),
                                kind: tvdash_core::models::MediaKind::Other,
                                title: col.title.clone(),
                                subtitle: None,
                                episode_label: None,
                                duration_ms: None,
                                resume_ms: None,
                                poster: Some(thumb.clone()),
                                summary: None,
                                extra: Default::default(),
                            }],
                            req_col,
                            ev_col,
                        );
                    }
                    let link = match plex_col.collection_entries(&col.rating_key).await {
                        Ok((items, watched_keys)) => {
                            crate::movie::build_collection_link(&key_col, &col, &items, &watched_keys.into_iter().collect::<HashSet<_>>())
                        }
                        Err(_) => None,
                    };
                    let _ = int_col.send(Internal::MovieCollection { movie_key: key_col, link });
                });
            }
        });
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
        let cfg = self.config.jellyfin.clone();
        self.spawn_sign_in(
            SourceKind::Jellyfin,
            |_| async move {
                let client = JellyfinClient::new(&cfg.server_url, &cfg.device_id, None).map_err(|e| e.to_string())?;
                client.authenticate_by_name(&username, &password).await.map_err(|e| match e {
                    MediaError::Unauthorized => "wrong username or password".into(),
                    e => e.to_string(),
                })
            },
            Internal::JellyfinSignedIn,
        );
    }

    /// Run a sign-in flow for `kind`, replacing one already running. Reports
    /// Starting, passes `flow` a callback that shows a code, then hands success
    /// back to the controller as `done(..)` or reports the failure.
    fn spawn_sign_in<T, Fut>(
        &mut self,
        kind: SourceKind,
        flow: impl FnOnce(Box<dyn Fn(String) + Send + Sync>) -> Fut,
        done: fn(T) -> Internal,
    ) where
        T: Send + 'static,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
    {
        if let Some(old) = self.sign_in.remove(&kind) {
            old.abort();
        }
        let (events, internal) = (self.events.clone(), self.internal.clone());
        let send = move |s| {
            let _ = events.send(AppEvent::SignIn(kind, s));
        };
        send(SignInState::Starting);
        let show = send.clone();
        let flow = flow(Box::new(move |code| show(SignInState::Code(code))));
        let task = tokio::spawn(async move {
            match flow.await {
                Ok(v) => {
                    let _ = internal.send(done(v));
                }
                Err(e) => {
                    warn!(source = kind.label(), error = %e, "sign-in failed");
                    send(SignInState::Failed(e));
                }
            }
        });
        self.sign_in.insert(kind, task);
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
        self.streaming_changed();
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

    fn poll_server_summary(&self, refresh_libraries: bool) {
        let Some(plex) = self.plex.as_ref() else {
            let _ = self.events.send(AppEvent::ServerError("Plex isn't connected".into()));
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                warn!(error = %error, "couldn't create Plex admin client");
                let _ = self.events.send(AppEvent::ServerError(error.to_string()));
                return;
            }
        };
        let events = self.events.clone();
        let plex = plex.clone();
        let cached_sections = self.server_sections.clone();
        let refresh_libraries = match cached_sections.lock() {
            Ok(sections) => refresh_libraries || sections.iter().any(|section| section.refreshing),
            Err(error) => {
                warn!(error = %error, "Plex library section cache was poisoned");
                true
            }
        };
        let local_playback = self
            .playback
            .as_ref()
            .map(|playback| playback.status.clone())
            .filter(|status| status.item.source == SourceKind::Plex);
        tokio::spawn(async move {
            match fetch_server_summary(&admin, &plex, local_playback, cached_sections, refresh_libraries).await {
                Ok(summary) => {
                    let _ = events.send(AppEvent::ServerSummary(summary));
                }
                Err(error) => {
                    warn!(error = %error, "Plex server dashboard refresh failed");
                    let _ = events.send(AppEvent::ServerError(error.to_string()));
                }
            }
        });
    }

    fn poll_server_activities(&self) {
        let Some(plex) = self.plex.as_ref() else {
            let _ = self.events.send(AppEvent::ServerError("Plex isn't connected".into()));
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                warn!(error = %error, "couldn't create Plex admin client for activities");
                return;
            }
        };
        let events = self.events.clone();
        tokio::spawn(async move {
            match admin.activities().await {
                Ok(activities) => {
                    let activities = activities
                        .into_iter()
                        .map(|activity| ServerActivity {
                            uuid: activity.uuid().to_owned(),
                            title: activity.title().to_owned(),
                            subtitle: activity.subtitle().to_owned(),
                            progress: activity.progress().unwrap_or_default().clamp(0.0, 100.0),
                            cancellable: activity.cancellable(),
                        })
                        .collect();
                    let _ = events.send(AppEvent::ServerActivities(activities));
                }
                Err(error) => warn!(error = %error, "Plex activity polling failed"),
            }
        });
    }

    fn refresh(&self) {
        self.poll_sessions();
        for source in self.sources() {
            let events = self.events.clone();
            let requested = self.posters_requested.clone();
            let backdrops_req = self.backdrops_requested.clone();
            tokio::spawn(async move {
                let kind = source.kind();
                match source.continue_watching().await {
                    Ok(items) => {
                        let note = format!("{} in Continue Watching", items.len());
                        let _ = events.send(AppEvent::Health { service: kind.into(), result: Ok(note) });
                        let _ = events.send(AppEvent::Account(kind, AccountStatus::Connected));
                        let _ = events.send(AppEvent::ContinueWatching(kind, items.clone()));
                        if let Some(hero) = items.first().cloned() {
                            fetch_backdrops(source.clone(), vec![hero], backdrops_req.clone(), events.clone());
                        }
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

    async fn on_jellyfin_signed_in(&mut self, auth: Auth) {
        self.sign_in.remove(&SourceKind::Jellyfin);
        let token = auth.token.clone();
        let stored = tokio::task::spawn_blocking(move || secrets::set(SecretKey::JellyfinToken, &token)).await;
        if !matches!(stored, Ok(Ok(()))) {
            error!(?stored, "couldn't store the Jellyfin token in the keyring");
            self.toast("Signed in, but the token couldn't be saved to the keyring");
        }
        let user_id = auth.user_id.clone();
        self.update_config(|c| c.jellyfin.user_id = user_id);
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
        self.update_config(|c| c.plex.server_url = url);
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

    async fn server_scan(&self, index: Option<usize>) {
        let Some(plex) = self.plex.as_ref() else {
            self.toast("Plex isn't connected");
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                self.toast(format!("Plex admin API isn't available: {error}"));
                return;
            }
        };
        match index {
            Some(idx) => match admin.sections().await {
                Ok(sections) => {
                    let Some(section) = sections.get(idx) else {
                        self.toast("That library isn't available right now");
                        return;
                    };
                    if let Err(e) = admin.scan(Some(&section.key)).await {
                        self.toast(format!("Couldn't scan {}: {e}", section.title));
                    } else {
                        self.toast(format!("Scanning {}…", section.title));
                        self.poll_server_summary(true);
                        self.poll_server_activities();
                    }
                }

                Err(e) => self.toast(format!("Couldn't list libraries: {e}")),
            },
            None => {
                if let Err(e) = admin.scan(None).await {
                    self.toast(format!("Couldn't scan all libraries: {e}"));
                } else {
                    self.toast("Scanning libraries…");
                    self.poll_server_summary(true);
                    self.poll_server_activities();
                }
            }
        }
    }

    async fn server_cancel_activity(&self, uuid: &str) {
        let Some(plex) = self.plex.as_ref() else {
            self.toast("Plex isn't connected");
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                self.toast(format!("Plex admin API isn't available: {error}"));
                return;
            }
        };
        match admin.cancel_activity(uuid).await {
            Ok(()) => {
                self.toast("Cancelled server activity");
                self.poll_server_summary(true);
                self.poll_server_activities();
            }
            Err(error) => self.toast(format!("Couldn't cancel activity: {error}")),
        }
    }

    async fn server_terminate(&mut self, session_id: &str, reason: &str) {
        if session_id == "this-tv" {
            self.clear_tv().await;
            return;
        }
        let Some(plex) = self.plex.as_ref() else {
            self.toast("Plex isn't connected");
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                self.toast(format!("Plex admin API isn't available: {error}"));
                return;
            }
        };
        match admin.terminate(session_id, reason).await {
            Ok(()) => {
                self.toast("Stopped Plex stream");
                self.poll_server_summary(false);
            }
            Err(error) => self.toast(format!("Plex didn't stop the stream: {error}")),
        }
    }

    async fn refresh_metadata(&self, item: MediaItem) {
        if item.source != SourceKind::Plex {
            self.toast("Metadata refresh is only available for Plex items");
            self.metadata_refresh_finished(item.id);
            return;
        }
        let Some(plex) = self.plex.as_ref() else {
            self.toast("Plex isn't connected");
            self.metadata_refresh_finished(item.id);
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                self.toast(format!("Plex admin API isn't available: {error}"));
                self.metadata_refresh_finished(item.id);
                return;
            }
        };
        match admin.refresh_item(&item.id).await {
            Ok(()) => {
                self.toast(format!("Refreshing metadata for {}…", item.title));
                let internal = self.internal.clone();
                let events = self.events.clone();
                let item_id = item.id.clone();
                tokio::spawn(async move {
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
                    loop {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        match admin.activities().await {
                            Ok(activities)
                                if !activities.iter().any(|activity| activity.title().contains(&item.title)) =>
                            {
                                break;
                            }
                            Ok(_) if tokio::time::Instant::now() < deadline => {}
                            Ok(_) => break,
                            Err(error) => {
                                warn!(error = %error, "couldn't poll Plex metadata refresh");
                                break;
                            }
                        }
                    }
                    let _ = events.send(AppEvent::MetadataRefreshFinished(item_id));
                    let _ = internal.send(Internal::Refresh);
                });
            }
            Err(error) => {
                self.toast(format!("Couldn't refresh metadata: {error}"));
                self.metadata_refresh_finished(item.id);
            }
        }
    }

    fn metadata_refresh_finished(&self, item_id: String) {
        let _ = self.events.send(AppEvent::MetadataRefreshFinished(item_id));
    }

    fn fetch_artwork(&self, item: MediaItem, backdrop: bool) {
        if item.source != SourceKind::Plex {
            self.toast("Artwork editing is only available for Plex items");
            return;
        }
        let Some(plex) = self.plex.as_ref() else {
            self.toast("Plex isn't connected");
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                self.toast(format!("Plex admin API isn't available: {error}"));
                return;
            }
        };
        let events = self.events.clone();
        tokio::spawn(async move {
            let result = async {
                let choices = if backdrop { admin.backs(&item.id).await? } else { admin.posters(&item.id).await? };
                let mut output = Vec::with_capacity(choices.len());
                for choice in choices {
                    let image_key = choice.thumb.as_deref().unwrap_or(&choice.key);
                    let bytes = admin.artwork_thumbnail(image_key, backdrop).await?;
                    let image = tokio::task::spawn_blocking(move || decode(&bytes))
                        .await
                        .map_err(|error| MediaError::Protocol(format!("decoding artwork thumbnail: {error}")))?
                        .ok_or_else(|| MediaError::Protocol("couldn't decode artwork thumbnail".into()))?;
                    output.push(tvdash_core::models::ArtworkChoice {
                        key: choice.key,
                        provider: choice.provider.unwrap_or_else(|| "Plex".into()),
                        selected: choice.selected,
                        image: Some(image),
                    });
                }
                Ok::<_, MediaError>(output)
            }
            .await
            .map_err(|error| error.to_string());
            let _ = events.send(AppEvent::ArtworkChoices { item_id: item.id, backdrop, result });
        });
    }

    async fn set_artwork(&self, item: MediaItem, key: &str, backdrop: bool) {
        let Some(plex) = self.plex.as_ref() else {
            self.toast("Plex isn't connected");
            return;
        };
        let admin = match plex.admin() {
            Ok(admin) => admin,
            Err(error) => {
                self.toast(format!("Plex admin API isn't available: {error}"));
                return;
            }
        };
        let result =
            if backdrop { admin.set_backdrop(&item.id, key).await } else { admin.set_poster(&item.id, key).await };
        match result {
            Ok(()) => {
                self.toast(if backdrop { "Backdrop updated" } else { "Poster updated" });
                self.refresh();
                if !backdrop && let Some(source) = self.source_for(SourceKind::Plex) {
                    let events = self.events.clone();
                    let item_id = item.id.clone();
                    let id = item.id.clone();
                    let plex = plex.clone();
                    tokio::spawn(async move {
                        match plex.item(&id).await {
                            Ok(Some(updated)) => match source.poster(&updated, POSTER_HEIGHT).await {
                                Ok(bytes) => match tokio::task::spawn_blocking(move || decode(&bytes)).await {
                                    Ok(Some(image)) => {
                                        let _ =
                                            events.send(AppEvent::Poster { source: SourceKind::Plex, item_id, image });
                                    }
                                    Ok(None) => warn!(item = %id, "couldn't decode updated poster"),
                                    Err(error) => warn!(error = %error, "poster decode task failed"),
                                },
                                Err(error) => warn!(error = %error, "couldn't reload updated poster"),
                            },
                            Ok(None) => warn!(item = %id, "Plex returned no metadata after changing poster"),
                            Err(error) => warn!(error = %error, "couldn't reload item after changing poster"),
                        }
                    });
                }
            }
            Err(error) => self.toast(format!("Couldn't update artwork: {error}")),
        }
    }

    // ── Playback ────────────────────────────────────────────────────

    async fn play(&mut self, item: MediaItem) {
        self.play_item(item, false).await;
    }

    async fn play_item(&mut self, mut item: MediaItem, is_extra: bool) {
        self.clear_tv().await;
        if item.kind == tvdash_core::models::MediaKind::Series
            && let Some(plex) = &self.plex
        {
            let ep_key = match plex.show_on_deck(&item.id).await.ok().flatten() {
                Some(ep) => Some(ep.rating_key),
                None => match plex.show_seasons(&item.id).await.ok().and_then(|s| s.into_iter().next()) {
                    Some(season) => plex
                        .season_episodes(&season.rating_key)
                        .await
                        .ok()
                        .and_then(|eps| eps.into_iter().next())
                        .map(|ep| ep.rating_key),
                    None => None,
                },
            };
            if let Some(key) = ep_key
                && let Ok(Some(resolved)) = plex.item(&key).await
            {
                item = resolved;
            }
        }
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
            screen_name: self
                .tv_output
                .clone()
                .or_else(|| Some(self.config.display.tv_output.clone()).filter(|s| !s.is_empty())),
            title: format::item_line(&item),
            start_ms: item.resume_ms,
            fullscreen: !self.windowed,
        };
        let in_window = match &self.video {
            Some(slot) if !self.video_failed => match Embedded::spawn(&opts, &url) {
                Ok(e) => {
                    slot.attach(e.handle());
                    Some(Player::InWindow(e))
                }
                Err(e) => {
                    warn!(error = %e, "libmpv couldn't start; using mpv's own window");
                    None
                }
            },
            _ => None,
        };
        let player = match in_window {
            Some(p) => p,
            None => match Mpv::spawn(&opts, &url).await {
                Ok(m) => Player::Window(m),
                Err(e) => {
                    error!(error = %e, "couldn't start mpv");
                    self.health(Service::Player, Err(e.to_string()));
                    self.toast(format!("Couldn't start mpv: {e}"));
                    return;
                }
            },
        };
        let where_ = if player.in_window() { "in the window" } else { "in mpv's window" };
        info!(title = %item.title, resume_ms = ?item.resume_ms, where_, "playing on the TV");
        self.health(Service::Player, Ok(format!("playing {} ({where_})", item.title)));

        let start = if is_extra { 0 } else { item.resume_ms.unwrap_or(0) };
        let reporter = spawn_reporter(source, item.clone());
        let _ = reporter.send((start, PlayState::Playing));
        let is_plex_ep = !is_extra && item.source == SourceKind::Plex && item.kind == tvdash_core::models::MediaKind::Episode;
        let item_id = item.id.clone();
        let status =
            PlayerStatus { duration_ms: item.duration_ms, item, state: PlayState::Playing, position_ms: start };
        self.pending_next_ep = None;
        self.emit(AppEvent::NextEpisodePrompt(None));
        self.emit(AppEvent::Player(Some(status.clone())));
        self.playback = Some(Playback {
            status,
            player,
            reporter,
            last_emitted_sec: None,
            decode_hint_shown: false,
            ended_eof: false,
            markers: Vec::new(),
            credits_prompted: false,
            credits_dismissed: false,
            playing_extra: is_extra,
        });
        if is_plex_ep && let Some(plex) = self.plex.clone() {
            let internal = self.internal.clone();
            tokio::spawn(async move {
                if let Ok(markers) = plex.markers(&item_id).await && !markers.is_empty() {
                    let _ = internal.send(Internal::Markers { item_id, markers });
                }
            });
        }
        self.save_runtime_state();
        self.update_inhibit().await;
        let internal = self.internal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let _ = internal.send(Internal::Refresh);
        });
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
                    self.streaming_changed();
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
            self.streaming_changed();
        }
        self.emit_sites();
        match site.domain().filter(|_| clear_cookies) {
            // The cookie store is encrypted and locked while the browser runs, so
            // open the browser's own settings for that site and let you clear it there.
            Some(domain) => {
                self.clear_tv().await;
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
                self.save_runtime_state();
                self.update_inhibit().await;
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
        self.clear_tv().await;
        if !self.launch_browser(&site.name, &site.url).await {
            return;
        }
        let service_id = crate::brand::service_id(&site.name);
        let now = chrono::Utc::now().timestamp();
        self.update_config(|c| crate::recent::push_recent(&mut c.tv.recent, &service_id, now));
        self.emit(AppEvent::RecentServices(self.config.tv.recent.clone()));
        self.publish_quick();
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
        self.streaming_changed();
    }

    /// Nothing on the TV: stop mpv and close the browser.
    async fn clear_tv(&mut self) {
        self.stop_playback().await;
        self.close_browser().await;
    }

    /// Save the "opened" times and show them in Settings › Streaming.
    fn streaming_changed(&self) {
        self.save_state();
        self.emit(AppEvent::Streaming(self.state.streaming_opened.clone()));
    }

    async fn close_browser(&mut self) {
        if let Some(b) = self.browser.take() {
            b.close().await;
            self.save_runtime_state();
            self.update_inhibit().await;
            self.emit(AppEvent::Browser(None));
            self.publish_quick();
        }
    }

    async fn on_browser_exited(&mut self) {
        let Some(b) = self.browser.take() else { return };
        self.save_runtime_state();
        self.update_inhibit().await;
        self.emit(AppEvent::Browser(None));
        self.publish_quick();
        if b.exited_immediately() {
            warn!(site = %b.site, "browser exited at once; another instance probably owns the profile");
            self.toast("The browser handed off to a window that was already open. Close it and try again.");
        } else {
            info!(site = %b.site, "browser closed on the TV");
        }
        drop(b); // signals any helpers left in its process group
    }

    async fn stop_playback(&mut self) {
        self.paused_for_weather = false;
        if let Some(p) = self.playback.take() {
            let _ = p.reporter.send((p.status.position_ms, PlayState::Stopped));
            self.detach_video(&p.player);
            p.player.quit().await;
            self.after_playback().await;
        }
    }

    /// The window stops drawing `player`'s video (it frees its side first).
    fn detach_video(&self, player: &Player) {
        if player.in_window()
            && let Some(slot) = &self.video
        {
            slot.detach();
        }
    }

    /// Title, progress and end time over the video (`None` = until replaced).
    async fn show_overlay(&mut self, duration_ms: Option<u32>) {
        let general = &self.config.general;
        let Some(p) = self.playback.as_mut() else { return };
        let text = format::overlay(&p.status, format::now_local(general.tz()), general.clock);
        if let Err(e) = p.player.show_text(&text, duration_ms).await {
            warn!(error = %e, "mpv overlay failed");
        }
    }

    /// The window can't draw video: carry on in mpv's own window from where it was.
    async fn video_failed(&mut self, reason: String) {
        self.video_failed = true;
        let Some(p) = self.playback.as_ref().filter(|p| p.player.in_window()) else { return };
        let item = MediaItem { resume_ms: Some(p.status.position_ms), ..p.status.item.clone() };
        warn!(%reason, "in-window video failed; switching to mpv's own window");
        self.health(Service::Player, Err(format!("in-window video: {reason}")));
        self.stop_playback().await;
        self.play(item).await;
    }

    async fn update_inhibit(&mut self) {
        let active = self.playback.is_some() || self.browser.is_some();
        if active && self.inhibit.is_none() {
            self.inhibit = crate::portal::IdleInhibit::acquire("Video playing on TV").await;
        } else if !active && let Some(inh) = self.inhibit.take() {
            inh.release().await;
        }
    }

    async fn after_playback(&mut self) {
        self.save_runtime_state();
        self.update_inhibit().await;
        self.emit(AppEvent::Player(None));
        self.emit_sleep();
        // Give the server a moment to record the stop before re-reading resume points.
        let internal = self.internal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = internal.send(Internal::Refresh);
        });
    }

    fn is_sleep_held(&self) -> bool {
        let Some(b) = &self.browser else { return false };
        if b.site.eq_ignore_ascii_case("Google Meet") {
            return true;
        }
        self.config.streaming.iter().any(|s| s.hold_sleep_timer && s.name.eq_ignore_ascii_case(&b.site))
    }

    fn sleep_snapshot(&self, now: i64) -> tvdash_core::SleepSnapshot {
        let warn_total = self.config.sleep.warn_secs();
        let default_minutes = if self.config.sleep.remember_last {
            self.sleep.last_minutes.unwrap_or(self.config.sleep.default)
        } else {
            self.config.sleep.default
        };
        let use_24h = self.config.general.clock == tvdash_core::config::ClockFormat::H24;
        let held = self.sleep_held;
        let active = self.sleep.is_active();
        let phase = self.sleep.phase().to_owned();
        let (left, ends_at, label) = match &self.sleep.timer {
            crate::sleep::SleepTimer::Off => (0, String::new(), "Off".into()),
            crate::sleep::SleepTimer::EndOfItem { .. } => {
                let is_movie = self
                    .playback
                    .as_ref()
                    .is_some_and(|p| p.status.item.kind == tvdash_core::models::MediaKind::Movie);
                (0, String::new(), if is_movie { "End of movie".into() } else { "End of episode".into() })
            }
            crate::sleep::SleepTimer::At { deadline } => {
                let left = (*deadline - now).max(0);
                let ends = crate::sleep::format_ends_at(*deadline, use_24h);
                let lbl = if held {
                    "Paused · on a call".into()
                } else {
                    crate::sleep::label(left)
                };
                (left, ends, lbl)
            }
        };
        tvdash_core::SleepSnapshot {
            active,
            phase,
            label,
            ends_at,
            left,
            warn_total,
            episode_finished: self.sleep_episode_finished,
            held,
            default_minutes,
        }
    }

    fn emit_sleep(&mut self) {
        let now = crate::sleep::now_unix();
        let snap = self.sleep_snapshot(now);
        let plex_playing = self.playback.as_ref().is_some_and(|p| p.status.item.source == SourceKind::Plex);
        if let Some(tx) = &self.sleep_tx {
            let _ = tx.send(crate::home_cmd::SleepHttpState::from_snapshot(&snap, plex_playing));
        }
        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            snap.phase,
            snap.label,
            snap.ends_at,
            if snap.phase == "warn" { snap.left } else { 0 },
            snap.episode_finished,
            snap.held
        );
        if key != self.sleep_last_key {
            self.sleep_last_key = key;
            self.emit(AppEvent::Sleep(snap));
        }
    }

    async fn on_sleep_tick(&mut self) {
        let now = crate::sleep::now_unix();
        let warn = self.config.sleep.warn_secs();
        let held = self.is_sleep_held();
        if held != self.sleep_held
            && let Some(tx) = &self.sports_tx
        {
            let _ = tx.send(crate::sports_watch::WatcherMsg::SetHeld(held));
        }
        self.sleep_held = held;
        if held {
            if self.sleep.hold(now, warn) {
                self.save_runtime_state();
            }
            self.emit_sleep();
            return;
        }
        match self.sleep.tick(now, warn) {
            crate::sleep::Tick::Idle => {}
            crate::sleep::Tick::Counting(_) => {
                self.emit_sleep();
            }
            crate::sleep::Tick::Warn(_) => {
                self.emit_sleep();
            }
            crate::sleep::Tick::Fire => {
                self.sleep_now().await;
            }
        }
    }

    async fn sleep_now(&mut self) {
        self.toast("Going to sleep · saving your place");
        self.sleep.off();
        self.sleep_episode_finished = false;
        self.sleep_finished_key = None;

        if let Some(mut p) = self.playback.take() {
            self.detach_video(&p.player);
            let fade = self.config.sleep.fade_duration();
            let steps = 20u32;
            let step_wait = fade / steps;
            for _ in 0..steps {
                let _ = p.player.add_volume(-5).await;
                if !step_wait.is_zero() {
                    tokio::time::sleep(step_wait).await;
                }
            }
            let _ = p.player.set_pause(true).await;
            let _ = p.reporter.send((p.status.position_ms, PlayState::Stopped));
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = p.player.set_volume(100.0).await;
            p.player.quit().await;
        }

        self.release_tv().await;
        tracing::info!("sleep timer fired");
        quit_ui();
    }

    /// Everything tvdash holds on the TV, released in order before the app exits:
    /// playback (reporting Stopped so Plex keeps your place), the browser, the
    /// sleep inhibitor, and the reattach state.
    async fn release_tv(&mut self) {
        if self.playback.is_some() {
            self.stop_playback().await;
            // The Stopped report goes out from the reporter task; give it a moment
            // before the runtime shuts down.
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        self.close_browser().await;
        if let Some(inh) = self.inhibit.take() {
            inh.release().await;
        }
        crate::instance::RuntimeState::clear(&crate::instance::runtime_state_path());
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
                if p.status.item.kind == tvdash_core::models::MediaKind::Episode
                    && p.status.item.source == SourceKind::Plex
                    && should_prompt_credits(p.status.position_ms, &p.markers, p.credits_prompted)
                    && let Some(plex) = self.plex.clone()
                {
                    p.credits_prompted = true;
                    let item_id = p.status.item.id.clone();
                    let internal = self.internal.clone();
                    let events = self.events.clone();
                    let requested = self.posters_requested.clone();
                    tokio::spawn(async move {
                        if let Ok(Some(mut next_ep)) = plex.adjacent_episode(&item_id, true).await {
                            next_ep.resume_ms = None;
                            fetch_posters(plex, vec![next_ep.clone()], requested, events);
                            let _ = internal.send(Internal::NextAtCredits { item_id, next_ep });
                        }
                    });
                }
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
                if reason == "eof" {
                    p.ended_eof = true;
                    if let Some(dur) = p.status.item.duration_ms {
                        p.status.position_ms = dur;
                    }
                }
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
                self.detach_video(&p.player);
                let final_pos = if p.ended_eof {
                    p.status.item.duration_ms.unwrap_or(p.status.position_ms)
                } else {
                    p.status.position_ms
                };
                let _ = p.reporter.send((final_pos, PlayState::Stopped));
                info!("mpv exited; TV back to idle");
                self.after_playback().await;
                if !p.playing_extra && p.ended_eof {
                    let now = crate::sleep::now_unix();
                    let warn = self.config.sleep.warn_secs();
                    if self.sleep.item_finished(&p.status.item.id, now, warn) {
                        self.sleep_episode_finished = true;
                        self.sleep_finished_key = Some(p.status.item.id);
                        self.save_runtime_state();
                        self.emit_sleep();
                        return;
                    }
                } else if !p.playing_extra && self.sleep.item_stopped() {
                    self.save_runtime_state();
                    self.emit_sleep();
                    self.toast("Sleep timer off · playback stopped");
                }
                if !p.playing_extra && self.open_movie.as_deref() == Some(p.status.item.id.as_str()) {
                    self.open_movie(p.status.item.id.clone());
                }
                if !p.playing_extra
                    && p.ended_eof
                    && p.status.item.kind == tvdash_core::models::MediaKind::Episode
                    && p.status.item.source == SourceKind::Plex
                    && let Some(plex) = self.plex.clone()
                {
                    let item_id = p.status.item.id;
                    let events = self.events.clone();
                    let requested = self.posters_requested.clone();
                    tokio::spawn(async move {
                        if let Ok(Some(mut next_ep)) = plex.adjacent_episode(&item_id, true).await {
                            next_ep.resume_ms = None;
                            fetch_posters(plex, vec![next_ep.clone()], requested, events.clone());
                            let _ = events.send(AppEvent::NextEpisodePrompt(Some(next_ep)));
                        }
                    });
                }
                return;
            }
        }
        let status = p.status.clone();
        if let Some(duration) = overlay {
            let text = format::overlay(&status, format::now_local(self.config.general.tz()), self.config.general.clock);
            if let Err(e) = p.player.show_text(&text, duration).await {
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

async fn fetch_server_summary(
    admin: &PlexAdmin,
    plex: &PlexClient,
    local_playback: Option<PlayerStatus>,
    cached_sections: Arc<Mutex<Vec<ServerLibrary>>>,
    refresh_libraries: bool,
) -> tvdash_media::Result<ServerSummary> {
    let info = admin.server_info().await?;
    let sessions = plex.now_playing().await?;
    let resources = admin.resources().await?;
    let bandwidth = admin.bandwidth().await?;
    let libraries = if refresh_libraries {
        let sections = admin
            .sections()
            .await?
            .into_iter()
            .map(|section| ServerLibrary {
                title: section.title,
                item_count: section.item_count,
                scanned_ago: humanized_scan_age(section.scanned_at),
                refreshing: section.refreshing,
            })
            .collect::<Vec<_>>();
        let mut cached = cached_sections
            .lock()
            .map_err(|error| MediaError::Protocol(format!("server section cache poisoned: {error}")))?;
        *cached = sections.clone();
        sections
    } else {
        cached_sections
            .lock()
            .map_err(|error| MediaError::Protocol(format!("server section cache poisoned: {error}")))?
            .clone()
    };

    let latest_resource = resources.iter().max_by_key(|sample| sample.at());
    let cpu_percent = latest_resource
        .map(|resource| percent(resource.host_cpu_utilization().or_else(|| resource.process_cpu_utilization())))
        .unwrap_or(0);
    let ram_percent = latest_resource
        .map(|resource| percent(resource.host_memory_utilization().or_else(|| resource.process_memory_utilization())))
        .unwrap_or(0);
    let latest_bandwidth_at = bandwidth.iter().map(|sample| sample.at()).max();
    let bandwidth_by_at = bandwidth.iter().fold(BTreeMap::<i64, (u64, u64)>::new(), |mut samples, sample| {
        let value = samples.entry(sample.at()).or_default();
        if sample.lan() {
            value.0 = value.0.saturating_add(sample.bytes());
        } else {
            value.1 = value.1.saturating_add(sample.bytes());
        }
        samples
    });
    let bandwidth_mbps = latest_bandwidth_at
        .and_then(|at| bandwidth_by_at.get(&at))
        .map_or(0, |(lan, wan)| (lan.saturating_add(*wan).saturating_mul(8) / 6 / 1_000_000) as u32);
    let latest_at = resources.iter().map(|sample| sample.at()).max().unwrap_or_default();
    let mut resource_samples: Vec<_> =
        resources.iter().filter(|sample| latest_at.saturating_sub(sample.at()) <= 180).collect();
    resource_samples.sort_by_key(|sample| sample.at());
    if resource_samples.len() > 30 {
        resource_samples.drain(..resource_samples.len() - 30);
    }
    let cpu_host: Vec<f32> =
        resource_samples.iter().map(|sample| sample.host_cpu_utilization().unwrap_or_default()).collect();
    let cpu_process: Vec<f32> =
        resource_samples.iter().map(|sample| sample.process_cpu_utilization().unwrap_or_default()).collect();
    let ram_host: Vec<f32> =
        resource_samples.iter().map(|sample| sample.host_memory_utilization().unwrap_or_default()).collect();
    let ram_process: Vec<f32> =
        resource_samples.iter().map(|sample| sample.process_memory_utilization().unwrap_or_default()).collect();
    let newest_bandwidth_at = bandwidth_by_at.keys().next_back().copied().unwrap_or_default();
    let mut bandwidth_samples: Vec<_> = bandwidth_by_at
        .iter()
        .filter(|(at, _)| newest_bandwidth_at.saturating_sub(**at) <= 180)
        .map(|(_, values)| *values)
        .collect();
    if bandwidth_samples.len() > 30 {
        bandwidth_samples.drain(..bandwidth_samples.len() - 30);
    }
    let bandwidth_lan: Vec<f32> =
        bandwidth_samples.iter().map(|(bytes, _)| *bytes as f32 * 8.0 / 6.0 / 1_000_000.0).collect();
    let bandwidth_wan: Vec<f32> =
        bandwidth_samples.iter().map(|(_, bytes)| *bytes as f32 * 8.0 / 6.0 / 1_000_000.0).collect();
    let bandwidth_max = bandwidth_lan.iter().chain(&bandwidth_wan).copied().fold(0.0f32, f32::max);
    let bandwidth_scale = match bandwidth_max {
        x if x <= 10.0 => 10,
        x if x <= 25.0 => 25,
        x if x <= 50.0 => 50,
        x if x <= 100.0 => 100,
        x => (x / 100.0).ceil() as u32 * 100,
    };
    let mut streams_detail: Vec<ServerStream> = sessions
        .iter()
        .map(|session| ServerStream {
            session_id: session.session_id.clone(),
            local: false,
            title: format::item_line(&session.item),
            device: session.device.clone(),
            detail: format!(
                "{} · {} · {} · {}",
                "Plex",
                if session.transcoding { "Transcode" } else { "Direct play" },
                if session.state == PlayState::Paused { "Paused" } else { "Playing" },
                format::duration(session.position_ms),
            ),
        })
        .collect();
    if let Some(status) = local_playback {
        streams_detail.push(ServerStream {
            session_id: None,
            local: true,
            title: format::item_line(&status.item),
            device: "This TV".into(),
            detail: if status.state == PlayState::Paused { "Paused".into() } else { "Playing".into() },
        });
    }
    Ok(ServerSummary {
        name: format!("{} · Plex {}", info.friendly_name, info.version),
        streams: (sessions.len() + streams_detail.iter().filter(|stream| stream.local).count()) as u32,
        transcodes: sessions.iter().filter(|session| session.transcoding).count() as u32,
        bandwidth_mbps,
        cpu_percent,
        ram_percent,
        status: "● online".to_owned(),
        plex_pass: info.has_plex_pass(),
        cpu_host_path: graph_path(&cpu_host, 100.0),
        cpu_process_path: graph_path(&cpu_process, 100.0),
        ram_host_path: graph_path(&ram_host, 100.0),
        ram_process_path: graph_path(&ram_process, 100.0),
        bandwidth_lan_path: graph_path(&bandwidth_lan, bandwidth_scale as f32),
        bandwidth_wan_path: graph_path(&bandwidth_wan, bandwidth_scale as f32),
        bandwidth_scale,
        streams_detail,
        libraries,
    })
}

fn graph_path(samples: &[f32], max: f32) -> String {
    let count = samples.len();
    if count == 0 || max <= 0.0 {
        return String::new();
    }
    let denominator = (count - 1).max(1) as f32;
    samples
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let x = i as f32 * 300.0 / denominator;
            let y = 100.0 - (value / max).clamp(0.0, 1.0) * 100.0;
            format!("{} {x:.1} {y:.1}", if i == 0 { "M" } else { "L" })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn percent(value: Option<f32>) -> u32 {
    value.unwrap_or_default().clamp(0.0, 100.0) as u32
}

fn humanized_scan_age(timestamp: Option<u64>) -> String {
    let Some(timestamp) = timestamp else {
        return "recently".to_owned();
    };
    let now = std::time::SystemTime::now();
    let Ok(now) = now.duration_since(std::time::UNIX_EPOCH) else {
        return "recently".to_owned();
    };
    let elapsed = now.as_secs().saturating_sub(timestamp);
    match elapsed {
        0..=59 => "just now".to_owned(),
        60..=119 => "1 min ago".to_owned(),
        120..=3599 => format!("{} min ago", elapsed / 60),
        3600..=86399 => format!("{} h ago", elapsed / 3600),
        _ => format!("{} d ago", elapsed / 86400),
    }
}

fn should_prompt_credits(pos_ms: u64, markers: &[tvdash_media::plex::Marker], prompted: bool) -> bool {
    !prompted && markers.iter().any(|m| m.kind == "credits" && pos_ms >= m.start_time_offset)
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
    use super::{decoder_hint, graph_path, safe_to_delete, should_prompt_credits};
    use tvdash_media::plex::Marker;

    #[test]
    fn credits_prompt_triggers_once_at_marker_start() {
        let m = vec![
            Marker { kind: "intro".into(), start_time_offset: 10_000, end_time_offset: 50_000 },
            Marker { kind: "credits".into(), start_time_offset: 1_200_000, end_time_offset: 1_260_000 },
        ];
        assert!(!should_prompt_credits(0, &[], false));
        assert!(!should_prompt_credits(1_199_999, &m, false));
        assert!(should_prompt_credits(1_200_000, &m, false));
        assert!(should_prompt_credits(1_230_000, &m, false));
        assert!(!should_prompt_credits(1_230_000, &m, true));
    }

    #[test]
    fn graph_paths_map_samples_to_the_viewbox() {
        assert_eq!(graph_path(&[], 100.0), "");
        assert_eq!(graph_path(&[0.0, 100.0], 100.0), "M 0.0 100.0 L 300.0 0.0");
        assert_eq!(graph_path(&[5.0], 10.0), "M 0.0 50.0");
    }

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

/// End the Slint event loop from the controller (main then shuts the runtime down).
fn quit_ui() {
    let _ = slint::invoke_from_event_loop(|| {
        let _ = slint::quit_event_loop();
    });
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

async fn fetch_season_episodes(
    plex: Arc<PlexClient>,
    season_key: String,
    requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    events: UnboundedSender<AppEvent>,
) {
    let result = plex.season_episodes(&season_key).await.map_err(|e| e.to_string());
    if let Ok(ref eps) = result {
        let thumb_items: Vec<tvdash_core::models::MediaItem> = eps
            .iter()
            .filter(|ep| ep.thumb.is_some())
            .map(|ep| tvdash_core::models::MediaItem {
                source: SourceKind::Plex,
                id: ep.rating_key.clone(),
                kind: tvdash_core::models::MediaKind::Episode,
                title: ep.title.clone(),
                subtitle: None,
                episode_label: None,
                duration_ms: ep.duration_ms,
                resume_ms: ep.view_offset_ms,
                poster: ep.thumb.clone(),
                summary: None,
                extra: Default::default(),
            })
            .collect();
        fetch_posters(plex, thumb_items, requested, events.clone());
    }
    let _ = events.send(AppEvent::SeasonEpisodes { season_key, result });
}

fn collection_poster_items(collections: &[tvdash_core::models::CollectionInfo]) -> Vec<MediaItem> {
    collections
        .iter()
        .filter(|c| c.thumb.is_some())
        .map(|c| MediaItem {
            source: SourceKind::Plex,
            id: c.rating_key.clone(),
            kind: tvdash_core::models::MediaKind::Other,
            title: c.title.clone(),
            subtitle: None,
            episode_label: None,
            duration_ms: None,
            resume_ms: None,
            poster: c.thumb.clone(),
            summary: None,
            extra: Default::default(),
        })
        .collect()
}

fn fetch_posters(
    source: Arc<dyn MediaSource>,
    items: Vec<MediaItem>,
    requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    events: UnboundedSender<AppEvent>,
) {
    static POSTER_SEM: std::sync::LazyLock<tokio::sync::Semaphore> =
        std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(4));

    for item in items.into_iter().filter(|i| i.poster.is_some()) {
        let key = (item.source, item.id.clone());
        if !requested.lock().unwrap().insert(key.clone()) {
            continue;
        }
        let (source, requested, events) = (source.clone(), requested.clone(), events.clone());
        tokio::spawn(async move {
            let Ok(_permit) = POSTER_SEM.acquire().await else { return };
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

fn fetch_backdrops(
    source: Arc<dyn MediaSource>,
    items: Vec<MediaItem>,
    requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
    events: UnboundedSender<AppEvent>,
) {
    static BD_SEM: std::sync::LazyLock<tokio::sync::Semaphore> =
        std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(4));

    for item in items {
        if item.extra.art.is_none() && item.poster.is_none() {
            continue;
        }
        let key = (item.source, item.id.clone());
        if !requested.lock().unwrap().insert(key.clone()) {
            continue;
        }
        let (source, requested, events) = (source.clone(), requested.clone(), events.clone());
        tokio::spawn(async move {
            let Ok(_permit) = BD_SEM.acquire().await else { return };
            let decoded = match source.backdrop(&item, 1300, 660).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || decode(&bytes)).await.ok().flatten(),
                Err(_) => match source.poster(&item, 450).await {
                    Ok(bytes) => tokio::task::spawn_blocking(move || decode_blurred(&bytes)).await.ok().flatten(),
                    Err(_) => None,
                },
            };
            match decoded {
                Some(image) => {
                    let _ = events.send(AppEvent::Backdrop { source: item.source, item_id: item.id, image });
                }
                None => {
                    requested.lock().unwrap().remove(&key);
                }
            }
        });
    }
}

pub(crate) fn decode_blurred(bytes: &[u8]) -> Option<RgbaImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let small = img.thumbnail(320, 320);
    let blurred = image::imageops::blur(&small, 16.0);
    Some(RgbaImage { width: blurred.width(), height: blurred.height(), pixels: blurred.into_raw() })
}

pub(crate) fn decode_sports_logo(bytes: &[u8]) -> Option<RgbaImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = if img.width() > 128 || img.height() > 128 {
        img.resize(128, 128, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = resized.to_rgba8();
    Some(RgbaImage { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() })
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
    #[ignore = "needs python3 and libmpv; run with --ignored"]
    async fn tv_layout_plays_in_the_window_and_falls_back() {
        let _serial = SERIAL.lock().await;
        if let Err(e) = tvdash_player::libmpv::load() {
            eprintln!("skipped: {e}");
            return;
        }
        let slot = VideoSlot::new();
        let for_controller = slot.clone();
        let mut h = Harness::start_with(
            "fake_plex.py",
            |c, port| {
                c.plex.account_url = format!("http://127.0.0.1:{port}");
                c.plex.client_id = "plex-client-e2e".into();
            },
            move |c, _| {
                c.set_video(for_controller);
                c.set_layout(Layout::Tv);
            },
        )
        .await;
        h.send(Command::SignIn(SourceKind::Plex));
        h.wait_for(|e| matches!(e, AppEvent::SignIn(_, SignInState::Done)).then_some(())).await;
        let items = h
            .wait_for(|e| match e {
                AppEvent::ContinueWatching(SourceKind::Plex, items) => Some(items.clone()),
                _ => None,
            })
            .await;
        let playing =
            |id: String| move |e: &AppEvent| matches!(e, AppEvent::Player(Some(s)) if s.item.id == id).then_some(());

        // TV only: the item plays in the window (the slot holds the core), no mpv window.
        h.send(Command::Play(items[0].clone()));
        h.wait_for(playing(items[0].id.clone())).await;
        assert!(slot.current().is_some(), "handed to the window");
        assert!(!h.mpv_log.exists(), "mpv's own window wasn't started");
        // Home leaves it playing behind the Home screen.
        h.send(Command::Home);
        sleep(Duration::from_millis(300)).await;
        assert!(slot.current().is_some(), "still playing after Home");

        // The window can't draw: carry on in mpv's own window from the same place.
        h.send(Command::VideoFailed("no OpenGL".into()));
        h.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;
        h.wait_for(playing(items[0].id.clone())).await;
        assert!(slot.current().is_none(), "the window let go");
        let (argv, _) = h.mpv_log();
        assert!(argv.contains(&"--start=600.000".to_string()), "resumed where it was: {argv:?}");
        h.send(Command::Stop);
        h.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;

        // For the rest of the run, mpv's window straight away.
        h.send(Command::Play(items[1].clone()));
        h.wait_for(playing(items[1].id.clone())).await;
        assert!(slot.current().is_none());
        h.send(Command::Stop);
        h.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn plex_pin_discovery_sessions_play_pause_stop() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_plex.py", |c, port| {
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
        assert_eq!(sessions[0].session_id.as_deref(), Some("200"));

        h.send(Command::ServerRefresh { refresh_libraries: true });
        let summary = h
            .wait_for(|e| match e {
                AppEvent::ServerSummary(summary) => Some(summary.clone()),
                _ => None,
            })
            .await;
        assert_eq!(summary.name, "Republic · Plex 1.40.0");
        assert_eq!((summary.streams, summary.transcodes, summary.bandwidth_mbps), (1, 0, 12));
        assert_eq!((summary.cpu_percent, summary.ram_percent), (40, 50));
        assert!(summary.plex_pass);
        assert_eq!(summary.libraries[0].item_count, 42);
        h.send(Command::ServerRefreshActivities);
        let activities = h
            .wait_for(|e| match e {
                AppEvent::ServerActivities(activities) => Some(activities.clone()),
                _ => None,
            })
            .await;
        assert_eq!(activities[0].uuid, "scan-1");

        h.send(Command::FetchArtwork { item: items[0].clone(), backdrop: false });
        let posters = h
            .wait_for(|e| match e {
                AppEvent::ArtworkChoices { item_id, backdrop: false, result: Ok(choices) } if item_id == "101" => {
                    Some(choices.clone())
                }
                _ => None,
            })
            .await;
        assert_eq!(posters.len(), 2);
        assert!(posters.iter().all(|choice| choice.image.is_some()));
        h.send(Command::SetArtwork { item: items[0].clone(), key: "/art/poster/2".into(), backdrop: false });
        h.wait_for(|e| {
            matches!(e, AppEvent::Poster { source: SourceKind::Plex, item_id, .. } if item_id == "101").then_some(())
        })
        .await;
        h.send(Command::FetchArtwork { item: items[0].clone(), backdrop: true });
        let backdrops = h
            .wait_for(|e| match e {
                AppEvent::ArtworkChoices { item_id, backdrop: true, result: Ok(choices) } if item_id == "101" => {
                    Some(choices.clone())
                }
                _ => None,
            })
            .await;
        assert_eq!(backdrops.len(), 1);
        h.send(Command::SetArtwork { item: items[0].clone(), key: "/art/backdrop/1".into(), backdrop: true });
        h.send(Command::RefreshMetadata(items[0].clone()));
        h.send(Command::ServerCancelActivity("scan-1".into()));
        h.wait_for(|e| matches!(e, AppEvent::Toast(text) if text.contains("Refreshing metadata")).then_some(())).await;

        h.send(Command::ServerScan { index: None });
        h.send(Command::ServerScan { index: Some(0) });
        h.send(Command::ServerTerminate { session_id: "200".into(), reason: "Stopped from test".into() });

        h.play_pause_stop(&items[0]).await;

        // What Plex saw.
        let log = h.server_log();
        let server_calls: Vec<&Value> = log.iter().filter(|r| r["path"] != "/api/v2/pins/77").collect();
        assert!(
            server_calls.iter().all(|r| r["client"] == "plex-client-e2e" && r["product"] == "tvdash"),
            "identifying headers on every call"
        );
        assert!(log.iter().any(|r| r["path"] == "/statistics/resources"));
        assert!(log.iter().any(|r| r["path"] == "/statistics/bandwidth"));
        assert!(log.iter().any(|r| r["path"] == "/activities"));
        assert!(log.iter().any(|r| r["path"] == "/library/sections/1/refresh"));
        assert!(log.iter().any(|r| r["path"] == "/library/sections/all/refresh"));
        assert!(log.iter().any(|r| r["path"] == "/library/metadata/101/posters"));
        assert!(log.iter().any(|r| r["path"] == "/library/metadata/101/arts"));
        assert!(
            log.iter().any(|r| r["path"] == "/library/metadata/101/poster" && r["query"]["url"][0] == "/art/poster/2")
        );
        assert!(
            log.iter().any(|r| r["path"] == "/library/metadata/101/art" && r["query"]["url"][0] == "/art/backdrop/1")
        );
        assert!(log.iter().any(|r| r["path"] == "/library/metadata/101/refresh"));
        assert!(log.iter().any(|r| r["method"] == "DELETE" && r["path"] == "/activities/scan-1"));
        assert!(log.iter().any(|r| r["path"] == "/status/sessions/terminate"
            && r["query"]["sessionId"][0] == "200"
            && r["query"]["reason"][0] == "Stopped from test"));
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
    async fn quit_reports_stopped_to_plex_and_quits_mpv() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_plex.py", |c, port| {
            c.plex.account_url = format!("http://127.0.0.1:{port}");
            c.plex.client_id = "plex-client-e2e".into();
        })
        .await;
        h.wait_for(|e| matches!(e, AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp)).then_some(())).await;
        h.send(Command::SignIn(SourceKind::Plex));
        h.wait_for(|e| matches!(e, AppEvent::SignIn(SourceKind::Plex, SignInState::Done)).then_some(())).await;
        let items = h
            .wait_for(|e| match e {
                AppEvent::ContinueWatching(SourceKind::Plex, items) => Some(items.clone()),
                _ => None,
            })
            .await;
        let start = items[0].resume_ms.unwrap_or(0);
        h.send(Command::Play(items[0].clone()));
        h.wait_for(|e| match e {
            AppEvent::Player(Some(s)) if s.position_ms >= start + 2_000 => Some(()),
            _ => None,
        })
        .await;

        // Settings › Quit while playing: playback ends cleanly before the app exits.
        h.send(Command::Quit);
        h.wait_for(|e| matches!(e, AppEvent::Player(None)).then_some(())).await;
        sleep(Duration::from_millis(500)).await;

        let states: Vec<String> = h
            .server_log()
            .iter()
            .filter(|r| r["path"] == "/:/timeline")
            .filter_map(|r| r["query"]["state"][0].as_str().map(str::to_owned))
            .collect();
        assert_eq!(states.last().map(String::as_str), Some("stopped"), "{states:?}");
        let (_, cmds) = h.mpv_log();
        assert!(cmds.contains(&json!(["quit"])), "mpv was asked to quit: {cmds:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn streaming_sites_open_switch_stop_and_handoff() {
        let _serial = SERIAL.lock().await;
        // Any fake works as the "server"; this test only uses the browser.
        let mut h = Harness::start("fake_plex.py", |c, _| {
            c.plex.enabled = false;
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
        let mut h = Harness::start("fake_plex.py", |c, _| {
            c.plex.enabled = false;
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
            logo: None,
            ..Default::default()
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
            logo: None,
            ..Default::default()
        };
        let endpoints = tvdash_discover::Endpoints { api: tmdb_url.clone(), images: format!("{tmdb_url}/t/p") };
        let mut h = Harness::start_with(
            "fake_plex.py",
            |c, port| {
                c.plex.account_url = format!("http://127.0.0.1:{port}");
                c.plex.client_id = "plex-client-e2e".into();
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

        // Logos are kept on disk, and a logo file in the config wins over TMDB's.
        let downloads = tmdb_calls("/t/p/w92");
        assert!(downloads >= 1);
        assert!(h.dir.join("logos").read_dir().unwrap().count() >= 1, "logos saved");
        let own = h.dir.join("max.png");
        image::RgbaImage::from_pixel(3, 3, image::Rgba([1, 2, 3, 255])).save(&own).unwrap();
        let max = StreamingSite { logo: Some(own.display().to_string()), ..saved[2].clone() };
        h.send(Command::SaveSite { index: Some(2), site: max });
        let all = h
            .wait_for(|e| match e {
                AppEvent::SiteLogos(l) => {
                    Some(l.iter().map(|(n, i)| (n.clone(), (i.width, i.height))).collect::<Vec<_>>())
                }
                _ => None,
            })
            .await;
        assert_eq!(all.len(), 3, "{all:?}");
        assert_eq!(all[2], ("Max".to_string(), (3, 3)), "the config's file, not TMDB's");
        assert_eq!(tmdb_calls("/t/p/w92"), downloads, "saved logos aren't downloaded again");

        // Sign in to Plex so its library's TMDB IDs are known.
        h.send(Command::SignIn(SourceKind::Plex));
        h.wait_for(|e| matches!(e, AppEvent::SignIn(_, SignInState::Done)).then_some(())).await;
        sleep(Duration::from_millis(500)).await;

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

    #[test]
    fn sports_logo_decode_resizes_to_fit_128() {
        let big = image::RgbaImage::from_pixel(500, 500, image::Rgba([10, 20, 30, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(big).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let decoded = decode_sports_logo(png.get_ref()).expect("decode_sports_logo");
        assert!(decoded.width <= 128 && decoded.height <= 128, "{}x{}", decoded.width, decoded.height);
        assert_eq!((decoded.width, decoded.height), (128, 128));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn forget_images_allows_refetch() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_plex.py", |c, _| c.jellyfin.enabled = false).await;
        h.wait_for(|e| matches!(e, AppEvent::ContinueWatching(SourceKind::Plex, _)).then_some(())).await;

        let item = MediaItem {
            id: "1".into(),
            source: SourceKind::Plex,
            kind: tvdash_core::models::MediaKind::Movie,
            title: "Test".into(),
            subtitle: None,
            summary: None,
            poster: Some("/library/metadata/1/thumb".into()),
            episode_label: None,
            resume_ms: None,
            duration_ms: None,
            extra: tvdash_core::models::MediaExtra { art: Some("/library/metadata/1/art".into()), ..Default::default() },
        };

        h.send(Command::PrefetchBackdrop(item.clone()));
        h.wait_for(|e| matches!(e, AppEvent::Backdrop { item_id, .. } if item_id == "1").then_some(())).await;

        // Second prefetch without ForgetImages is a no-op; after ForgetImages it fetches again.
        h.send(Command::ForgetImages {
            posters: Vec::new(),
            backdrops: vec![(SourceKind::Plex, "1".into())],
            logos: Vec::new(),
        });
        h.send(Command::PrefetchBackdrop(item));
        h.wait_for(|e| matches!(e, AppEvent::Backdrop { item_id, .. } if item_id == "1").then_some(())).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn setting_the_location_saves_it() {
        let _serial = SERIAL.lock().await;
        let mut h = Harness::start("fake_plex.py", |c, _| {
            c.plex.enabled = false;
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
