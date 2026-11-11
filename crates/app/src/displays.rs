//! Put each window full screen on its configured output (plan §7).
//!
//! Wayland apps can't position windows, but they can request full screen on a
//! specific monitor. Monitors are matched by connector name (`HDMI-A-1`, `eDP-1`).

use slint::ComponentHandle;
use slint::winit_030::{
    WinitWindowAccessor,
    winit::{monitor::MonitorHandle, window::Fullscreen},
};
use tracing::{info, warn};
use tvdash_core::config::DisplayConfig;

use crate::{LaptopWindow, TvWindow};

pub fn place(tv: &TvWindow, laptop: &LaptopWindow, display: &DisplayConfig, fullscreen: bool) {
    spawn_placement(tv, "tv", display.tv_output.clone(), fullscreen, true);
    spawn_placement(laptop, "laptop", display.laptop_output.clone(), fullscreen, false);
}

fn spawn_placement<C: ComponentHandle + 'static>(
    component: &C,
    role: &'static str,
    output: String,
    fullscreen: bool,
    is_tv: bool,
) {
    let component = component.clone_strong();
    let spawned = slint::spawn_local(async move {
        let window = match component.window().winit_window().await {
            Ok(w) => w,
            Err(e) => {
                warn!(role, error = %e, "no winit window; can't place it");
                return;
            }
        };

        let monitors: Vec<MonitorHandle> = window.available_monitors().collect();
        if is_tv {
            // Logged once, so config.toml's [display] names are easy to fill in.
            for m in &monitors {
                info!(
                    name = m.name().as_deref().unwrap_or("?"),
                    size = ?m.size(),
                    scale = m.scale_factor(),
                    "available monitor"
                );
            }
            // Nobody should see a mouse pointer on the TV.
            window.set_cursor_visible(false);
        }

        if !fullscreen {
            info!(role, "windowed mode; not going full screen");
            return;
        }

        match monitors.into_iter().find(|m| m.name().as_deref() == Some(output.as_str())) {
            Some(m) => {
                info!(role, output = %output, "full screen on configured output");
                window.set_fullscreen(Some(Fullscreen::Borderless(Some(m))));
            }
            None => {
                warn!(
                    role,
                    output = %output,
                    "configured output not found; going full screen on the current display \
                     (check [display] in config.toml against the monitor list above)"
                );
                window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            }
        }
    });
    if let Err(e) = spawned {
        warn!(role, error = %e, "couldn't schedule window placement");
    }
}
