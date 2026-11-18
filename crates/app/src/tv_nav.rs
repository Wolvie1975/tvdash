//! Focus navigation for the TV-only v2 Tile Hub (arrows + OK + Back + mouse).

use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use slint::{ComponentHandle, Model, SharedString, platform::Key};
use tvdash_core::config::Layout;

use crate::{
    Accounts, CollectionPageState, Dash, Display, LaptopWindow, Library, Nav, Server, ShowPageState, TvUi,
    UiTheme, Watch,
    focus::{self, Dir, Rect},
    shell_tv::{Page, ShellState, Tab},
};

const KBD_KEYS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Ambient idle starts after this long without a key on Home.
pub const AMBIENT_AFTER: Duration = Duration::from_secs(5 * 60);

struct NavRuntime {
    shell: ShellState,
    cw_scroll: f32,
    lib_scroll: f32,
    show_scroll: f32,
    col_scroll: f32,
    last_key: Instant,
}

thread_local! {
    static STATE: RefCell<NavRuntime> = RefCell::new(NavRuntime {
        shell: ShellState::default(),
        cw_scroll: 0.0,
        lib_scroll: 0.0,
        show_scroll: 0.0,
        col_scroll: 0.0,
        last_key: Instant::now(),
    });
}

fn page_name(page: &Page) -> &'static str {
    match page {
        Page::Home => "home",
        Page::Library => "library",
        Page::Show(_) => "show",
        Page::Collection(_) => "collection",
        Page::Services => "services",
        Page::Search => "search",
        Page::Settings => "settings",
    }
}

fn hint_for(page: &Page) -> &'static str {
    match page {
        Page::Home => "OK Select · Back Home · F12 Return from service",
        Page::Services => "OK Launch service · Back Home · F12 Return",
        _ => "OK Select · Back Return",
    }
}

/// Set the starting layout and wire TV shell callbacks.
pub fn init(laptop: &LaptopWindow, layout: Layout) {
    set_layout(laptop, layout);

    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_key(move |text| weak.upgrade().is_some_and(|l| key(&l, &text)));

    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_tile_activated(move |idx| {
        if let Some(l) = weak.upgrade() {
            activate_tile(&l, idx.max(0) as usize);
        }
    });

    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_tile_hovered(move |idx| {
        if let Some(l) = weak.upgrade() {
            hover_tile(&l, idx.max(0) as usize);
        }
    });

    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_tab_clicked(move |idx| {
        if let Some(l) = weak.upgrade() {
            if let Some(tab) = Tab::from_index(idx.max(0) as usize) {
                STATE.with_borrow_mut(|s| {
                    s.shell.bar_focused = false;
                    s.shell.switch_tab(tab);
                    s.last_key = Instant::now();
                });
                sync(&l);
            }
        }
    });

    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_back_clicked(move || {
        if let Some(l) = weak.upgrade() {
            go_back(&l);
        }
    });

    let weak = laptop.as_weak();
    laptop.global::<TvUi>().on_scroll_panorama(move |delta| {
        if let Some(l) = weak.upgrade() {
            scroll_panorama(&l, delta);
        }
    });

    sync(laptop);
}

pub fn current(laptop: &LaptopWindow) -> Layout {
    if laptop.global::<Nav>().get_layout() == "tv" {
        Layout::Tv
    } else {
        Layout::Dual
    }
}

pub fn set_layout(laptop: &LaptopWindow, layout: Layout) {
    let nav = laptop.global::<Nav>();
    nav.set_layout(if layout == Layout::Tv { "tv".into() } else { "dual".into() });
    if layout == Layout::Tv {
        let tv = laptop.global::<TvUi>();
        tv.set_focus_ticket(tv.get_focus_ticket() + 1);
        sync(laptop);
    }
}

pub fn go_home(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    tv.set_ambient(false);
    tv.set_video_full(false);
    STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.switch_tab(Tab::Home);
        s.shell.focused_tile = 1;
        s.cw_scroll = 0.0;
        s.last_key = Instant::now();
    });
    sync(laptop);
    tv.set_focus_ticket(tv.get_focus_ticket() + 1);
}

pub fn tick(laptop: &LaptopWindow) {
    if current(laptop) != Layout::Tv {
        return;
    }
    let tv = laptop.global::<TvUi>();
    if tv.get_ambient() || tv.get_video_full() {
        return;
    }
    let idle = STATE.with_borrow(|s| s.shell.current() == &Page::Home && s.last_key.elapsed() >= AMBIENT_AFTER);
    if idle {
        tv.set_ambient(true);
    }
}

