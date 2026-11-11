// Dev tool: render both windows offscreen to PNG (no display needed).
// Usage: cargo run --example snapshot -- out.png laptop|tv
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;
slint::include_modules!();

struct P(Rc<MinimalSoftwareWindow>);
impl slint::platform::Platform for P {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> { Ok(self.0.clone()) }
}

fn main() {
    let out = std::env::args().nth(1).unwrap();
    let which = std::env::args().nth(2).unwrap();
    let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(P(w.clone()))).unwrap();
    let (width, height) = if which == "tv" { (1920, 1080) } else { (1600, 1000) };
    w.set_size(slint::PhysicalSize::new(width, height));
    let snap = if which == "tv" {
        let t = TvWindow::new().unwrap();
        t.set_clock_hh("07".into()); t.set_clock_mm("41".into()); t.set_date_text("Wednesday, 11 November".into());
        t.show().unwrap(); t.window().take_snapshot().unwrap()
    } else {
        let l = LaptopWindow::new().unwrap();
        l.set_clock_hh("07".into()); l.set_clock_mm("41".into()); l.set_date_text("Wednesday, 11 November".into());
        let sites: Vec<SharedString> = ["Netflix","Disney+","Hulu","Prime Video","Apple TV+","YouTube TV"].iter().map(|s| (*s).into()).collect();
        l.set_streaming_sites(ModelRc::new(VecModel::from(sites)));
        l.set_keyring_status("GNOME Keyring connected".into());
        l.set_keyring_dot(l.global::<Theme>().get_ok());
        l.show().unwrap(); l.window().take_snapshot().unwrap()
    };
    image::save_buffer(&out, snap.as_bytes(), snap.width(), snap.height(), image::ExtendedColorType::Rgba8).unwrap();
}
