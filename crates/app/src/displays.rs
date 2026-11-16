//! Put each window full screen on its configured output (plan §7) and watch
//! for display changes to switch layouts automatically (Part A).
//!
//! Wayland apps can't position windows, but they can request full screen on a
//! specific monitor. Monitors are matched by connector name (`HDMI-A-1`, `eDP-1`).

use std::{cell::RefCell, time::{Duration, Instant}};

use futures_util::StreamExt;
use slint::{ComponentHandle, Timer};
use slint::winit_030::{
    WinitWindowAccessor,
    winit::{monitor::MonitorHandle, window::Fullscreen},
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};
use tvdash_core::{
    AppEvent, Command, Layout, LayoutSetting,
    config::DisplayConfig,
    layout::{decide, is_internal},
    models::Geometry,
};

use crate::{Accounts, LaptopWindow, Library, SiteEditor, TvWindow, Watch};

const DEBOUNCE: Duration = Duration::from_secs(3);
const POLL_FALLBACK: Duration = Duration::from_secs(5);
const DEFER_MAX: Duration = Duration::from_secs(30);

/// What switching layouts while running needs.
struct Shell {
    tv: TvWindow,
    display: DisplayConfig,
    fullscreen: bool,
    commands: UnboundedSender<Command>,
    events: UnboundedSender<AppEvent>,
    setting: LayoutSetting,
    cli_override: Option<Layout>,
    last_outputs: Vec<String>,
    defer_started: Option<Instant>,
    defer_timer: Option<Timer>,
}

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
}

/// Keep what's needed to switch layouts later (call once at startup).
pub fn remember(
    tv: &TvWindow,
    display: &DisplayConfig,
    fullscreen: bool,
    cli_override: Option<Layout>,
    commands: UnboundedSender<Command>,
    events: UnboundedSender<AppEvent>,
) {
    SHELL.set(Some(Shell {
        tv: tv.clone_strong(),
        display: display.clone(),
        fullscreen,
        commands,
        events,
        setting: display.layout,
        cli_override,
        last_outputs: Vec::new(),
        defer_started: None,
        defer_timer: None,
    }));
}

/// Query GNOME Mutter for currently active logical monitor connector names.
/// Returns `None` if Mutter D-Bus isn't available (e.g. non-GNOME session).
pub async fn active_outputs_dbus() -> Option<Vec<String>> {
    let conn = zbus::Connection::session().await.ok()?;
    let reply = conn
        .call_method(
            Some("org.gnome.Mutter.DisplayConfig"),
            "/org/gnome/Mutter/DisplayConfig",
            Some("org.gnome.Mutter.DisplayConfig"),
            "GetCurrentState",
            &(),
        )
        .await
        .ok()?;

    // GetCurrentState returns:
    // (u serial, a((ssss)a(siiddada{sv})a{sv}) monitors, a(iiduba(ssss)a{sv}) logical_monitors, a{sv} properties)
    type MonitorId = (String, String, String, String);
    type Mode = (String, i32, i32, f64, f64, Vec<f64>, std::collections::HashMap<String, zbus::zvariant::OwnedValue>);
    type Monitor = (MonitorId, Vec<Mode>, std::collections::HashMap<String, zbus::zvariant::OwnedValue>);
    type LogicalMonitor = (i32, i32, f64, u32, bool, Vec<MonitorId>, std::collections::HashMap<String, zbus::zvariant::OwnedValue>);

    let (_, _, logical_monitors, _): (
        u32,
        Vec<Monitor>,
        Vec<LogicalMonitor>,
        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    ) = reply.body().deserialize().ok()?;

    let mut connectors = Vec::new();
    for (_, _, _, _, _, monitors, _) in logical_monitors {
        for (connector, _, _, _) in monitors {
            if !connectors.contains(&connector) {
                connectors.push(connector);
            }
        }
    }
    Some(connectors)
}

#[zbus::proxy(
    interface = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path = "/org/gnome/Mutter/DisplayConfig"
)]
trait DisplayConfigProxy {
    #[zbus(signal)]
    fn monitors_changed(&self) -> zbus::Result<()>;
}

