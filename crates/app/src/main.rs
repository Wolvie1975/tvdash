//! tvdash: laptop control dashboard + TV playback window, one process.

mod bridge;
mod clock;
mod displays;

use std::{sync::Arc, time::Duration};

use anyhow::{Context, bail};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tvdash_core::{AppEvent, Command, Config, secrets};

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
    let config = Arc::new(loaded.config);

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

    // UI → background commands.
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    runtime.spawn(handle_commands(cmd_rx, config.clone()));
    wire_callbacks(&laptop, cmd_tx);

    // Background → UI events.
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AppEvent>();
    runtime.spawn(bridge::forward(event_rx, laptop.as_weak(), tv.as_weak()));

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

/// M1: log commands. M2 wires mpv + media sources, M4 the browser launcher.
async fn handle_commands(mut rx: mpsc::UnboundedReceiver<Command>, config: Arc<Config>) {
    while let Some(cmd) = rx.recv().await {
        match &cmd {
            Command::OpenSite(i) => match config.streaming.get(*i) {
                Some(site) => info!(site = %site.name, url = %site.url, "open site requested (browser launcher lands in M4)"),
                None => warn!(index = i, "open site: no such streaming entry"),
            },
            other => info!(command = ?other, "command received (player lands in M2)"),
        }
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
