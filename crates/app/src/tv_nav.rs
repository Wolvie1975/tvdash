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

use crate::{Accounts, CollectionPageState, Dash, Display, LaptopWindow, Library, ShowPageState, TvUi, UiTheme, Watch};

/// Ambient idle starts after this long without a key on Home.
pub const AMBIENT_AFTER: Duration = Duration::from_secs(5 * 60);
const SCREENS: [&str; 5] = ["home", "search", "library", "services", "settings"];
/// Rail positions: the five screens, then the theme button.
const RAIL_THEME: usize = 5;
/// Services grid columns (matches tv/shell.slint).
const GRID_COLUMNS: usize = 5;
/// Library Browse All grid columns (matches tv/shell.slint).
const BROWSE_COLUMNS: usize = 8;
/// On-screen keyboard columns (A-Z, 0-9 = 6x6).
const KBD_COLUMNS: usize = 6;
const KBD_KEYS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
/// A-Z filter bar columns in Library Browse All (2 rows x 14).
const LETTER_COLUMNS: usize = 14;
const BROWSE_LETTERS: [&str; 28] = [
    "", "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L",
    "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];

struct NavState {
    /// Last focused item per row id, so coming back lands on the same item.
    cols: HashMap<&'static str, usize>,
    last_watch_row: Option<&'static str>,
    last_key: Instant,
}

thread_local! {
    static S: RefCell<NavState> =
        RefCell::new(NavState { cols: HashMap::new(), last_watch_row: None, last_key: Instant::now() });
}

/// Set the starting layout and take the TV shell's keys.
pub fn init(laptop: &LaptopWindow, layout: Layout) {
    set_layout(laptop, layout);
    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_key(move |text| weak.upgrade().is_some_and(|l| key(&l, &text)));
    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_click(move |row, col| {
        if let Some(l) = weak.upgrade() {
            click(&l, &row, col.max(0) as usize);
        }
    });
    sync(laptop);
}

/// Show the dashboard pages or the TV shell in the main window.
pub fn set_layout(laptop: &LaptopWindow, layout: Layout) {
    laptop.global::<crate::Nav>().set_layout(if layout == Layout::Tv { "tv" } else { "dual" }.into());
    laptop.global::<TvUi>().set_layout_tv(layout == Layout::Tv);
    crate::bridge::set_tv_layout(laptop, layout == Layout::Tv);
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
            if ui.get_confirm_cw_index() >= 0 {
                vec![("confirm-cw", 2)]
            } else {
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
                let continue_ = if cw == "items" { dash.get_continue_watching().row_count().min(5) } else { 0 };
                vec![
                    ("hero", hero),
                    ("prompts", prompts),
                    ("continue", continue_),
                    ("services", dash.get_streaming_sites().row_count().min(10)),
                ]
            }
        }
        "search" => {
            let w = laptop.global::<Watch>();
            if w.get_detail_open() {
                vec![("watch-detail", w.get_detail_actions().row_count() + 1)]
            } else {
                let mut r = vec![
                    ("search-kbd", KBD_KEYS.len()),
                    ("search-actions", 5),
                    ("watch-lib", w.get_library().row_count()),
                    ("watch-inc", w.get_included().row_count()),
                ];
                if !w.get_only_now() {
                    r.push(("watch-extra", w.get_extra().row_count()));
                    r.push(("watch-none", w.get_none().row_count()));
                }
                r.push(("watch-check", w.get_checking().row_count()));
                r
            }
        }
        "services" => vec![("grid", dash.get_streaming_sites().row_count())],
        "library" => {
            let sp = laptop.global::<ShowPageState>();
            let cp = laptop.global::<CollectionPageState>();
            let lib = laptop.global::<Library>();
            if sp.get_open() {
                vec![
                    ("show-actions", 3),
                    ("show-seasons", sp.get_seasons().row_count()),
                    ("show-episodes", sp.get_episodes().row_count()),
                ]
            } else if cp.get_open() {
                vec![("col-actions", 5), ("col-items", cp.get_items().row_count())]
            } else if lib.get_detail_open() {
                vec![("lib-detail", if lib.get_detail().can_resume { 3 } else { 2 })]
            } else if lib.get_browse_all() {
                    vec![
                        ("lib-tabs", 3),
                        ("lib-letters", BROWSE_LETTERS.len()),
                        ("lib-browse", lib.get_browse_items().row_count()),
                    ]
                } else {
                    let mut shelf_rows = vec![
                        ("lib-tabs", 3),
                        ("lib-continue", lib.get_continue_watching().row_count()),
                        ("lib-recent", lib.get_recently_added().row_count()),
                        ("lib-finish", lib.get_finish_season().row_count()),
                    ];
                    if lib.get_media_tab() == "movies" {
                        shelf_rows.push(("lib-collections", lib.get_collections().row_count()));
                    }
                    shelf_rows.push(("lib-week", lib.get_added_week().row_count()));
                    shelf_rows.push(("lib-again", lib.get_watch_again().row_count()));
                    shelf_rows
                }
            }
        "settings" => {
            let mut r = vec![("set-tabs", 4)];
            match laptop.global::<TvUi>().get_settings_tab().as_str() {
                "accounts" => r.push(("set-accounts", laptop.global::<Accounts>().get_accounts().row_count())),
                "server" => r.push(("server-scan", 1)),
                "changelog" => {}
                _ => {
                    r.extend([("set-layout", 2), ("set-theme", 3), ("set-units", 2), ("set-clock", 2), ("set-quit", 1)]);
                }
            }
            r
        }
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

fn close_watch_detail(laptop: &LaptopWindow) {
    laptop.global::<Watch>().invoke_close_detail();
    if let Some(row_id) = S.with_borrow(|s| s.last_watch_row)
        && let Some((_, n)) = rows(laptop).into_iter().find(|(id, _)| *id == row_id)
    {
        focus_row(laptop, row_id, n);
        return;
    }
    sync(laptop);
}

fn show(laptop: &LaptopWindow, screen: &str) {
    let ui = laptop.global::<TvUi>();
    ui.set_screen(screen.into());
    ui.set_rail_open(false);
    ui.set_focus("".into());
    if screen == "library" {
        let lib = laptop.global::<Library>();
        if lib.get_browse_items().row_count() == 0 {
            lib.invoke_set_media_tab(lib.get_media_tab());
        }
    }
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

    // ── Search physical keyboard input ──
    if ui.get_screen() == "search" && ui.get_focus() != "rail" && !laptop.global::<Watch>().get_detail_open() {
        let w = laptop.global::<Watch>();
        if is(text, Key::Backspace) && !w.get_query().is_empty() {
            let mut q = w.get_query().to_string();
            q.pop();
            w.set_query(q.as_str().into());
            w.invoke_edited(q.into());
            return true;
        }
        if let Some(ch) = text.as_str().chars().next()
            && text.as_str().chars().count() == 1
            && (ch.is_ascii_alphanumeric() || ch == ' ' || ch == '-' || ch == '\'')
        {
            let mut q = w.get_query().to_string();
            q.push(ch);
            w.set_query(q.as_str().into());
            w.invoke_edited(q.into());
            return true;
        }
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

    if id == "grid" || id == "lib-browse" {
        let cols = if id == "lib-browse" { BROWSE_COLUMNS } else { GRID_COLUMNS };
        if left && col.is_multiple_of(cols) {
            open_rail(laptop);
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right {
            ui.set_col((col + 1).min(len - 1) as i32);
        } else if up && col >= cols {
            ui.set_col((col - cols) as i32);
        } else if up && at > 0 {
            remember(laptop);
            focus_row(laptop, rows[at - 1].0, rows[at - 1].1);
        } else if down && col + cols < len {
            ui.set_col((col + cols) as i32);
        }
    } else if id == "lib-letters" {
        if left && col.is_multiple_of(LETTER_COLUMNS) {
            open_rail(laptop);
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right {
            ui.set_col((col + 1).min(len - 1) as i32);
        } else if up && col >= LETTER_COLUMNS {
            ui.set_col((col - LETTER_COLUMNS) as i32);
        } else if up && at > 0 {
            remember(laptop);
            focus_row(laptop, rows[at - 1].0, rows[at - 1].1);
        } else if down && col + LETTER_COLUMNS < len {
            ui.set_col((col + LETTER_COLUMNS) as i32);
        } else if down && at + 1 < rows.len() {
            remember(laptop);
            focus_row(laptop, rows[at + 1].0, rows[at + 1].1);
        }
    } else if id == "search-kbd" {
        let first_result = rows.iter().find(|(r, _)| r.starts_with("watch-")).copied();
        if left && col.is_multiple_of(KBD_COLUMNS) {
            open_rail(laptop);
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right && (col % KBD_COLUMNS == KBD_COLUMNS - 1 || col + 1 == len) {
            if let Some((r, n)) = first_result {
                remember(laptop);
                focus_row(laptop, r, n);
            }
        } else if right {
            ui.set_col((col + 1).min(len - 1) as i32);
        } else if up && col >= KBD_COLUMNS {
            ui.set_col((col - KBD_COLUMNS) as i32);
        } else if down && col + KBD_COLUMNS < len {
            ui.set_col((col + KBD_COLUMNS) as i32);
        } else if down {
            if let Some((_, n)) = rows.iter().find(|(r, _)| *r == "search-actions") {
                remember(laptop);
                ui.set_focus("search-actions".into());
                ui.set_col((col % KBD_COLUMNS).min(*n - 1) as i32);
            }
        }
    } else if id == "search-actions" {
        let first_result = rows.iter().find(|(r, _)| r.starts_with("watch-")).copied();
        if left && col == 0 {
            open_rail(laptop);
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right && col + 1 < len {
            ui.set_col(col as i32 + 1);
        } else if right {
            if let Some((r, n)) = first_result {
                remember(laptop);
                focus_row(laptop, r, n);
            }
        } else if up {
            remember(laptop);
            ui.set_focus("search-kbd".into());
            ui.set_col((30 + col.min(KBD_COLUMNS - 1)).min(KBD_KEYS.len() - 1) as i32);
        } else if down {
            if let Some((r, n)) = first_result {
                remember(laptop);
                focus_row(laptop, r, n);
            }
        }
    } else if id.starts_with("watch-") && id != "watch-detail" {
        if left && col == 0 {
            remember(laptop);
            focus_row(laptop, "search-kbd", KBD_KEYS.len());
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right {
            ui.set_col((col + 1).min(len - 1) as i32);
        } else if up && at > 0 && rows[at - 1].0.starts_with("watch-") {
            remember(laptop);
            focus_row(laptop, rows[at - 1].0, rows[at - 1].1);
        } else if down && at + 1 < rows.len() {
            remember(laptop);
            focus_row(laptop, rows[at + 1].0, rows[at + 1].1);
        }
    } else if id == "continue" {
        if left {
            open_rail(laptop);
        } else if right {
            if let Some((_, n)) = rows.iter().find(|(r, _)| *r == "services") {
                remember(laptop);
                focus_row(laptop, "services", *n);
            }
        } else if up && col > 0 {
            ui.set_col(col as i32 - 1);
        } else if up && at > 0 {
            remember(laptop);
            focus_row(laptop, rows[at - 1].0, rows[at - 1].1);
        } else if down && col + 1 < len {
            ui.set_col(col as i32 + 1);
        }
    } else if id == "services" {
        if left && col.is_multiple_of(2) {
            if let Some((prev, n)) = rows.iter().find(|(r, _)| *r == "continue" || *r == "prompts") {
                remember(laptop);
                focus_row(laptop, prev, *n);
            } else {
                open_rail(laptop);
            }
        } else if left {
            ui.set_col(col as i32 - 1);
        } else if right && col.is_multiple_of(2) && col + 1 < len {
            ui.set_col(col as i32 + 1);
        } else if up && col >= 2 {
            ui.set_col((col - 2) as i32);
        } else if up {
            if let Some((_, n)) = rows.iter().find(|(r, _)| *r == "hero") {
                remember(laptop);
                focus_row(laptop, "hero", *n);
            }
        } else if down && col + 2 < len {
            ui.set_col((col + 2) as i32);
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
        if id == "confirm-cw" {
            ui.set_confirm_cw_index(-1);
            sync(laptop);
        } else if id == "lib-detail" {
            laptop.global::<Library>().invoke_close_detail();
            sync(laptop);
        } else if id == "watch-detail" {
            close_watch_detail(laptop);
        } else if id.starts_with("show-") {
            laptop.global::<ShowPageState>().invoke_close();
            sync(laptop);
        } else if id.starts_with("col-") {
            laptop.global::<CollectionPageState>().invoke_close();
            sync(laptop);
        } else if ui.get_screen() != "home" {
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
    } else if text.as_str() == "a" || text.as_str() == "A" {
        dash.invoke_transport("audio".into());
    } else if text.as_str() == "s" || text.as_str() == "S" {
        dash.invoke_transport("sub".into());
    } else if text.as_str() == "-" {
        dash.invoke_transport("vol-down".into());
    } else if text.as_str() == "+" || text.as_str() == "=" {
        dash.invoke_transport("vol-up".into());
    } else if text.as_str() == "p" || text.as_str() == "P" {
        dash.invoke_transport("prev-ep".into());
    } else if text.as_str() == "n" || text.as_str() == "N" {
        dash.invoke_transport("next-ep".into());
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

/// Handle a mouse or touch click on any interactive element in the TV shell.
pub fn click(laptop: &LaptopWindow, row: &str, col: usize) {
    S.with_borrow_mut(|s| s.last_key = Instant::now());
    let ui = laptop.global::<TvUi>();
    ui.set_focus_ticket(ui.get_focus_ticket() + 1);
    match row {
        "ambient" => {
            ui.set_ambient(false);
        }
        "video" => {
            ui.set_video_full(false);
            show(laptop, "home");
            focus_row(laptop, "hero", rows(laptop).iter().find(|(id, _)| *id == "hero").map_or(1, |(_, n)| *n));
        }
        "rail" => {
            if col == RAIL_THEME {
                cycle_theme(laptop);
            } else if let Some(&screen) = SCREENS.get(col) {
                show(laptop, screen);
            }
        }
        _ => {
            if ui.get_rail_open() {
                ui.set_rail_open(false);
            }
            ui.set_focus(row.into());
            ui.set_col(col as i32);
            activate(laptop, row, col);
        }
    }
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
        "continue" => {
            let ui = laptop.global::<TvUi>();
            ui.set_confirm_cw_index(col as i32);
            sync(laptop);
        }
        "confirm-cw" => {
            let ui = laptop.global::<TvUi>();
            let idx = ui.get_confirm_cw_index();
            ui.set_confirm_cw_index(-1);
            if col == 0 && idx >= 0 {
                dash.invoke_play_item(idx);
            }
            sync(laptop);
        }
        "services" | "grid" => {
            if let Some(site) = dash.get_streaming_sites().row_data(col) {
                dash.invoke_open_site(site.index);
            }
        }
        "search-kbd" => {
            if let Some(ch) = KBD_KEYS.chars().nth(col) {
                let w = laptop.global::<Watch>();
                let mut q = w.get_query().to_string();
                q.push(ch.to_ascii_lowercase());
                w.set_query(q.as_str().into());
                w.invoke_edited(q.into());
            }
        }
        "search-actions" => {
            let w = laptop.global::<Watch>();
            match col {
                0 => {
                    let mut q = w.get_query().to_string();
                    q.push(' ');
                    w.set_query(q.as_str().into());
                    w.invoke_edited(q.into());
                }
                1 => {
                    let mut q = w.get_query().to_string();
                    q.pop();
                    w.set_query(q.as_str().into());
                    w.invoke_edited(q.into());
                }
                2 => {
                    w.set_query("".into());
                    w.set_message("".into());
                    w.invoke_edited("".into());
                }
                3 => {
                    w.set_only_now(!w.get_only_now());
                    sync(laptop);
                }
                4 => w.invoke_search(w.get_query()),
                _ => {}
            }
        }
        "watch-lib" | "watch-inc" | "watch-extra" | "watch-none" | "watch-check" => {
            let row_id: &'static str = match row {
                "watch-lib" => "watch-lib",
                "watch-inc" => "watch-inc",
                "watch-extra" => "watch-extra",
                "watch-none" => "watch-none",
                _ => "watch-check",
            };
            S.with_borrow_mut(|s| {
                s.cols.insert(row_id, col);
                s.last_watch_row = Some(row_id);
            });
            let w = laptop.global::<Watch>();
            let model = match row {
                "watch-lib" => w.get_library(),
                "watch-inc" => w.get_included(),
                "watch-extra" => w.get_extra(),
                "watch-none" => w.get_none(),
                _ => w.get_checking(),
            };
            if let Some(card) = model.row_data(col) {
                w.invoke_select(card.key);
            }
        }
        "watch-detail" => {
            let w = laptop.global::<Watch>();
            let actions = w.get_detail_actions();
            if let Some(a) = actions.row_data(col) {
                w.invoke_action(a);
            } else {
                close_watch_detail(laptop);
            }
        }
        "lib-tabs" => {
            let lib = laptop.global::<Library>();
            match col {
                0 => lib.invoke_set_media_tab("tv".into()),
                1 => lib.invoke_set_media_tab("movies".into()),
                2 => lib.set_browse_all(!lib.get_browse_all()),
                _ => {}
            }
            sync(laptop);
        }
        "lib-letters" => {
            if let Some(&letter) = BROWSE_LETTERS.get(col) {
                laptop.global::<Library>().invoke_set_browse_letter(letter.into());
                sync(laptop);
            }
        }
        "lib-continue" | "lib-recent" | "lib-week" | "lib-again" | "lib-browse" => {
            let lib = laptop.global::<Library>();
            let section = match row {
                "lib-continue" => "continue",
                "lib-recent" => "recent",
                "lib-week" => "added-week",
                "lib-again" => "watch-again",
                _ => "browse",
            };
            lib.invoke_select(section.into(), col as i32);
            let show_key = lib.get_detail().show_key;
            if !show_key.is_empty() {
                lib.invoke_close_detail();
                lib.invoke_open_show(show_key);
            } else {
                sync(laptop);
            }
        }
        "lib-detail" => {
            let lib = laptop.global::<Library>();
            let can_resume = lib.get_detail().can_resume;
            match col {
                0 => lib.invoke_play_detail(false),
                1 if can_resume => lib.invoke_play_detail(true),
                _ => lib.invoke_close_detail(),
            }
            sync(laptop);
        }
        "lib-finish" => {
            let lib = laptop.global::<Library>();
            if let Some(item) = lib.get_finish_season().row_data(col) {
                lib.invoke_open_show(item.rating_key);
            }
        }
        "lib-collections" => {
            let lib = laptop.global::<Library>();
            if let Some(c) = lib.get_collections().row_data(col) {
                lib.invoke_open_collection(c.rating_key);
            }
        }
        "col-actions" => {
            let cp = laptop.global::<CollectionPageState>();
            match col {
                0 => {
                    let key = cp.get_up_next_key();
                    if !key.is_empty() {
                        cp.invoke_play_item(key, true);
                    }
                }
                1 => cp.invoke_shuffle(),
                2 => {
                    let next = match cp.get_order().as_str() {
                        "collection" => "release",
                        "release" => "alpha",
                        _ => "collection",
                    };
                    cp.invoke_set_order(next.into());
                }
                3 => cp.invoke_set_hide_watched(!cp.get_hide_watched()),
                4 => {
                    cp.invoke_close();
                    sync(laptop);
                }
                _ => {}
            }
        }
        "col-items" => {
            let cp = laptop.global::<CollectionPageState>();
            if let Some(item) = cp.get_items().row_data(col) {
                cp.invoke_play_item(item.rating_key, item.can_resume);
            }
        }
        "show-actions" => {
            let sp = laptop.global::<ShowPageState>();
            match col {
                0 => {
                    let key = sp.get_up_next_key();
                    if !key.is_empty() {
                        sp.invoke_play_episode(key, sp.get_can_resume());
                    }
                }
                1 => sp.invoke_shuffle(),
                2 => {
                    sp.invoke_close();
                    sync(laptop);
                }
                _ => {}
            }
        }
        "show-seasons" => {
            let sp = laptop.global::<ShowPageState>();
            if let Some(s) = sp.get_seasons().row_data(col) {
                sp.invoke_select_season(s.rating_key);
            }
        }
        "show-episodes" => {
            let sp = laptop.global::<ShowPageState>();
            if let Some(ep) = sp.get_episodes().row_data(col) {
                sp.invoke_play_episode(ep.rating_key, ep.progress > 0.0);
            }
        }
        "set-tabs" => {
            let tab = ["display", "accounts", "server", "changelog"].get(col).copied().unwrap_or("display");
            laptop.global::<TvUi>().set_settings_tab(tab.into());
            sync(laptop);
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
        "server-scan" => laptop.global::<crate::Server>().invoke_scan_all(),
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
            kind: MediaKind::Episode,
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
                layout_setting: tvdash_core::LayoutSetting::Auto,
                tv_connected: true,
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
        let (etx, _erx) = tokio::sync::mpsc::unbounded_channel();
        crate::displays::remember(&tv, &tvdash_core::config::DisplayConfig::default(), false, None, utx, etx);
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

        // Continue Watching arrives: the hero (Resume) and the column appear.
        let t = crate::TvWindow::new().unwrap();
        bridge::apply_for_test(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![item("a"), item("b")]));
        assert_eq!(l.global::<Dash>().get_tv().mode, "next");
        // Left from services col 0 moves to the Continue Watching column.
        assert!(press(&l, Key::LeftArrow));
        assert_eq!(ui.get_focus(), "continue");
        assert!(press(&l, Key::DownArrow) && press(&l, Key::DownArrow));
        assert_eq!(ui.get_col(), 1, "clamped to the column");
        // Right to services and back left: the column remembers where we were.
        press(&l, Key::RightArrow);
        assert_eq!(ui.get_focus(), "services");
        press(&l, Key::LeftArrow);
        assert_eq!((ui.get_focus().as_str(), ui.get_col()), ("continue", 1));
        // OK opens confirmation; second OK plays the focused card.
        press(&l, Key::Return);
        assert_eq!((ui.get_focus().as_str(), ui.get_confirm_cw_index()), ("confirm-cw", 1));
        press(&l, Key::Return);
        assert!(matches!(next_cmd(&rx), Command::Play(i) if i.id == "b"));
        assert_eq!(ui.get_confirm_cw_index(), -1);

        // Left from the Continue Watching column opens the rail on the current screen.
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
        assert!(key(&l, &SharedString::from("a")));
        assert!(matches!(next_cmd(&rx), Command::CycleAudio));
        assert!(key(&l, &SharedString::from("s")));
        assert!(matches!(next_cmd(&rx), Command::CycleSub));
        assert!(key(&l, &SharedString::from("-")));
        assert!(matches!(next_cmd(&rx), Command::VolumeDelta(-5)));
        assert!(key(&l, &SharedString::from("+")));
        assert!(matches!(next_cmd(&rx), Command::VolumeDelta(5)));
        assert!(key(&l, &SharedString::from("p")));
        assert!(matches!(next_cmd(&rx), Command::PreviousEpisode));
        assert!(key(&l, &SharedString::from("n")));
        assert!(matches!(next_cmd(&rx), Command::NextEpisode));
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
        assert_eq!(ui.get_focus(), "set-tabs");
        press(&l, Key::DownArrow);
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
        press(&l, Key::DownArrow); // set-layout
        press(&l, Key::DownArrow); // set-theme
        press(&l, Key::DownArrow); // set-units
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

    #[test]
    fn search_keyboard_results_and_detail() {
        use tvdash_core::models::{WatchHit, WatchTier};
        let (l, rx) = setup();
        let ui = l.global::<TvUi>();
        let w = l.global::<Watch>();
        show(&l, "search");
        assert_eq!((ui.get_focus().as_str(), ui.get_col()), ("search-kbd", 0));

        // On-screen keyboard: 'A', then Right to 'B', then Down to 'H'.
        press(&l, Key::Return);
        press(&l, Key::RightArrow);
        press(&l, Key::Return);
        assert_eq!(w.get_query(), "ab");

        // Physical keyboard typing appends and Backspace deletes.
        assert!(key(&l, &SharedString::from("c")));
        assert_eq!(w.get_query(), "abc");
        assert!(press(&l, Key::Backspace));
        assert_eq!(w.get_query(), "ab");

        // Down to search-actions, col 4 ("Go") submits immediately.
        for _ in 0..6 {
            press(&l, Key::DownArrow);
        }
        assert_eq!(ui.get_focus(), "search-actions");
        ui.set_col(4);
        press(&l, Key::Return);
        assert!(matches!(next_cmd(&rx), Command::WhereToWatch(q) if q == "ab"));

        // Results arrive: Right from keyboard/actions jumps to results, OK opens detail, Back closes it.
        let hit = WatchHit {
            tmdb_id: 603,
            is_tv: false,
            title: "The Matrix".into(),
            year: Some(1999),
            overview: "Wake up, Neo.".into(),
            tier: WatchTier::Included,
            services: vec![(0, "Netflix".into())],
            elsewhere: vec![],
            library: None,
            link: None,
        };
        crate::watch_ui::on_results(&l, "ab".into(), vec![hit], None);
        press(&l, Key::DownArrow);
        assert_eq!(ui.get_focus(), "watch-inc");
        press(&l, Key::Return);
        assert!(w.get_detail_open());
        assert_eq!(ui.get_focus(), "watch-detail");
        press(&l, Key::Escape);
        assert!(!w.get_detail_open());
        assert_eq!(ui.get_focus(), "watch-inc");
    }

    #[test]
    fn library_tabs_and_show_detail() {
        let (l, rx) = setup();
        let ui = l.global::<TvUi>();
        let lib = l.global::<Library>();
        show(&l, "library");
        assert_eq!((ui.get_focus().as_str(), ui.get_col()), ("lib-tabs", 0));
        assert!(matches!(next_cmd(&rx), Command::LoadLibraryShelves { is_tv: true, .. }));

        // Switch to Movies tab.
        press(&l, Key::RightArrow);
        press(&l, Key::Return);
        assert_eq!(lib.get_media_tab(), "movies");
        assert!(matches!(next_cmd(&rx), Command::LoadLibraryShelves { is_tv: false, .. }));

        // Toggle Browse All and pick a letter from the A-Z filter bar.
        press(&l, Key::RightArrow);
        press(&l, Key::Return);
        assert!(lib.get_browse_all());
        press(&l, Key::DownArrow);
        assert_eq!(ui.get_focus(), "lib-letters");
        press(&l, Key::RightArrow);
        press(&l, Key::RightArrow);
        press(&l, Key::Return);
        assert_eq!(lib.get_browse_letter(), "A");

        // Open a Show detail overlay and navigate seasons / close.
        let t = crate::TvWindow::new().unwrap();
        let mut show_item = item("show-1");
        show_item.kind = MediaKind::Series;
        bridge::apply_for_test(&l, &t, AppEvent::ContinueWatching(SourceKind::Plex, vec![show_item]));
        lib.set_browse_all(false);
        sync(&l);
        l.global::<ShowPageState>().set_open(true);
        sync(&l);
        assert_eq!(ui.get_focus(), "show-actions");
        press(&l, Key::Escape);
        assert!(!l.global::<ShowPageState>().get_open());
        assert_eq!(ui.get_focus(), "lib-tabs");
    }

    #[test]
    fn night_mode_forces_dark_on_tv() {
        let (l, _rx) = setup();
        bridge::set_theme(&l, ThemeMode::Mist);
        assert_eq!(l.global::<crate::Theme>().get_mode(), UiTheme::Mist);

        // Enable 24-hour night window (00:00..23:59) on TV layout -> forces Dark.
        let mut disp = tvdash_core::config::DisplayConfig::default();
        disp.night_mode = true;
        disp.night_start = "00:00".into();
        disp.night_end = "23:59".into();
        bridge::set_night_config(&l, disp);
        assert_eq!(l.global::<crate::Theme>().get_mode(), UiTheme::Dark);

        // Switching back to Dual layout restores Mist.
        set_layout(&l, Layout::Dual);
        assert_eq!(l.global::<crate::Theme>().get_mode(), UiTheme::Mist);
    }
}
