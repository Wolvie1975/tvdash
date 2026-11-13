//! Drive the real windows headless (Slint's testing backend) through events
//! and check what the pages would show.

use slint::Model;
use tvdash_core::{
    Command,
    models::{LibraryData, MediaKind, PlayState, RgbaImage},
};

use super::*;

fn windows() -> (LaptopWindow, TvWindow) {
    i_slint_backend_testing::init_no_event_loop();
    let (l, t) = (LaptopWindow::new().unwrap(), TvWindow::new().unwrap());
    init(
        &l,
        UiConfig {
            tz: Some(chrono_tz::America::Chicago),
            clock: ClockFormat::H24,
            location: String::new(),
            seconds: false,
            paths: vec!["Log file: /tmp/tvdash.log".into()],
            units: Units::Fahrenheit,
            theme: ThemeMode::Dark,
            sites: tvdash_core::config::resolve_apps(&[]),
            library: Default::default(),
        },
    );
    (l, t)
}

fn send(l: &LaptopWindow, t: &TvWindow, e: AppEvent) {
    STATE.with_borrow_mut(|s| apply(s, l, t, e));
}

fn item(source: SourceKind, id: &str, title: &str) -> MediaItem {
    MediaItem {
        source,
        id: id.into(),
        kind: MediaKind::Movie,
        title: title.into(),
        subtitle: Some("1999".into()),
        episode_label: None,
        duration_ms: Some(7_200_000),
        resume_ms: Some(3_600_000),
        poster: Some("p".into()),
        ..Default::default()
    }
}

fn account(l: &LaptopWindow, id: &str) -> AccountRow {
    let rows = l.global::<Accounts>().get_accounts();
    rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("no row {id}"))
}

#[test]
fn setup_flow_advances_on_success() {
    let (l, t) = windows();
    let setup = l.global::<Setup>();
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp));
    send(&l, &t, AppEvent::ShowSetup);
    assert_eq!(l.global::<Nav>().get_page(), "setup");
    assert_eq!((setup.get_step(), setup.get_plex_state().as_str()), (0, "idle"));

    // Plex: code shown on the setup page and in Settings; success moves on.
    send(&l, &t, AppEvent::SignIn(SourceKind::Plex, SignInState::Code("WXYZ".into())));
    assert_eq!((setup.get_plex_state().as_str(), setup.get_plex_code().as_str()), ("code", "WXYZ"));
    assert_eq!(account(&l, "plex").status, "Code WXYZ");
    send(&l, &t, AppEvent::SignIn(SourceKind::Plex, SignInState::Done));
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::Connected));
    send(&l, &t, AppEvent::Server(SourceKind::Plex, Some("http://10.0.0.5:32400".into())));
    assert_eq!(setup.get_step(), 1);
    assert_eq!(setup.get_plex_state(), "done");
    assert!(setup.get_plex_message().contains("10.0.0.5"));
    assert_eq!((account(&l, "plex").state.as_str(), account(&l, "plex").secondary.as_str()), ("ok", "Sign out"));

    // Back on the Plex step, a failure stays there with the reason.
    setup.set_step(0);
    send(&l, &t, AppEvent::SignIn(SourceKind::Plex, SignInState::Failed("the code expired".into())));
    sync_setup(&l);
    assert_eq!((setup.get_step(), setup.get_plex_state().as_str()), (0, "error"));
    assert!(setup.get_plex_message().contains("expired"));
}

