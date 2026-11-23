//! UI callbacks → `Command`s (or UI-only navigation). Runs on the UI thread.

use slint::ComponentHandle;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, warn};
use tvdash_core::{Command, models::SourceKind};

use crate::{
    Accounts, Artwork, CollectionPageState, Dash, Display, LaptopWindow, Library, Nav, Scoreboard, Server, Setup,
    ShowPageState, SiteEditor, SleepState, Sports, SportsAlertState, SportsAlertsSettings, Watch, bridge, watch_ui,
};
use tvdash_core::config::StreamingSite;

fn parse_source(s: &str) -> Option<SourceKind> {
    match s {
        "jellyfin" => Some(SourceKind::Jellyfin),
        "plex" => Some(SourceKind::Plex),
        _ => None,
    }
}

/// How long Quit waits for the controller to stop playback and close the browser
/// (Brave gets up to 5 s after SIGTERM) before ending the event loop anyway.
const QUIT_FALLBACK: std::time::Duration = std::time::Duration::from_secs(8);

/// Quit even if the controller never answers [`Command::Quit`] (e.g. it panicked).
pub fn quit_fallback() {
    slint::Timer::single_shot(QUIT_FALLBACK, || {
        tracing::warn!("controller didn't finish quitting in time; exiting anyway");
        let _ = slint::quit_event_loop();
    });
}

/// Window close / Settings › Quit: clean up through the controller, then exit.
pub fn request_quit(cmd_tx: &UnboundedSender<Command>) {
    if cmd_tx.send(Command::Quit).is_err() {
        let _ = slint::quit_event_loop(); // controller already gone
        return;
    }
    quit_fallback();
}