pub fn sync(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    STATE.with_borrow_mut(|s| {
        let rects = tile_rects(laptop, s.shell.current());
        if !rects.is_empty() && s.shell.focused_tile >= rects.len() {
            s.shell.focused_tile = 0;
        }
        if let Some(&r) = rects.get(s.shell.focused_tile) {
            match s.shell.current() {
                Page::Library => {
                    let lib = laptop.global::<Library>();
                    let browse = lib.get_browse_all();
                    let min_scroll_idx = if browse { 31 } else { 3 };
                    if s.shell.focused_tile >= min_scroll_idx {
                        let (vp_y, vp_h) = if browse { (244.0, 760.0) } else { (184.0, 820.0) };
                        let vert = Rect { x: r.y - vp_y, y: 0.0, w: r.h, h: r.w };
                        s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, vp_h, 16.0);
                    } else {
                        s.lib_scroll = 0.0;
                    }
                }
                Page::Show(_) if s.shell.focused_tile >= 3 => {
                    let local = Rect { x: r.x - 660.0, ..r };
                    s.show_scroll = focus::panorama_scroll(local, s.show_scroll, 1260.0, 96.0);
                }
                Page::Collection(_) if s.shell.focused_tile >= 2 => {
                    let local = Rect { x: r.x - 660.0, ..r };
                    s.col_scroll = focus::panorama_scroll(local, s.col_scroll, 1260.0, 96.0);
                }
                _ => {}
            }
        }
        s.cw_scroll = 0.0;
        if s.shell.current() != &Page::Library {
            s.lib_scroll = 0.0;
        }
        if !matches!(s.shell.current(), Page::Show(_)) {
            s.show_scroll = 0.0;
        }
        if !matches!(s.shell.current(), Page::Collection(_)) {
            s.col_scroll = 0.0;
        }
        tv.set_active_tab(s.shell.tab as i32);
        tv.set_bar_focused(s.shell.bar_focused);
        tv.set_screen(page_name(s.shell.current()).into());
        tv.set_focused_tile(s.shell.focused_tile as i32);
        tv.set_cw_scroll(s.cw_scroll);
        tv.set_lib_scroll(s.lib_scroll);
        tv.set_show_scroll(s.show_scroll);
        tv.set_col_scroll(s.col_scroll);
        tv.set_hint_text(hint_for(s.shell.current()).into());
    });
}

