// Dev tool: render windows offscreen to PNG (no display needed).
// Usage: cargo run --example snapshot -- out.png <mode> [height] [width]
// THEME=stone|mist picks a light theme (default dark).
// Modes: watch, settings-streaming, site-editor (CONFIRM=1), home, home-playing, home-next, home-code, home-streaming, library, library-search,
//        settings, setup-0 … setup-2, tv
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

slint::include_modules!();

#[path = "../src/tint.rs"]
#[allow(dead_code)]
mod tint;

use tvdash_core::config::ThemeMode;

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
    dash.set_clock_hh("21".into());
    dash.set_clock_mm("04".into());
    dash.set_weekday("Wednesday".into());
    dash.set_date_text("November 11".into());
    let tiles: Vec<SiteTileData> = ["Netflix", "Disney+", "Hulu", "Prime Video", "Apple TV+", "YouTube TV", "Max"]
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (bg, fg) = tint::site_colors(tint::site_hue(n, i), theme_mode);
            SiteTileData {
                index: i as i32,
                name: (*n).into(),
                monogram: n[..1].into(),
                bg,
                fg,
                logo: Image::default(),
                has_logo: false,
            }
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
        ("Some Show", "S2 · E5 · The Pilot", 0.33, true, "Jellyfin"),
        ("Plex Show", "S3 · E2 · Chapter One", 0.4, true, "Plex"),
        ("A Film", "1999", 0.8, true, "Jellyfin"),
        ("No Poster Show", "S1 · E2", 0.1, false, "Plex"),
    ]);

    let sites_rows = model(
        [
            ("Netflix", "Set up · opened today", true),
            ("Disney+", "Set up · opened 3 days ago", true),
            ("Hulu", "Not signed in yet", false),
            ("Prime Video", "Not signed in yet", false),
            ("Apple TV+", "Not signed in yet", false),
            ("YouTube TV", "Set up · opened yesterday", true),
        ]
        .iter()
        .enumerate()
        .map(|(i, (n, s, o))| {
            let (bg, fg) = tint::site_colors(tint::site_hue(n, i), theme_mode);
            SiteRow {
                index: i as i32,
                name: (*n).into(),
                url: format!("https://www.{}.com", n.to_lowercase().replace([' ', '+'], "")).into(),
                status: (*s).into(),
                opened: *o,
                hidden: i == 4,
                monogram: n[..1].into(),
                bg,
                fg,
            }
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
                at_text: "Stopped at 31:40".into(),
                left_text: "14 min left".into(),
                progress: 0.7,
                poster: fake_poster(60),
                has_poster: true,
                ..Default::default()
            });
            dash.set_tint(tint::card_tint(tint::DEFAULT, theme_mode));
        }
        "home-playing" => {
            dash.set_continue_watching(row);
            dash.set_cw_state("items".into());
            dash.set_tv(TvCard {
                mode: "playing".into(),
                label: "ON THE TV".into(),
                source: "Jellyfin".into(),
                title: "The Long Drive Home".into(),
                subtitle: "S2 · E7 · Crossroads".into(),
                at_text: "24:10 / 52:00".into(),
                left_text: "28 min left · ends 21:32".into(),
                progress: 0.46,
                paused: true,
                poster: fake_poster(140),
                has_poster: true,
                ..Default::default()
            });
            dash.set_tint(tint::card_tint(teal, theme_mode));
            dash.set_sessions(model(vec![SessionRow {
                badge: "Plex".into(),
                title: "Plex Film · 2001".into(),
                detail: "Bedroom TV · Paused · 1:02:10 / 2:00:00".into(),
            }]));
        }
        "library" | "library-search" => {
            l.global::<Nav>().set_page("library".into());
            let lib = l.global::<Library>();
            lib.set_continue_watching(row.clone());
            lib.set_recently_added(cards(&[
                ("New Film", "2026", 0.0, true, "Plex"),
                ("Docu Series", "S1 · E1 · Origins", 0.0, true, "Jellyfin"),
                ("Another Film", "2025", 0.0, false, "Jellyfin"),
            ]));
            if mode == "library-search" {
                lib.set_query("film".into());
                lib.set_results_title("RESULTS FOR “FILM”".into());
                lib.set_results(cards(&[
                    ("A Film", "1999", 0.8, true, "Jellyfin"),
                    ("Plex Film", "2001", 0.0, true, "Plex"),
                    ("Another Film", "2025", 0.0, false, "Jellyfin"),
                ]));
                lib.set_selected_section("results".into());
                lib.set_selected_index(0);
                lib.set_detail_open(true);
                lib.set_detail(DetailData {
                    title: "A Film".into(),
                    subtitle: "1999".into(),
                    badge: "JELLYFIN".into(),
                    meta: "Movie · 2 h 5 min · 25 min left".into(),
                    poster: fake_poster(0),
                    has_poster: true,
                    progress: 0.8,
                    can_resume: true,
                });
            }
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
                        account("jellyfin", "Jellyfin", "Not connected", "", "off", "Sign in", ""),
                    ]));
                }
                "tv-ambient" => ui.set_ambient(true),
                _ => {
                    ui.set_focus("continue".into());
                    ui.set_col(1);
                }
            }
        }
        "watch" => {
            l.global::<Nav>().set_page("watch".into());
            let w = l.global::<Watch>();
            w.set_query("matrix".into());
            let wc = |key: &str, title: &str, meta: &str, line: &str, tier: &str, hue: u8| WatchCard {
                key: key.into(),
                title: title.into(),
                meta: meta.into(),
                line: line.into(),
                tier: tier.into(),
                poster: fake_poster(hue),
                has_poster: hue > 0,
            };
            w.set_library(model(vec![wc(
                "movie:604",
                "The Matrix Reloaded",
                "2003 · Movie",
                "In Jellyfin",
                "library",
                30,
            )]));
            w.set_included(model(vec![
                wc("movie:603", "The Matrix", "1999 · Movie", "On Netflix", "included", 90),
                wc("movie:624860", "The Matrix Resurrections", "2021 · Movie", "On Max, Netflix", "included", 150),
            ]));
            w.set_extra(model(vec![
                wc("tv:1399", "Matrix Tales", "2011 · Show", "Peacock (not one of yours)", "extra", 200),
                wc("movie:606", "The Animatrix", "2003 · Movie", "Rent · Apple TV", "extra", 0),
            ]));
            w.set_none(model(vec![wc(
                "movie:605",
                "The Matrix Rare Cut",
                "Movie",
                "Not streaming in the US",
                "none",
                0,
            )]));
            w.set_selected("movie:603".into());
            w.set_detail_open(true);
            w.set_detail(wc("movie:603", "The Matrix", "1999 · Movie", "On Netflix", "included", 90));
            w.set_detail_overview(
                "A hacker learns the truth about his reality and his role in the war against its controllers.".into(),
            );
            w.set_detail_yours(model(vec!["Netflix".into()]));
            w.set_detail_elsewhere(model(vec!["Rent · Apple TV".into(), "Buy · Amazon Video".into()]));
            w.set_detail_actions(model(vec![
                WatchAction { label: "Open on Netflix".into(), kind: "open".into(), site: 0, primary: true },
                WatchAction { label: "All options on TMDB".into(), kind: "tmdb".into(), site: -1, primary: false },
            ]));
        }
        "settings-display" => {
            l.global::<Nav>().set_page("settings".into());
            l.global::<Nav>().set_settings_tab("display".into());
            l.global::<Display>().set_seconds(true);
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
                row("Jellyfin", "ok", "21:03 · 1 min ago · 4 in Continue Watching", ""),
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
        "site-editor" => {
            let ed = l.global::<SiteEditor>();
            ed.set_index(1);
            ed.set_name("Disney+".into());
            ed.set_url("https://www.disneyplus.com".into());
            ed.set_search("https://www.disneyplus.com/search".into());
            ed.set_providers("337".into());
            ed.set_error("Put {title} in the search page where the title goes.".into());
            ed.set_confirm_remove(std::env::var("CONFIRM").is_ok());
            ed.set_open(true);
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
                account("jellyfin", "Jellyfin", "Connected", "http://republic-city:8096", "ok", "", "Sign out"),
            ]));
            acc.set_confirm_clear(true);
        }
        m if m.starts_with("setup-") => {
            l.global::<Nav>().set_page("setup".into());
            let setup = l.global::<Setup>();
            let step: i32 = m[6..].parse().unwrap();
            setup.set_step(step);
            setup.set_jellyfin_url("http://republic-city:8096".into());
            if step > 0 {
                setup.set_plex_state("done".into());
            }
            match step {
                0 => {
                    setup.set_plex_state("code".into());
                    setup.set_plex_code("WXYZ".into());
                }
                1 => setup.set_jellyfin_state("idle".into()),
                _ => {}
            }
        }
        _ => {
            dash.set_cw_state("prompts".into());
            dash.set_prompts(model(vec![
                prompt("jellyfin", "Jellyfin isn't connected.", "Sign in to Jellyfin", false),
                prompt("plex", "Plex isn't connected.", "Sign in to Plex", false),
            ]));
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