pub fn wire(laptop: &LaptopWindow, cmd_tx: UnboundedSender<Command>) {
    bridge::alerts::set_cmd_tx(cmd_tx.clone());
    let send = move |cmd: Command| {
        if cmd_tx.send(cmd).is_err() {
            error!("command handler stopped");
        }
    };
    wire_dash(laptop, send.clone());
    wire_site_editor(laptop, send.clone());
    wire_sleep(laptop, send.clone());
    wire_sports_alert(laptop, send.clone());
    wire_intro(laptop, send.clone());

    let home_state = laptop.global::<crate::HomeState>();
    let s = send.clone();
    home_state.on_launch_app(move |id| {
        if let Some(idx) = bridge::site_index_by_id(&id) {
            s(Command::OpenSite(idx));
        }
    });
    let weak = laptop.as_weak();
    home_state.on_open_apps_page(move || {
        if let Some(l) = weak.upgrade() {
            crate::tv_nav::go_apps(&l);
        }
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    home_state.on_activate_hero(move || match bridge::hero_action() {
        bridge::HeroAction::FocusPlayer => {
            if let Some(l) = weak.upgrade() {
                let tv = l.global::<crate::TvUi>();
                tv.set_video_full(true);
                tv.set_hud_open(true);
            }
        }
        bridge::HeroAction::OpenSite(idx) => s(Command::OpenSite(idx)),
        bridge::HeroAction::Play(item) => s(Command::Play(item)),
        bridge::HeroAction::OpenSearch => {
            if let Some(l) = weak.upgrade() {
                crate::tv_nav::go_search(&l);
            }
        }
    });
    let s = send.clone();
    home_state.on_activate_up_next(move |key| {
        if let Some(item) = bridge::up_next_item_by_key(&key) {
            s(Command::Play(item));
        }
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    home_state.on_open_game(move |event_id, date| {
        if let Some(l) = weak.upgrade() {
            crate::tv_nav::go_sports_game(&l, &event_id, &date);
        }
        s(Command::LoadScoreboard {
            day: date.to_string(),
            show: "all".into(),
            final_only: false,
        });
    });

    let sports = laptop.global::<Sports>();
    let (weak, s) = (laptop.as_weak(), send.clone());
    sports.on_load(move |schedule, page| {
        if let Some(l) = weak.upgrade() {
            let sp = l.global::<Sports>();
            sp.set_loading(true);
            sp.set_error("".into());
        }
        s(Command::LoadSportsEvents {
            schedule: schedule.to_string(),
            page: page.max(0) as u32,
        });
    });

    let s = send.clone();
    sports.on_open_url(move |label, url| {
        s(Command::OpenUrl { label: label.to_string(), url: url.to_string() });
    });

    let scoreboard = laptop.global::<Scoreboard>();
    let (weak, s) = (laptop.as_weak(), send.clone());
    scoreboard.on_load(move |day, show, final_only| {
        if let Some(l) = weak.upgrade() {
            let sb = l.global::<Scoreboard>();
            sb.set_loading(true);
            sb.set_error("".into());
        }
        s(Command::LoadScoreboard {
            day: day.to_string(),
            show: show.to_string(),
            final_only,
        });
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    scoreboard.on_toggle_star(move |event_id| {
        if let Some(l) = weak.upgrade() {
            bridge::optimistic_toggle_star(&l, &event_id);
        }
        s(Command::ToggleStarGame(event_id.to_string()));
    });

    let weak = laptop.as_weak();
    scoreboard.on_prev_day_clicked(move || {
        if let Some(l) = weak.upgrade() {
            let sb = l.global::<Scoreboard>();
            let prev = sb.get_prev_day();
            if !prev.is_empty() {
                sb.invoke_load(prev, sb.get_show(), false);
            }
        }
    });

    let weak = laptop.as_weak();
    scoreboard.on_next_day_clicked(move || {
        if let Some(l) = weak.upgrade() {
            let sb = l.global::<Scoreboard>();
            let next = sb.get_next_day();
            if !next.is_empty() {
                sb.invoke_load(next, sb.get_show(), false);
            }
        }
    });

    let weak = laptop.as_weak();
    scoreboard.on_today_clicked(move || {
        if let Some(l) = weak.upgrade() {
            let sb = l.global::<Scoreboard>();
            sb.invoke_load("".into(), sb.get_show(), false);
        }
    });

    let weak = laptop.as_weak();
    scoreboard.on_set_filter(move |show| {
        if let Some(l) = weak.upgrade() {
            let sb = l.global::<Scoreboard>();
            sb.invoke_load(sb.get_day(), show, false);
        }
    });

    let weak = laptop.as_weak();
    scoreboard.on_refresh(move || {
        if let Some(l) = weak.upgrade() {
            let sb = l.global::<Scoreboard>();
            sb.invoke_load(sb.get_day(), sb.get_show(), false);
        }
    });

    let s = send.clone();
    scoreboard.on_open_url(move |label, url| {
        s(Command::OpenUrl { label: label.to_string(), url: url.to_string() });
    });
    wire_watch(laptop, send.clone());
    wire_library(laptop, send.clone());
    wire_accounts(laptop, send.clone());
    wire_server(laptop, send.clone());
    wire_artwork(laptop, send.clone());
    wire_setup(laptop, send);
}

fn wire_dash(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let dash = laptop.global::<Dash>();

    let (weak, s) = (laptop.as_weak(), send.clone());
    dash.on_open_site(move |i| {
        if let Some(l) = weak.upgrade() {
            bridge::set_last_played_site(&l, i as usize);
        }
        s(Command::OpenSite(i as usize));
    });

    let s = send.clone();
    dash.on_play_item(move |i| match bridge::row(i as usize) {
        Some(item) => s(Command::Play(item)),
        None => warn!(index = i, "play: no such card"),
    });

    let s = send.clone();
    dash.on_play_item_from_start(move |i| match bridge::row(i as usize) {
        Some(mut item) => {
            item.resume_ms = None;
            s(Command::Play(item));
        }
        None => warn!(index = i, "play from start: no such card"),
    });

    let s = send.clone();
    dash.on_play_up_next(move || {
        if let Some(item) = bridge::up_next() {
            s(Command::Play(item));
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    dash.on_play_next_ep(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<Dash>().set_next_ep_open(false);
        }
        if let Some(item) = bridge::take_next_ep() {
            s(Command::Play(item));
        }
    });

    let weak = laptop.as_weak();
    dash.on_dismiss_next_ep(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<Dash>().set_next_ep_open(false);
        }
        bridge::clear_next_ep();
    });

    let weak = laptop.as_weak();
    dash.on_set_cw_filter(move |filter| {
        if let Some(l) = weak.upgrade() {
            bridge::set_cw_filter(&l, filter.to_string());
        }
    });

    let weak = laptop.as_weak();
    dash.on_open_screensaver(move || {
        if let Some(l) = weak.upgrade() {
            let d = l.global::<Dash>();
            d.set_search_open(false);
            d.set_screensaver_open(true);
        }
    });

    let weak = laptop.as_weak();
    dash.on_close_screensaver(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<Dash>().set_screensaver_open(false);
        }
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
    let ll = laptop.global::<crate::LibraryLanding>();
    ll.on_focus_changed(move |idx| {
        if let Some(l) = weak.upgrade() {
            let s2 = s.clone();
            bridge::on_library_focus_changed(&l, idx, move |item| s2(Command::PrefetchBackdrop(item)));
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    ll.on_open_watching_now(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<crate::LibraryLanding>().set_watching_now_open(true);
            l.global::<crate::TvUi>().set_focused_tile(0);
        }
        s(Command::RefreshSessions);
    });

    let weak = laptop.as_weak();
    ll.on_close_watching_now(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<crate::LibraryLanding>().set_watching_now_open(false);
            l.global::<crate::TvUi>().set_focused_tile(3);
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    ll.on_select_watching_now(move |idx| {
        if let Some(item) = bridge::watching_now_item(idx as usize) {
            if let Some(l) = weak.upgrade() {
                l.global::<crate::LibraryLanding>().set_watching_now_open(false);
            }
            if item.kind == tvdash_core::models::MediaKind::Movie {
                s(Command::Play(item));
            } else if let Some(l) = weak.upgrade() {
                l.global::<Library>().invoke_open_show(item.id.into());
            }
        }
    });

    let s = send.clone();
    laptop.global::<crate::TvUi>().on_clear_recent(move || s(Command::ClearRecentServices));

    let s = send.clone();
    laptop.global::<crate::TvUi>().on_play_last_played(move || match bridge::last_played() {
        Some(bridge::LastPlayed::Media(item)) => s(Command::Play(item)),
        Some(bridge::LastPlayed::Site(idx)) => s(Command::OpenSite(idx)),
        None => {}
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    dash.on_transport(move |action| {
        let paused = weak.upgrade().is_some_and(|l| l.global::<Dash>().get_tv().paused);
        let cmd = match action.as_str() {
            "back" => Command::Seek(-10),
            "forward" => Command::Seek(30),
            "restart" => Command::SeekTo(0),
            "play-pause" if paused => Command::Resume,
            "play-pause" => Command::Pause,
            "stop" => Command::Stop,
            "audio" => Command::CycleAudio,
            "sub" => Command::CycleSub,
            "vol-down" => Command::VolumeDelta(-5),
            "vol-up" => Command::VolumeDelta(5),
            "prev-ep" => Command::PreviousEpisode,
            "next-ep" => Command::NextEpisode,
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

    let (weak, s) = (laptop.as_weak(), send.clone());
    laptop.global::<crate::Display>().on_set_layout_setting(move |value| {
        let Some(setting) = tvdash_core::LayoutSetting::parse(&value) else {
            return;
        };
        if let Some(l) = weak.upgrade() {
            l.global::<crate::Display>().set_layout_setting(setting.as_str().into());
            crate::displays::apply_layout_setting(&l, setting);
        }
        s(Command::SetLayoutSetting(setting));
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

    let (weak, s) = (laptop.as_weak(), send.clone());
    laptop.global::<crate::Display>().on_set_follow_system(move || {
        let mode = tvdash_core::config::ThemeMode::FollowSystem;
        if let Some(l) = weak.upgrade() {
            bridge::set_theme(&l, mode);
        }
        s(Command::SetTheme(mode));
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    laptop.global::<crate::Display>().on_set_dashboard_ui(move |ui| {
        let mode = tvdash_core::config::DashboardUi::parse(&ui);
        if let Some(l) = weak.upgrade() {
            l.global::<crate::Display>().set_dashboard_ui(mode.as_str().into());
        }
        s(Command::SetDashboardUi(mode));
    });

    let s = send.clone();
    dash.on_quit(move || {
        s(Command::Quit);
        quit_fallback();
    });
}

fn wire_library(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let lib = laptop.global::<Library>();

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_set_filter(move |f| {
        if let Some(l) = weak.upgrade() {
            bridge::set_filter(&l, &f);
            let missing = bridge::missing_browse_posters();
            if !missing.is_empty() {
                s(Command::FetchPosters(missing));
            }
        }
    });

    let reload_shelves = |l: &LaptopWindow, s: &dyn Fn(Command)| {
        let lib = l.global::<Library>();
        let is_tv = lib.get_media_tab() != "movies";
        let sort = tvdash_core::models::LibrarySort::parse(&lib.get_sort());
        let popular_days = lib.get_popular_days().max(7) as u32;
        s(Command::LoadLibraryShelves { is_tv, sort, popular_days });
    };

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_set_media_tab(move |tab| {
        if let Some(l) = weak.upgrade() {
            bridge::set_media_tab(&l, &tab);
            reload_shelves(&l, &s);
            let missing = bridge::missing_browse_posters();
            if !missing.is_empty() {
                s(Command::FetchPosters(missing));
            }
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_set_sort(move |sort| {
        if let Some(l) = weak.upgrade() {
            l.global::<Library>().set_sort(sort);
            reload_shelves(&l, &s);
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_set_popular_days(move |days| {
        if let Some(l) = weak.upgrade() {
            l.global::<Library>().set_popular_days(days);
            reload_shelves(&l, &s);
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_set_browse_letter(move |letter| {
        if let Some(l) = weak.upgrade() {
            bridge::set_browse_letter(&l, &letter);
            let missing = bridge::missing_browse_posters();
            if !missing.is_empty() {
                s(Command::FetchPosters(missing));
            }
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_open_show(move |key| {
        if let Some(l) = weak.upgrade() {
            let sp = l.global::<ShowPageState>();
            sp.set_loading(true);
            sp.set_episodes(slint::ModelRc::default());
            sp.set_selected_season("".into());
        }
        s(Command::OpenShow(key.to_string()));
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    lib.on_open_collection(move |key| {
        if let Some(l) = weak.upgrade() {
            l.global::<CollectionPageState>().set_loading(true);
        }
        s(Command::OpenCollection(key.to_string()));
    });

    let s = send.clone();
    lib.on_play_key(move |key, resume| {
        s(Command::PlayRatingKey { rating_key: key.to_string(), resume });
    });

    let sp = laptop.global::<ShowPageState>();
    let (weak, s) = (laptop.as_weak(), send.clone());
    sp.on_select_season(move |key| {
        if let Some(l) = weak.upgrade() {
            let sp = l.global::<ShowPageState>();
            if sp.get_selected_season() != key {
                sp.set_episodes(slint::ModelRc::default());
            }
            sp.set_selected_season(key.clone());
            crate::bridge::on_season_selected(&l, key.as_str());
        }
        s(Command::LoadSeasonEpisodes(key.to_string()));
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    sp.on_play_episode(move |key, resume| {
        s(Command::PlayRatingKey { rating_key: key.to_string(), resume });
        if let Some(l) = weak.upgrade() {
            l.global::<ShowPageState>().set_open(false);
            l.global::<Nav>().set_page("home".into());
        }
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    sp.on_play_from_start(move || {
        if let Some(l) = weak.upgrade() {
            let key = l.global::<ShowPageState>().get_up_next_key().to_string();
            if !key.is_empty() {
                s(Command::PlayRatingKey { rating_key: key, resume: false });
            }
            l.global::<ShowPageState>().set_open(false);
            l.global::<Nav>().set_page("home".into());
        }
    });
    let weak = laptop.as_weak();
    sp.on_close(move || {
        if let Some(l) = weak.upgrade() {
            let sp = l.global::<ShowPageState>();
            sp.set_open(false);
            sp.set_episodes(slint::ModelRc::default());
        }
    });

    let cp = laptop.global::<CollectionPageState>();
    let weak = laptop.as_weak();
    cp.on_set_order(move |order| {
        if let Some(l) = weak.upgrade() {
            l.global::<CollectionPageState>().set_order(order);
            bridge::reorder_collection(&l);
        }
    });
    let weak = laptop.as_weak();
    cp.on_set_hide_watched(move |hide| {
        if let Some(l) = weak.upgrade() {
            l.global::<CollectionPageState>().set_hide_watched(hide);
            bridge::reorder_collection(&l);
        }
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    cp.on_play_item(move |key, resume| {
        s(Command::PlayRatingKey { rating_key: key.to_string(), resume });
        if let Some(l) = weak.upgrade() {
            l.global::<CollectionPageState>().set_open(false);
            l.global::<Nav>().set_page("home".into());
        }
    });
    let weak = laptop.as_weak();
    cp.on_close(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<CollectionPageState>().set_open(false);
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

fn wire_server(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let server = laptop.global::<Server>();

    let s = send.clone();
    server.on_open(move || {
        s(Command::ServerRefresh { refresh_libraries: true });
        s(Command::ServerRefreshActivities);
    });

    let s = send.clone();
    server.on_refresh(move || s(Command::ServerRefresh { refresh_libraries: false }));

    let s = send.clone();
    server.on_refresh_activities(move || s(Command::ServerRefreshActivities));

    let s = send.clone();
    server.on_cancel_activity(move |uuid| s(Command::ServerCancelActivity(uuid.to_string())));

    let s = send.clone();
    server.on_terminate(move |session_id, reason| {
        s(Command::ServerTerminate { session_id: session_id.to_string(), reason: reason.to_string() })
    });

    let s = send.clone();
    server.on_scan_all(move || s(Command::ServerScan { index: None }));

    let s = send.clone();
    server.on_scan_section(move |index| s(Command::ServerScan { index: Some(index as usize) }));
}

fn wire_artwork(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let artwork = laptop.global::<Artwork>();
    let weak_request = laptop.as_weak();
    let s = send.clone();
    artwork.on_request(move |backdrop| {
        let Some(laptop) = weak_request.upgrade() else { return };
        let Some(item) = bridge::selected() else { return };
        let artwork = laptop.global::<Artwork>();
        artwork.set_open(true);
        artwork.set_backdrop(backdrop);
        artwork.set_loading(true);
        artwork.set_error("".into());
        s(Command::FetchArtwork { item, backdrop });
    });

    let weak_set = laptop.as_weak();
    let s = send.clone();
    artwork.on_set(move |key| {
        let Some(laptop) = weak_set.upgrade() else { return };
        let Some(item) = bridge::selected() else { return };
        let backdrop = laptop.global::<Artwork>().get_backdrop();
        laptop.global::<Artwork>().set_open(false);
        s(Command::SetArtwork { item, key: key.to_string(), backdrop });
    });

    let weak_refresh = laptop.as_weak();
    let s = send.clone();
    artwork.on_refresh(move || {
        if let Some(item) = bridge::selected() {
            if let Some(laptop) = weak_refresh.upgrade() {
                laptop.global::<Library>().set_metadata_refreshing(true);
            }
            s(Command::RefreshMetadata(item));
        }
    });

    let weak_close = laptop.as_weak();
    artwork.on_close(move || {
        if let Some(laptop) = weak_close.upgrade() {
            laptop.global::<Artwork>().set_open(false);
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
            ("tautulli", "remove", _) => Command::ClearTautulli,
            ("events", "remove", _) => Command::SetEventsPassword(None),
            _ => {
                warn!(%id, %action, "unknown account action");
                return;
            }
        };
        s(cmd);
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    acc.on_submit(move |id, value| {
        if id == "tmdb" && !value.trim().is_empty() {
            s(Command::SetTmdbToken(value.to_string()));
        } else if id == "jellyfin" {
            let url = value.trim().trim_end_matches('/').to_string();
            if url.starts_with("http://") || url.starts_with("https://") {
                s(Command::JellyfinServer(url));
                if let Some(l) = weak.upgrade() {
                    l.global::<Accounts>().set_editing("".into());
                }
            } else if let Some(l) = weak.upgrade() {
                l.global::<Accounts>().set_edit_error("Enter a URL starting with http:// or https://".into());
            }
        } else if id == "tautulli" {
            let trimmed = value.trim();
            if let Some((url, key)) = trimmed.split_once('|').or_else(|| trimmed.split_once(' ')) {
                let url = url.trim().to_string();
                let api_key = key.trim().to_string();
                if !url.is_empty() && !api_key.is_empty() {
                    s(Command::SetTautulli { url, api_key });
                    if let Some(l) = weak.upgrade() {
                        l.global::<Accounts>().set_editing("".into());
                    }
                }
            } else if let Some(l) = weak.upgrade() {
                l.global::<Accounts>().set_edit_error("Enter: http://host:8181 | API_KEY".into());
            }
        } else if id == "events" {
            let pw = value.trim().to_string();
            if !pw.is_empty() {
                s(Command::SetEventsPassword(Some(pw)));
                if let Some(l) = weak.upgrade() {
                    l.global::<Accounts>().set_editing("".into());
                }
            } else if let Some(l) = weak.upgrade() {
                l.global::<Accounts>().set_edit_error("Enter the SQL Server password".into());
            }
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
            crate::tv_nav::sync(&l);
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
                    l.global::<Watch>().set_detail_open(false);
                    if l.global::<crate::TvUi>().get_layout_tv() {
                        l.global::<crate::TvUi>().set_screen("home".into());
                        crate::tv_nav::sync(&l);
                    }
                    l.global::<Nav>().set_page("home".into());
                }
            }
            "library" => {
                if l.global::<crate::TvUi>().get_layout_tv() {
                    if let Some(item) = hit.library.filter(|i| i.kind == tvdash_core::models::MediaKind::Series) {
                        l.global::<ShowPageState>().set_loading(true);
                        send(Command::OpenShow(item.id));
                        l.global::<crate::TvUi>().set_screen("library".into());
                        l.global::<Watch>().set_detail_open(false);
                        crate::tv_nav::sync(&l);
                        return;
                    }
                    l.global::<crate::TvUi>().set_screen("library".into());
                    l.global::<Watch>().set_detail_open(false);
                    crate::tv_nav::sync(&l);
                }
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
            // Fields the editor doesn't show are kept from the existing site.
            let existing = index.and_then(|i| all.get(i));
            let site = StreamingSite {
                name: ed.get_name().trim().to_owned(),
                url: ed.get_url().trim().to_owned(),
                search_url: Some(ed.get_search().trim().to_owned()).filter(|u| !u.is_empty()),
                tmdb_providers,
                hidden: existing.is_some_and(|x| x.hidden),
                logo: existing.and_then(|x| x.logo.clone()),
                logo_box: existing.and_then(|x| x.logo_box.clone()),
                hold_sleep_timer: existing.is_some_and(|x| x.hold_sleep_timer),
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

fn wire_sleep(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let sl = laptop.global::<SleepState>();

    let weak = laptop.as_weak();
    sl.on_open_picker(move || {
        let Some(l) = weak.upgrade() else { return };
        let use_24h = !l.global::<Display>().get_h12();
        let now = crate::sleep::now_unix();
        let presets = [15i64, 30, 45, 60, 90, 120];
        let ends: Vec<slint::SharedString> = presets
            .iter()
            .map(|m| format!("Ends {}", crate::sleep::format_ends_at(now + m * 60, use_24h)).into())
            .collect();
        let sl = l.global::<SleepState>();
        sl.set_preset_ends(slint::ModelRc::new(slint::VecModel::from(ends)));
        let start_focus = if sl.get_active() {
            let sel = sl.get_selected_tile();
            if sel >= 0 { sel } else { 3 }
        } else {
            let def = sl.get_default_minutes() as i64;
            presets.iter().position(|&m| m == def).unwrap_or(3) as i32
        };
        sl.set_focused_tile(start_focus);
        sl.set_picker_open(true);
    });

    let weak = laptop.as_weak();
    sl.on_close_picker(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<SleepState>().set_picker_open(false);
        }
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    sl.on_set_minutes(move |mins| {
        if let Some(l) = weak.upgrade() {
            let sl = l.global::<SleepState>();
            let tile = [15, 30, 45, 60, 90, 120].iter().position(|&m| m == mins).map(|i| i as i32).unwrap_or(-1);
            sl.set_selected_tile(tile);
            sl.set_picker_open(false);
        }
        s(Command::SleepSetMinutes(mins as i64));
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    sl.on_set_end_of_item(move || {
        if let Some(l) = weak.upgrade() {
            let sl = l.global::<SleepState>();
            if !sl.get_end_of_item_ok() {
                return;
            }
            sl.set_selected_tile(6);
            sl.set_picker_open(false);
        }
        s(Command::SleepSetEndOfItem);
    });

    let (weak, s) = (laptop.as_weak(), send.clone());
    sl.on_off(move || {
        if let Some(l) = weak.upgrade() {
            l.global::<SleepState>().set_selected_tile(7);
        }
        s(Command::SleepOff);
    });

    let s = send.clone();
    sl.on_extend(move || s(Command::SleepExtend));

    let s = send.clone();
    sl.on_sleep_now(move || s(Command::SleepNow));

    let s = send.clone();
    sl.on_play_next(move || s(Command::SleepPlayNext));

    let (weak, s) = (laptop.as_weak(), send);
    sl.on_set_warn_before(move |val| {
        if let Some(l) = weak.upgrade() {
            l.global::<SleepState>().set_warn_before(val.clone());
        }
        s(Command::SetSleepWarnBefore(val.to_string()));
    });
}

fn wire_sports_alert(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let al = laptop.global::<SportsAlertState>();
    let weak = laptop.as_weak();
    al.on_primary_clicked(move || {
        if let Some(l) = weak.upgrade() {
            bridge::alerts::on_primary(&l);
        }
    });
    let weak = laptop.as_weak();
    al.on_dismiss_clicked(move || {
        if let Some(l) = weak.upgrade() {
            bridge::alerts::on_dismiss(&l);
        }
    });

    let cfg_ui = laptop.global::<SportsAlertsSettings>();
    let (weak, s) = (laptop.as_weak(), send.clone());
    cfg_ui.on_set_soon_minutes(move |mins| {
        if let Some(l) = weak.upgrade() {
            let g = l.global::<SportsAlertsSettings>();
            g.set_soon_minutes(mins);
            s(Command::SetSportsAlertsConfig(tvdash_core::config::SportsAlertsConfig {
                enabled: true,
                soon_minutes: mins.max(0) as u32,
                final_on: g.get_final_on(),
                during_video: if g.get_during_video() == "hold" {
                    tvdash_core::config::DuringVideo::Hold
                } else {
                    tvdash_core::config::DuringVideo::Show
                },
            }));
        }
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    cfg_ui.on_set_final_on(move |on| {
        if let Some(l) = weak.upgrade() {
            let g = l.global::<SportsAlertsSettings>();
            g.set_final_on(on);
            s(Command::SetSportsAlertsConfig(tvdash_core::config::SportsAlertsConfig {
                enabled: true,
                soon_minutes: g.get_soon_minutes().max(0) as u32,
                final_on: on,
                during_video: if g.get_during_video() == "hold" {
                    tvdash_core::config::DuringVideo::Hold
                } else {
                    tvdash_core::config::DuringVideo::Show
                },
            }));
        }
    });
    let (weak, s) = (laptop.as_weak(), send.clone());
    cfg_ui.on_set_during_video(move |mode| {
        if let Some(l) = weak.upgrade() {
            let g = l.global::<SportsAlertsSettings>();
            g.set_during_video(mode.clone());
            s(Command::SetSportsAlertsConfig(tvdash_core::config::SportsAlertsConfig {
                enabled: true,
                soon_minutes: g.get_soon_minutes().max(0) as u32,
                final_on: g.get_final_on(),
                during_video: if mode.as_str() == "hold" {
                    tvdash_core::config::DuringVideo::Hold
                } else {
                    tvdash_core::config::DuringVideo::Show
                },
            }));
        }
    });
    let s = send.clone();
    cfg_ui.on_unstar_game(move |eid| s(Command::UnstarGame(eid.to_string())));
    let s = send;
    cfg_ui.on_refresh_follows(move || s(Command::RefreshSportsFollows));
}

fn wire_intro(laptop: &LaptopWindow, send: impl Fn(Command) + Clone + 'static) {
    let intro = laptop.global::<crate::Intro>();
    let (weak, s) = (laptop.as_weak(), send);
    intro.on_set_enabled(move |on| {
        if let Some(l) = weak.upgrade() {
            l.global::<crate::Intro>().set_enabled(on);
        }
        s(Command::SetStartupIntro(on));
    });
    let weak = laptop.as_weak();
    intro.on_preview(move || {
        if let Some(l) = weak.upgrade() {
            let skin = if l.global::<Display>().get_dashboard_ui() == "blades" {
                crate::intro::Skin::Blades
            } else {
                crate::intro::Skin::Tiles
            };
            crate::intro::preview(&l, skin);
        }
    });
    let weak = laptop.as_weak();
    intro.on_skip(move || {
        if let Some(l) = weak.upgrade() {
            crate::intro::skip(&l);
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
