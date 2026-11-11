// Dev tool: render windows offscreen to PNG (no display needed).
// Usage: cargo run --example snapshot -- out.png <laptop|laptop-playing|laptop-code|tv> [height]
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::rc::Rc;
slint::include_modules!();

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

fn prompt(source: &str, message: &str, action: &str, warn: bool) -> AccountPrompt {
    AccountPrompt {
        source: source.into(),
        message: message.into(),
        action: action.into(),
        action_id: "sign-in".into(),
        warn,
    }
}

fn laptop(mode: &str) -> LaptopWindow {
    let l = LaptopWindow::new().unwrap();
    l.set_clock_hh("21".into());
    l.set_clock_mm("04".into());
    l.set_date_text("Wednesday, 11 November".into());
    let sites: Vec<SharedString> =
        ["Netflix", "Disney+", "Hulu", "Prime Video", "Apple TV+", "YouTube TV"].iter().map(|s| (*s).into()).collect();
    l.set_streaming_sites(ModelRc::new(VecModel::from(sites)));
    l.set_keyring_status("GNOME Keyring connected".into());
    l.set_keyring_dot(l.global::<Theme>().get_ok());
    match mode {
        "laptop-code" => {
            l.set_cw_state("code".into());
            l.set_signin_code("WXYZ".into());
            l.set_cw_message("On any device, go to plex.tv/link and enter this code.".into());
        }
        "laptop-playing" => {
            let cards: Vec<PosterCardData> = [
                ("Some Show", "S2 · E5 · The Pilot", 0.33, true),
                ("Plex Show", "S3 · E2 · Chapter One", 0.4, true),
                ("A Film", "1999", 0.8, true),
                ("No Poster Show", "S1 · E2", 0.1, false),
                ("Another Show", "S4 · E1 · Return", 0.55, true),
            ]
            .iter()
            .enumerate()
            .map(|(i, (t, s, p, has))| PosterCardData {
                title: (*t).into(),
                subtitle: (*s).into(),
                badge: if i == 1 { "Plex".into() } else { "Jellyfin".into() },
                progress: *p,
                has_poster: *has,
                poster: if *has { fake_poster((i * 70) as u8) } else { Image::default() },
            })
            .collect();
            l.set_continue_watching(ModelRc::new(VecModel::from(cards)));
            l.set_cw_state("items".into());
            l.set_now_playing_title("Some Show".into());
            l.set_now_playing_detail("Jellyfin · S2 · E5 · The Pilot · 12:34 / 45:00 · ends 21:36".into());
            l.set_now_playing_progress(0.28);
            l.set_transport_enabled(true);
            l.set_paused(true);
            let expired = prompt("plex", "Plex sign-in expired.", "Reconnect", true);
            l.set_prompts(ModelRc::new(VecModel::from(vec![prompt(
                "plex",
                "Can't reach Plex: connection refused",
                "Retry",
                true,
            )])));
            l.set_tv_prompts(ModelRc::new(VecModel::from(vec![expired])));
            l.set_sessions(ModelRc::new(VecModel::from(vec![SessionRow {
                badge: "Jellyfin".into(),
                title: "A Film · 1999".into(),
                detail: "Bedroom TV · Paused · 1:02:10 / 2:00:00".into(),
            }])));
        }
        _ => {
            l.set_cw_state("prompts".into());
            l.set_prompts(ModelRc::new(VecModel::from(vec![
                prompt("jellyfin", "Jellyfin isn't connected.", "Sign in to Jellyfin", false),
                prompt("plex", "Plex isn't connected.", "Sign in to Plex", false),
            ])));
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
        t.set_date_text("Wednesday, 11 November".into());
        t.show().unwrap();
        t.window().take_snapshot().unwrap()
    } else {
        let h = std::env::args().nth(3).and_then(|h| h.parse().ok()).unwrap_or(1000);
        w.set_size(slint::PhysicalSize::new(1600, h));
        let l = laptop(&mode);
        l.show().unwrap();
        l.window().take_snapshot().unwrap()
    };
    image::save_buffer(&out, snap.as_bytes(), snap.width(), snap.height(), image::ExtendedColorType::Rgba8).unwrap();
}
