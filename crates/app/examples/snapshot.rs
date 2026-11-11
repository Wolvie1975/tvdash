// Dev tool: render windows offscreen to PNG (no display needed).
// Usage: cargo run --example snapshot -- out.png <mode> [height] [width]
// THEME=stone|mist picks a light theme (default dark).
// Modes: home, home-playing, home-next, home-code, home-streaming, library, library-search,
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
            SiteTileData { name: (*n).into(), monogram: n[..1].into(), bg, fg, logo: Image::default(), has_logo: false }
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
        .map(|(n, s, o)| SiteRow { name: (*n).into(), status: (*s).into(), opened: *o })
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
        l.window().take_snapshot().unwrap()
    };
    image::save_buffer(&out, snap.as_bytes(), snap.width(), snap.height(), image::ExtendedColorType::Rgba8).unwrap();
}
