//! Focus navigation for the TV-only layout (arrows + OK + Back).
//!
//! Each screen is a list of rows built from the live models (hero buttons,
//! Continue Watching, services…). This module keeps which row and item have
//! focus, remembers the last item per row so moving up and down doesn't lose
//! your place, and runs the focused item's action through the same callbacks
//! the laptop layout uses.

use std::{
    cell::RefCell,
    collections::HashMap,
    time::{Duration, Instant},
};

use slint::{ComponentHandle, Model, SharedString, platform::Key};
use tvdash_core::config::Layout;

use crate::{Accounts, Dash, Display, LaptopWindow, Library, TvUi, UiTheme};

/// Ambient idle starts after this long without a key on Home.
pub const AMBIENT_AFTER: Duration = Duration::from_secs(5 * 60);
const SCREENS: [&str; 5] = ["home", "search", "library", "services", "settings"];
/// Rail positions: the five screens, then the theme button.
const RAIL_THEME: usize = 5;
/// Services grid columns (matches tv/shell.slint).
const GRID_COLUMNS: usize = 5;

struct NavState {
    /// Last focused item per row id, so coming back lands on the same item.
    cols: HashMap<&'static str, usize>,
    last_key: Instant,
}

thread_local! {
    static S: RefCell<NavState> =
        RefCell::new(NavState { cols: HashMap::new(), last_key: Instant::now() });
}

/// Set the starting layout and take the TV shell's keys.
pub fn init(laptop: &LaptopWindow, layout: Layout) {
    set_layout(laptop, layout);
    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_key(move |text| weak.upgrade().is_some_and(|l| key(&l, &text)));
    sync(laptop);
}

/// Show the dashboard pages or the TV shell in the main window.
pub fn set_layout(laptop: &LaptopWindow, layout: Layout) {
    laptop.global::<crate::Nav>().set_layout(if layout == Layout::Tv { "tv" } else { "dual" }.into());
    laptop.global::<TvUi>().set_layout_tv(layout == Layout::Tv);
    if layout == Layout::Tv {
        sync(laptop);
    }
}

pub fn current(laptop: &LaptopWindow) -> Layout {
    if is_tv(laptop) { Layout::Tv } else { Layout::Dual }
}

fn is_tv(laptop: &LaptopWindow) -> bool {
    laptop.global::<crate::Nav>().get_layout() == "tv"
}

/// Rows of the current screen with how many items each has (empty rows left out).
fn rows(laptop: &LaptopWindow) -> Vec<(&'static str, usize)> {
    let ui = laptop.global::<TvUi>();
    let dash = laptop.global::<Dash>();
    let rows: Vec<(&'static str, usize)> = match ui.get_screen().as_str() {
        "home" => {
            let hero = match dash.get_tv().mode.as_str() {
                // Play/Pause, (Full screen,) Stop
                "playing" => 2 + ui.get_video_on() as usize,
                "streaming" | "next" => 1,
                _ => 0,
            };
            let cw = dash.get_cw_state();
            let prompts = match cw.as_str() {
                "code" | "busy" => 1,
                _ => dash.get_prompts().row_count(),
            };
            let continue_ = if cw == "items" { dash.get_continue_watching().row_count() } else { 0 };
            vec![
                ("hero", hero),
                ("prompts", prompts),
                ("continue", continue_),
                ("services", dash.get_streaming_sites().row_count()),
            ]
        }
        "services" => vec![("grid", dash.get_streaming_sites().row_count())],
        "library" => {
            let lib = laptop.global::<Library>();
            vec![
                ("lib-continue", lib.get_continue_watching().row_count()),
                ("lib-recent", lib.get_recently_added().row_count()),
            ]
        }
        "settings" => vec![
            ("set-layout", 2),
            ("set-theme", 3),
            ("set-units", 2),
            ("set-clock", 2),
            ("set-accounts", laptop.global::<Accounts>().get_accounts().row_count()),
            ("set-quit", 1),
        ],
        _ => vec![],
    };
    rows.into_iter().filter(|(_, n)| *n > 0).collect()
}

/// Put focus somewhere valid after the screen's rows changed (new data, playback started…).
pub fn sync(laptop: &LaptopWindow) {
    if !is_tv(laptop) {
        return;
    }
    let ui = laptop.global::<TvUi>();
    if ui.get_focus() == "rail" {
        return;
    }
    let rows = rows(laptop);
    let focus = ui.get_focus();
    match rows.iter().find(|(id, _)| *id == focus.as_str()) {
        Some((_, n)) => {
            if ui.get_col() as usize >= *n {
                ui.set_col(*n as i32 - 1);
            }
        }
        None => match rows.first() {
            Some((id, n)) => focus_row(laptop, id, *n),
            None => {
                ui.set_focus("".into());
                ui.set_col(0);
            }
        },
    }
}

