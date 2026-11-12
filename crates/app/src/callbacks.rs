//! UI callbacks → `Command`s (or UI-only navigation). Runs on the UI thread.

use slint::ComponentHandle;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, warn};
use tvdash_core::{Command, models::SourceKind};

use crate::{Accounts, Dash, LaptopWindow, Library, Nav, Setup, Watch, bridge, watch_ui};

fn parse_source(s: &str) -> Option<SourceKind> {
    match s {
        "plex" => Some(SourceKind::Plex),
        _ => None,
    }
}

pub fn wire(laptop: &LaptopWindow, cmd_tx: UnboundedSender<Command>) {
    bridge::set_commands(cmd_tx.clone());
    let send = move |cmd: Command| {
        if cmd_tx.send(cmd).is_err() {
            error!("command handler stopped");
        }
    };
    wire_dash(laptop, send.clone());
    wire_watch(laptop, send.clone());
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
    dash.on_play_card(move |tab, i| match bridge::tab_row(&tab, i as usize) {
        Some(item) => s(Command::Play(item)),
        None => warn!(%tab, index = i, "play: no such poster"),
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

    let s = send.clone();
    laptop.global::<crate::TvUi>().on_overlay(move || s(Command::Overlay));

    let (weak, s) = (laptop.as_weak(), send.clone());
    dash.on_transport(move |action| {
        let paused = weak.upgrade().is_some_and(|l| l.global::<Dash>().get_tv().paused);
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
        s(cmd);
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    laptop.global::<crate::Display>().on_changed(move || {
        let Some(l) = weak.upgrade() else { return };
        let d = l.global::<crate::Display>();
        use tvdash_core::config::{ClockFormat, Units};
        let units = if d.get_celsius() { Units::Celsius } else { Units::Fahrenheit };
        let clock = if d.get_h12() { ClockFormat::H12 } else { ClockFormat::H24 };
        bridge::set_display(&l, units, clock, d.get_seconds());
        s(Command::SetDisplay { units, clock, seconds: d.get_seconds() });
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    laptop.global::<crate::Display>().on_find_place(move |q| {
        let Some(l) = weak.upgrade() else { return };
        let d = l.global::<crate::Display>();
        if q.trim().is_empty() {
            return;
        }
        d.set_place_query(q.clone());
        d.set_place_searching(true);
        d.set_place_message("".into());
        s(Command::FindPlace(q.trim().to_owned()));
    });

    let weak = laptop.as_weak();
    laptop.global::<crate::Display>().on_set_layout(move |tv| {
        if let Some(l) = weak.upgrade() {
            let layout = if tv { tvdash_core::config::Layout::Tv } else { tvdash_core::config::Layout::Dual };
            crate::displays::switch_layout(&l, layout);
        }
    });

    let s = send.clone();
    laptop.global::<crate::Display>().on_pick_place(move |i| {
        if let Some(place) = bridge::place(i as usize) {
            s(Command::SetLocation(place));
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    dash.on_set_theme(move |theme| {
        let mode = bridge::theme_mode(theme);
        if let Some(l) = weak.upgrade() {
            bridge::set_theme(&l, mode);
        }
        s(Command::SetTheme(mode));
    });

    dash.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

fn wire_library(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let lib = laptop.global::<Library>();
    let browse = laptop.global::<crate::Browse>();

    // Opening the Library re-reads Plex (unless it was read a minute ago).
    let s = send.clone();
    laptop.global::<Nav>().on_opened(move |page| {
        if page == "library" {
            s(Command::RefreshLibrary);
        }
    });

    let weak = laptop.as_weak();
    lib.on_set_tab(move |t| {
        if let Some(l) = weak.upgrade() {
            bridge::set_tab(&l, &t);
        }
    });

    let weak = laptop.as_weak();
    lib.on_shuffle(move || {
        if let Some(l) = weak.upgrade() {
            bridge::shuffle(&l);
        }
    });

    let s = send.clone();
    lib.on_play_pick(move || {
        if let Some(item) = bridge::pick() {
            s(bridge::play_command(&item));
        }
    });

    let s = send.clone();
    lib.on_play_box(move |i| {
        if let Some(item) = bridge::box_item(i as usize) {
            s(bridge::play_command(&item));
        }
    });

    let weak = laptop.as_weak();
    lib.on_open_item(move |shelf, i| {
        if let Some(l) = weak.upgrade() {
            bridge::open_item(&l, &shelf, i as usize);
        }
    });

    let weak = laptop.as_weak();
    lib.on_browse(move |filter| {
        if let Some(l) = weak.upgrade() {
            bridge::browse(&l, &filter);
        }
    });

    let weak = laptop.as_weak();
    browse.on_select(move |i| {
        if let Some(l) = weak.upgrade() {
            bridge::select(&l, i as usize);
        }
    });

    let weak = laptop.as_weak();
    browse.on_set_unwatched(move |on| {
        if let Some(l) = weak.upgrade() {
            bridge::set_unwatched(&l, on);
        }
    });

    // Play from Browse (or the TV layout's rows); back to wherever the
    // transport buttons are.
    let play = {
        let weak = laptop.as_weak();
        let s = send.clone();
        move |from_start: bool| {
            let Some(mut item) = bridge::selected() else { return };
            if from_start {
                item.resume_ms = None;
            }
            s(bridge::play_command(&item));
            if let Some(l) = weak.upgrade()
                && l.global::<Nav>().get_layout() == "dual"
            {
                l.global::<Nav>().set_page("home".into());
            }
        }
    };
    browse.on_play(play.clone());
    lib.on_play_detail(play);
    lib.on_select(|section, i| bridge::select_row(&section, i as usize));
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
            ("tmdb", "remove", _) => Command::SetTmdbToken(String::new()),
            _ => {
                warn!(%id, %action, "unknown account action");
                return;
            }
        };
        s(cmd);
    });

    let s = send.clone();
    acc.on_submit(move |id, value| {
        if id == "tmdb" && !value.trim().is_empty() {
            s(Command::SetTmdbToken(value.to_string()));
        }
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

    let s = send.clone();
    setup.on_sign_in(move |source| {
        if let Some(kind) = parse_source(&source) {
            s(Command::SignIn(kind));
        }
    });

    let s = send.clone();
    setup.on_cancel(move |source| {
        if let Some(kind) = parse_source(&source) {
            s(Command::CancelSignIn(kind));
        }
    });

    let weak = laptop.as_weak();
    setup.on_finish(move || {
        send(Command::SetupFinished);
        if let Some(l) = weak.upgrade() {
            l.global::<Nav>().set_page("home".into());
        }
    });
}

fn wire_watch(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let w = laptop.global::<Watch>();

    let (weak, s) = (laptop.as_weak(), send.clone());
    w.on_edited(move |q| {
        if let Some(l) = weak.upgrade() {
            let s = s.clone();
            watch_ui::debounce(&l, q.to_string(), move |q| s(Command::WhereToWatch(q)));
        }
    });

    let weak = laptop.as_weak();
    w.on_close(move || {
        if let Some(l) = weak.upgrade() {
            watch_ui::close(&l);
        }
    });

    w.on_action(move |key| {
        let Some(hit) = watch_ui::hit(&key) else { return };
        let Some((_, _, action)) = watch_ui::action(&hit, watch_ui::resume_for_show) else { return };
        send(match action {
            watch_ui::Action::Play(item) => Command::Play(item),
            watch_ui::Action::PlayShow(show) => Command::PlayShow(show),
            watch_ui::Action::Open { site, title } => Command::OpenOnService { site, title },
        });
    });
}
