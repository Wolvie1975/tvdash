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
use tvdash_core::{AppEvent, Command, Config, SecretKey, models::SourceKind, secrets};

slint::include_modules!();

const USAGE: &str = "\
Usage: tvdash [OPTIONS]

Options:
  --windowed        Don't go full screen (development on a single display)
  --check-keyring   Test GNOME Keyring read/write, list which secrets are set, and exit
  --set-secret KEY  Read a value from stdin and store it in the keyring, e.g.
                    tvdash --set-secret calendar_ics_url
  --clear-secret KEY  Remove a secret from the keyring
                    KEY: plex_token, jellyfin_token, calendar_ics_url,
                         portainer_api_key, uptime_kuma_api_key
  -h, --help        Show this help

Logging: set RUST_LOG (default: info), e.g. RUST_LOG=tvdash=debug";

#[derive(Debug, Default)]
struct Args {
    windowed: bool,
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
    if let Some(key) = args.set_secret {
        return set_secret(key);
    }
    if let Some(key) = args.clear_secret {
        secrets::delete(key)?;
        println!("{} removed from the keyring", key.as_str());
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

    slint::BackendSelector::new().backend_name("winit".into()).select().context("selecting the Slint winit backend")?;

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
    wire_callbacks(&laptop, cmd_tx.clone());

    // Header widgets: weather, calendar, homelab (each on its own interval).
    {
        let _rt = runtime.enter();
        let reader: tvdash_widgets::SecretReader = Arc::new(|k| secrets::get(k).ok().flatten());
        tvdash_widgets::spawn(&config, reader, tvdash_widgets::Endpoints::default(), event_tx.clone());
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
    displays::place(&tv, &laptop, &config.display, !args.windowed, cmd_tx);

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
    laptop.on_prompt_action(move |source, action| match (parse_source(&source), action.as_str()) {
        (Some(kind), "sign-in") => s(Command::SignIn(kind)),
        (Some(_), "retry") => s(Command::Refresh),
        _ => warn!(%source, %action, "unknown prompt action"),
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

    let weak = laptop.as_weak();
    laptop.on_calendar_action(move || {
        if let Some(l) = weak.upgrade() {
            bridge::long_toast(
                &l,
                "Paste Google Calendar's secret iCal address with: tvdash --set-secret calendar_ics_url \
                 (the setup screen arrives in M6)",
            );
        }
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
