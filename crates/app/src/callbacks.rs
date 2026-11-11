//! UI callbacks → `Command`s (or UI-only navigation). Runs on the UI thread.

use slint::ComponentHandle;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, warn};
use tvdash_core::{Command, models::SourceKind};

use crate::{Accounts, Dash, LaptopWindow, Library, Nav, Setup, SiteEditor, Watch, bridge, watch_ui};
use tvdash_core::config::StreamingSite;

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
    wire_site_editor(laptop, send.clone());
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

fn wire_watch(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let w = laptop.global::<Watch>();

    let (weak, s) = (laptop.as_weak(), send.clone());
    w.on_edited(move |q| {
        if let Some(l) = weak.upgrade() {
            let s = s.clone();
            watch_ui::debounce(&l, q.to_string(), move |q| s(Command::WhereToWatch(q)));
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    w.on_search(move |q| {
        if let Some(l) = weak.upgrade()
            && watch_ui::start(&l, &q)
        {
            s(Command::WhereToWatch(q.trim().to_owned()));
        }
    });

    let weak = laptop.as_weak();
    w.on_select(move |key| {
        if let Some(l) = weak.upgrade() {
            watch_ui::select(&l, &key);
        }
    });

    let weak = laptop.as_weak();
    w.on_close_detail(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<Watch>().set_detail_open(false);
        }
    });

    let weak = laptop.as_weak();
    w.on_action(move |a| {
        let Some(l) = weak.upgrade() else { return };
        let Some(hit) = watch_ui::hit(&l.global::<Watch>().get_selected()) else { return };
        match a.kind.as_str() {
            "play" => {
                if let Some(item) = hit.library {
                    send(Command::Play(item));
                    l.global::<Nav>().set_page("home".into());
                }
            }
            "library" => {
                let lib = l.global::<Library>();
                lib.set_query(hit.title.as_str().into());
                if bridge::start_search(&l, &hit.title) {
                    send(Command::Search(hit.title.clone()));
                }
                l.global::<Nav>().set_page("library".into());
            }
            "open" if a.site >= 0 => {
                send(Command::OpenOnService { site: a.site as usize, title: hit.title });
            }
            "tmdb" => {
                if let Some(url) = hit.link {
                    send(Command::OpenUrl { label: "TMDB".into(), url });
                }
            }
            other => warn!(kind = other, "unknown Where to watch action"),
        }
    });
}

/// Parse "8, 1796 15" into provider IDs.
pub fn parse_providers(text: &str) -> Result<Vec<u32>, String> {
    text.split([',', ' ', ';'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<u32>().map_err(|_| format!("“{t}” isn't a TMDB provider ID (a number).")))
        .collect()
}

fn wire_site_editor(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let ed = laptop.global::<SiteEditor>();

    let weak = laptop.as_weak();
    ed.on_add(move || {
        let Some(l) = weak.upgrade() else { return };
        let ed = l.global::<SiteEditor>();
        ed.set_index(-1);
        for set in [SiteEditor::set_name, SiteEditor::set_url, SiteEditor::set_search, SiteEditor::set_providers] {
            set(&ed, "".into());
        }
        ed.set_url("https://".into());
        ed.set_error("".into());
        ed.set_note("".into());
        ed.set_confirm_remove(false);
        ed.set_open(true);
    });

    let weak = laptop.as_weak();
    ed.on_edit(move |index| {
        let Some(l) = weak.upgrade() else { return };
        let Some(site) = bridge::sites().get(index as usize).cloned() else { return };
        let ed = l.global::<SiteEditor>();
        ed.set_index(index);
        ed.set_name(site.name.into());
        ed.set_url(site.url.into());
        ed.set_search(site.search_url.unwrap_or_default().into());
        let ids: Vec<String> = site.tmdb_providers.iter().map(u32::to_string).collect();
        ed.set_providers(ids.join(", ").into());
        ed.set_error("".into());
        ed.set_note("".into());
        ed.set_confirm_remove(false);
        ed.set_clear_cookies(true);
        ed.set_open(true);
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    ed.on_save(move || {
        let Some(l) = weak.upgrade() else { return };
        let ed = l.global::<SiteEditor>();
        let index = (ed.get_index() >= 0).then_some(ed.get_index() as usize);
        let all = bridge::sites();
        let result = parse_providers(&ed.get_providers()).and_then(|tmdb_providers| {
            let site = StreamingSite {
                name: ed.get_name().trim().to_owned(),
                url: ed.get_url().trim().to_owned(),
                search_url: Some(ed.get_search().trim().to_owned()).filter(|u| !u.is_empty()),
                tmdb_providers,
                hidden: index.and_then(|i| all.get(i)).is_some_and(|x| x.hidden),
            };
            let others: Vec<&StreamingSite> =
                all.iter().enumerate().filter(|(i, _)| Some(*i) != index).map(|(_, x)| x).collect();
            site.validate(&others).map(|()| site)
        });
        match result {
            Ok(site) => {
                s(Command::SaveSite { index, site });
                ed.set_open(false);
            }
            Err(e) => ed.set_error(e.into()),
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    ed.on_test(move || {
        let Some(l) = weak.upgrade() else { return };
        let ed = l.global::<SiteEditor>();
        let url = ed.get_url().trim().to_owned();
        if url.starts_with("https://") || url.starts_with("http://") {
            ed.set_error("".into());
            s(Command::OpenUrl { label: "Test".into(), url });
        } else {
            ed.set_error("Enter a start page (https://…) to test.".into());
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    ed.on_remove(move || {
        let Some(l) = weak.upgrade() else { return };
        let ed = l.global::<SiteEditor>();
        if ed.get_index() >= 0 {
            s(Command::RemoveSite { index: ed.get_index() as usize, clear_cookies: ed.get_clear_cookies() });
        }
        ed.set_confirm_remove(false);
        ed.set_open(false);
    });

    let s = send.clone();
    ed.on_move(move |from, to| {
        if from >= 0 && to >= 0 {
            s(Command::MoveSite { from: from as usize, to: to as usize });
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    ed.on_find(move || {
        let Some(l) = weak.upgrade() else { return };
        let ed = l.global::<SiteEditor>();
        let name = ed.get_name().trim().to_owned();
        if name.is_empty() {
            ed.set_note("Type the tile name first.".into());
        } else {
            ed.set_note("Looking it up on TMDB…".into());
            s(Command::FindProviders(name));
        }
    });

    ed.on_set_hidden(move |index, hidden| {
        if index >= 0 {
            send(Command::SetSiteHidden(index as usize, hidden));
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn provider_ids() {
        assert_eq!(super::parse_providers("8, 1796 15;"), Ok(vec![8, 1796, 15]));
        assert_eq!(super::parse_providers(""), Ok(vec![]));
        assert!(super::parse_providers("8, netflix").unwrap_err().contains("netflix"));
    }
}
