//! tvdash: laptop control dashboard + TV playback window, one process.

mod apps;
mod brand;
mod bridge;
mod callbacks;
mod clock;
mod controller;
mod displays;
mod focus;
mod format;
mod home_cmd;
mod image_cache;
mod instance;
mod intro;
mod next_game;
mod portal;
mod recent;
mod scoreboard_view;
mod shell_tv;
mod sleep;
mod sports_alerts;
mod sports_follows;
mod sports_watch;
mod tint;
mod tv_nav;
mod up_next;
mod video;
mod watch;
mod watch_ui;
mod whats_on;

use std::{sync::Arc, time::Duration};

use anyhow::{Context, bail};
use slint::{CloseRequestResponse, ComponentHandle};
use tokio::sync::mpsc;
use tracing::{info, warn};
use tvdash_core::{AppEvent, Command, Config, SecretKey, secrets};

slint::include_modules!();

const USAGE: &str = "\
Usage: tvdash [OPTIONS]

Options:
  --windowed        Don't go full screen (development on a single display)
  --layout tv|dual  Override [display] layout for this run
  --home            Tell the running tvdash to go Home (stops what's on the TV);
                    bind it to a GNOME keyboard shortcut
  --sleep [ARG]     Control the sleep timer: MINUTES (1-480), end, now, off, or status (default)
  --alert-test KIND Show a sample sports alert card: soon or final
  --alerts [status] Print followed teams, starred count, and last poll status
  --check-keyring   Test GNOME Keyring read/write, list which secrets are set, and exit
  --set-secret KEY  Read a value from stdin and store it in the keyring, e.g.
                    tvdash --set-secret tmdb_api_key
  --clear-secret KEY  Remove a secret from the keyring
                    KEY: plex_token, tmdb_api_key, tautulli_api_key, events_sql_password
  -h, --help        Show this help

Logging: set RUST_LOG (default: info), e.g. RUST_LOG=tvdash=debug";

#[derive(Debug, Default)]
struct Args {
    windowed: bool,
    home: bool,
    sleep: Option<String>,
    alert_test: Option<String>,
    alerts_status: bool,
    layout: Option<tvdash_core::config::Layout>,
    check_keyring: bool,
    set_secret: Option<SecretKey>,
    clear_secret: Option<SecretKey>,
}

fn secret_arg(value: Option<String>) -> anyhow::Result<SecretKey> {
    let name = value.ok_or_else(|| anyhow::anyhow!("missing KEY\n\n{USAGE}"))?;
    SecretKey::parse(&name).ok_or_else(|| anyhow::anyhow!("unknown secret {name:?}\n\n{USAGE}"))
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1).peekable();
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--windowed" => args.windowed = true,
            "--home" => args.home = true,
            "--sleep" => {
                let next = match argv.peek() {
                    Some(v) if !v.starts_with('-') => argv.next().unwrap(),
                    _ => "status".into(),
                };
                args.sleep = Some(next);
            }
            "--alert-test" => {
                let kind = argv.next().unwrap_or_else(|| "soon".into());
                if !matches!(kind.as_str(), "soon" | "final") {
                    bail!("--alert-test takes soon or final\n\n{USAGE}");
                }
                args.alert_test = Some(kind);
            }
            "--alerts" => {
                if let Some(v) = argv.peek()
                    && !v.starts_with('-')
                {
                    let sub = argv.next().unwrap();
                    if sub != "status" {
                        bail!("--alerts takes status\n\n{USAGE}");
                    }
                }
                args.alerts_status = true;
            }
            "--layout" => {
                let v = argv.next().unwrap_or_default();
                args.layout = Some(
                    tvdash_core::config::Layout::parse(&v)
                        .ok_or_else(|| anyhow::anyhow!("--layout takes tv or dual\n\n{USAGE}"))?,
                );
            }
            "--check-keyring" => args.check_keyring = true,
            "--set-secret" => args.set_secret = Some(secret_arg(argv.next())?),
            "--clear-secret" => args.clear_secret = Some(secret_arg(argv.next())?),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}\n\n{USAGE}"),
        }
    }
    Ok(args)
}

/// `~/.local/state/tvdash/tvdash.log` (the previous run's is kept as `tvdash.log.1`).
pub fn log_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().join(".local/state")))?;
    Some(base.join("tvdash").join("tvdash.log"))
}

/// Log to stderr and to the log file (shown in Settings › Diagnostics).
fn init_logging() {
    use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file = log_path().and_then(|path| {
        std::fs::create_dir_all(path.parent()?).ok()?;
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
        std::fs::File::create(&path).ok()
    });
    let file_layer = file.map(|f| {
        tracing_subscriber::fmt::layer().with_ansi(false).with_writer(std::sync::Mutex::new(f)).with_filter(filter())
    });
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_filter(filter()))
        .with(file_layer)
        .init();
}