fn tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
    let tv = laptop.global::<TvUi>();
    let dash = laptop.global::<Dash>();
    match page {
        Page::Home => {
            // Indices 0..=9 match home.slint fixed tiles; 10..16 are Continue Watching (2x3).
            let mut rects = vec![
                Rect { x: 96.0, y: 136.0, w: 210.0, h: 590.0 },   // 0: Clock + Plex info
                Rect { x: 322.0, y: 136.0, w: 728.0, h: 590.0 },  // 1: Hero
                Rect { x: 1066.0, y: 136.0, w: 240.0, h: 186.0 }, // 2: Search
                Rect { x: 1066.0, y: 338.0, w: 240.0, h: 186.0 }, // 3: Library
                Rect { x: 1066.0, y: 540.0, w: 240.0, h: 186.0 }, // 4: Screensaver
            ];
            let home_svcs = tv.get_home_services().row_count().min(3);
            for i in 0..3 {
                if i < home_svcs {
                    rects.push(Rect { x: 96.0 + i as f32 * 242.0, y: 742.0, w: 226.0, h: 230.0 });
                } else {
                    rects.push(Rect { x: -9999.0, y: -9999.0, w: 0.0, h: 0.0 });
                }
            }
            // 8: All services, 9: Weather
            rects.push(Rect { x: 822.0, y: 742.0, w: 228.0, h: 230.0 });
            rects.push(Rect { x: 1066.0, y: 742.0, w: 240.0, h: 230.0 });

            let cw_count = dash.get_continue_watching().row_count().min(6);
            for i in 0..cw_count {
                let col = (i % 2) as f32;
                let row = (i / 2) as f32;
                rects.push(Rect {
                    x: 1338.0 + col * 296.0,
                    y: 176.0 + row * 270.0,
                    w: 280.0,
                    h: 254.0,
                });
            }
            rects
        }
        Page::Services => {
            let n = tv.get_services().row_count();
            (0..n)
                .map(|i| {
                    let col = (i % 5) as f32;
                    let row = (i / 5) as f32;
                    Rect {
                        x: 96.0 + col * 348.0,
                        y: 180.0 + row * 376.0,
                        w: 332.0,
                        h: 360.0,
                    }
                })
                .collect()
        }
        Page::Library => {
            let lib = laptop.global::<Library>();
            // 0..=2: TV Series, Movies, Browse All
            let mut rects = vec![
                Rect { x: 96.0, y: 122.0, w: 140.0, h: 44.0 },
                Rect { x: 268.0, y: 122.0, w: 112.0, h: 44.0 },
                Rect { x: 416.0, y: 122.0, w: 180.0, h: 44.0 },
            ];
            if lib.get_browse_all() {
                for i in 0..28 {
                    rects.push(Rect {
                        x: 96.0 + i as f32 * 62.0,
                        y: 184.0,
                        w: 58.0,
                        h: 44.0,
                    });
                }
                let n = lib.get_browse_items().row_count();
                for i in 0..n {
                    rects.push(Rect {
                        x: 96.0 + (i % 8) as f32 * 218.0,
                        y: 252.0 + (i / 8) as f32 * 276.0,
                        w: 198.0,
                        h: 256.0,
                    });
                }
            } else {
                let is_tv = lib.get_media_tab() == "tv";
                let cw_n = lib.get_continue_watching().row_count().min(4);
                let rec_n = lib.get_recently_added().row_count().min(8);
                let fin_n = if is_tv { lib.get_finish_season().row_count().min(4) } else { 0 };
                let col_n = if !is_tv { lib.get_collections().row_count().min(6) } else { 0 };
                let pop_n = lib.get_popular().row_count().min(8);
                let wag_n = lib.get_watch_again().row_count().min(8);

                let rec_y = if cw_n > 0 { 304.0 } else { 0.0 };
                let fin_y = rec_y + if rec_n > 0 { 324.0 } else { 0.0 };
                let col_y = fin_y + if fin_n > 0 { 196.0 } else { 0.0 };
                let pop_y = col_y + if col_n > 0 { 314.0 } else { 0.0 };
                let wag_y = pop_y + if pop_n > 0 { 324.0 } else { 0.0 };

                for i in 0..cw_n {
                    rects.push(Rect { x: 96.0 + i as f32 * 432.0, y: 184.0 + 36.0, w: 416.0, h: 240.0 });
                }
                for i in 0..rec_n {
                    rects.push(Rect { x: 96.0 + i as f32 * 218.0, y: 184.0 + rec_y + 36.0, w: 198.0, h: 260.0 });
                }
                for i in 0..fin_n {
                    rects.push(Rect { x: 96.0 + i as f32 * 432.0, y: 184.0 + fin_y + 36.0, w: 416.0, h: 132.0 });
                }
                for i in 0..col_n {
                    rects.push(Rect { x: 96.0 + i as f32 * 290.0, y: 184.0 + col_y + 36.0, w: 272.0, h: 250.0 });
                }
                for i in 0..pop_n {
                    rects.push(Rect { x: 96.0 + i as f32 * 218.0, y: 184.0 + pop_y + 36.0, w: 198.0, h: 260.0 });
                }
                for i in 0..wag_n {
                    rects.push(Rect { x: 96.0 + i as f32 * 218.0, y: 184.0 + wag_y + 36.0, w: 198.0, h: 260.0 });
                }
            }
            rects
        }
        Page::Show(_) => {
            let show = laptop.global::<ShowPageState>();
            let mut rects = vec![
                Rect { x: 96.0, y: 472.0, w: 520.0, h: 140.0 },
                Rect { x: 96.0, y: 628.0, w: 252.0, h: 140.0 },
                Rect { x: 364.0, y: 628.0, w: 252.0, h: 140.0 },
            ];
            let ep_n = show.get_episodes().row_count();
            for i in 0..ep_n {
                rects.push(Rect {
                    x: 660.0 + (i / 3) as f32 * 400.0,
                    y: 196.0 + (i % 3) as f32 * 256.0,
                    w: 384.0,
                    h: 240.0,
                });
            }
            rects
        }
        Page::Collection(_) => {
            let col = laptop.global::<CollectionPageState>();
            let mut rects = vec![
                Rect { x: 96.0, y: 472.0, w: 520.0, h: 140.0 },
                Rect { x: 96.0, y: 628.0, w: 520.0, h: 140.0 },
            ];
            let n = col.get_items().row_count();
            for i in 0..n {
                rects.push(Rect {
                    x: 660.0 + (i / 3) as f32 * 400.0,
                    y: 180.0 + (i % 3) as f32 * 256.0,
                    w: 384.0,
                    h: 240.0,
                });
            }
            rects
        }
        Page::Search => {
            let mut rects = Vec::with_capacity(48);
            // 0..35: 6x6 keyboard
            for i in 0..36 {
                rects.push(Rect {
                    x: 96.0 + (i % 6) as f32 * 86.0,
                    y: 224.0 + (i / 6) as f32 * 84.0,
                    w: 70.0,
                    h: 70.0,
                });
            }
            // 36..38: Space, Delete, Clear
            for i in 0..3 {
                rects.push(Rect {
                    x: 96.0 + i as f32 * 172.0,
                    y: 732.0,
                    w: 156.0,
                    h: 64.0,
                });
            }
            // 39..: 4-across results
            let res_n = laptop.global::<Watch>().get_included().row_count().min(8);
            for i in 0..res_n {
                rects.push(Rect {
                    x: 660.0 + (i % 4) as f32 * 296.0,
                    y: 180.0 + (i / 4) as f32 * 260.0,
                    w: 280.0,
                    h: 240.0,
                });
            }
            rects
        }
        Page::Settings => {
            // 0..=3: SubTabs (Display, Accounts, Server, What's new)
            let mut rects = vec![
                Rect { x: 96.0, y: 122.0, w: 100.0, h: 44.0 },
                Rect { x: 232.0, y: 122.0, w: 128.0, h: 44.0 },
                Rect { x: 396.0, y: 122.0, w: 92.0, h: 44.0 },
                Rect { x: 524.0, y: 122.0, w: 156.0, h: 44.0 },
            ];
            match tv.get_settings_tab().as_str() {
                "accounts" => {
                    let list_y = if dash.get_cw_state() == "code" { 316.0 } else { 184.0 };
                    let n = laptop.global::<Accounts>().get_accounts().row_count();
                    for i in 0..n {
                        rects.push(Rect {
                            x: 96.0 + (i % 2) as f32 * 872.0,
                            y: list_y + (i / 2) as f32 * 152.0,
                            w: 856.0,
                            h: 136.0,
                        });
                    }
                }
                "server" => {
                    rects.push(Rect { x: 1604.0, y: 180.0, w: 220.0, h: 52.0 });
                }
                "changelog" => {}
                _ => {
                    rects.extend([
                        Rect { x: 96.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 528.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 960.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 1392.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 96.0, y: 420.0, w: 416.0, h: 220.0 },
                        Rect { x: 528.0, y: 420.0, w: 416.0, h: 220.0 },
                    ]);
                    let places_n = laptop.global::<Display>().get_places().row_count().min(4);
                    for i in 0..places_n {
                        rects.push(Rect {
                            x: 960.0,
                            y: 656.0 + i as f32 * 64.0,
                            w: 848.0,
                            h: 54.0,
                        });
                    }
                }
            }
            rects
        }
    }
}