#[test]
fn tv_card_follows_playback_browser_and_theme() {
    let (l, t) = windows();
    let dash = l.global::<Dash>();
    assert_eq!(dash.get_tv().mode, "idle");
    assert_eq!(dash.get_streaming_sites().row_count(), 10);

    // Something to resume: the card offers it, tinted from its poster.
    send(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![item(SourceKind::Plex, "p1", "Plex Film")]));
    let tv = dash.get_tv();
    assert_eq!((tv.mode.as_str(), tv.label.as_str(), tv.source.as_str()), ("next", "UP NEXT", "Plex"));
    assert_eq!((tv.at_text.as_str(), tv.left_text.as_str()), ("Stopped at 1:00:00", "1 h left"));
    let before = dash.get_tint().card;
    let red = RgbaImage { width: 2, height: 2, pixels: [200, 30, 30, 255].repeat(4) };
    send(&l, &t, AppEvent::Poster { source: SourceKind::Plex, item_id: "p1".into(), image: red });
    assert!(dash.get_tv().has_poster);
    assert_ne!(dash.get_tint().card, before, "tint follows the poster");

    // Playback wins over "up next"; the browser shows when mpv isn't running.
    let status = PlayerStatus {
        item: item(SourceKind::Plex, "p9", "Other Film"),
        state: PlayState::Paused,
        position_ms: 754_000,
        duration_ms: Some(2_700_000),
        subtitles: true,
    };
    send(&l, &t, AppEvent::Player(Some(status)));
    let tv = dash.get_tv();
    assert_eq!((tv.mode.as_str(), tv.title.as_str(), tv.paused), ("playing", "Other Film", true));
    assert!(tv.subtitles, "the CC button is lit while subtitles show");
    assert_eq!(tv.at_text, "12:34 / 45:00");
    assert!(tv.left_text.starts_with("32 min left · ends "));
    assert_eq!(t.get_message(), "Starting Other Film…");
    send(&l, &t, AppEvent::Player(None));
    send(&l, &t, AppEvent::Browser(Some("Hulu".into())));
    let tv = dash.get_tv();
    assert_eq!((tv.mode.as_str(), tv.monogram.as_str(), tv.has_logo), ("streaming", "H", true));
    send(&l, &t, AppEvent::Browser(None));
    assert_eq!(dash.get_tv().mode, "next");
    assert_eq!(t.get_message(), "");

    // Theme: laptop switches, tiles and tint follow; the TV stays dark.
    let dark_tile = dash.get_streaming_sites().row_data(0).unwrap().logo;
    set_theme(&l, ThemeMode::Stone);
    assert_eq!(l.global::<Theme>().get_mode(), UiTheme::Stone);
    assert_ne!(dash.get_streaming_sites().row_data(0).unwrap().logo, dark_tile, "light-theme logo");
    assert_eq!(t.global::<Theme>().get_mode(), UiTheme::Dark);
}

#[test]
fn ten_fixed_apps_with_logos() {
    let (l, _t) = windows();
    let tiles = l.global::<Dash>().get_streaming_sites();
    let names: Vec<String> = tiles.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, tvdash_core::config::APPS.map(|a| a.name.to_string()));
    assert!(tiles.iter().all(|t| t.logo.size().width > 0), "every tile has its logo");
    assert!(tiles.iter().enumerate().all(|(i, t)| t.index == i as i32));
    let rows = l.global::<Accounts>().get_sites();
    assert_eq!(rows.row_count(), 10);
    assert_eq!(rows.row_data(5).unwrap().url, "https://www.netflix.com");
}

