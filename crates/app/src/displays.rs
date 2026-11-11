//! Put each window full screen on its configured output (plan §7).
//!
//! Wayland apps can't position windows, but they can request full screen on a
//! specific monitor. Monitors are matched by connector name (`HDMI-A-1`, `eDP-1`).

use slint::ComponentHandle;
use slint::winit_030::{
    WinitWindowAccessor,
    winit::{monitor::MonitorHandle, window::Fullscreen},
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};
use tvdash_core::{Command, config::DisplayConfig, models::Geometry};

use crate::{LaptopWindow, TvWindow};

/// `commands` receives the TV output's geometry (for placing the streaming browser).
pub fn place(
    tv: &TvWindow,
    laptop: &LaptopWindow,
    display: &DisplayConfig,
    fullscreen: bool,
    commands: UnboundedSender<Command>,
) {
    spawn_placement(tv, "tv", display.tv_output.clone(), fullscreen, Some(commands));
    spawn_placement(laptop, "laptop", display.laptop_output.clone(), fullscreen, None);
}

/// Desktop (logical) geometry. winit reports Wayland outputs as logical × scale.
fn geometry(m: &MonitorHandle) -> Geometry {
    let scale = m.scale_factor();
    let pos = m.position().to_logical::<i32>(scale);
    let size = m.size().to_logical::<u32>(scale);
    Geometry { x: pos.x, y: pos.y, width: size.width, height: size.height }
}

fn spawn_placement<C: ComponentHandle + 'static>(
    component: &C,
    role: &'static str,
    output: String,
    fullscreen: bool,
    // Set for the TV window only.
    tv_commands: Option<UnboundedSender<Command>>,
) {
    let is_tv = tv_commands.is_some();
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

        let target = monitors.into_iter().find(|m| m.name().as_deref() == Some(output.as_str()));
        if let (Some(m), Some(tx)) = (&target, &tv_commands) {
            let g = geometry(m);
            info!(?g, "TV output geometry");
            let _ = tx.send(Command::TvGeometry(g));
        }

        if !fullscreen {
            info!(role, "windowed mode; not going full screen");
            return;
        }

        match target {
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
