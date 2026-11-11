//! Background controller: owns media sources, sign-in flows and the mpv
//! session. Receives `Command`s from the UI and emits `AppEvent`s back.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tracing::{error, info, warn};
use tvdash_core::{
    AppEvent, Command, Config, SecretKey,
    events::PlayerStatus,
    models::{AccountStatus, MediaItem, PlayState, RgbaImage, SignInState, SourceKind},
    secrets,
};
use tvdash_media::{
    MediaError, MediaSource,
    jellyfin::{Auth, JellyfinClient},
};
use tvdash_player::mpv::{Mpv, MpvEvent, MpvOptions};

use crate::format;

const REFRESH_EVERY: Duration = Duration::from_secs(120);
const PROGRESS_EVERY: Duration = Duration::from_secs(10);
const POSTER_HEIGHT: u32 = 480;
const QUICK_CONNECT_POLL: Duration = Duration::from_secs(2);
const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Messages from tasks the controller spawned.
pub enum Internal {
    JellyfinSignedIn(Auth),
    Refresh,
}

struct Playback {
    status: PlayerStatus,
    mpv: Mpv,
    /// Sequential progress reports for this item (keeps start → progress → stop ordered).
    reporter: UnboundedSender<(u64, PlayState)>,
    last_emitted_sec: Option<u64>,
}

pub struct Controller {
    config: Config,
    config_path: PathBuf,
    windowed: bool,
    events: UnboundedSender<AppEvent>,
    internal: UnboundedSender<Internal>,
    jellyfin: Option<Arc<JellyfinClient>>,
    sign_in: Option<JoinHandle<()>>,
    playback: Option<Playback>,
    posters_requested: Arc<Mutex<HashSet<(SourceKind, String)>>>,
}