#[test]
fn weather_states_and_diagnostics() {
    let (l, t) = windows();
    let dash = l.global::<Dash>();
    // Failing before any data: shown as unavailable with the reason.
    send(&l, &t, AppEvent::Health { service: Service::Weather, result: Err("HTTP 502".into()) });
    assert!(dash.get_weather_on() && dash.get_weather_warn());
    assert_eq!(dash.get_weather_note(), "Unavailable: HTTP 502");

    let w = Weather { temperature: 51.0, code: 2, condition: "Partly cloudy".into(), high: None, low: None };
    send(&l, &t, AppEvent::Weather(Some(w)));
    send(&l, &t, AppEvent::Health { service: Service::Weather, result: Ok("51 Partly cloudy".into()) });
    assert!(!dash.get_weather_warn() && !dash.get_weather_stale());
    assert_eq!((dash.get_weather_temp().as_str(), dash.get_weather_note().as_str()), ("51°F", ""));

    // Old data: dimmed with its age; a later failure adds the amber dot.
    STATE.with_borrow_mut(|s| s.weather.as_mut().unwrap().1 -= chrono::TimeDelta::minutes(25));
    send(&l, &t, AppEvent::Health { service: Service::Weather, result: Err("timed out".into()) });
    assert!(dash.get_weather_stale() && dash.get_weather_warn());
    assert_eq!(dash.get_weather_note(), "Updated 25 min ago");

    // Diagnostics: one row per service; the newest result decides the dot.
    let rows = l.global::<Diagnostics>().get_rows();
    let weather = rows.iter().find(|r| r.name == "Weather (Open-Meteo)").unwrap();
    assert_eq!(weather.state, "warn");
    assert!(weather.last_error.ends_with("timed out") && weather.last_ok.ends_with("51 Partly cloudy"));
    assert_eq!(rows.iter().find(|r| r.name == "Plex").unwrap().state, "off");
    send(&l, &t, AppEvent::KeyringStatus(Ok(())));
    let rows = l.global::<Diagnostics>().get_rows();
    assert_eq!(rows.iter().find(|r| r.name == "GNOME Keyring").unwrap().state, "ok");
    assert_eq!(l.global::<Diagnostics>().get_paths().row_count(), 1);

    // Display settings: the reading keeps its units until the new fetch arrives.
    set_display(&l, Units::Celsius, ClockFormat::H12, true);
    assert_eq!(dash.get_weather_temp(), "51°F");
}

#[test]
fn location_search_pick_and_timezone() {
    use tvdash_core::models::Place;
    let (l, t) = windows();
    let d = l.global::<Display>();
    let place =
        |label: &str, tz: &str| Place { label: label.into(), latitude: 1.0, longitude: 2.0, timezone: tz.into() };
    d.set_place_query("chic".into());
    // Results for an older query are ignored.
    send(&l, &t, AppEvent::Places { query: "ch".into(), result: Ok(vec![place("Old", "")]) });
    assert_eq!(d.get_places().row_count(), 0);
    send(
        &l,
        &t,
        AppEvent::Places {
            query: "chic".into(),
            result: Ok(vec![place("Chicago, Illinois, United States", "America/Chicago"), place("Chico", "")]),
        },
    );
    assert_eq!(d.get_places().row_count(), 2);
    assert_eq!(super::place(0).unwrap().label, "Chicago, Illinois, United States");
    send(&l, &t, AppEvent::Places { query: "chic".into(), result: Ok(vec![]) });
    assert!(d.get_place_message().contains("No place called “chic”"));

    // Picked: name shown, results cleared, clocks follow the place's zone.
    STATE.with_borrow_mut(|s| s.tz = None);
    send(
        &l,
        &t,
        AppEvent::Location { name: "Chicago, Illinois, United States".into(), timezone: "America/Chicago".into() },
    );
    assert_eq!(d.get_location_name(), "Chicago, Illinois, United States");
    assert_eq!((d.get_places().row_count(), d.get_place_query().as_str()), (0, ""));
    assert_eq!(STATE.with_borrow(|s| s.tz), Some(chrono_tz::America::Chicago));
}

