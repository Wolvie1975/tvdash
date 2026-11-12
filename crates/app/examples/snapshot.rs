// Dev tool: render windows offscreen to PNG (no display needed).
// Usage: cargo run --example snapshot -- out.png <mode> [height] [width]
// THEME=stone|mist picks a light theme (default dark).
// Modes: home-search, settings-streaming, home, home-playing, home-next, home-code, home-streaming, library (LIB_TAB=movies), browse, saver, saver-idle, collection, show (show-9, show-20 PICKER=1),
//        settings, setup-0, setup-1, tv
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

slint::include_modules!();

#[path = "../src/tint.rs"]
#[allow(dead_code)]
mod tint;

#[path = "../src/logos.rs"]
mod logos;

use tvdash_core::config::{APPS, ThemeMode};

struct P(Rc<MinimalSoftwareWindow>);
impl slint::platform::Platform for P {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn fake_poster(hue: u8) -> Image {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(200, 300);
    for (i, px) in buf.make_mut_slice().iter_mut().enumerate() {
        let y = (i / 200) as u32;
        *px = Rgba8Pixel { r: hue / 2, g: (y * 120 / 300) as u8 + 40, b: 255 - hue, a: 255 };
    }
    Image::from_rgba8(buf)
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn cards(list: &[(&str, &str, f32, bool, &str)]) -> ModelRc<PosterCardData> {
    model(
        list.iter()
            .enumerate()
            .map(|(i, (t, s, p, has, badge))| PosterCardData {
                accent: tint::progress_color(tint::Hue { degrees: (i * 70) as f32 + 200.0, saturation: 0.8 }),
                placeholder: tint::card_tint(tint::DEFAULT, ThemeMode::Dark).poster,
                title: (*t).into(),
                subtitle: (*s).into(),
                badge: (*badge).into(),
                progress: *p,
                has_poster: *has,
                poster: if *has { fake_poster((i * 70) as u8) } else { Image::default() },
            })
            .collect(),
    )
}

fn prompt(source: &str, message: &str, action: &str, warn: bool) -> AccountPrompt {
    AccountPrompt {
        source: source.into(),
        message: message.into(),
        action: action.into(),
        action_id: "sign-in".into(),
        warn,
    }
}

#[allow(clippy::too_many_arguments)]
fn account(
    id: &str,
    name: &str,
    status: &str,
    detail: &str,
    state: &str,
    primary: &str,
    secondary: &str,
) -> AccountRow {
    AccountRow {
        id: id.into(),
        name: name.into(),
        status: status.into(),
        detail: detail.into(),
        state: state.into(),
        primary: primary.into(),
        primary_id: "x".into(),
        secondary: secondary.into(),
        secondary_id: "x".into(),
        input_hint: "".into(),
    }
}

fn laptop(mode: &str, theme_mode: ThemeMode) -> LaptopWindow {
    let l = LaptopWindow::new().unwrap();
    let dash = l.global::<Dash>();
    let theme = l.global::<Theme>();
    theme.set_mode(match theme_mode {
        ThemeMode::Dark => UiTheme::Dark,
        ThemeMode::Stone => UiTheme::Stone,
        ThemeMode::Mist => UiTheme::Mist,
    });
    dash.set_clock_hh("9".into());
    dash.set_clock_mm("04".into());
    dash.set_clock_suffix("PM".into());
    dash.set_weekday("Wednesday".into());
    dash.set_date_text("November 11".into());
    let dark = theme_mode == ThemeMode::Dark;
    let tiles: Vec<SiteTileData> = APPS
        .iter()
        .enumerate()
        .map(|(i, a)| SiteTileData {
            index: i as i32,
            name: a.name.into(),
            logo: logos::logo(a.id, dark).unwrap_or_default(),
        })
        .collect();
    dash.set_streaming_sites(model(tiles));
    dash.set_keyring_status("GNOME Keyring connected".into());
    dash.set_keyring_dot(theme.get_ok());
    dash.set_weather_on(true);
    dash.set_weather_temp("48°F".into());
    dash.set_weather_detail("Light rain · H 55 L 39".into());
    dash.set_weather_icon("rain".into());
    let teal = tint::Hue { degrees: 182.0, saturation: 0.8 };

    let row = cards(&[
        ("Harbor Lights", "S1 · E3 · The Breakwater", 0.7, true, "S1 · E3"),
        ("Night Shift", "S4 · E1", 0.12, true, "S4 · E1"),
        ("Field Notes", "S1 · E9", 0.88, true, "S1 · E9"),
        ("Northbound", "S2 · E2", 0.2, true, "S2 · E2"),
        ("The Long Drive Home", "S2 · E7 · Crossroads", 0.46, true, "S2 · E7"),
        ("No Poster Show", "S3 · E5", 0.6, false, "S3 · E5"),
        ("Saltwater", "S1 · E1", 0.1, true, "S1 · E1"),
    ]);
    let movies = cards(&[
        ("The Quiet Year", "2019", 0.35, true, "1 h 5 min left"),
        ("Iron Valley", "2021", 0.8, true, "22 min left"),
        ("Low Tide", "2016", 0.08, true, "1 h 20 min left"),
    ]);
    dash.set_cw_tv(row.clone());
    dash.set_cw_movies(movies);
    if std::env::var("CW_TAB").as_deref() == Ok("movies") {
        dash.set_cw_tab("movies".into());
    }

    let opened = ["Set up · opened today", "Not signed in yet", "Set up · opened 3 days ago"];
    let sites_rows = model(
        APPS.iter()
            .enumerate()
            .map(|(i, a)| SiteRow {
                index: i as i32,
                name: a.name.into(),
                url: a.url.into(),
                status: opened[i % 3].into(),
                opened: i % 3 != 1,
                logo: logos::logo(a.id, dark).unwrap_or_default(),
            })
            .collect(),
    );
    l.global::<Accounts>().set_sites(sites_rows);

    match mode {
        "home-code" => {
            dash.set_cw_state("code".into());
            dash.set_signin_code("WXYZ".into());
            dash.set_cw_message("On any device, go to plex.tv/link and enter this code.".into());
        }
        "home-streaming" => {
            dash.set_cw_state("empty".into());
            dash.set_tv(TvCard {
                mode: "streaming".into(),
                label: "ON THE TV".into(),
                source: "Browser".into(),
                title: "Netflix".into(),
                subtitle: "Streaming full screen on the TV. Close the browser to go back to the idle screen.".into(),
                monogram: "N".into(),
                logo: logos::logo("netflix", dark).unwrap_or_default(),
                has_logo: true,
                ..Default::default()
            });
            dash.set_tint(tint::card_tint(tint::Hue { degrees: 338.0, saturation: 0.7 }, theme_mode));
        }
        "home-next" => {
            dash.set_continue_watching(row);
            dash.set_cw_state("items".into());
            dash.set_tv(TvCard {
                mode: "next".into(),
                label: "UP NEXT".into(),
                source: "Plex".into(),
                note: "Nothing on the TV. Pick up where you left off:".into(),
                title: "Harbor Lights".into(),
                subtitle: "S1 · E3 · The Breakwater".into(),
                episode_line: "Season 1 · Episode 3 — The Breakwater".into(),
                facts: "2024 · TV-PG · 45 min · Mystery".into(),
                genres: "Mystery · Coastal".into(),
                overview: if std::env::var("NO_OVERVIEW").is_ok() { "".into() } else {
                    "A storm cuts the harbour off from the mainland, and the lighthouse keeper finds a logbook that should not exist.".into()
                },
                at_text: "Stopped at 31:40".into(),
                left_text: "14 min left".into(),
                progress: 0.7,
                poster: fake_poster(60),
                has_poster: true,
                ..Default::default()
            });
            dash.set_tint(tint::card_tint(tint::DEFAULT, theme_mode));
        }
        "home-playing" | "home-search" => {
            dash.set_continue_watching(row);
            dash.set_cw_state("items".into());
            dash.set_tv(TvCard {
                mode: "playing".into(),
                label: "ON THE TV".into(),
                source: "Plex".into(),
                title: "The Long Drive Home".into(),
                subtitle: "S2 · E7 · Crossroads".into(),
                episode_line: "Season 2 · Episode 7 — Crossroads".into(),
                facts: "2024 · TV-14 · 52 min · Drama".into(),
                genres: "Drama · Road trip".into(),
                overview: "Three siblings take a wrong exit outside Amarillo and spend the night deciding whether to keep driving or turn back. Their mother's old map turns up in the glovebox, with a route nobody remembers drawing.".into(),
                at_text: "24:10 / 52:00".into(),
                left_text: "28 min left · ends 9:32 PM".into(),
                progress: 0.46,
                paused: false,
                poster: fake_poster(140),
                has_poster: true,
                ..Default::default()
            });
            dash.set_tint(tint::card_tint(teal, theme_mode));
            if mode == "home-search" {
                dash.set_search_open(true);
                let w = l.global::<Watch>();
                w.set_query("harbor".into());
                let wc = |title: &str, meta: &str, action: &str, primary: bool, hue: u8| WatchCard {
                    key: title.into(),
                    title: title.into(),
                    meta: meta.into(),
                    action: action.into(),
                    primary,
                    show: false,
                    poster: fake_poster(hue),
                    has_poster: hue > 0,
                };
                w.set_library(model(vec![wc("Harbor Lights", "TV series · 2024 · in Plex", "Resume on TV", true, 60)]));
                w.set_included(model(vec![
                    wc("The Harbor", "Movie · 2019 · included with Prime Video", "Open on TV", false, 120),
                    wc("Northern Lights", "TV series · 2021 · included with Netflix", "Open on TV", false, 200),
                ]));
                w.set_extra(model(vec![wc("Harbor Lights", "Movie · 1998 · Rent · Apple TV", "", false, 30)]));
                w.set_none(model(vec![wc(
                    "Lights Out at the Harbor",
                    "Movie · 2012 · not available in the US",
                    "",
                    false,
                    0,
                )]));
            }
        }
        "library" => {
            l.global::<Nav>().set_page("library".into());
            let lib = l.global::<Library>();
            lib.set_state("ready".into());
            let movies = std::env::var("LIB_TAB").as_deref() == Ok("movies");
            lib.set_tab(if movies { "movies" } else { "tv" }.into());
            let hue = |d: f32| tint::Hue { degrees: d, saturation: 0.7 };
            lib.set_has_pick(true);
            lib.set_pick_tint(tint::card_tint(hue(if movies { 265.0 } else { 195.0 }), theme_mode));
            lib.set_pick(if movies {
                PickData {
                    title: "Paper Moon City".into(),
                    meta: "Unwatched · Movie · 1 h 58 min · Plex".into(),
                    why: "Picked at random from movies you haven't watched.".into(),
                    action: "Play on TV".into(),
                    poster: fake_poster(160),
                    has_poster: true,
                }
            } else {
                PickData {
                    title: "Northbound".into(),
                    meta: "Unwatched series · 16 episodes · Plex".into(),
                    why: "A show you haven't started yet.".into(),
                    action: "Start S1 · E1 on TV".into(),
                    poster: fake_poster(90),
                    has_poster: true,
                }
            });
            let shelf = |title: &str, line: &str, d: f32, segments: i32, done: i32, progress: f32| {
                let t = tint::card_tint(hue(d), theme_mode);
                ShelfItem {
                    title: title.into(),
                    line: line.into(),
                    poster: fake_poster(d as u8),
                    has_poster: true,
                    card: t.card,
                    art: t.poster,
                    accent: t.accent,
                    sec: t.sec,
                    segments,
                    done,
                    progress,
                }
            };
            if movies {
                lib.set_box_title("CONTINUE WATCHING".into());
                lib.set_box_rule("Movies you started and didn't finish".into());
                lib.set_box_items(model(vec![
                    shelf("The Quiet Year", "1 h 5 min left", 45.0, 0, 0, 0.35),
                    shelf("Iron Valley", "22 min left", 20.0, 0, 0, 0.8),
                    shelf("Low Tide", "1 h 20 min left", 60.0, 0, 0, 0.08),
                ]));
            } else {
                lib.set_box_title("FINISH A SEASON".into());
                lib.set_box_rule("Shows with 3 or fewer episodes left".into());
                lib.set_box_items(model(vec![
                    shelf("Harbor Lights", "Season 1 · 2 episodes left", 215.0, 10, 8, 0.8),
                    shelf("Field Notes", "Season 1 · 1 episode left", 140.0, 10, 9, 0.9),
                    shelf("Night Shift", "Season 4 · 3 episodes left", 355.0, 8, 5, 0.6),
                ]));
            }
            let six = |names: [&str; 6]| {
                let list: Vec<(&str, &str, f32, bool, &str)> = names.iter().map(|n| (*n, "", 0.0, true, "")).collect();
                cards(&list)
            };
            lib.set_added(six([
                "Glass Rivers",
                "Second Summer",
                "The Long Way",
                "Ember Street",
                "Saltwater",
                "Quiet Hours",
            ]));
            lib.set_added_count(9);
            lib.set_added_rule("New shows and new episodes since Monday".into());
            lib.set_again(six(["North Pier", "Halfway House", "Red Kite", "Small Hours", "The Orchard", "Static"]));
            lib.set_again_count(14);
            lib.set_again_rule("Shows you've watched all the way through".into());
            let genres: Vec<GenreTile> =
                [("Drama", 38), ("Comedy", 24), ("Sci-fi", 12), ("Crime", 15), ("Documentary", 9), ("Animation", 11)]
                    .iter()
                    .zip([215.0, 45.0, 265.0, 355.0, 140.0, 195.0])
                    .map(|((n, c), d)| {
                        let t = tint::card_tint(tint::Hue { degrees: d, saturation: 0.6 }, theme_mode);
                        GenreTile {
                            name: (*n).into(),
                            count: c.to_string().into(),
                            bg: t.card,
                            edge: t.edge,
                            sec: t.sec,
                        }
                    })
                    .collect();
            lib.set_genres(model(genres));
            let colls: Vec<CollectionCard> = [
                ("Studio Ember", "8 movies · 3 watched", 30.0, 0.37),
                ("The Harbor Trilogy", "3 movies · all watched", 200.0, 1.0),
                ("Night Shift Films", "5 movies · 1 watched · smart", 280.0, 0.2),
                ("Summer Classics", "12 movies · 0 watched · smart", 60.0, 0.0),
                ("Paper Moon", "4 movies · 2 watched", 340.0, 0.5),
                ("Extra", "2 movies · 0 watched", 120.0, 0.0),
            ]
            .iter()
            .map(|(n, line, d, p)| {
                let t = tint::card_tint(tint::Hue { degrees: *d, saturation: 0.7 }, theme_mode);
                CollectionCard {
                    title: (*n).into(),
                    line: (*line).into(),
                    poster: fake_poster(*d as u8),
                    has_poster: true,
                    card: t.card,
                    art: t.poster,
                    accent: t.accent,
                    sec: t.sec,
                    track: t.track,
                    progress: *p,
                }
            })
            .collect();
            lib.set_collection_count(23);
            lib.set_collections(model(colls));
            if let Ok(sort) = std::env::var("SORT") {
                lib.set_sort(sort.as_str().into());
            }
            lib.set_sort_open(std::env::var("SORT_OPEN").is_ok());
        }
        "saver" | "saver-idle" => {
            let sv = l.global::<Saver>();
            sv.set_on(true);
            sv.set_time("12:32 PM".into());
            sv.set_day("Thursday".into());
            sv.set_has_weather(true);
            sv.set_temp("51°".into());
            sv.set_condition("partly sunny".into());
            sv.set_label("ON THE TV · NOTHING PLAYING".into());
            sv.set_title("Up next: Harbor Lights".into());
            sv.set_line("Season 1 · Episode 3 · 14 min left".into());
            sv.set_accent(tint::card_tint(tint::DEFAULT, ThemeMode::Dark).accent);
            if mode == "saver" {
                sv.set_label("ON THE TV · PLAYING · PLEX".into());
                sv.set_title("The Long Drive Home".into());
                sv.set_line("Season 2 · Episode 7 — Crossroads".into());
                sv.set_show_progress(true);
                sv.set_progress(0.46);
                sv.set_at("24:10 / 52:00".into());
                sv.set_ends("Ends at 1:00 PM".into());
                sv.set_accent(tint::card_tint(tint::Hue { degrees: 172.0, saturation: 0.8 }, ThemeMode::Dark).accent);
            }
        }
        "collection" => {
            l.global::<Nav>().set_page("collection".into());
            let c = l.global::<Collection>();
            let t = tint::card_tint(tint::Hue { degrees: 30.0, saturation: 0.7 }, theme_mode);
            c.set_tint(t.clone());
            c.set_crumb("Movies › Collections › Studio Ember".into());
            c.set_title("Studio Ember".into());
            c.set_meta("8 movies · 1998–2024 · 15 h 40 min in total".into());
            c.set_progress(3.0 / 8.0);
            c.set_watched_text("3 of 8 watched".into());
            c.set_summary("Hand-drawn films from the Ember studio, from its first feature to the newest.".into());
            c.set_up_next("Up next: The Lantern Keeper · 1 h 5 min left".into());
            c.set_action("Resume The Lantern Keeper on TV".into());
            c.set_can_shuffle(true);
            c.set_backdrop(fake_poster(30));
            c.set_has_backdrop(true);
            let names = [
                "Paper Kites",
                "The Lantern Keeper",
                "Moss Garden",
                "Night Ferry",
                "The Ember Fox",
                "Lighthouse Row",
                "Small Lights",
                "Saltwater Road",
            ];
            let movies: Vec<CollectionMovie> = names
                .iter()
                .enumerate()
                .map(|(i, n)| CollectionMovie {
                    position: format!("#{}", i + 1).into(),
                    title: (*n).into(),
                    line: format!(
                        "{} · {}",
                        1998 + i * 3,
                        if i < 3 {
                            "watched"
                        } else if i == 3 {
                            "1 h 5 min left"
                        } else {
                            "unwatched"
                        }
                    )
                    .into(),
                    poster: fake_poster((i * 30) as u8),
                    has_poster: true,
                    placeholder: t.poster,
                    accent: t.accent,
                    progress: if i == 3 { 0.4 } else { 0.0 },
                    watched: i < 3,
                })
                .collect();
            c.set_movies(model(movies));
        }
        m if m.starts_with("show") => {
            l.global::<Nav>().set_page("show".into());
            let sh = l.global::<Show>();
            sh.set_state("ready".into());
            sh.set_tint(tint::card_tint(tint::Hue { degrees: 215.0, saturation: 0.7 }, theme_mode));
            sh.set_crumb("TV / Series › Harbor Lights".into());
            sh.set_title("Harbor Lights".into());
            sh.set_meta("2024 · TV-PG · 3 seasons · Mystery".into());
            sh.set_summary("A coastal town, a lighthouse keeper and a logbook that keeps writing itself.".into());
            sh.set_up_next("Up next: S1 · E3 — The Breakwater · 14 min left".into());
            sh.set_action("Resume S1 · E3 on TV".into());
            sh.set_first("Play from S1 · E1".into());
            sh.set_backdrop(fake_poster(200));
            sh.set_has_backdrop(true);
            let n: usize = m.strip_prefix("show-").and_then(|x| x.parse().ok()).unwrap_or(3);
            let tabs: Vec<SeasonTab> = (0..n)
                .map(|i| {
                    let last = i + 1 == n && n > 2;
                    SeasonTab {
                        title: if last { "Specials".into() } else { format!("Season {}", i + 1).into() },
                        short: if last { "Sp".into() } else { format!("S{}", i + 1).into() },
                        progress_text: if i == 0 { "2 of 10 watched".into() } else { "10 episodes".into() },
                        progress: if i == 0 { 0.2 } else { 0.0 },
                    }
                })
                .collect();
            sh.set_selector(
                if n <= 6 {
                    "tabs"
                } else if n <= 14 {
                    "pills"
                } else {
                    "picker"
                }
                .into(),
            );
            sh.set_seasons_text(format!("{n} seasons · 0 watched").into());
            sh.set_seasons(model(tabs));
            sh.set_season(0);
            sh.set_picker_open(std::env::var("PICKER").is_ok());
            let titles = [
                "Low Water",
                "The Keeper",
                "The Breakwater",
                "Fog Signal",
                "Night Ferry",
                "Saltwater",
                "Harbor Lights",
            ];
            let rows: Vec<EpisodeRow> = titles
                .iter()
                .enumerate()
                .map(|(i, t)| EpisodeRow {
                    number: format!("{:02}", i + 1).into(),
                    title: (*t).into(),
                    line: if i < 2 {
                        "45 min · watched".into()
                    } else if i == 2 {
                        "45 min · 14 min left".into()
                    } else {
                        "45 min".into()
                    },
                    summary: "A storm cuts the harbour off from the mainland, and the keeper finds the logbook.".into(),
                    still: fake_poster((i * 40) as u8),
                    has_still: true,
                    placeholder: tint::card_tint(tint::DEFAULT, theme_mode).poster,
                    accent: tint::progress_color(tint::DEFAULT),
                    watched: i < 2,
                    progress: if i == 2 { 0.7 } else { 0.0 },
                    current: i == 2,
                })
                .collect();
            sh.set_episodes(model(rows));
        }
        "browse" => {
            l.global::<Nav>().set_page("browse".into());
            let b = l.global::<Browse>();
            b.set_title("All movies".into());
            b.set_count_text("412 titles".into());
            let names = [
                "Harbor Lights",
                "The Quiet Year",
                "Paper Moon City",
                "Iron Valley",
                "Low Tide",
                "Northbound",
                "The Orchard",
                "Static",
                "Glass Rivers",
                "Second Summer",
                "The Long Way",
                "Ember Street",
                "Saltwater",
                "Quiet Hours",
                "North Pier",
            ];
            let list: Vec<(&str, &str, f32, bool, &str)> = names
                .iter()
                .enumerate()
                .map(|(i, n)| (*n, "", if i % 4 == 1 { 0.4 } else { 0.0 }, i != 7, "Movie"))
                .collect();
            b.set_cards(cards(&list));
            b.set_selected(2);
            b.set_detail_open(true);
            b.set_detail(DetailData {
                title: "Paper Moon City".into(),
                subtitle: "".into(),
                badge: "PLEX".into(),
                meta: "Movie · 2024 · 1 h 52 min · Mystery".into(),
                extra: "Watched once".into(),
                overview: "A night-shift cartographer finds a district on the old maps that was never built, and a tram line that still runs to it.".into(),
                action: "Play on TV".into(),
                poster: fake_poster(160),
                has_poster: true,
                progress: 0.0,
                can_resume: false,
            });
        }
        m if m.starts_with("tv-") => {
            l.global::<Nav>().set_layout("tv".into());
            dash.set_continue_watching(row);
            dash.set_cw_state("items".into());
            dash.set_tv(TvCard {
                mode: "next".into(),
                label: "UP NEXT".into(),
                source: "Plex".into(),
                title: "Harbor Lights".into(),
                subtitle: "S1 · E3 · The Breakwater".into(),
                at_text: "Stopped at 31:40".into(),
                left_text: "14 min left".into(),
                progress: 0.7,
                poster: fake_poster(60),
                has_poster: true,
                ..Default::default()
            });
            dash.set_tint(tint::card_tint(tint::DEFAULT, theme_mode));
            let ui = l.global::<TvUi>();
            match m {
                "tv-rail" => {
                    ui.set_rail_open(true);
                    ui.set_focus("rail".into());
                    ui.set_col(3);
                }
                "tv-services" => {
                    ui.set_screen("services".into());
                    ui.set_focus("grid".into());
                    ui.set_col(6);
                }
                "tv-settings" => {
                    ui.set_screen("settings".into());
                    ui.set_focus("set-theme".into());
                    ui.set_col(1);
                    l.global::<Accounts>().set_accounts(model(vec![
                        account("plex", "Plex", "Connected", "", "ok", "", "Sign out"),
                        account("tmdb", "TMDB · Where to watch", "Not set up", "", "off", "Add token", ""),
                    ]));
                }
                "tv-ambient" => ui.set_ambient(true),
                _ => {
                    ui.set_focus("continue".into());
                    ui.set_col(1);
                }
            }
        }
        "settings-display" => {
            l.global::<Nav>().set_page("settings".into());
            l.global::<Nav>().set_settings_tab("display".into());
            l.global::<Display>().set_seconds(true);
            l.global::<TvUi>().set_layout_tv(false);
            let d = l.global::<Display>();
            d.set_location_name("Chicago, Illinois, United States".into());
            d.set_place_query("spring".into());
            d.set_places(model(vec![
                "Springfield, Illinois, United States".into(),
                "Springfield, Missouri, United States".into(),
                "Springfield, Massachusetts, United States".into(),
            ]));
        }
        "settings-diagnostics" => {
            l.global::<Nav>().set_page("settings".into());
            l.global::<Nav>().set_settings_tab("diagnostics".into());
            let row = |n: &str, st: &str, ok: &str, err: &str| HealthRow {
                name: n.into(),
                state: st.into(),
                last_ok: ok.into(),
                last_error: err.into(),
            };
            let d = l.global::<Diagnostics>();
            d.set_rows(model(vec![
                row(
                    "Plex",
                    "warn",
                    "20:41 · 23 min ago · 2 in Continue Watching",
                    "21:03 · server unreachable: connection refused",
                ),
                row("TMDB", "ok", "20:58 · 6 min ago · search “matrix”: 5 results", ""),
                row("Weather (Open-Meteo)", "ok", "20:55 · 9 min ago · 48 Light rain", ""),
                row("GNOME Keyring", "ok", "20:30 · 34 min ago · read/write OK", ""),
                row("mpv", "off", "", ""),
                row("Streaming browser", "ok", "20:12 · 52 min ago · opened Netflix", ""),
            ]));
            d.set_paths(model(vec![
                "Log file: /home/you/.local/state/tvdash/tvdash.log".into(),
                "Settings: /home/you/.config/tvdash/config.toml".into(),
            ]));
        }
        "settings-streaming" => {
            l.global::<Nav>().set_page("settings".into());
            l.global::<Nav>().set_settings_tab("streaming".into());
        }
        "settings" => {
            l.global::<Nav>().set_page("settings".into());
            let acc = l.global::<Accounts>();
            acc.set_accounts(model(vec![
                account(
                    "plex",
                    "Plex",
                    "Sign-in expired",
                    "http://192.168.1.20:32400",
                    "warn",
                    "Reconnect",
                    "Sign out",
                ),
                account("tmdb", "TMDB · Where to watch", "Connected", "", "ok", "", "Replace token"),
            ]));
            acc.set_confirm_clear(true);
        }
        m if m.starts_with("setup-") => {
            l.global::<Nav>().set_page("setup".into());
            let setup = l.global::<Setup>();
            let step: i32 = m[6..].parse().unwrap();
            setup.set_step(step);
            if step > 0 {
                setup.set_plex_state("done".into());
            }
            if step == 0 {
                setup.set_plex_state("code".into());
                setup.set_plex_code("WXYZ".into());
            }
        }
        _ => {
            dash.set_cw_state("prompts".into());
            dash.set_prompts(model(vec![prompt("plex", "Plex isn't connected.", "Sign in to Plex", false)]));
            dash.set_tv_prompts(model(vec![prompt("plex", "Plex sign-in expired.", "Reconnect", true)]));
        }
    }
    l
}

fn main() {
    let out = std::env::args().nth(1).expect("out.png");
    let mode = std::env::args().nth(2).expect("mode");
    let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(P(w.clone()))).unwrap();
    let snap = if mode == "tv" {
        w.set_size(slint::PhysicalSize::new(1920, 1080));
        let t = TvWindow::new().unwrap();
        t.set_clock_hh("21".into());
        t.set_clock_mm("04".into());
        t.set_date_text("Wednesday, November 11".into());
        t.show().unwrap();
        t.window().take_snapshot().unwrap()
    } else {
        let h = std::env::args().nth(3).and_then(|h| h.parse().ok()).unwrap_or(1080);
        let width = std::env::args().nth(4).and_then(|w| w.parse().ok()).unwrap_or(1920);
        w.set_size(slint::PhysicalSize::new(width, h));
        let theme = match std::env::var("THEME").as_deref() {
            Ok("stone") => ThemeMode::Stone,
            Ok("mist") => ThemeMode::Mist,
            _ => ThemeMode::Dark,
        };
        let l = laptop(&mode, theme);
        l.show().unwrap();
        // Let timers and animations (the crossfade) finish.
        for _ in 0..3 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        slint::platform::update_timers_and_animations();
        l.window().take_snapshot().unwrap()
    };
    image::save_buffer(&out, snap.as_bytes(), snap.width(), snap.height(), image::ExtendedColorType::Rgba8).unwrap();
}