enum Step {
    Command(Command),
    Mpv(Option<MpvEvent>),
    Internal(Internal),
    Refresh,
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
        let c = Self {
            config,
            config_path,
            windowed,
            events,
            internal,
            jellyfin: None,
            sign_in: None,
            playback: None,
            posters_requested: Arc::default(),
        };
        (c, internal_rx)
    }

    fn emit(&self, event: AppEvent) {
        let _ = self.events.send(event);
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

        loop {
            let step = {
                let mpv_next = async {
                    match self.playback.as_mut() {
                        Some(p) => p.mpv.next_event().await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    c = commands.recv() => c.map_or(Step::Shutdown, Step::Command),
                    e = mpv_next => Step::Mpv(e),
                    Some(m) = internal.recv() => Step::Internal(m),
                    _ = refresh.tick() => Step::Refresh,
                    _ = progress.tick() => Step::Progress,
                }
            };
            match step {
                Step::Command(c) => self.on_command(c).await,
                Step::Mpv(e) => self.on_mpv(e).await,
                Step::Internal(Internal::JellyfinSignedIn(auth)) => self.on_jellyfin_signed_in(auth).await,
                Step::Internal(Internal::Refresh) | Step::Refresh => self.refresh(),
                Step::Progress => self.report_tick(),
                Step::Shutdown => break,
            }
        }
        self.stop_playback().await;
    }

    async fn init(&mut self) {
        let cfg = &self.config.jellyfin;
        if !cfg.enabled {
            return;
        }
        let token = match tokio::task::spawn_blocking(|| secrets::get(SecretKey::JellyfinToken)).await {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                warn!(error = %e, "couldn't read Jellyfin token from the keyring");
                None
            }
            Err(e) => {
                warn!(error = %e, "keyring task failed");
                None
            }
        };
        match (token, cfg.user_id.is_empty()) {
            (Some(token), false) => {
                let auth = Auth {
                    token,
                    user_id: cfg.user_id.clone(),
                };
                match JellyfinClient::new(&cfg.server_url, &cfg.device_id, Some(auth)) {
                    Ok(c) => self.jellyfin = Some(Arc::new(c)),
                    Err(e) => self.emit(AppEvent::Account(SourceKind::Jellyfin, AccountStatus::Unreachable(e.to_string()))),
                }
            }
            _ => self.emit(AppEvent::Account(SourceKind::Jellyfin, AccountStatus::NotSetUp)),
        }
        // Plex arrives in M3.
        self.emit(AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp));
        self.refresh();
    }

    async fn on_command(&mut self, cmd: Command) {
        match cmd {
            Command::Play(item) => self.play(item).await,
            Command::Pause | Command::Resume => {
                let pause = matches!(cmd, Command::Pause);
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.mpv.set_pause(pause).await {
                        warn!(error = %e, "mpv pause failed");
                    }
            }
            Command::Seek(secs) => {
                if let Some(p) = self.playback.as_mut()
                    && let Err(e) = p.mpv.seek(secs).await {
                        warn!(error = %e, "mpv seek failed");
                    }
            }
            Command::Stop => self.stop_playback().await,
            Command::SignIn(SourceKind::Jellyfin) => self.start_jellyfin_sign_in(),
            Command::CancelSignIn(kind) => {
                if let Some(task) = self.sign_in.take() {
                    task.abort();
                    self.emit(AppEvent::SignIn(kind, SignInState::Cancelled));
                }
            }
            Command::SignIn(SourceKind::Plex) => self.toast("Plex sign-in arrives in M3"),
            Command::Refresh => self.refresh(),
            Command::OpenSite(i) => match self.config.streaming.get(i) {
                Some(site) => {
                    info!(site = %site.name, url = %site.url, "open site requested (M4)");
                    self.toast(format!("{} on the TV arrives in M4", site.name));
                }
                None => warn!(index = i, "open site: no such streaming entry"),
            },
            Command::CloseBrowser | Command::Search(_) => info!(command = ?cmd, "not implemented yet"),
        }
    }

    // ── Library rows ────────────────────────────────────────────────

    fn sources(&self) -> Vec<Arc<dyn MediaSource>> {
        let mut v: Vec<Arc<dyn MediaSource>> = Vec::new();
        if let Some(j) = &self.jellyfin {
            v.push(j.clone());
        }
        v
    }

    fn source_for(&self, kind: SourceKind) -> Option<Arc<dyn MediaSource>> {
        self.sources().into_iter().find(|s| s.kind() == kind)
    }

    fn refresh(&self) {
        for source in self.sources() {
            let events = self.events.clone();
            let requested = self.posters_requested.clone();
            tokio::spawn(async move {
                let kind = source.kind();
                match source.continue_watching().await {
                    Ok(items) => {
                        let _ = events.send(AppEvent::Account(kind, AccountStatus::Connected));
                        let _ = events.send(AppEvent::ContinueWatching(kind, items.clone()));
                        fetch_posters(source, items, requested, events);
                    }
                    Err(e) => {
                        warn!(source = kind.label(), error = %e, "continue watching failed");
                        let _ = events.send(AppEvent::Account(kind, account_status(&e)));
                    }
                }
            });
        }
    }

    // ── Sign-in ─────────────────────────────────────────────────────

    fn start_jellyfin_sign_in(&mut self) {
        if let Some(old) = self.sign_in.take() {
            old.abort();
        }
        let cfg = self.config.jellyfin.clone();
        let events = self.events.clone();
        let internal = self.internal.clone();
        self.sign_in = Some(tokio::spawn(async move {
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
        }));
    }

    async fn on_jellyfin_signed_in(&mut self, auth: Auth) {
        self.sign_in = None;
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
                self.emit(AppEvent::Account(SourceKind::Jellyfin, AccountStatus::Connected));
                self.refresh();
            }
            Err(e) => self.emit(AppEvent::SignIn(SourceKind::Jellyfin, SignInState::Failed(e.to_string()))),
        }
    }

    // ── Playback ────────────────────────────────────────────────────

    async fn play(&mut self, item: MediaItem) {
        self.stop_playback().await;
        let Some(source) = self.source_for(item.source) else {
            self.toast(format!("{} isn't connected", item.source.label()));
            return;
        };
        let url = match source.stream_url(&item).await {
            Ok(u) => u,
            Err(e) => {
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
                self.toast(format!("Couldn't start mpv: {e}"));
                return;
            }
        };
        info!(title = %item.title, resume_ms = ?item.resume_ms, "playing on the TV");

        let start = item.resume_ms.unwrap_or(0);
        let reporter = spawn_reporter(source, item.clone());
        let _ = reporter.send((start, PlayState::Playing));
        let status = PlayerStatus {
            duration_ms: item.duration_ms,
            item,
            state: PlayState::Playing,
            position_ms: start,
        };
        self.emit(AppEvent::Player(Some(status.clone())));
        self.playback = Some(Playback {
            status,
            mpv,
            reporter,
            last_emitted_sec: None,
        });
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
            && p.status.state == PlayState::Playing {
                let _ = p.reporter.send((p.status.position_ms, PlayState::Playing));
            }
    }
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
                    let _ = events.send(AppEvent::Poster {
                        source: item.source,
                        item_id: item.id,
                        image,
                    });
                }
                None => {
                    requested.lock().unwrap().remove(&key); // retry on next refresh
                }
            }
        });
    }
}

fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            Some(RgbaImage {
                width: rgba.width(),
                height: rgba.height(),
                pixels: rgba.into_raw(),
            })
        }
        Err(e) => {
            warn!(error = %e, "poster decode failed");
            None
        }
    }
}

#[cfg(test)]
mod e2e {
    //! Drives the real controller against fake Jellyfin + fake mpv (Python).
    //! Run with: `cargo test -p tvdash -- --ignored`

    use super::*;
    use serde_json::Value;
    use std::{path::Path, process::Stdio};
    use tokio::time::{sleep, timeout};

    const FAKES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes");

    async fn wait_for<T>(rx: &mut UnboundedReceiver<AppEvent>, mut pick: impl FnMut(&AppEvent) -> Option<T>) -> T {
        timeout(Duration::from_secs(15), async {
            loop {
                let ev = rx.recv().await.expect("controller stopped");
                if let Some(v) = pick(&ev) {
                    return v;
                }
            }
        })
        .await
        .expect("timed out waiting for event")
    }

    fn read_log(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs python3; run with --ignored"]
    async fn sign_in_continue_watching_play_pause_stop() {
        let dir = std::env::temp_dir().join(format!("tvdash-e2e-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        let (port_file, jf_log, mpv_log) = (dir.join("port"), dir.join("jellyfin.log"), dir.join("mpv.log"));

        let _server = tokio::process::Command::new(format!("{FAKES}/fake_jellyfin.py"))
            .arg(&port_file)
            .arg(&jf_log)
            .stdout(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("start fake jellyfin");
        let port = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(p) = std::fs::read_to_string(&port_file)
                    && !p.is_empty() {
                        return p;
                    }
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();

        // SAFETY: test-only; no other threads read these variables concurrently.
        unsafe {
            std::env::set_var("FAKE_MPV_LOG", &mpv_log);
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
        }

        let mut config = Config::default();
        config.jellyfin.server_url = format!("http://127.0.0.1:{port}");
        config.jellyfin.device_id = "dev-e2e".into();
        config.player.mpv = format!("{FAKES}/fake_mpv.py");
        let config_path = dir.join("config.toml");
        config.save_to(&config_path).unwrap();

        let (ev_tx, mut ev) = unbounded_channel();
        let (cmd, cmd_rx) = unbounded_channel();
        let (controller, internal_rx) = Controller::new(config, config_path.clone(), false, ev_tx);
        tokio::spawn(controller.run(cmd_rx, internal_rx));

        // Not signed in yet (and no keyring in CI) → sign-in prompt.
        wait_for(&mut ev, |e| {
            matches!(e, AppEvent::Account(SourceKind::Jellyfin, AccountStatus::NotSetUp)).then_some(())
        })
        .await;

        // Quick Connect: code shown, approved on second poll, token exchanged.
        cmd.send(Command::SignIn(SourceKind::Jellyfin)).unwrap();
        let code = wait_for(&mut ev, |e| match e {
            AppEvent::SignIn(_, SignInState::Code(c)) => Some(c.clone()),
            _ => None,
        })
        .await;
        assert_eq!(code, "424242");
        wait_for(&mut ev, |e| matches!(e, AppEvent::SignIn(_, SignInState::Done)).then_some(())).await;
        let saved = Config::load_or_create_at(&config_path).unwrap().config;
        assert_eq!(saved.jellyfin.user_id, "u1", "user id persisted to config");

        // Continue Watching (via the pre-10.9 fallback route) and posters.
        let items = wait_for(&mut ev, |e| match e {
            AppEvent::ContinueWatching(SourceKind::Jellyfin, items) => Some(items.clone()),
            _ => None,
        })
        .await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].resume_ms, Some(600_000));
        let mut posters = HashSet::new();
        while posters.len() < 2 {
            let id = wait_for(&mut ev, |e| match e {
                AppEvent::Poster { item_id, image, .. } => {
                    assert_eq!((image.width, image.height), (4, 6));
                    Some(item_id.clone())
                }
                _ => None,
            })
            .await;
            posters.insert(id);
        }

        // Play → position advances from the resume point.
        cmd.send(Command::Play(items[0].clone())).unwrap();
        wait_for(&mut ev, |e| match e {
            AppEvent::Player(Some(s)) if s.position_ms >= 602_000 && s.duration_ms == Some(2_700_000) => Some(()),
            _ => None,
        })
        .await;

        // Pause / seek / resume / stop from the laptop.
        cmd.send(Command::Pause).unwrap();
        wait_for(&mut ev, |e| matches!(e, AppEvent::Player(Some(s)) if s.state == PlayState::Paused).then_some(()))
            .await;
        cmd.send(Command::Seek(30)).unwrap();
        cmd.send(Command::Resume).unwrap();
        wait_for(&mut ev, |e| matches!(e, AppEvent::Player(Some(s)) if s.state == PlayState::Playing).then_some(()))
            .await;
        cmd.send(Command::Stop).unwrap();
        wait_for(&mut ev, |e| matches!(e, AppEvent::Player(None)).then_some(())).await;
        sleep(Duration::from_millis(500)).await; // let the Stopped report land

        // What Jellyfin saw.
        let jf = read_log(&jf_log);
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
        let mpv = read_log(&mpv_log);
        let argv: Vec<String> = serde_json::from_value(mpv[0]["argv"].clone()).unwrap();
        assert!(argv.contains(&"--start=600.000".into()));
        assert!(argv.contains(&"--fs-screen-name=HDMI-A-1".into()));
        assert!(argv.last().unwrap().contains("/Videos/ep1/stream?static=true&api_key=tok"));
        let cmds: Vec<&Value> = mpv.iter().filter_map(|l| l.get("command")).collect();
        let has = |want: Value| cmds.iter().any(|c| **c == want);
        assert!(has(serde_json::json!(["set_property", "pause", true])));
        assert!(has(serde_json::json!(["seek", 30, "relative"])));
        assert!(has(serde_json::json!(["quit"])));
        let overlay = cmds.iter().find(|c| c[0] == "show-text" && c[1].as_str().unwrap().contains("Paused")).unwrap();
        assert!(overlay[1].as_str().unwrap().starts_with("Some Show · S1 · E1 · The Pilot\nPaused · "));

        std::fs::remove_dir_all(&dir).ok();
    }

    fn uuid_like() -> String {
        format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
    }
}