fn main() -> anyhow::Result<()> {
    init_logging();

    let args = parse_args()?;
    if args.check_keyring {
        return check_keyring();
    }
    if let Some(key) = args.set_secret {
        return set_secret(key);
    }
    if let Some(key) = args.clear_secret {
        secrets::delete(key)?;
        println!("{} removed from the keyring", key.as_str());
        return Ok(());
    }

    let socket = instance::socket_path();
    if args.home {
        return match instance::send(&socket, instance::Message::Home) {
            Ok(true) => Ok(()),
            Ok(false) => bail!("tvdash isn't running"),
            Err(e) => Err(e).context("talking to the running tvdash"),
        };
    }
    if let Some(arg) = args.sleep {
        return match instance::send_reply(&socket, instance::Message::Sleep(arg.clone())) {
            Ok(Some(reply)) => {
                if let Some(err) = reply.strip_prefix("error:") {
                    bail!("{}", err.trim());
                }
                if arg == "status" {
                    println!("{reply}");
                }
                Ok(())
            }
            Ok(None) => bail!("tvdash isn't running"),
            Err(e) => Err(e).context("talking to the running tvdash"),
        };
    }
    if args.alerts_status {
        return match instance::send_reply(&socket, instance::Message::AlertsStatus) {
            Ok(Some(reply)) => {
                println!("{reply}");
                Ok(())
            }
            Ok(None) => bail!("tvdash isn't running"),
            Err(e) => Err(e).context("talking to the running tvdash"),
        };
    }
    if let Some(kind) = args.alert_test {
        return match instance::send(&socket, instance::Message::AlertTest(kind)) {
            Ok(true) => Ok(()),
            Ok(false) => bail!("tvdash isn't running"),
            Err(e) => Err(e).context("talking to the running tvdash"),
        };
    }
    if matches!(instance::send(&socket, instance::Message::Ping), Ok(true)) {
        println!("tvdash is already running.");
        return Ok(());
    }
    let Some(_lock) = instance::acquire_lock(Duration::from_secs(2)) else {
        println!("tvdash is already running.");
        return Ok(());
    };

    let loaded = Config::load_or_create().context("loading config")?;
    if loaded.created {
        info!(path = %loaded.path.display(), "created default config; edit it to set outputs and location");
    } else {
        info!(path = %loaded.path.display(), "loaded config");
    }
    let config = Arc::new(loaded.config.clone());

    // All network/IPC work runs here; the UI thread only receives finished data.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("tvdash-rt")
        .build()
        .context("starting tokio runtime")?;

    // Desktop OpenGL rather than GLES (Slint's default): mpv's hardware decoding (VAAPI)
    // needs it for video inside the window. Slint falls back if the driver says no.
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .require_opengl()
        .select()
        .context("selecting the Slint winit backend")?;

    let startup_state = runtime.block_on(displays::startup_display_state_dbus());
    let decision = match &startup_state {
        Some((outputs, _)) => {
            info!(?outputs, "active outputs at startup");
            tvdash_core::layout::decide(
                config.display.layout,
                outputs,
                &config.display.tv_output,
                &config.display.laptop_output,
            )
        }
        None => {
            let d = tvdash_core::layout::decide(
                config.display.layout,
                &[],
                &config.display.tv_output,
                &config.display.laptop_output,
            );
            // Without Mutter D-Bus we don't know yet; assume TV is connected until winit checks.
            tvdash_core::Decision { tv_connected: true, ..d }
        }
    };
    let layout = args.layout.unwrap_or(tvdash_core::Layout::Tv);

    // Design units for TV-only are 1920x1080; scale 1440p/4K proportionally.
    if layout == tvdash_core::Layout::Tv && !args.windowed && std::env::var_os("SLINT_SCALE_FACTOR").is_none() {
        if let Some((_, heights)) = &startup_state {
            let h = heights
                .get(&config.display.tv_output)
                .copied()
                .or_else(|| {
                    heights
                        .iter()
                        .find(|(name, _)| *name != &config.display.laptop_output)
                        .map(|(_, &h)| h)
                })
                .or_else(|| heights.values().copied().max());
            if let Some(h) = h {
                if h > 0 {
                    let scale = h as f32 / 1080.0;
                    if (scale - 1.0).abs() > 0.01 {
                        info!(height = h, scale, "setting SLINT_SCALE_FACTOR for TV-only 1080p design units");
                        // SAFETY: called on the main thread before Slint creates any windows.
                        unsafe { std::env::set_var("SLINT_SCALE_FACTOR", format!("{scale:.3}")) };
                    }
                }
            }
        }
    }

    let tv = TvWindow::new()?;
    let laptop = LaptopWindow::new()?;

    bridge::init(
        &laptop,
        bridge::UiConfig {
            tz: config.general.tz(),
            clock: config.general.clock,
            location: config.general.location.clone(),
            seconds: config.general.seconds,
            paths: [
                ("Log file", log_path()),
                ("Settings", Some(loaded.path.clone())),
                ("App state", tvdash_core::state::default_path()),
                ("Where to watch cache", tvdash_core::state::default_path().map(|p| p.with_file_name("cache.db"))),
            ]
            .into_iter()
            .filter_map(|(what, p)| Some(format!("{what}: {}", p?.display())))
            .collect(),
            units: config.general.units,
            theme: config.general.theme,
            layout_setting: config.display.layout,
            dashboard_ui: config.display.dashboard_ui,
            tv_connected: decision.tv_connected,
            sites: config.streaming.clone(),
        },
    );
    bridge::set_night_config(&laptop, config.display.clone());
    bridge::set_recent_services(&laptop, config.tv.recent.clone());
    laptop.global::<Setup>().set_jellyfin_url(config.jellyfin.server_url.as_str().into());

    // Background → UI events.
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let (alert_http_tx, alert_http_rx) = tokio::sync::watch::channel(None);
    let (alert_status_tx, alert_status_rx) = tokio::sync::watch::channel("Alerts: starting…".to_string());
    runtime.spawn(bridge::forward(
        event_rx,
        laptop.as_weak(),
        tv.as_weak(),
        alert_http_tx,
        alert_status_tx,
    ));
    runtime.spawn(portal::watch_color_scheme(event_tx.clone()));

    // Header widgets (weather).
    let (general_tx, general_rx) = tokio::sync::watch::channel(config.general.clone());
    let _widgets = {
        let _rt = runtime.enter();
        tvdash_widgets::spawn(&config, tvdash_widgets::Endpoints::default(), event_tx.clone(), general_rx)
    };

    // UI → background commands, handled by the controller.
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    bridge::set_command_sender(cmd_tx.clone());
    let (mut controller, internal_rx) =
        controller::Controller::new((*config).clone(), loaded.path.clone(), args.windowed, event_tx.clone());
    controller.set_general_tx(general_tx);
    info!(?layout, tv_connected = decision.tv_connected, "screen layout");
    controller.set_layout(layout);
    // Video inside the window (TV-only layout) needs libmpv; without it mpv opens its own window.
    match tvdash_player::libmpv::load() {
        Ok(version) => {
            info!(version, "libmpv loaded: videos play inside the window");
            let slot = tvdash_player::libmpv::VideoSlot::new();
            video::install(&laptop, Some(&tv), slot.clone(), cmd_tx.clone());
            controller.set_video(slot);
        }
        Err(e) => warn!(error = %e, "videos play in mpv's own window (install mpv-libs for in-window video)"),
    }
    let (sleep_http_tx, sleep_http_rx) = tokio::sync::watch::channel(home_cmd::SleepHttpState::off());
    controller.set_sleep_tx(sleep_http_tx);
    runtime.spawn(controller.run(cmd_rx, internal_rx));
    callbacks::wire(&laptop, cmd_tx.clone());
    laptop.global::<SleepState>().set_warn_before(config.sleep.warn_before.clone().into());

    tv_nav::init(&laptop, layout);

    // `tvdash --home` / `tvdash --sleep` (and refusing a second copy).
    {
        let _rt = runtime.enter();
        let (tx, weak) = (cmd_tx.clone(), laptop.as_weak());
        let sleep_rx_sock = sleep_http_rx.clone();
        let listening = instance::listen(socket.clone(), move |m| match m {
            instance::Message::Ping => None,
            instance::Message::Home => {
                let _ = tx.send(Command::Home);
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(l) = weak.upgrade() {
                        l.global::<Nav>().set_page("home".into());
                        tv_nav::go_home(&l);
                        let _ = l.window().show();
                    }
                });
                None
            }
            instance::Message::Sleep(arg) => match arg.as_str() {
                "status" => {
                    let st = sleep_rx_sock.borrow();
                    if st.phase == "off" {
                        Some("Off".into())
                    } else if st.ends_at.is_empty() {
                        Some(st.label.clone())
                    } else {
                        Some(format!("{} · ends {}", st.label, st.ends_at))
                    }
                }
                "off" => {
                    let _ = tx.send(Command::SleepOff);
                    None
                }
                "now" => {
                    let _ = tx.send(Command::SleepNow);
                    None
                }
                "end" => {
                    if sleep_rx_sock.borrow().plex_playing {
                        let _ = tx.send(Command::SleepSetEndOfItem);
                        None
                    } else {
                        Some("error: end of item requires Plex playback".into())
                    }
                }
                other => match other.parse::<i64>() {
                    Ok(mins) if (1..=480).contains(&mins) => {
                        let _ = tx.send(Command::SleepSetMinutes(mins));
                        None
                    }
                    _ => Some(format!("error: invalid sleep argument {other:?} (use 1-480, end, now, off, status)")),
                },
            },
            instance::Message::AlertTest(kind) => {
                let _ = tx.send(Command::SportsAlertTest(kind));
                None
            }
            instance::Message::AlertsStatus => Some(alert_status_rx.borrow().clone()),
        });
        if let Err(e) = listening {
            warn!(error = %e, "no instance socket; tvdash --home won't work");
        }
        home_cmd::serve_pill(
            home_cmd::PILL_PORT,
            cmd_tx.clone(),
            laptop.as_weak(),
            sleep_http_rx,
            alert_http_rx,
        );
    }

    // M1 done-check: keyring round-trip, reported in the laptop footer.
    {
        let tx = event_tx.clone();
        runtime.spawn_blocking(move || {
            let result = secrets::self_test().map_err(|e| e.to_string());
            match &result {
                Ok(()) => info!("keyring self-test passed"),
                Err(e) => warn!(error = %e, "keyring self-test failed"),
            }
            let ok = result.is_ok();
            let _ = tx.send(AppEvent::KeyringStatus(result));
            // Calendar and homelab were removed; drop their old entries.
            if ok && let Err(e) = secrets::purge_retired() {
                warn!(error = %e, "couldn't remove old calendar/homelab keyring entries");
            }
        });
    }

    let _clock = clock::start(&laptop, &tv, &config.general);

    // Closing either window quits the app.
    for window in [laptop.window(), tv.window()] {
        let tx = cmd_tx.clone();
        window.on_close_requested(move || {
            callbacks::request_quit(&tx);
            CloseRequestResponse::HideWindow
        });
    }

    displays::remember(&tv, &config.display, !args.windowed, args.layout, cmd_tx.clone(), event_tx.clone());
    runtime.spawn(displays::watch(event_tx.clone()));
    match layout {
        tvdash_core::config::Layout::Dual => {
            // TV first so it lands on the primary display even if output matching fails.
            tv.show()?;
            laptop.show()?;
            displays::place(&tv, &laptop, &config.display, !args.windowed, cmd_tx);
        }
        tvdash_core::config::Layout::Tv => {
            // One window, full screen on the TV; the idle TV window isn't used.
            laptop.window().set_size(slint::LogicalSize::new(1920.0, 1080.0));
            laptop.show()?;
            displays::place_tv_only(&laptop, &config.display, !args.windowed, cmd_tx);
        }
    }
    if startup_state.is_none() {
        displays::on_displays_changed(&laptop);
    }

    intro::start(&laptop, config.display.dashboard_ui.into(), &config.startup);

    slint::run_event_loop_until_quit()?;
    instance::cleanup(&socket);

    drop(event_tx);
    runtime.shutdown_timeout(Duration::from_secs(1));
    Ok(())
}

fn set_secret(key: SecretKey) -> anyhow::Result<()> {
    use std::io::{IsTerminal, Read};
    if std::io::stdin().is_terminal() {
        eprint!("Paste the value for {} and press Enter: ", key.as_str());
    }
    let mut value = String::new();
    if std::io::stdin().is_terminal() {
        std::io::stdin().read_line(&mut value)?;
    } else {
        std::io::stdin().read_to_string(&mut value)?;
    }
    let value = value.trim();
    if value.is_empty() {
        bail!("empty value; nothing stored");
    }
    secrets::set(key, value).context("storing in the keyring")?;
    println!("{} stored in the keyring (service \"{}\")", key.as_str(), tvdash_core::APP_NAME);
    Ok(())
}

fn check_keyring() -> anyhow::Result<()> {
    secrets::self_test().context("keyring self-test failed")?;
    println!("keyring: write/read/delete OK (service \"{}\")", tvdash_core::APP_NAME);
    for (key, present) in secrets::presence()? {
        println!("  {:<20} {}", key.as_str(), if present { "set" } else { "—" });
    }
    Ok(())
}