#[test]
fn library_shelves_browse_and_detail() {
    let (l, t) = windows();
    let lib = l.global::<Library>();
    let browse_ui = l.global::<crate::Browse>();
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp));
    assert_eq!(lib.get_state(), "signin");
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::Connected));
    assert_eq!(lib.get_state(), "loading");

    let now = chrono::Utc::now().timestamp();
    let movie = |id: &str, views, resume: Option<u64>, genre: &str| MediaItem {
        kind: MediaKind::Movie,
        view_count: views,
        resume_ms: resume,
        added_at: Some(now),
        genres: vec![genre.into()],
        summary: Some(format!("About {id}.")),
        ..item(SourceKind::Plex, id, id)
    };
    let show = |id: &str, leaves, viewed| MediaItem {
        kind: MediaKind::Series,
        leaf_count: Some(leaves),
        viewed_leaf_count: Some(viewed),
        added_at: Some(0),
        genres: vec!["Drama".into()],
        ..item(SourceKind::Plex, id, id)
    };
    let data = LibraryData {
        movies: vec![movie("a", 0, None, "Drama"), movie("b", 0, None, "Comedy"), movie("c", 3, Some(60_000), "Drama")],
        shows: vec![show("s1", 8, 0), show("s2", 10, 10), show("s3", 20, 18)],
        seasons: vec![tvdash_core::models::Season {
            show_id: "s3".into(),
            show_title: "s3".into(),
            index: Some(2),
            leaf_count: 10,
            viewed_leaf_count: 8,
        }],
        new_episodes: vec![],
        collections: vec![],
    };
    send(&l, &t, AppEvent::Library(SourceKind::Plex, data));
    assert_eq!(lib.get_state(), "ready");

    // TV / Series first: the one unstarted show, the season to finish, a show to watch again.
    let (pick, boxed, _) = library::shelf_ids();
    assert_eq!((pick.as_deref(), boxed.as_slice()), (Some("s1"), &["s3".to_string()][..]));
    assert_eq!(lib.get_pick().action, "Start S1 · E1 on TV");
    let item0 = lib.get_box_items().row_data(0).unwrap();
    assert_eq!((item0.line.as_str(), item0.segments, item0.done), ("Season 2 · 2 episodes left", 10, 8));
    assert_eq!(lib.get_again_count(), 1);
    assert_eq!(lib.get_genres().row_data(0).unwrap().name, "Drama");

    // Movies: a random unwatched one; Shuffle moves to the other.
    set_tab(&l, "movies");
    let first = library::shelf_ids().0.unwrap();
    assert!(first == "a" || first == "b", "{first}");
    shuffle(&l);
    let second = library::shelf_ids().0.unwrap();
    assert_ne!(first, second);
    assert_eq!(lib.get_box_title(), "CONTINUE WATCHING");
    assert_eq!(lib.get_box_items().row_data(0).unwrap().title, "c");
    assert_eq!((lib.get_added_count(), lib.get_again_count()), (3, 1));

    // Browse a genre, then pick a movie for the detail panel.
    browse(&l, "genre:Drama");
    assert_eq!(l.global::<Nav>().get_page(), "browse");
    assert_eq!((browse_ui.get_title().as_str(), browse_ui.get_count_text().as_str()), ("Drama · movies", "2 titles"));
    select(&l, 1);
    let d = browse_ui.get_detail();
    assert_eq!((d.title.as_str(), d.action.as_str(), d.can_resume), ("c", "Resume on TV", true));
    assert_eq!((d.extra.as_str(), d.overview.as_str()), ("Watched 3 times", "About c."));
    assert_eq!(selected().unwrap().id, "c");
    set_unwatched(&l, true);
    assert_eq!(browse_ui.get_count_text(), "1 title");
    assert!(!browse_ui.get_detail_open(), "the list changed under the panel");

    // A shelf poster opens Browse on that shelf with it selected.
    open_item(&l, "again", 0);
    assert_eq!((browse_ui.get_title().as_str(), browse_ui.get_selected()), ("Watch again", 0));
    assert_eq!(browse_ui.get_detail().title, "c");

    // A show in progress resumes its episode; otherwise its next one plays.
    let s1 = show("s1", 8, 0);
    assert!(matches!(play_command(&s1), Command::PlayShow(ref x) if x.id == "s1"));

    // Sign-out empties it.
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::NotSetUp));
    send(&l, &t, AppEvent::Library(SourceKind::Plex, LibraryData::default()));
    assert_eq!(lib.get_state(), "signin");
}

#[test]
fn expired_states_and_streaming_sites() {
    let (l, t) = windows();
    let dash = l.global::<Dash>();
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::Expired));
    // Expired → On the TV card, not the Continue Watching banners.
    assert_eq!(dash.get_tv_prompts().row_count(), 1);
    assert_eq!(dash.get_prompts().row_count(), 0);
    let plex = account(&l, "plex");
    assert_eq!(
        (plex.state.as_str(), plex.primary.as_str(), plex.primary_id.as_str()),
        ("warn", "Reconnect", "sign-in")
    );

    send(&l, &t, AppEvent::Streaming(BTreeMap::from([("Netflix".to_string(), Utc::now())])));
    let sites = l.global::<Accounts>().get_sites();
    assert_eq!(sites.row_data(5).unwrap().status, "Set up · opened today");
    assert_eq!(sites.row_data(3).unwrap().status, "Not signed in yet");
}

