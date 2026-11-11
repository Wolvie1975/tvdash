//! UI callbacks → `Command`s (or UI-only navigation). Runs on the UI thread.

use slint::ComponentHandle;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, warn};
use tvdash_core::{Command, models::SourceKind};

use crate::{Accounts, Dash, LaptopWindow, Library, Nav, Setup, bridge};

fn parse_source(s: &str) -> Option<SourceKind> {
    match s {
        "jellyfin" => Some(SourceKind::Jellyfin),
        "plex" => Some(SourceKind::Plex),
        _ => None,
    }
}

pub fn wire(laptop: &LaptopWindow, cmd_tx: UnboundedSender<Command>) {
    let send = move |cmd: Command| {
        if cmd_tx.send(cmd).is_err() {
            error!("command handler stopped");
        }
    };
    wire_dash(laptop, send.clone());
    wire_library(laptop, send.clone());
    wire_accounts(laptop, send.clone());
    wire_setup(laptop, send);
}

fn wire_dash(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let dash = laptop.global::<Dash>();

    let s = send.clone();
    dash.on_open_site(move |i| s(Command::OpenSite(i as usize)));

    let s = send.clone();
    dash.on_play_item(move |i| match bridge::row(i as usize) {
        Some(item) => s(Command::Play(item)),
        None => warn!(index = i, "play: no such card"),
    });

    let s = send.clone();
    dash.on_prompt_action(move |source, action| match (parse_source(&source), action.as_str()) {
        (Some(kind), "sign-in") => s(Command::SignIn(kind)),
        (Some(_), "retry") => s(Command::Refresh),
        _ => warn!(%source, %action, "unknown prompt action"),
    });

    let s = send.clone();
    dash.on_cancel_sign_in(move |source| {
        if let Some(kind) = parse_source(&source) {
            s(Command::CancelSignIn(kind));
        }
    });

    let weak = laptop.as_weak();
    dash.on_transport(move |action| {
        let paused = weak.upgrade().is_some_and(|l| l.global::<Dash>().get_paused());
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

    dash.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

fn wire_library(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let lib = laptop.global::<Library>();

    let weak = laptop.as_weak();
    lib.on_set_filter(move |f| {
        if let Some(l) = weak.upgrade() {
            bridge::set_filter(&l, &f);
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_search(move |q| {
        if let Some(l) = weak.upgrade()
            && bridge::start_search(&l, &q)
        {
            s(Command::Search(q.trim().to_owned()));
        }
    });

    let weak = laptop.as_weak();
    lib.on_clear_search(move || {
        if let Some(l) = weak.upgrade() {
            bridge::clear_search(&l);
        }
    });

    let weak = laptop.as_weak();
    lib.on_select(move |section, i| {
        if let Some(l) = weak.upgrade() {
            bridge::select(&l, &section, i as usize);
        }
    });

    let weak = laptop.as_weak();
    lib.on_play_detail(move |from_start| {
        let Some(mut item) = bridge::selected() else { return };
        if from_start {
            item.resume_ms = None;
        }
        send(Command::Play(item));
        // Back to the dashboard, where the transport buttons are.
        if let Some(l) = weak.upgrade() {
            bridge::close_detail(&l);
            l.global::<Nav>().set_page("home".into());
        }
    });

    let weak = laptop.as_weak();
    lib.on_close_detail(move || {
        if let Some(l) = weak.upgrade() {
            bridge::close_detail(&l);
        }
    });
}

fn wire_accounts(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let acc = laptop.global::<Accounts>();

    let s = send.clone();
    acc.on_action(move |id, action| {
        let cmd = match (id.as_str(), action.as_str(), parse_source(&id)) {
            (_, "sign-in", Some(kind)) => Command::SignIn(kind),
            (_, "sign-out", Some(kind)) => Command::SignOut(kind),
            (_, "cancel", Some(kind)) => Command::CancelSignIn(kind),
            (_, "retry", _) => Command::Refresh,
            _ => {
                warn!(%id, %action, "unknown account action");
                return;
            }
        };
        s(cmd);
    });

    let s = send.clone();
    acc.on_open_site(move |i| s(Command::OpenSite(i as usize)));

    let s = send.clone();
    acc.on_clear_profile(move || s(Command::ClearBrowserProfile));

    let weak = laptop.as_weak();
    acc.on_run_setup(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<Setup>().set_step(0);
            l.global::<Nav>().set_page("setup".into());
            bridge::sync_setup(&l);
        }
    });
}

fn wire_setup(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let setup = laptop.global::<Setup>();

    let (weak, s) = (laptop.as_weak(), send.clone());
    setup.on_sign_in(move |source| {
        let Some(kind) = parse_source(&source) else { return };
        if kind == SourceKind::Jellyfin
            && let Some(l) = weak.upgrade()
        {
            s(Command::JellyfinServer(l.global::<Setup>().get_jellyfin_url().to_string()));
        }
        s(Command::SignIn(kind));
    });

    let s = send.clone();
    setup.on_cancel(move |source| {
        if let Some(kind) = parse_source(&source) {
            s(Command::CancelSignIn(kind));
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    setup.on_jellyfin_password(move |username, password| {
        if let Some(l) = weak.upgrade() {
            s(Command::JellyfinServer(l.global::<Setup>().get_jellyfin_url().to_string()));
        }
        s(Command::JellyfinPassword { username: username.to_string(), password: password.to_string() });
    });

    let weak = laptop.as_weak();
    setup.on_finish(move || {
        send(Command::SetupFinished);
        if let Some(l) = weak.upgrade() {
            l.global::<Nav>().set_page("home".into());
        }
    });
}
