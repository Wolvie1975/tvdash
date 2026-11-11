//! tvdash: laptop control dashboard + TV playback window, one process.

mod bridge;
mod clock;
mod controller;
mod displays;
mod format;

use std::{sync::Arc, time::Duration};

use anyhow::{Context, bail};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tvdash_core::{AppEvent, Command, Config, models::SourceKind, secrets};

slint::include_modules!();

const USAGE: &str = "\
Usage: tvdash [OPTIONS]

Options:
  --windowed        Don't go full screen (development on a single display)
  --check-keyring   Test GNOME Keyring read/write, list which secrets are set, and exit
  -h, --help        Show this help

Logging: set RUST_LOG (default: info), e.g. RUST_LOG=tvdash=debug";

#[derive(Debug, Default)]
struct Args {
    windowed: bool,
    check_keyring: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args::default();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--windowed" => args.windowed = true,
            "--check-keyring" => args.check_keyring = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}\n\n{USAGE}"),
        }
    }
    Ok(args)
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = parse_args()?;
    if args.check_keyring {
        return check_keyring();
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

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()
        .context("selecting the Slint winit backend")?;

    let tv = TvWindow::new()?;
    let laptop = LaptopWindow::new()?;

    let sites: Vec<SharedString> = config.streaming.iter().map(|s| s.name.as_str().into()).collect();
    laptop.set_streaming_sites(ModelRc::new(VecModel::from(sites)));

    bridge::init(&laptop, config.general.tz(), config.general.clock);

    // Background → UI events.
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();
    runtime.spawn(bridge::forward(event_rx, laptop.as_weak(), tv.as_weak()));

    // UI → background commands, handled by the controller.
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    let (controller, internal_rx) =
        controller::Controller::new((*config).clone(), loaded.path.clone(), args.windowed, event_tx.clone());
    runtime.spawn(controller.run(cmd_rx, internal_rx));
    wire_callbacks(&laptop, cmd_tx);

    // M1 done-check: keyring round-trip, reported in the laptop footer.
    {
        let tx = event_tx.clone();
        runtime.spawn_blocking(move || {
            let result = secrets::self_test().map_err(|e| e.to_string());
            match &result {
                Ok(()) => info!("keyring self-test passed"),
                Err(e) => warn!(error = %e, "keyring self-test failed"),
            }
            let _ = tx.send(AppEvent::KeyringStatus(result));
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

    // TV first so it lands on the primary display even if output matching fails.
    tv.show()?;
    laptop.show()?;
    displays::place(&tv, &laptop, &config.display, !args.windowed);

    slint::run_event_loop_until_quit()?;

    drop(event_tx);
    runtime.shutdown_timeout(Duration::from_secs(1));
    Ok(())
}

fn wire_callbacks(laptop: &LaptopWindow, cmd_tx: mpsc::UnboundedSender<Command>) {
    let send = move |cmd: Command| {
        if cmd_tx.send(cmd).is_err() {
            error!("command handler stopped");
        }
    };

    let s = send.clone();
    laptop.on_open_site(move |i| s(Command::OpenSite(i as usize)));

    let s = send.clone();
    laptop.on_play_item(move |i| match bridge::row(i as usize) {
        Some(item) => s(Command::Play(item)),
        None => warn!(index = i, "play: no such card"),
    });

    let s = send.clone();
    laptop.on_sign_in(move |source| match parse_source(&source) {
        Some(kind) => s(Command::SignIn(kind)),
        None => warn!(%source, "sign-in: unknown source"),
    });

    let s = send.clone();
    laptop.on_cancel_sign_in(move |source| {
        if let Some(kind) = parse_source(&source) {
            s(Command::CancelSignIn(kind));
        }
    });

    let laptop_weak = laptop.as_weak();
    laptop.on_transport(move |action| {
        let paused = laptop_weak.upgrade().is_some_and(|l| l.get_paused());
        let cmd = match action.as_str() {
            "back" => Command::Seek(-10),
            "forward" => Command::Seek(30),
            "play-pause" if paused => Command::Resume,
            "play-pause" => Command::Pause,
            "stop" => Command::Stop,
            other => {
                warn!(action = other, "unknown transport action");
                return;
            }
        };
        send(cmd);
    });

    laptop.on_open_settings(|| info!("Settings › Accounts arrives in M6"));
    laptop.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

fn parse_source(s: &str) -> Option<SourceKind> {
    match s {
        "jellyfin" => Some(SourceKind::Jellyfin),
        "plex" => Some(SourceKind::Plex),
        _ => None,
    }
}

fn check_keyring() -> anyhow::Result<()> {
    secrets::self_test().context("keyring self-test failed")?;
    println!("keyring: write/read/delete OK (service \"{}\")", tvdash_core::APP_NAME);
    for (key, present) in secrets::presence()? {
        println!("  {:<20} {}", key.as_str(), if present { "set" } else { "—" });
    }
    Ok(())
}