fn hover_tile(laptop: &LaptopWindow, idx: usize) {
    STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.focused_tile = idx;
        s.last_key = Instant::now();
    });
    // Update focus ring without jumping panorama scroll out from under the cursor.
    let tv = laptop.global::<TvUi>();
    tv.set_bar_focused(false);
    tv.set_focused_tile(idx as i32);
}

fn scroll_panorama(laptop: &LaptopWindow, delta: f32) {
    let tv = laptop.global::<TvUi>();
    let step = -delta * 2.0;
    STATE.with_borrow_mut(|s| {
        s.last_key = Instant::now();
        match s.shell.current() {
            Page::Home => {
                s.cw_scroll = (s.cw_scroll + step).clamp(0.0, 2400.0);
                tv.set_cw_scroll(s.cw_scroll);
            }
            Page::Library => {
                s.lib_scroll = (s.lib_scroll + step).clamp(0.0, 4000.0);
                tv.set_lib_scroll(s.lib_scroll);
            }
            Page::Show(_) => {
                s.show_scroll = (s.show_scroll + step).clamp(0.0, 3600.0);
                tv.set_show_scroll(s.show_scroll);
            }
            Page::Collection(_) => {
                s.col_scroll = (s.col_scroll + step).clamp(0.0, 3600.0);
                tv.set_col_scroll(s.col_scroll);
            }
            _ => {}
        }
    });
}

fn go_back(laptop: &LaptopWindow) -> bool {
    let was_show = STATE.with_borrow(|s| matches!(s.shell.current(), Page::Show(_)));
    let was_col = STATE.with_borrow(|s| matches!(s.shell.current(), Page::Collection(_)));
    let changed = STATE.with_borrow_mut(|s| s.shell.back());
    if changed {
        if was_show {
            laptop.global::<ShowPageState>().invoke_close();
        }
        if was_col {
            laptop.global::<CollectionPageState>().invoke_close();
        }
        sync(laptop);
    }
    changed
}