#[test]
fn continue_watching_tabs_and_box_details() {
    let (l, t) = windows();
    let dash = l.global::<Dash>();
    let ep = MediaItem {
        kind: MediaKind::Episode,
        title: "Harbor Lights".into(),
        subtitle: Some("The Breakwater".into()),
        episode_label: Some("S1 · E3".into()),
        season: Some(1),
        episode: Some(3),
        show_id: Some("50".into()),
        duration_ms: Some(2_700_000),
        resume_ms: Some(1_900_000),
        ..item(SourceKind::Plex, "e1", "Harbor Lights")
    };
    let film = item(SourceKind::Plex, "m1", "A Film");
    send(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![ep.clone(), film]));
    // Two tabs; badges are the episode and the time left.
    let (tv, movies) = (dash.get_cw_tv(), dash.get_cw_movies());
    assert_eq!((tv.row_count(), movies.row_count()), (1, 1));
    assert_eq!(
        (tv.row_data(0).unwrap().badge.as_str(), movies.row_data(0).unwrap().badge.as_str()),
        ("S1 · E3", "1 h left")
    );
    assert_eq!(tab_row("movies", 0).unwrap().id, "m1");
    assert_eq!(tab_row("tv", 0).unwrap().id, "e1");
    assert_eq!(crate::watch_ui::resume_for_show("50").unwrap().id, "e1", "search can resume the show");

    // Up next: the newest item, described once its details arrive.
    let card = dash.get_tv();
    assert_eq!((card.mode.as_str(), card.episode_line.as_str()), ("next", "Season 1 · Episode 3 — The Breakwater"));
    assert_eq!((card.overview.as_str(), card.facts.as_str()), ("", "45 min"));
    let full = MediaItem {
        summary: Some("A storm cuts the harbour off.".into()),
        year: Some(2024),
        content_rating: Some("TV-PG".into()),
        genres: vec!["Mystery".into(), "Coastal".into()],
        resume_ms: None,
        ..ep
    };
    send(&l, &t, AppEvent::Details(full));
    let card = dash.get_tv();
    assert_eq!(card.overview, "A storm cuts the harbour off.");
    assert_eq!(card.facts, "2024 · TV-PG · 45 min · Mystery");
    assert_eq!(card.genres, "Mystery · Coastal");
    assert_eq!(card.left_text, "13 min left", "progress stays the row's own");
}

#[test]
fn screensaver_shows_whats_on_the_tv() {
    let (l, t) = windows();
    let sv = l.global::<crate::Saver>();
    assert_eq!((sv.get_label().as_str(), sv.get_title().as_str()), ("ON THE TV · NOTHING PLAYING", "Nothing playing"));

    let ep = MediaItem {
        kind: MediaKind::Episode,
        season: Some(1),
        episode: Some(3),
        subtitle: Some("The Breakwater".into()),
        duration_ms: Some(2_700_000),
        resume_ms: Some(1_860_000),
        ..item(SourceKind::Plex, "e1", "Harbor Lights")
    };
    send(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![ep.clone()]));
    assert_eq!(sv.get_title(), "Up next: Harbor Lights");
    assert_eq!(sv.get_line(), "Season 1 · Episode 3 · 14 min left");
    assert!(!sv.get_show_progress());

    let playing = |state| PlayerStatus {
        item: ep.clone(),
        state,
        position_ms: 1_450_000,
        duration_ms: Some(3_120_000),
        subtitles: false,
    };
    send(&l, &t, AppEvent::Player(Some(playing(PlayState::Playing))));
    assert_eq!(sv.get_label(), "ON THE TV · PLAYING · PLEX");
    assert_eq!(
        (sv.get_title().as_str(), sv.get_line().as_str()),
        ("Harbor Lights", "Season 1 · Episode 3 — The Breakwater")
    );
    assert!(sv.get_show_progress());
    assert_eq!(sv.get_at(), "24:10 / 52:00");
    assert!(sv.get_ends().starts_with("Ends at "));
    send(&l, &t, AppEvent::Player(Some(playing(PlayState::Paused))));
    assert_eq!(sv.get_label(), "ON THE TV · PAUSED · PLEX");

    // The sentences: time (24 h here) and weather in words.
    assert!(sv.get_time().contains(':'));
    send(
        &l,
        &t,
        AppEvent::Weather(Some(Weather {
            temperature: 50.6,
            code: 2,
            condition: "Partly sunny".into(),
            high: None,
            low: None,
        })),
    );
    assert_eq!(
        (sv.get_has_weather(), sv.get_temp().as_str(), sv.get_condition().as_str()),
        (true, "51°", "partly sunny")
    );

    // Drift: a new spot each minute.
    let before = (sv.get_drift_x(), sv.get_drift_y());
    on_minute(&l, &t);
    assert_ne!((sv.get_drift_x(), sv.get_drift_y()), before);
}