fn focus_row(laptop: &LaptopWindow, id: &'static str, len: usize) {
    let ui = laptop.global::<TvUi>();
    let col = S.with_borrow(|s| s.cols.get(id).copied().unwrap_or(0)).min(len.saturating_sub(1));
    ui.set_focus(id.into());
    ui.set_col(col as i32);
}

fn remember(laptop: &LaptopWindow) {
    let ui = laptop.global::<TvUi>();
    let focus = ui.get_focus();
    if let Some((id, _)) = rows(laptop).into_iter().find(|(id, _)| *id == focus.as_str()) {
        S.with_borrow_mut(|s| s.cols.insert(id, ui.get_col() as usize));
    }
}

fn open_rail(laptop: &LaptopWindow) {
    remember(laptop);
    let ui = laptop.global::<TvUi>();
    let current = SCREENS.iter().position(|s| *s == ui.get_screen().as_str()).unwrap_or(0);
    ui.set_rail_open(true);
    ui.set_focus("rail".into());
    ui.set_col(current as i32);
}

fn close_rail(laptop: &LaptopWindow) {
    let ui = laptop.global::<TvUi>();
    ui.set_rail_open(false);
    ui.set_focus("".into());
    sync(laptop);
}

fn show(laptop: &LaptopWindow, screen: &str) {
    let ui = laptop.global::<TvUi>();
    ui.set_screen(screen.into());
    ui.set_rail_open(false);
    ui.set_focus("".into());
    sync(laptop);
}

/// Back to Home (the `tvdash --home` shortcut, Back on Home's rail…).
pub fn go_home(laptop: &LaptopWindow) {
    S.with_borrow_mut(|s| s.last_key = Instant::now());
    let ui = laptop.global::<TvUi>();
    ui.set_ambient(false);
    ui.set_video_full(false); // a video in the window keeps playing behind Home
    show(laptop, "home");
    ui.set_focus_ticket(ui.get_focus_ticket() + 1);
}

/// Once a minute: ambient idle after a few quiet minutes on Home.
pub fn tick(laptop: &LaptopWindow) {
    if !is_tv(laptop) {
        return;
    }
    let ui = laptop.global::<TvUi>();
    let quiet = S.with_borrow(|s| s.last_key.elapsed() >= AMBIENT_AFTER);
    let mode = laptop.global::<Dash>().get_tv().mode;
    if quiet && ui.get_screen() == "home" && !ui.get_rail_open() && (mode == "idle" || mode == "next") {
        ui.set_ambient(true);
    }
}

fn is(text: &SharedString, key: Key) -> bool {
    *text == SharedString::from(key)
}