fn activate_library_selection(laptop: &LaptopWindow, lib: &Library, section: &str, idx: usize) {
    lib.invoke_select(section.into(), idx as i32);
    let show_key = lib.get_detail().show_key;
    if !show_key.is_empty() {
        lib.invoke_close_detail();
        lib.invoke_open_show(show_key);
        STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
        sync(laptop);
    } else {
        lib.invoke_play_detail(false);
    }
}

fn activate_tile(laptop: &LaptopWindow, idx: usize) {
    let page = STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.focused_tile = idx;
        s.last_key = Instant::now();
        s.shell.current().clone()
    });
    let tv = laptop.global::<TvUi>();
    let dash = laptop.global::<Dash>();

    match page {
        Page::Home => match idx {
            0 => {}
            1 => {
                if tv.get_video_on() {
                    tv.set_video_full(true);
                } else if dash.get_continue_watching().row_count() > 0 {
                    dash.invoke_play_item(0);
                }
            }
            2 => {
                STATE.with_borrow_mut(|s| s.shell.push(Page::Search));
                sync(laptop);
            }
            3 => {
                STATE.with_borrow_mut(|s| s.shell.switch_tab(Tab::Library));
                sync(laptop);
            }
            4 => tv.set_ambient(true),
            5..=7 => {
                let svcs = tv.get_home_services();
                if let Some(svc) = svcs.row_data(idx - 5) {
                    dash.invoke_open_site(svc.index);
                }
            }
            8 => {
                STATE.with_borrow_mut(|s| s.shell.switch_tab(Tab::Services));
                sync(laptop);
            }
            9 => {} // Weather tile
            i if i >= 10 => {
                let cw_idx = i - 10;
                if cw_idx < dash.get_continue_watching().row_count() {
                    dash.invoke_play_item(cw_idx as i32);
                }
            }
            _ => {}
        },
        Page::Services => {
            let svcs = tv.get_services();
            if let Some(svc) = svcs.row_data(idx) {
                dash.invoke_open_site(svc.index);
            }
        }
        Page::Library => {
            let lib = laptop.global::<Library>();
            if idx < 3 {
                STATE.with_borrow_mut(|s| s.lib_scroll = 0.0);
                tv.set_lib_scroll(0.0);
                match idx {
                    0 => lib.invoke_set_media_tab("tv".into()),
                    1 => lib.invoke_set_media_tab("movies".into()),
                    _ => lib.set_browse_all(!lib.get_browse_all()),
                }
                return;
            }
            let is_tv = lib.get_media_tab() == "tv";
            if lib.get_browse_all() {
                if idx < 31 {
                    let li = idx - 3;
                    let letter = match li {
                        0 => String::new(),
                        27 => "#".to_string(),
                        n => ((b'A' + (n as u8 - 1)) as char).to_string(),
                    };
                    STATE.with_borrow_mut(|s| s.lib_scroll = 0.0);
                    tv.set_lib_scroll(0.0);
                    lib.invoke_set_browse_letter(letter.into());
                } else {
                    activate_library_selection(laptop, &lib, "browse", idx - 31);
                }
                return;
            }

            let item_idx = idx - 3;
            let cw_n = lib.get_continue_watching().row_count().min(4);
            let rec_n = lib.get_recently_added().row_count().min(8);
            let fin_n = if is_tv { lib.get_finish_season().row_count().min(4) } else { 0 };
            let col_n = if !is_tv { lib.get_collections().row_count().min(6) } else { 0 };
            let pop_n = lib.get_popular().row_count().min(8);

            if item_idx < cw_n {
                activate_library_selection(laptop, &lib, "continue", item_idx);
            } else if item_idx < cw_n + rec_n {
                activate_library_selection(laptop, &lib, "recent", item_idx - cw_n);
            } else if item_idx < cw_n + rec_n + fin_n {
                if let Some(row) = lib.get_finish_season().row_data(item_idx - cw_n - rec_n) {
                    lib.invoke_open_show(row.rating_key);
                    STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
                    sync(laptop);
                }
            } else if item_idx < cw_n + rec_n + fin_n + col_n {
                let c_idx = item_idx - cw_n - rec_n - fin_n;
                if let Some(col) = lib.get_collections().row_data(c_idx) {
                    lib.invoke_open_collection(col.rating_key);
                    STATE.with_borrow_mut(|s| s.shell.push(Page::Collection(String::new())));
                    sync(laptop);
                }
            } else if item_idx < cw_n + rec_n + fin_n + col_n + pop_n {
                if let Some(item) = lib.get_popular().row_data(item_idx - cw_n - rec_n - fin_n - col_n) {
                    if is_tv {
                        lib.invoke_open_show(item.rating_key);
                        STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
                        sync(laptop);
                    } else {
                        lib.invoke_play_key(item.rating_key, true);
                    }
                }
            } else {
                let w_idx = item_idx - cw_n - rec_n - fin_n - col_n - pop_n;
                activate_library_selection(laptop, &lib, "watch-again", w_idx);
            }
        }
        Page::Show(_) => {
            let show = laptop.global::<ShowPageState>();
            match idx {
                0 => {
                    let key = show.get_up_next_key();
                    if !key.is_empty() {
                        show.invoke_play_episode(key, true);
                    }
                }
                1 => show.invoke_play_from_start(),
                2 => show.invoke_shuffle(),
                i if i >= 3 => {
                    if let Some(ep) = show.get_episodes().row_data(i - 3) {
                        show.invoke_play_episode(ep.rating_key, true);
                    }
                }
                _ => {}
            }
        }
        Page::Collection(_) => {
            let col = laptop.global::<CollectionPageState>();
            match idx {
                0 => {
                    let key = col.get_up_next_key();
                    if !key.is_empty() {
                        col.invoke_play_item(key, true);
                    }
                }
                1 => col.invoke_shuffle(),
                i if i >= 2 => {
                    if let Some(item) = col.get_items().row_data(i - 2) {
                        col.invoke_play_item(item.rating_key, true);
                    }
                }
                _ => {}
            }
        }
        Page::Search => {
            let w = laptop.global::<Watch>();
            let mut q = w.get_query().to_string();
            match idx {
                0..=35 => {
                    if let Some(ch) = KBD_KEYS.chars().nth(idx) {
                        q.push(ch.to_ascii_lowercase());
                        w.set_query(q.clone().into());
                        w.invoke_edited(q.into());
                    }
                }
                36 => {
                    if !q.is_empty() && !q.ends_with(' ') {
                        q.push(' ');
                        w.set_query(q.clone().into());
                        w.invoke_edited(q.into());
                    }
                }
                37 => {
                    q.pop();
                    w.set_query(q.clone().into());
                    w.invoke_edited(q.into());
                }
                38 => {
                    w.set_query("".into());
                    w.invoke_edited("".into());
                }
                i if i >= 39 => {
                    if let Some(item) = w.get_included().row_data(i - 39) {
                        w.invoke_select(item.key);
                    }
                }
                _ => {}
            }
        }
        Page::Settings => {
            if idx < 4 {
                let next_tab = match idx {
                    1 => "accounts",
                    2 => "server",
                    3 => "changelog",
                    _ => "display",
                };
                tv.set_confirm_quit(false);
                tv.set_settings_tab(next_tab.into());
                if next_tab != "accounts" {
                    laptop.global::<Accounts>().set_editing("".into());
                }
                if next_tab == "server" {
                    laptop.global::<Server>().invoke_open();
                }
                return;
            }
            let sub = tv.get_settings_tab();
            match sub.as_str() {
                "accounts" => {
                    let acc = laptop.global::<Accounts>();
                    if let Some(a) = acc.get_accounts().row_data(idx - 4) {
                        let action = if !a.primary_id.is_empty() {
                            a.primary_id
                        } else {
                            a.secondary_id
                        };
                        if action == "edit" {
                            acc.set_edit_error("".into());
                            acc.set_editing(a.id);
                        } else if !action.is_empty() {
                            acc.invoke_action(a.id, action);
                        }
                    }
                }
                "server" => {
                    if idx == 4 {
                        laptop.global::<Server>().invoke_scan_all();
                    }
                }
                "changelog" => {}
                _ => {
                    let disp = laptop.global::<Display>();
                    let item = idx - 4;
                    if item != 5 {
                        tv.set_confirm_quit(false);
                    }
                    match item {
                        0 => {
                            let next = match disp.get_layout_setting().as_str() {
                                "auto" => "tv",
                                "tv" => "dual",
                                _ => "auto",
                            };
                            disp.set_layout_setting(next.into());
                            disp.invoke_set_layout_setting(next.into());
                        }
                        1 => {
                            let next = match laptop.global::<crate::Theme>().get_mode() {
                                UiTheme::Dark => UiTheme::Stone,
                                UiTheme::Stone => UiTheme::Mist,
                                UiTheme::Mist => UiTheme::Dark,
                            };
                            dash.invoke_set_theme(next);
                        }
                        2 => {
                            disp.set_celsius(!disp.get_celsius());
                            disp.invoke_changed();
                        }
                        3 => {
                            disp.set_h12(!disp.get_h12());
                            disp.invoke_changed();
                        }
                        4 => tv.invoke_clear_recent(),
                        5 => {
                            if tv.get_confirm_quit() {
                                dash.invoke_quit();
                            } else {
                                tv.set_confirm_quit(true);
                            }
                        }
                        6..=9 => disp.invoke_pick_place((item - 6) as i32),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn is_key(text: &str, key: Key) -> bool {
    text == SharedString::from(key).as_str()
}

fn decode_dir(text: &str) -> Option<Dir> {
    if is_key(text, Key::UpArrow) {
        Some(Dir::Up)
    } else if is_key(text, Key::DownArrow) {
        Some(Dir::Down)
    } else if is_key(text, Key::LeftArrow) {
        Some(Dir::Left)
    } else if is_key(text, Key::RightArrow) {
        Some(Dir::Right)
    } else {
        None
    }
}

fn is_ok(text: &str) -> bool {
    text == "\n" || text == "\r" || text == " " || is_key(text, Key::Return)
}

fn is_back(text: &str) -> bool {
    is_key(text, Key::Escape) || is_key(text, Key::Backspace) || is_key(text, Key::Back)
}

pub fn key(laptop: &LaptopWindow, text: &str) -> bool {
    if current(laptop) != Layout::Tv {
        return false;
    }
    let tv = laptop.global::<TvUi>();
    STATE.with_borrow_mut(|s| s.last_key = Instant::now());

    if tv.get_ambient() {
        tv.set_ambient(false);
        return true;
    }

    if tv.get_video_on() && tv.get_video_full() {
        if is_back(text) {
            if tv.get_hud_open() {
                tv.set_hud_open(false);
            } else {
                tv.set_video_full(false);
            }
            return true;
        }
        if tv.get_hud_open() {
            let cur = tv.get_focused_tile().clamp(0, 6);
            match decode_dir(text) {
                Some(Dir::Left) => {
                    tv.set_focused_tile((cur - 1).max(0));
                    return true;
                }
                Some(Dir::Right) => {
                    tv.set_focused_tile((cur + 1).min(6));
                    return true;
                }
                _ => {}
            }
            if is_ok(text) {
                let dash = laptop.global::<Dash>();
                match cur {
                    0 => dash.invoke_transport("back".into()),
                    1 => dash.invoke_transport("play-pause".into()),
                    2 => dash.invoke_transport("forward".into()),
                    3 => {
                        dash.invoke_transport("stop".into());
                        tv.set_hud_open(false);
                        tv.set_video_full(false);
                    }
                    4 => {
                        tv.set_hud_open(false);
                        tv.set_video_full(false);
                    }
                    5 => dash.invoke_transport("sub".into()),
                    6 => dash.invoke_transport("audio".into()),
                    _ => {}
                }
                return true;
            }
        } else if is_ok(text) {
            tv.set_focused_tile(1);
            tv.set_hud_open(true);
            tv.invoke_overlay();
            return true;
        }
        return false;
    }

    if is_back(text) {
        return go_back(laptop);
    }

    let bar_focused = STATE.with_borrow(|s| s.shell.bar_focused);
    if bar_focused {
        match decode_dir(text) {
            Some(Dir::Left) => {
                STATE.with_borrow_mut(|s| s.shell.switch_tab(s.shell.tab.prev()));
                sync(laptop);
                return true;
            }
            Some(Dir::Right) => {
                STATE.with_borrow_mut(|s| s.shell.switch_tab(s.shell.tab.next()));
                sync(laptop);
                return true;
            }
            Some(Dir::Down) => {
                STATE.with_borrow_mut(|s| s.shell.bar_focused = false);
                sync(laptop);
                return true;
            }
            _ => {}
        }
        if is_ok(text) {
            STATE.with_borrow_mut(|s| s.shell.bar_focused = false);
            sync(laptop);
            return true;
        }
        return false;
    }

    if let Some(dir) = decode_dir(text) {
        let page = STATE.with_borrow(|s| s.shell.current().clone());
        let rects = tile_rects(laptop, &page);
        let from = STATE.with_borrow(|s| s.shell.focused_tile);
        if let Some(next_idx) = focus::next(&rects, from, dir) {
            if rects[next_idx].w > 0.0 {
                STATE.with_borrow_mut(|s| s.shell.focused_tile = next_idx);
                sync(laptop);
                return true;
            }
        }
        if dir == Dir::Up {
            STATE.with_borrow_mut(|s| s.shell.bar_focused = true);
            sync(laptop);
            return true;
        }
        return false;
    }

    if is_ok(text) {
        let idx = STATE.with_borrow(|s| s.shell.focused_tile);
        activate_tile(laptop, idx);
        return true;
    }

    // Physical keyboard typing when on Search
    if STATE.with_borrow(|s| s.shell.current() == &Page::Search) {
        let mut chars = text.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if ch.is_ascii_alphanumeric() || ch == ' ' {
                let w = laptop.global::<Watch>();
                let mut q = w.get_query().to_string();
                q.push(ch);
                w.set_query(q.clone().into());
                w.invoke_edited(q.into());
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use tvdash_core::{
        Command,
        config::{ClockFormat, StreamingSite, ThemeMode, Units},
    };

    use super::*;
    use crate::bridge::{self, UiConfig};

    fn test_window() -> (LaptopWindow, mpsc::Receiver<Command>) {
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
                sites: ["YouTube TV", "YouTube", "Disney+", "Hulu", "Netflix"]
                    .into_iter()
                    .map(site)
                    .collect(),
            },
        );
        let (tx, rx) = mpsc::channel();
        let (utx, mut urx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || {
            while let Some(c) = urx.blocking_recv() {
                let _ = tx.send(c);
            }
        });
        crate::callbacks::wire(&l, utx);
        init(&l, Layout::Tv);
        go_home(&l);
        (l, rx)
    }

    #[test]
    fn home_arrows_tab_bar_and_service_launch() {
        let (l, rx) = test_window();
        let tv = l.global::<TvUi>();
        assert_eq!(tv.get_screen(), "home");
        assert_eq!(tv.get_focused_tile(), 1); // Hero

        // Up from Hero focuses TabBar; Right switches to Library; Back returns to Home
        assert!(key(&l, &SharedString::from(Key::UpArrow)));
        assert!(tv.get_bar_focused());
        assert!(key(&l, &SharedString::from(Key::RightArrow)));
        assert_eq!(tv.get_screen(), "library");
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert_eq!(tv.get_screen(), "home");

        // Down from Hero lands on recent service tile, OK opens it
        assert!(key(&l, &SharedString::from(Key::DownArrow)));
        let svc_tile = tv.get_focused_tile();
        assert!((5..=7).contains(&svc_tile));
        assert!(key(&l, "\n"));
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Command::OpenSite(_)));
    }

    #[test]
    fn search_settings_hud_and_ambient() {
        let (l, rx) = test_window();
        let tv = l.global::<TvUi>();
        let w = l.global::<Watch>();

        // Open Search (tile 2 on Home) and type via OK + physical key
        activate_tile(&l, 2);
        assert_eq!(tv.get_screen(), "search");
        assert!(key(&l, "\n")); // 'A'
        assert!(key(&l, "b"));  // physical 'b'
        assert_eq!(w.get_query(), "ab");
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert_eq!(tv.get_screen(), "home");

        // Switch to Settings via TabBar and toggle Celsius (tile 6; 0..=3 are SubTabs)
        assert!(key(&l, &SharedString::from(Key::UpArrow)));
        for _ in 0..3 {
            assert!(key(&l, &SharedString::from(Key::RightArrow)));
        }
        assert_eq!(tv.get_screen(), "settings");
        activate_tile(&l, 6);
        assert!(l.global::<Display>().get_celsius());
        activate_tile(&l, 1);
        assert_eq!(tv.get_settings_tab(), "accounts");
        activate_tile(&l, 0);
        assert_eq!(tv.get_settings_tab(), "display");

        // Full-screen video HUD: OK opens HUD (emits Overlay), Right -> +30s, OK dispatches Seek(30)
        tv.set_video_on(true);
        tv.set_video_full(true);
        assert!(key(&l, "\n"));
        assert!(tv.get_hud_open());
        assert!(key(&l, &SharedString::from(Key::RightArrow)));
        assert!(key(&l, "\n"));
        let cmds: Vec<Command> = std::iter::from_fn(|| rx.recv_timeout(Duration::from_millis(150)).ok()).collect();
        assert!(cmds.iter().any(|c| matches!(c, Command::Overlay)), "expected Overlay in {cmds:?}");
        assert!(cmds.iter().any(|c| matches!(c, Command::Seek(30))), "expected Seek(30) in {cmds:?}");
    }
}