/// Watch for display changes and notify the UI thread after a 3 s debounce.
/// Falls back to 5 s polling if Mutter's D-Bus signal isn't available.
pub async fn watch(events: UnboundedSender<AppEvent>) {
    if let Err(e) = watch_dbus(&events).await {
        warn!(error = %e, "Mutter DisplayConfig watcher unavailable; polling displays every 5 s");
        let mut tick = tokio::time::interval(POLL_FALLBACK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tick.tick().await;
        loop {
            tick.tick().await;
            if events.send(AppEvent::DisplaysChanged).is_err() {
                break;
            }
        }
    }
}

async fn watch_dbus(events: &UnboundedSender<AppEvent>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = DisplayConfigProxyProxy::new(&conn).await?;
    let mut changes = proxy.receive_monitors_changed().await?;
    while changes.next().await.is_some() {
        // Closing a lid or plugging a TV can fire multiple signals; debounce 3 s.
        let _ = tokio::time::timeout(DEBOUNCE, async {
            while changes.next().await.is_some() {}
        })
        .await;
        if events.send(AppEvent::DisplaysChanged).is_err() {
            break;
        }
    }
    Ok(())
}

/// User changed Settings › Display › Layout (`auto` | `dual` | `tv`).
pub fn apply_layout_setting(laptop: &LaptopWindow, setting: LayoutSetting) {
    SHELL.with_borrow_mut(|shell| {
        if let Some(s) = shell {
            s.setting = setting;
            s.cli_override = None;
        }
    });
    recheck_displays(laptop, true);
}

/// Called on the UI thread when `AppEvent::DisplaysChanged` arrives.
pub fn on_displays_changed(laptop: &LaptopWindow) {
    recheck_displays(laptop, false);
}

fn has_unsaved_or_search(laptop: &LaptopWindow) -> bool {
    !laptop.global::<Library>().get_query().is_empty()
        || !laptop.global::<Watch>().get_query().is_empty()
        || laptop.global::<SiteEditor>().get_open()
        || !laptop.global::<Accounts>().get_editing().is_empty()
}

fn recheck_displays(laptop: &LaptopWindow, force_eval: bool) {
    let laptop_weak = laptop.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(laptop) = laptop_weak.upgrade() else { return };
        let window = match laptop.window().winit_window().await {
            Ok(w) => w,
            Err(_) => return,
        };
        let names: Vec<String> = window
            .available_monitors()
            .filter_map(|m| m.name())
            .collect();

        enum Action {
            None,
            Switch { layout: Layout, toast: Option<&'static str> },
            Defer,
        }

        let action = SHELL.with_borrow_mut(|shell| {
            let Some(s) = shell else { return Action::None };
            if !force_eval && s.last_outputs == names {
                return Action::None;
            }
            s.last_outputs = names.clone();

            let decision = decide(
                s.setting,
                &names,
                &s.display.tv_output,
                &s.display.laptop_output,
            );
            let _ = s.events.send(AppEvent::TvConnected(decision.tv_connected));

            let target = s.cli_override.unwrap_or(decision.layout);
            let current = crate::tv_nav::current(&laptop);
            if target == current {
                s.defer_started = None;
                s.defer_timer = None;
                return Action::None;
            }

            // Respect switch_while_running = false when in auto mode (unless user just changed the setting).
            if !force_eval && s.setting == LayoutSetting::Auto && !s.display.switch_while_running {
                return Action::None;
            }

            if !force_eval && has_unsaved_or_search(&laptop) {
                let started = *s.defer_started.get_or_insert_with(Instant::now);
                if started.elapsed() < DEFER_MAX {
                    return Action::Defer;
                }
            }
            s.defer_started = None;
            s.defer_timer = None;

            let toast = if !force_eval && s.setting == LayoutSetting::Auto {
                Some(match target {
                    Layout::Tv => "Switched to TV only — laptop screen is off",
                    Layout::Dual => "Switched to two screens — laptop screen is back",
                })
            } else {
                None
            };
            Action::Switch { layout: target, toast }
        });

        match action {
            Action::None => {}
            Action::Defer => {
                let weak = laptop.as_weak();
                let timer = Timer::default();
                timer.start(slint::TimerMode::SingleShot, Duration::from_secs(1), move || {
                    if let Some(l) = weak.upgrade() {
                        recheck_displays(&l, false);
                    }
                });
                SHELL.with_borrow_mut(|shell| {
                    if let Some(s) = shell {
                        s.defer_timer = Some(timer);
                    }
                });
            }
            Action::Switch { layout, toast } => {
                switch_layout(&laptop, layout);
                if let Some(msg) = toast {
                    SHELL.with_borrow(|shell| {
                        if let Some(s) = shell {
                            let _ = s.events.send(AppEvent::Toast(msg.into()));
                        }
                    });
                }
            }
        }
    });
}

/// Switch between two screens and TV only while running.
pub fn switch_layout(laptop: &LaptopWindow, layout: Layout) {
    SHELL.with_borrow(|shell| {
        let Some(s) = shell else { return };
        if crate::tv_nav::current(laptop) == layout {
            return;
        }
        info!(?layout, "switching layout");
        crate::tv_nav::set_layout(laptop, layout);
        match layout {
            Layout::Tv => {
                let _ = s.tv.hide();
                if !s.fullscreen {
                    laptop.window().set_size(slint::LogicalSize::new(1920.0, 1080.0));
                }
                spawn_placement(laptop, "tv", s.display.tv_output.clone(), s.fullscreen, Some(s.commands.clone()));
                crate::tv_nav::go_home(laptop);
            }
            Layout::Dual => {
                if let Err(e) = s.tv.show() {
                    warn!(error = %e, "couldn't show the TV window");
                }
                if !s.fullscreen {
                    laptop.window().set_size(slint::LogicalSize::new(1600.0, 1000.0));
                }
                place(&s.tv, laptop, &s.display, s.fullscreen, s.commands.clone());
            }
        }
        let _ = s.commands.send(Command::SetLayout(layout));
    });
}

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

/// TV-only layout: the main window goes full screen on the TV output.
pub fn place_tv_only(
    laptop: &LaptopWindow,
    display: &DisplayConfig,
    fullscreen: bool,
    commands: UnboundedSender<Command>,
) {
    spawn_placement(laptop, "tv", display.tv_output.clone(), fullscreen, Some(commands));
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
        } else {
            // Back on the laptop panel after a layout switch.
            window.set_cursor_visible(true);
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
            if let Some(name) = m.name() {
                let _ = tx.send(Command::TvOutput(name));
            }
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