#[test]
fn waking_tap_does_nothing_else() {
    use slint::{
        LogicalPosition,
        platform::{PointerEventButton, WindowEvent},
    };
    let (l, _t) = windows();
    l.window().set_size(slint::LogicalSize::new(1920.0, 1080.0));
    l.show().unwrap();
    let tap = |x: f32, y: f32| {
        let position = LogicalPosition::new(x, y);
        let w = l.window();
        w.dispatch_event(WindowEvent::PointerMoved { position });
        w.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        w.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        slint::platform::update_timers_and_animations();
    };
    // The Settings button sits at the header's right edge.
    let settings = (1920.0 - 64.0 - 32.0, 40.0 + 70.0);
    l.global::<crate::Saver>().set_on(true);
    tap(settings.0, settings.1);
    assert!(!l.global::<crate::Saver>().get_on(), "woke up");
    assert_eq!(l.global::<Nav>().get_page(), "home", "the tap didn't press Settings");
    // Awake, the same tap does press it.
    tap(settings.0, settings.1);
    assert_eq!(l.global::<Nav>().get_page(), "settings");
    l.hide().unwrap();
}

#[test]
fn sort_collections_and_show_page() {
    use tvdash_core::models::{Collection, ShowData, ShowSeason};
    let (l, t) = windows();
    let lib = l.global::<Library>();
    send(&l, &t, AppEvent::Account(SourceKind::Plex, AccountStatus::Connected));
    let movie = |id: &str, title: &str, year, views, added| MediaItem {
        kind: MediaKind::Movie,
        year: Some(year),
        view_count: views,
        added_at: Some(added),
        genres: vec!["Drama".into()],
        resume_ms: None,
        ..item(SourceKind::Plex, id, title)
    };
    let (a, b, c) =
        (movie("a", "The Zebra", 2001, 2, 1), movie("b", "Apple", 1999, 2, 3), movie("c", "Mango", 2010, 0, 2));
    let data = LibraryData {
        movies: vec![a.clone(), b.clone(), c.clone()],
        collections: vec![Collection {
            id: "k".into(),
            title: "Fruit".into(),
            smart: true,
            movies: vec![a.clone(), c.clone(), b.clone()],
            ..Default::default()
        }],
        ..Default::default()
    };
    send(&l, &t, AppEvent::Library(SourceKind::Plex, data));

    // Sort: Watch again follows it ("The Zebra" sorts under Z).
    set_tab(&l, "movies");
    set_sort(&l, "az");
    assert_eq!(lib.get_sort(), "az");
    let again: Vec<String> = lib.get_again().iter().map(|c| c.title.to_string()).collect();
    assert_eq!(again, ["Apple", "The Zebra"]);
    set_sort(&l, "za");
    assert_eq!(lib.get_again().row_data(0).unwrap().title, "The Zebra");

    // The Collections row and a collection's page.
    let row = lib.get_collections().row_data(0).unwrap();
    assert_eq!((row.title.as_str(), row.line.as_str()), ("Fruit", "3 movies · 2 watched · smart"));
    open_collection(&l, 0);
    assert_eq!(l.global::<Nav>().get_page(), "collection");
    let page = l.global::<crate::Collection>();
    assert_eq!(page.get_meta(), "3 movies · 1999–2010 · 6 h in total");
    assert_eq!((page.get_watched_text().as_str(), page.get_action().as_str()), ("2 of 3 watched", "Play Mango on TV"));
    assert_eq!(collection_up_next().unwrap().id, "c");
    let first = |p: &crate::Collection| p.get_movies().row_data(0).unwrap();
    assert_eq!((first(&page).position.as_str(), first(&page).title.as_str()), ("#1", "The Zebra"));
    set_collection_order(&l, "release");
    assert_eq!((first(&page).position.as_str(), first(&page).title.as_str()), ("1999", "Apple"));
    set_collection_hide(&l, true);
    assert_eq!(page.get_movies().row_count(), 1, "only Mango is unwatched");
    collection_details(&l, 0);
    assert!(page.get_detail_open());
    assert_eq!(selected().unwrap().id, "c");

    // A show page: opens on the season of the up-next episode, Specials last.
    let show = MediaItem { kind: MediaKind::Series, ..item(SourceKind::Plex, "s", "Harbor Lights") };
    open_show(&l, show.clone());
    assert_eq!(l.global::<Nav>().get_page(), "show");
    let sp = l.global::<crate::Show>();
    assert_eq!(sp.get_state(), "loading");
    let ep = |season, n, views, resume: Option<u64>| MediaItem {
        id: format!("{season}x{n}"),
        kind: MediaKind::Episode,
        season: Some(season),
        episode: Some(n),
        subtitle: Some(format!("Ep {n}")),
        view_count: views,
        resume_ms: resume,
        duration_ms: Some(2_700_000),
        ..Default::default()
    };
    let season = |index, leaves, viewed| ShowSeason {
        id: format!("{index}"),
        index,
        title: String::new(),
        leaf_count: leaves,
        viewed_leaf_count: viewed,
    };
    send(
        &l,
        &t,
        AppEvent::Show(ShowData {
            show: MediaItem { year: Some(2024), genres: vec!["Mystery".into()], ..show },
            seasons: vec![season(0, 1, 0), season(1, 2, 2), season(2, 2, 0)],
            episodes: vec![
                ep(0, 1, 0, None),
                ep(1, 1, 1, None),
                ep(1, 2, 1, None),
                ep(2, 1, 0, Some(1_000_000)),
                ep(2, 2, 0, None),
            ],
        }),
    );
    assert_eq!(sp.get_state(), "ready");
    assert_eq!(sp.get_selector(), "tabs");
    let tabs: Vec<String> = sp.get_seasons().iter().map(|t| t.title.to_string()).collect();
    assert_eq!(tabs, ["Season 1", "Season 2", "Specials"]);
    assert_eq!(sp.get_season(), 1, "opens on the up-next season");
    assert_eq!(sp.get_action(), "Resume S2 · E1 on TV");
    assert_eq!(sp.get_up_next(), "Up next: S2 · E1 — Ep 1 · 28 min left");
    assert_eq!(sp.get_first(), "Play from S1 · E1");
    assert_eq!(sp.get_meta(), "2024 · 2 seasons · Mystery");
    let row = sp.get_episodes().row_data(0).unwrap();
    assert_eq!((row.number.as_str(), row.current, row.line.as_str()), ("01", true, "45 min · 28 min left"));
    select_season(&l, 2);
    let row = sp.get_episodes().row_data(0).unwrap();
    assert_eq!((row.number.as_str(), sp.get_seasons().row_data(0).unwrap().progress_text.as_str()), ("S1", "Watched"));
    assert_eq!(crate::bridge::show_actions::first().unwrap().id, "1x1");
}