/// Handle one key. Returns true when it was used.
pub fn key(laptop: &LaptopWindow, text: &SharedString) -> bool {
    S.with_borrow_mut(|s| s.last_key = Instant::now());
    let ui = laptop.global::<TvUi>();
    if ui.get_ambient() {
        ui.set_ambient(false);
        return true;
    }
    if ui.get_video_full() {
        return video_key(laptop, text);
    }
    let (left, right, up, down) =
        (is(text, Key::LeftArrow), is(text, Key::RightArrow), is(text, Key::UpArrow), is(text, Key::DownArrow));
    let ok = is(text, Key::Return) || text.as_str() == " " || text.as_str() == "\n";
    let back = is(text, Key::Escape) || is(text, Key::Backspace);

    // ── Rail ──
    if ui.get_focus() == "rail" {
        let col = ui.get_col() as usize;
        if up {
            ui.set_col(col.saturating_sub(1) as i32);
        } else if down {
            ui.set_col((col + 1).min(RAIL_THEME) as i32);
        } else if right || back {
            close_rail(laptop);
        } else if ok {
            if col == RAIL_THEME {
                cycle_theme(laptop);
            } else {
                show(laptop, SCREENS[col]);
            }
        } else {
            return false;
        }
        return true;
    }

    // ── Screen ──
    sync(laptop);
    let rows = rows(laptop);
    let focus = ui.get_focus();
    let at = rows.iter().position(|(id, _)| *id == focus.as_str());
    let col = ui.get_col() as usize;
    let Some(at) = at else {
        if left {
            open_rail(laptop);
            return true;
        }
        if back && ui.get_screen() != "home" {
            show(laptop, "home");
            return true;
        }
        return back;
    };
    let (id, len) = rows[at];

    if id == "grid" {
        if left && col.is_multiple_of(GRID_COLUMNS) {
            open_rail(laptop);
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right {
            ui.set_col((col + 1).min(len - 1) as i32);
        } else if up && col >= GRID_COLUMNS {
            ui.set_col((col - GRID_COLUMNS) as i32);
        } else if down && col + GRID_COLUMNS < len {
            ui.set_col((col + GRID_COLUMNS) as i32);
        }
    } else if left && col == 0 {
        open_rail(laptop);
    } else if left {
        ui.set_col(col as i32 - 1);
    } else if right {
        ui.set_col((col + 1).min(len - 1) as i32);
    } else if up && at > 0 {
        remember(laptop);
        focus_row(laptop, rows[at - 1].0, rows[at - 1].1);
    } else if down && at + 1 < rows.len() {
        remember(laptop);
        focus_row(laptop, rows[at + 1].0, rows[at + 1].1);
    }
    if ok {
        activate(laptop, id, col);
    } else if back {
        if ui.get_screen() != "home" {
            show(laptop, "home");
        } else {
            open_rail(laptop);
        }
    }
    left || right || up || down || ok || back
}

/// Keys while a video fills the screen: OK (or ↑/↓) shows the overlay, Space
/// pauses, ←/→ seek, Back shows Home over the still-playing video.
fn video_key(laptop: &LaptopWindow, text: &SharedString) -> bool {
    let (ui, dash) = (laptop.global::<TvUi>(), laptop.global::<Dash>());
    if is(text, Key::Return) || text.as_str() == "\n" || is(text, Key::UpArrow) || is(text, Key::DownArrow) {
        ui.invoke_overlay();
    } else if text.as_str() == " " {
        dash.invoke_transport("play-pause".into());
    } else if is(text, Key::LeftArrow) {
        dash.invoke_transport("back".into());
    } else if is(text, Key::RightArrow) {
        dash.invoke_transport("forward".into());
    } else if is(text, Key::Escape) || is(text, Key::Backspace) {
        ui.set_video_full(false);
        show(laptop, "home");
        focus_row(laptop, "hero", rows(laptop).iter().find(|(id, _)| *id == "hero").map_or(1, |(_, n)| *n));
    } else {
        return false;
    }
    true
}

fn cycle_theme(laptop: &LaptopWindow) {
    let next = match laptop.global::<crate::Theme>().get_mode() {
        UiTheme::Dark => UiTheme::Stone,
        UiTheme::Stone => UiTheme::Mist,
        _ => UiTheme::Dark,
    };
    laptop.global::<Dash>().invoke_set_theme(next);
}

fn activate(laptop: &LaptopWindow, row: &str, col: usize) {
    let dash = laptop.global::<Dash>();
    match row {
        "hero" => match (dash.get_tv().mode.as_str(), col) {
            ("playing", 0) => dash.invoke_transport("play-pause".into()),
            ("playing", 1) if laptop.global::<TvUi>().get_video_on() => laptop.global::<TvUi>().set_video_full(true),
            ("playing", _) | ("streaming", _) => dash.invoke_transport("stop".into()),
            ("next", _) => dash.invoke_play_item(0),
            _ => {}
        },
        "prompts" => match dash.get_cw_state().as_str() {
            "code" | "busy" => dash.invoke_cancel_sign_in(dash.get_signin_source()),
            _ => {
                if let Some(p) = dash.get_prompts().row_data(col) {
                    dash.invoke_prompt_action(p.source, p.action_id);
                }
            }
        },
        "continue" => dash.invoke_play_item(col as i32),
        "services" | "grid" => {
            if let Some(site) = dash.get_streaming_sites().row_data(col) {
                dash.invoke_open_site(site.index);
            }
        }
        "lib-continue" | "lib-recent" => {
            let lib = laptop.global::<Library>();
            let section = if row == "lib-continue" { "continue" } else { "recent" };
            lib.invoke_select(section.into(), col as i32);
            lib.invoke_play_detail(false);
        }
        "set-layout" => {
            let layout = if col == 1 { Layout::Tv } else { Layout::Dual };
            crate::displays::switch_layout(laptop, layout);
        }
        "set-theme" => dash.invoke_set_theme([UiTheme::Dark, UiTheme::Stone, UiTheme::Mist][col.min(2)]),
        "set-units" | "set-clock" => {
            let d = laptop.global::<Display>();
            if row == "set-units" {
                d.set_celsius(col == 1);
            } else {
                d.set_h12(col == 1);
            }
            d.invoke_changed();
        }
        "set-accounts" => {
            if let Some(a) = laptop.global::<Accounts>().get_accounts().row_data(col) {
                let action = if !a.primary_id.is_empty() { a.primary_id } else { a.secondary_id };
                // Token entry needs typing; that stays on the laptop layout for now.
                if !action.is_empty() && action != "edit" {
                    laptop.global::<Accounts>().invoke_action(a.id, action);
                }
            }
        }
        "set-quit" => dash.invoke_quit(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    //! Drive the TV layout headless with key presses.

    use super::*;
    use crate::bridge::{self, UiConfig};
    use std::sync::mpsc;
    use tvdash_core::{
        AppEvent, Command,
        config::{ClockFormat, StreamingSite, ThemeMode, Units},
        models::{MediaItem, MediaKind, SourceKind},
    };

    fn press(l: &LaptopWindow, k: Key) -> bool {
        key(l, &SharedString::from(k))
    }

    fn item(id: &str) -> MediaItem {
        MediaItem {
            source: SourceKind::Plex,
            id: id.into(),
            kind: MediaKind::Movie,
            title: id.into(),
            subtitle: None,
            episode_label: None,
            duration_ms: Some(1000),
            resume_ms: Some(500),
            poster: None,
        }
    }

    fn setup() -> (LaptopWindow, mpsc::Receiver<Command>) {
        i_slint_backend_testing::init_no_event_loop();
        let l = LaptopWindow::new().unwrap();
        let site = |n: &str| StreamingSite { name: n.into(), url: "https://x.example".into(), ..Default::default() };
        bridge::init(
            &l,
            UiConfig {
                tz: None,
                clock: ClockFormat::H24,
                location: String::new(),
                seconds: false,
                paths: vec![],
                units: Units::Fahrenheit,
                theme: ThemeMode::Dark,
                sites: (0..7).map(|i| site(&format!("S{i}"))).collect(),
            },
        );
        let (tx, rx) = mpsc::channel();
        let (utx, mut urx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || {
            while let Some(c) = urx.blocking_recv() {
                let _ = tx.send(c);
            }
        });
        crate::callbacks::wire(&l, utx.clone());
        let tv = crate::TvWindow::new().unwrap();
        crate::displays::remember(&tv, &tvdash_core::config::DisplayConfig::default(), false, utx);
        init(&l, Layout::Tv);
        (l, rx)
    }

    fn next_cmd(rx: &mpsc::Receiver<Command>) -> Command {
        rx.recv_timeout(Duration::from_secs(2)).expect("a command")
    }

    #[test]
    fn arrows_rows_rail_and_activation() {
        let (l, rx) = setup();
        let ui = l.global::<TvUi>();
        assert_eq!(l.global::<crate::Nav>().get_layout(), "tv");
        // Nothing playing, nothing to resume: services row has focus.
        assert_eq!((ui.get_focus().as_str(), ui.get_col()), ("services", 0));

        // Continue Watching arrives: the hero (Resume) and the row appear.
        let t = crate::TvWindow::new().unwrap();
        bridge::apply_for_test(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![item("a"), item("b")]));
        assert_eq!(l.global::<Dash>().get_tv().mode, "next");
        assert!(press(&l, Key::UpArrow));
        assert_eq!(ui.get_focus(), "continue");
        assert!(press(&l, Key::RightArrow) && press(&l, Key::RightArrow));
        assert_eq!(ui.get_col(), 1, "clamped to the row");
        // Down to services and back up: the row remembers where we were.
        press(&l, Key::DownArrow);
        assert_eq!(ui.get_focus(), "services");
        press(&l, Key::UpArrow);
        assert_eq!((ui.get_focus().as_str(), ui.get_col()), ("continue", 1));
        // OK plays the focused card.
        press(&l, Key::Return);
        assert!(matches!(next_cmd(&rx), Command::Play(i) if i.id == "b"));

        // Left from the first item opens the rail on the current screen.
        press(&l, Key::LeftArrow);
        press(&l, Key::LeftArrow);
        assert!(ui.get_rail_open());
        assert_eq!((ui.get_focus().as_str(), ui.get_col()), ("rail", 0));
        // Down to Services, OK: the services grid.
        for _ in 0..3 {
            press(&l, Key::DownArrow);
        }
        press(&l, Key::Return);
        assert_eq!((ui.get_screen().as_str(), ui.get_focus().as_str()), ("services", "grid"));
        assert!(!ui.get_rail_open());
        // Grid: down moves a whole row (5); OK opens the site.
        press(&l, Key::DownArrow);
        assert_eq!(ui.get_col(), 5);
        press(&l, Key::Return);
        assert!(matches!(next_cmd(&rx), Command::OpenSite(5)));
        // Back returns Home.
        press(&l, Key::Escape);
        assert_eq!(ui.get_screen(), "home");

        // Theme from the rail.
        press(&l, Key::LeftArrow);
        while ui.get_focus() != "rail" {
            press(&l, Key::LeftArrow);
        }
        for _ in 0..6 {
            press(&l, Key::DownArrow);
        }
        assert_eq!(ui.get_col(), RAIL_THEME as i32);
        press(&l, Key::Return);
        assert_eq!(l.global::<crate::Theme>().get_mode(), UiTheme::Stone);
        assert!(matches!(next_cmd(&rx), Command::SetTheme(ThemeMode::Stone)));
    }

    #[test]
    fn video_in_the_window_full_screen_and_home() {
        use tvdash_core::{events::PlayerStatus, models::PlayState};
        let (l, rx) = setup();
        let ui = l.global::<TvUi>();
        let t = crate::TvWindow::new().unwrap();
        let status =
            PlayerStatus { item: item("a"), state: PlayState::Playing, position_ms: 0, duration_ms: Some(1000) };
        bridge::apply_for_test(&l, &t, AppEvent::Player(Some(status)));
        // The window draws the video: Home's card gets Full screen between Pause and Stop.
        ui.set_video_on(true);
        sync(&l);
        assert_eq!(rows(&l)[0], ("hero", 3));
        while ui.get_focus() != "hero" {
            assert!(press(&l, Key::UpArrow));
        }
        press(&l, Key::RightArrow);
        press(&l, Key::Return);
        assert!(ui.get_video_full());

        // Full screen: Space pauses, OK shows the overlay, arrows seek.
        assert!(key(&l, &SharedString::from(" ")));
        assert!(matches!(next_cmd(&rx), Command::Pause));
        press(&l, Key::Return);
        assert!(matches!(next_cmd(&rx), Command::Overlay));
        press(&l, Key::LeftArrow);
        assert!(matches!(next_cmd(&rx), Command::Seek(-10)));
        press(&l, Key::RightArrow);
        assert!(matches!(next_cmd(&rx), Command::Seek(30)));
        // Back: Home over the still-playing video, focus on the card.
        press(&l, Key::Escape);
        assert!(!ui.get_video_full() && ui.get_video_on());
        assert_eq!((ui.get_screen().as_str(), ui.get_focus().as_str()), ("home", "hero"));
        // Stop is the card's last button.
        ui.set_col(2);
        press(&l, Key::Return);
        assert!(matches!(next_cmd(&rx), Command::Stop));
        // `tvdash --home` while full screen also shows Home (the video keeps playing).
        ui.set_video_full(true);
        go_home(&l);
        assert!(!ui.get_video_full());
    }

    #[test]
    fn settings_ambient_and_home() {
        let (l, rx) = setup();
        let ui = l.global::<TvUi>();
        show(&l, "settings");
        assert_eq!(ui.get_focus(), "set-layout");
        press(&l, Key::Return); // "Two screens": switches right away, and is saved
        assert!(matches!(next_cmd(&rx), Command::SetLayout(Layout::Dual)));
        assert!(!ui.get_layout_tv());
        assert_eq!(current(&l), Layout::Dual);
        // …and back from the laptop's Settings › Display.
        l.global::<Display>().invoke_set_layout(true);
        assert!(matches!(next_cmd(&rx), Command::SetLayout(Layout::Tv)));
        assert_eq!((current(&l), ui.get_screen().as_str()), (Layout::Tv, "home"));
        show(&l, "settings");
        press(&l, Key::DownArrow);
        press(&l, Key::DownArrow);
        press(&l, Key::RightArrow);
        press(&l, Key::Return); // °C
        assert!(matches!(next_cmd(&rx), Command::SetDisplay { units: Units::Celsius, .. }));

        // Ambient after quiet minutes on Home; any key wakes it without acting.
        go_home(&l);
        S.with_borrow_mut(|s| s.last_key = Instant::now() - AMBIENT_AFTER);
        tick(&l);
        assert!(ui.get_ambient());
        assert!(press(&l, Key::Return));
        assert!(!ui.get_ambient());
        assert!(rx.try_recv().is_err(), "waking doesn't press the button");
    }
}
