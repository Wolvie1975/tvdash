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

        let names: Vec<String> = monitors.iter().map(|m| m.name().unwrap_or_default()).collect();
        let target = match pick_output(&names, &output, is_tv) {
            Some((i, exact)) => {
                if !exact {
                    warn!(
                        role,
                        configured = %output,
                        using = %names[i],
                        "configured output not found; picked one automatically \
                         (set [display] in config.toml to make it explicit)"
                    );
                }
                monitors.into_iter().nth(i)
            }
            None => None,
        };
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

/// Built-in panels: `eDP-1`, `LVDS-1`, `DSI-1`.
fn is_internal(name: &str) -> bool {
    ["eDP", "LVDS", "DSI"].iter().any(|p| name.starts_with(p))
}

/// Index of the monitor to use, and whether it matched the configured name.
/// Without an exact match: the laptop window takes the built-in panel and the
/// TV window the first external output.
fn pick_output(names: &[String], configured: &str, is_tv: bool) -> Option<(usize, bool)> {
    if let Some(i) = names.iter().position(|n| n == configured) {
        return Some((i, true));
    }
    names.iter().position(|n| is_internal(n) != is_tv).map(|i| (i, false))
}

#[cfg(test)]
mod tests {
    use super::pick_output;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_name_wins() {
        let n = names(&["eDP-1", "HDMI-1", "DP-2"]);
        assert_eq!(pick_output(&n, "DP-2", true), Some((2, true)));
        assert_eq!(pick_output(&n, "eDP-1", false), Some((0, true)));
    }

    #[test]
    fn falls_back_to_internal_and_external() {
        // The default config says HDMI-A-1, but this machine calls it HDMI-1.
        let n = names(&["eDP-1", "HDMI-1"]);
        assert_eq!(pick_output(&n, "HDMI-A-1", true), Some((1, false)));
        assert_eq!(pick_output(&n, "eDP-2", false), Some((0, false)));
        // Only one screen: the TV gets nothing to pick; it stays where it opened.
        assert_eq!(pick_output(&names(&["eDP-1"]), "HDMI-A-1", true), None);
    }
}
