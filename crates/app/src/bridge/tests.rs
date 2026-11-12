//! Drive the real windows headless (Slint's testing backend) through events
//! and check what the pages would show.

use slint::Model;
use tvdash_core::models::{MediaKind, PlayState, RgbaImage};

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
    };
    send(&l, &t, AppEvent::Player(Some(status)));
    let tv = dash.get_tv();
    assert_eq!((tv.mode.as_str(), tv.title.as_str(), tv.paused), ("playing", "Other Film", true));
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
fn library_search_and_detail() {
    let (l, t) = windows();
    let lib = l.global::<Library>();
    send(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![item(SourceKind::Plex, "p1", "Plex Film")]));
    send(&l, &t, AppEvent::RecentlyAdded(SourceKind::Plex, vec![item(SourceKind::Plex, "p2", "New One")]));
    assert_eq!((lib.get_continue_watching().row_count(), lib.get_recently_added().row_count()), (1, 1));
    assert_eq!(l.global::<Dash>().get_cw_state(), "items");

    // Search: stale results for an old query are ignored.
    assert!(start_search(&l, " film "));
    assert!(lib.get_searching());
    assert_eq!(lib.get_results_title(), "RESULTS FOR “FILM”");
    send(&l, &t, AppEvent::SearchResults { query: "old".into(), items: vec![item(SourceKind::Plex, "x", "Stale")] });
    assert_eq!(lib.get_results().row_count(), 0);
    let found = vec![item(SourceKind::Plex, "p1", "Plex Film")];
    send(&l, &t, AppEvent::SearchResults { query: "film".into(), items: found });
    assert!(!lib.get_searching());
    assert_eq!(lib.get_results().row_count(), 1);

    // Detail panel, then its poster arrives.
    select(&l, "results", 0);
    assert!(lib.get_detail_open());
    let d = lib.get_detail();
    assert_eq!((d.title.as_str(), d.badge.as_str(), d.can_resume), ("Plex Film", "PLEX", true));
    assert_eq!(d.meta, "Movie · 2 h · 1 h left");
    assert!(!d.has_poster);
    let image = RgbaImage { width: 1, height: 1, pixels: vec![255; 4] };
    send(&l, &t, AppEvent::Poster { source: SourceKind::Plex, item_id: "p1".into(), image });
    assert!(lib.get_detail().has_poster);
    assert!(lib.get_results().row_data(0).unwrap().has_poster);
    assert_eq!(selected().unwrap().id, "p1");
    clear_search(&l);
    assert_eq!(lib.get_results_title(), "");
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
