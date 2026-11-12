//! tvdash: laptop control dashboard + TV playback window, one process.

mod bridge;
mod callbacks;
mod clock;
mod controller;
mod displays;
mod format;
mod inhibit;
mod instance;
mod logos;
mod shelves;
mod tint;
mod tv_nav;
mod video;
mod watch;
mod watch_ui;

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
  --check-keyring   Test GNOME Keyring read/write, list which secrets are set, and exit
  --set-secret KEY  Read a value from stdin and store it in the keyring, e.g.
                    tvdash --set-secret tmdb_api_key
  --clear-secret KEY  Remove a secret from the keyring
                    KEY: plex_token, tmdb_api_key
  -h, --help        Show this help

Logging: set RUST_LOG (default: info), e.g. RUST_LOG=tvdash=debug";

#[derive(Debug, Default)]
struct Args {
    windowed: bool,
    home: bool,
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
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--windowed" => args.windowed = true,
            "--home" => args.home = true,
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
    if matches!(instance::send(&socket, instance::Message::Ping), Ok(true)) {
        println!("tvdash is already running.");
        return Ok(());
    }

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
            sites: config.streaming.clone(),
        },
    );

    // Background → UI events.
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();
    runtime.spawn(bridge::forward(event_rx, laptop.as_weak(), tv.as_weak()));

    // Header widgets (weather).
    let (general_tx, general_rx) = tokio::sync::watch::channel(config.general.clone());
    let _widgets = {
        let _rt = runtime.enter();
        tvdash_widgets::spawn(&config, tvdash_widgets::Endpoints::default(), event_tx.clone(), general_rx)
    };

    // UI → background commands, handled by the controller.
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    let (mut controller, internal_rx) =
        controller::Controller::new((*config).clone(), loaded.path.clone(), args.windowed, event_tx.clone());
    controller.set_general_tx(general_tx);
    let layout = args.layout.unwrap_or(config.display.layout);
    info!(?layout, "screen layout");
    controller.set_layout(layout);
    // Video inside the window (TV-only layout) needs libmpv; without it mpv opens its own window.
    match tvdash_player::libmpv::load() {
        Ok(version) => {
            info!(version, "libmpv loaded: videos play inside the window in the TV-only layout");
            let slot = tvdash_player::libmpv::VideoSlot::new();
            video::install(&laptop, slot.clone(), cmd_tx.clone());
            controller.set_video(slot);
        }
        Err(e) => warn!(error = %e, "videos play in mpv's own window (install mpv-libs for in-window video)"),
    }
    runtime.spawn(controller.run(cmd_rx, internal_rx));
    callbacks::wire(&laptop, cmd_tx.clone());

    tv_nav::init(&laptop, layout);

    // `tvdash --home` (and refusing a second copy).
    {
        let _rt = runtime.enter();
        let (tx, weak) = (cmd_tx.clone(), laptop.as_weak());
        let listening = instance::listen(socket.clone(), move |m| {
            if m != instance::Message::Home {
                return;
            }
            let _ = tx.send(Command::Home);
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(l) = weak.upgrade() {
                    l.global::<Nav>().set_page("home".into());
                    tv_nav::go_home(&l);
                }
            });
        });
        if let Err(e) = listening {
            warn!(error = %e, "no instance socket; tvdash --home won't work");
        }
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
        window.on_close_requested(|| {
            let _ = slint::quit_event_loop();
            CloseRequestResponse::HideWindow
        });
    }

    displays::remember(&tv, &config.display, !args.windowed, cmd_tx.clone());
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
