//! Focus navigation for the TV-only v2 Tile Hub (arrows + OK + Back + mouse).

use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use slint::{ComponentHandle, Model, SharedString, platform::Key};
use tvdash_core::config::Layout;

use crate::{
    Accounts, Artwork, CollectionPageState, Dash, Display, LaptopWindow, Library, LibraryLanding, MoviePageState, Nav,
    Scoreboard, Server, ShowPageState, SiteEditor, SleepState, Sports, SportsAlertState, SportsAlertsSettings, TvUi,
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
    pending_collection_focus: Option<String>,
}

thread_local! {
    static STATE: RefCell<NavRuntime> = RefCell::new(NavRuntime {
        shell: ShellState::default(),
        cw_scroll: 0.0,
        lib_scroll: 0.0,
        show_scroll: 0.0,
        col_scroll: 0.0,
        last_key: Instant::now(),
        pending_collection_focus: None,
    });
}

fn page_name(page: &Page) -> &'static str {
    match page {
        Page::Home => "home",
        Page::Apps => "apps",
        Page::Library => "library",
        Page::Show(_) => "show",
        Page::Movie => "movie",
        Page::Collection(_) => "collection",
        Page::Collections => "collections",
        Page::Sports => "sports",
        Page::Search => "search",
        Page::Settings => "settings",
        Page::DashboardPicker => "dashboard_picker",
        Page::QuickSwitchPicker => "quick_switch_picker",
    }
}

fn hint_for(page: &Page) -> &'static str {
    match page {
        Page::Home => "OK Select · Back Home · F12 Return from service",
        Page::Apps => "OK Launch app · Back Home",
        Page::Collections => "OK Open collection · Back Home",
        Page::Movie => "OK Play · ↓ Cast & crew · Back",
        Page::DashboardPicker => "OK Switch · ‹ › Pick · Back Cancel",
        Page::QuickSwitchPicker => "OK Add/remove · Back Done",
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
    let tv = laptop.global::<TvUi>();
    tv.set_focus_ticket(tv.get_focus_ticket() + 1);
    sync(laptop);
}

pub fn go_home(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    tv.set_ambient(false);
    tv.set_video_full(false);
    STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.switch_tab(Tab::Home);
        s.shell.focused_tile = 0;
        s.cw_scroll = 0.0;
        s.last_key = Instant::now();
    });
    sync(laptop);
    tv.set_focus_ticket(tv.get_focus_ticket() + 1);
}

pub fn go_apps(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    tv.set_ambient(false);
    tv.set_video_full(false);
    STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.switch_tab(Tab::Apps);
        s.shell.focused_tile = 0;
        s.lib_scroll = 0.0;
        s.last_key = Instant::now();
    });
    sync(laptop);
    tv.set_focus_ticket(tv.get_focus_ticket() + 1);
}

pub fn go_search(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    tv.set_ambient(false);
    tv.set_video_full(false);
    STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.push(Page::Search);
        s.last_key = Instant::now();
    });
    sync(laptop);
    tv.set_focus_ticket(tv.get_focus_ticket() + 1);
}

pub fn go_sports_game(laptop: &LaptopWindow, event_id: &str, date: &str) {
    let tv = laptop.global::<TvUi>();
    tv.set_ambient(false);
    tv.set_video_full(false);
    tv.set_sports_tab("scoreboard".into());
    let sb = laptop.global::<Scoreboard>();
    sb.set_show("all".into());
    sb.set_show_label("All sports".into());
    if !date.is_empty() {
        sb.set_day(date.into());
    }
    let mut target_tile = 7usize;
    let groups = sb.get_groups();
    'find: for gi in 0..groups.row_count() {
        if let Some(grp) = groups.row_data(gi) {
            let cards = grp.cards;
            for ci in 0..cards.row_count() {
                if let Some(c) = cards.row_data(ci)
                    && c.event_id == event_id
                    && c.star_focus_id >= 0
                {
                    target_tile = c.star_focus_id as usize;
                    break 'find;
                }
            }
        }
    }
    STATE.with_borrow_mut(|s| {
        s.shell.bar_focused = false;
        s.shell.switch_tab(Tab::Sports);
        s.shell.focused_tile = target_tile;
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
        crate::bridge::start_ambient(laptop);
        tv.set_ambient(true);
    }
}

pub fn push_show(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if !matches!(s.shell.current(), Page::Show(_)) {
            s.shell.push(Page::Show(String::new()));
        }
    });
    sync(laptop);
}

pub fn push_collection(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if !matches!(s.shell.current(), Page::Collection(_)) {
            s.shell.push(Page::Collection(String::new()));
        }
    });
    sync(laptop);
}

fn quick_switch_rects(laptop: &LaptopWindow) -> Vec<Rect> {
    let mut rects = vec![Rect { x: 96.0, y: 254.0, w: 1728.0, h: 96.0 }];
    let qs = laptop.global::<crate::QuickSwitch>();
    if !qs.get_automatic() {
        for i in 0..qs.get_choices().row_count() {
            let col = (i % 5) as f32;
            let row = (i / 5) as f32;
            rects.push(Rect {
                x: 96.0 + col * 349.0,
                y: 376.0 + row * 182.0,
                w: 332.0,
                h: 166.0,
            });
        }
    }
    rects
}

fn blades_movie_rects(laptop: &LaptopWindow) -> Vec<Rect> {
    let mp = laptop.global::<MoviePageState>();
    let ox = 228.0;
    let oy = 24.0;
    let mut rects = vec![Rect { x: ox + 44.0, y: oy + 318.0, w: 300.0, h: 58.0 }]; // 0: Play
    let has_col = mp.get_in_collection();
    // Matches V3Movie.y-shift: 24 px below the spec's y 404 / 504 / 580; 100 up without a card.
    let y_shift = if has_col { 24.0 } else { -76.0 };
    // From start / Trailer sit at the right end of the switch row (panel 1464 wide, 44 px
    // margins: right edge at x 1420). Focus indices stay 1 and 2, right after Play.
    let switch_y = oy + 504.0 + y_shift;
    let trailer_w = 150.0;
    let from_start_w = 140.0;
    let right = ox + 1420.0;
    if mp.get_show_from_start() {
        let trailer_space = if mp.get_has_trailer() { trailer_w + 12.0 } else { 0.0 };
        rects.push(Rect { x: right - trailer_space - from_start_w, y: switch_y, w: from_start_w, h: 48.0 });
    }
    if mp.get_has_trailer() {
        rects.push(Rect { x: right - trailer_w, y: switch_y, w: trailer_w, h: 48.0 });
    }
    if has_col {
        rects.push(Rect { x: ox + 44.0, y: oy + 428.0, w: 760.0, h: 76.0 });
    }
    rects.push(Rect { x: ox + 44.0, y: switch_y, w: 160.0, h: 48.0 }); // Cast & crew
    let has_extras = mp.get_extras().row_count() > 0;
    if has_extras {
        rects.push(Rect { x: ox + 210.0, y: switch_y, w: 120.0, h: 48.0 }); // Extras
    }
    let content_y = oy + 580.0 + y_shift;
    if mp.get_tab() == "extras" && has_extras {
        for i in 0..mp.get_extras().row_count() {
            let col = (i % 3) as f32;
            let row = (i / 3) as f32;
            rects.push(Rect { x: ox + 334.0 + col * 322.0, y: content_y + row * 246.0, w: 296.0, h: 222.0 });
        }
    } else {
        for i in 0..mp.get_crew().row_count() {
            rects.push(Rect { x: ox + 44.0, y: content_y + 34.0 + i as f32 * 84.0, w: 260.0, h: 72.0 });
        }
        for i in 0..mp.get_cast().row_count() {
            let col = (i % 6) as f32;
            let row = (i / 6) as f32;
            rects.push(Rect {
                x: ox + 334.0 + col * 160.0,
                y: content_y + 34.0 + row * 288.0,
                w: 142.0,
                h: 264.0,
            });
        }
    }
    rects
}

fn classic_movie_rects(laptop: &LaptopWindow) -> Vec<Rect> {
    let mp = laptop.global::<MoviePageState>();
    let mut rects = vec![Rect { x: 96.0, y: 452.0, w: 440.0, h: 120.0 }]; // 0: Play
    let mut bx = 552.0;
    if mp.get_show_from_start() {
        rects.push(Rect { x: bx, y: 452.0, w: 260.0, h: 120.0 });
        bx += 276.0;
    }
    if mp.get_has_trailer() {
        rects.push(Rect { x: bx, y: 452.0, w: 220.0, h: 120.0 });
        bx += 236.0;
    }
    if mp.get_in_collection() {
        rects.push(Rect { x: bx, y: 452.0, w: (1824.0 - bx).max(260.0), h: 120.0 });
    }
    rects.push(Rect { x: 96.0, y: 612.0, w: 168.0, h: 52.0 }); // Cast & crew tab
    let has_extras = mp.get_extras().row_count() > 0;
    if has_extras {
        rects.push(Rect { x: 300.0, y: 612.0, w: 110.0, h: 52.0 }); // Extras tab
    }
    if mp.get_tab() == "extras" && has_extras {
        for i in 0..mp.get_extras().row_count() {
            let col = (i % 3) as f32;
            let row = (i / 3) as f32;
            rects.push(Rect { x: 388.0 + col * 484.0, y: 700.0 + row * 356.0, w: 468.0, h: 316.0 });
        }
    } else {
        for i in 0..mp.get_crew().row_count() {
            rects.push(Rect { x: 96.0, y: 740.0 + i as f32 * 96.0, w: 260.0, h: 84.0 });
        }
        for i in 0..mp.get_cast().row_count() {
            let col = (i % 6) as f32;
            let row = (i / 6) as f32;
            rects.push(Rect { x: 388.0 + col * 242.0, y: 700.0 + row * 388.0, w: 226.0, h: 356.0 });
        }
    }
    rects
}

fn orbit_movie_rects(laptop: &LaptopWindow) -> Vec<Rect> {
    let mp = laptop.global::<MoviePageState>();
    let mut rects = vec![Rect { x: 110.0, y: 500.0, w: 104.0, h: 104.0 }]; // 0: Play
    let mut bx = 430.0;
    if mp.get_show_from_start() {
        rects.push(Rect { x: bx, y: 530.0, w: 150.0, h: 44.0 });
        bx += 168.0;
    }
    if mp.get_has_trailer() {
        rects.push(Rect { x: bx, y: 530.0, w: 140.0, h: 44.0 });
        bx += 158.0;
    }
    rects.push(Rect { x: bx, y: 530.0, w: 56.0, h: 44.0 }); // ···
    if mp.get_in_collection() {
        rects.push(Rect { x: 110.0, y: 638.0, w: 600.0, h: 96.0 });
    }
    rects.push(Rect { x: 1000.0, y: 220.0, w: 220.0, h: 48.0 }); // Cast & crew tab
    let has_extras = mp.get_extras().row_count() > 0;
    if has_extras {
        rects.push(Rect { x: 1232.0, y: 220.0, w: 180.0, h: 48.0 }); // Extras tab
    }
    if mp.get_tab() == "extras" && has_extras {
        for i in 0..mp.get_extras().row_count() {
            rects.push(Rect { x: 1000.0, y: 320.0 + i as f32 * 96.0, w: 810.0, h: 88.0 });
        }
    } else {
        let crew_n = mp.get_crew().row_count();
        for i in 0..crew_n {
            rects.push(Rect { x: 1000.0 + i as f32 * 274.0, y: 320.0, w: 258.0, h: 72.0 });
        }
        let cast_y = if crew_n > 0 { 448.0 } else { 320.0 };
        for i in 0..mp.get_cast().row_count() {
            let col = (i % 4) as f32;
            let row = (i / 4) as f32;
            rects.push(Rect { x: 1000.0 + col * 207.0, y: cast_y + row * 238.0, w: 187.0, h: 210.0 });
        }
    }
    rects
}

fn movie_activate_tile(laptop: &LaptopWindow, idx: usize) {
    let mp = laptop.global::<MoviePageState>();
    let is_orbit = laptop.global::<Display>().get_dashboard_ui() == "orbit";
    let mut cur = 0usize;
    if idx == cur {
        mp.invoke_play();
        return;
    }
    cur += 1;
    if mp.get_show_from_start() {
        if idx == cur {
            mp.invoke_play_from_start();
            return;
        }
        cur += 1;
    }
    if mp.get_has_trailer() {
        if idx == cur {
            mp.invoke_play_trailer();
            return;
        }
        cur += 1;
    }
    if is_orbit {
        if idx == cur {
            mp.invoke_toggle_watched();
            return;
        }
        cur += 1;
    }
    if mp.get_in_collection() {
        if idx == cur {
            mp.invoke_open_collection();
            return;
        }
        cur += 1;
    }
    if idx == cur {
        mp.invoke_set_tab("cast".into());
        return;
    }
    cur += 1;
    let has_extras = mp.get_extras().row_count() > 0;
    if has_extras {
        if idx == cur {
            mp.invoke_set_tab("extras".into());
            return;
        }
        cur += 1;
    }
    let rel = idx.saturating_sub(cur);
    if mp.get_tab() == "extras" && has_extras {
        if let Some(ex) = mp.get_extras().row_data(rel) {
            mp.invoke_play_extra(ex.key);
        }
    } else {
        let crew_n = mp.get_crew().row_count();
        if rel < crew_n {
            if let Some(p) = mp.get_crew().row_data(rel) {
                mp.invoke_search_person(p.name);
            }
        } else if let Some(p) = mp.get_cast().row_data(rel - crew_n) {
            mp.invoke_search_person(p.name);
        }
    }
}

pub fn push_movie(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        s.show_scroll = 0.0;
        if !matches!(s.shell.current(), Page::Movie) {
            s.shell.push(Page::Movie);
        }
        s.shell.focused_tile = 0;
    });
    laptop.global::<TvUi>().set_show_scroll(0.0);
    sync(laptop);
}

pub fn push_search_results(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if !matches!(s.shell.current(), Page::Search) {
            s.shell.push(Page::Search);
        }
        s.shell.focused_tile = 39;
    });
    sync(laptop);
}

pub fn open_collection_from_movie(laptop: &LaptopWindow, movie_key: String) {
    STATE.with_borrow_mut(|s| {
        s.pending_collection_focus = Some(movie_key);
        if !matches!(s.shell.current(), Page::Collection(_)) {
            s.shell.push(Page::Collection(String::new()));
        }
    });
    sync(laptop);
}

pub fn on_collection_loaded(laptop: &LaptopWindow) {
    let target = STATE.with_borrow_mut(|s| s.pending_collection_focus.take());
    let Some(movie_key) = target else { return };
    let col = laptop.global::<CollectionPageState>();
    let items = col.get_items();
    for i in 0..items.row_count() {
        if let Some(row) = items.row_data(i)
            && row.rating_key.as_str() == movie_key
        {
            STATE.with_borrow_mut(|s| {
                if matches!(s.shell.current(), Page::Collection(_)) {
                    s.shell.focused_tile = 1 + i;
                }
            });
            sync(laptop);
            break;
        }
    }
}

pub fn focus_movie_tab(laptop: &LaptopWindow, _tab: &str) {
    let mp = laptop.global::<MoviePageState>();
    let is_orbit = laptop.global::<Display>().get_dashboard_ui() == "orbit";
    let first = 2
        + usize::from(mp.get_show_from_start())
        + usize::from(mp.get_has_trailer())
        + usize::from(is_orbit)
        + usize::from(mp.get_in_collection())
        + usize::from(mp.get_extras().row_count() > 0);
    STATE.with_borrow_mut(|s| {
        if matches!(s.shell.current(), Page::Movie) {
            s.shell.focused_tile = first;
        }
    });
    sync(laptop);
}

pub fn open_dashboard_picker(laptop: &LaptopWindow) {
    let cur = tvdash_core::config::DashboardUi::parse(&laptop.global::<Display>().get_dashboard_ui());
    let cur_idx = crate::bridge::display::enabled_uis()
        .iter()
        .position(|&u| u == cur)
        .unwrap_or(0);
    STATE.with_borrow_mut(|s| {
        if !matches!(s.shell.current(), Page::DashboardPicker) {
            s.shell.push(Page::DashboardPicker);
        }
        s.shell.focused_tile = cur_idx;
    });
    sync(laptop);
}

pub fn pop_picker(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if matches!(s.shell.current(), Page::DashboardPicker) {
            s.shell.back();
        }
    });
    sync(laptop);
}

pub fn open_quick_switch_picker(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if !matches!(s.shell.current(), Page::QuickSwitchPicker) {
            s.shell.push(Page::QuickSwitchPicker);
        }
        s.shell.focused_tile = 0;
    });
    sync(laptop);
}

pub fn open_settings_display_focused_on_picker(laptop: &LaptopWindow) {
    laptop.global::<TvUi>().set_settings_tab("display".into());
    let picker_tile = match tvdash_core::config::DashboardUi::parse(&laptop.global::<Display>().get_dashboard_ui()) {
        tvdash_core::config::DashboardUi::Classic => 13,
        tvdash_core::config::DashboardUi::Blades => 13,
        tvdash_core::config::DashboardUi::Ps5 => 0,
        // Orbit's Settings focus table only covers the left-rail categories (0 = Display).
        tvdash_core::config::DashboardUi::Orbit => 0,
    };
    STATE.with_borrow_mut(|s| {
        s.shell.switch_tab(Tab::Settings);
        s.shell.focused_tile = picker_tile; // "Dashboard UI" row in each UI's Settings › Display
    });
    sync(laptop);
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
                    if browse && s.shell.focused_tile >= 32 {
                        let vert = Rect { x: r.y - 256.0, y: 0.0, w: r.h, h: r.w };
                        s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 748.0, 16.0);
                    } else if browse {
                        s.lib_scroll = 0.0;
                    }
                }
                Page::Apps => {
                    let n = laptop.global::<crate::AppsPage>().get_apps().row_count();
                    if s.shell.focused_tile < n {
                        let vert = Rect { x: r.y - 124.0, y: 0.0, w: r.h, h: r.w };
                        s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 824.0, 16.0);
                    }
                }
                Page::Collections => {
                    let vert = Rect { x: r.y - 180.0, y: 0.0, w: r.h, h: r.w };
                    s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 824.0, 16.0);
                }
                Page::Movie => {
                    let rects = tile_rects(laptop, &Page::Movie);
                    let mp = laptop.global::<MoviePageState>();
                    let content_base = 2
                        + usize::from(mp.get_show_from_start())
                        + usize::from(mp.get_has_trailer())
                        + usize::from(mp.get_in_collection())
                        + usize::from(mp.get_extras().row_count() > 0);
                    let crew_n = if mp.get_tab() == "cast" { mp.get_crew().row_count() } else { 0 };
                    let grid_base = content_base + crew_n;
                    if s.shell.focused_tile < grid_base {
                        s.show_scroll = 0.0;
                    } else if let Some(r) = rects.get(s.shell.focused_tile) {
                        let rel_y = r.y - 700.0;
                        let max_scroll = (rects.last().map(|l| l.y + l.h - 1020.0).unwrap_or(0.0)).max(0.0);
                        if rel_y < s.show_scroll {
                            s.show_scroll = rel_y.max(0.0);
                        } else if rel_y + r.h > s.show_scroll + 820.0 {
                            s.show_scroll = (rel_y + r.h - 820.0).clamp(0.0, max_scroll);
                        }
                    }
                    laptop.global::<TvUi>().set_show_scroll(s.show_scroll);
                }
                Page::Show(_) => {
                    let show = laptop.global::<ShowPageState>();
                    let season_n = show.get_seasons().row_count();
                    let grid_base = 2 + season_n;
                    if s.shell.focused_tile >= grid_base {
                        let ep_idx = s.shell.focused_tile - grid_base;
                        if let Some(ep) = show.get_episodes().row_data(ep_idx) {
                            show.set_focused_episode(ep);
                        }
                    }
                }
                Page::Collection(_) if s.shell.focused_tile >= 1 => {
                    let vert = Rect { x: r.y - 180.0, y: 0.0, w: r.h, h: r.w };
                    s.col_scroll = focus::panorama_scroll(vert, s.col_scroll, 824.0, 16.0);
                }
                Page::Collection(_) => {
                    s.col_scroll = 0.0;
                }
                Page::Sports => {
                    let sp = laptop.global::<Sports>();
                    if sp.get_rows().row_count() == 0 && !sp.get_loading() && sp.get_error().is_empty() {
                        sp.invoke_load(sp.get_schedule(), 0);
                    }
                }
                Page::Settings if tv.get_settings_tab() == "sites" => {
                    if s.shell.focused_tile >= 8 {
                        let vert = Rect { x: r.y - 244.0, y: 0.0, w: r.h, h: r.w };
                        s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 836.0, 16.0);
                    } else {
                        s.lib_scroll = 0.0;
                    }
                }
                _ => {}
            }
        }
        s.cw_scroll = 0.0;
        let keep_settings_scroll = matches!(s.shell.current(), Page::Settings) && tv.get_settings_tab() == "sites";
        if !matches!(s.shell.current(), Page::Apps | Page::Library | Page::Collections) && !keep_settings_scroll {
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
    if tv.get_screen() == "library" {
        crate::bridge::ensure_library_loaded(laptop);
        let lib = laptop.global::<Library>();
        if !lib.get_browse_all() {
            let tile = tv.get_focused_tile();
            if tile >= 4 {
                laptop.global::<LibraryLanding>().invoke_focus_changed(tile);
            }
        }
    }
    if tv.get_screen() == "collections" {
        crate::bridge::ensure_collections_loaded(laptop);
    }
}

fn orbit_tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
    match page {
        Page::Home => {
            let mut rects = vec![
                Rect { x: 110.0, y: 420.0, w: 104.0, h: 104.0 }, // 0: Hero Play
                Rect { x: 340.0, y: 448.0, w: 140.0, h: 46.0 },  // 1: From start
                Rect { x: 496.0, y: 448.0, w: 72.0, h: 46.0 },   // 2: More
            ];
            let hs = laptop.global::<crate::HomeState>();
            let up_n = hs.get_up_next().row_count().min(4);
            for i in 0..up_n {
                let indent = match i { 0 => 120.0, 1 => 70.0, 2 => 40.0, _ => 30.0 };
                rects.push(Rect { x: 1180.0 + indent, y: 185.0 + i as f32 * 110.0, w: 600.0, h: 104.0 });
            }
            if hs.get_next_game().visible || hs.get_just_added().visible {
                rects.push(Rect { x: 1270.0, y: 640.0, w: 550.0, h: 108.0 });
            }
            rects.push(Rect { x: 885.0, y: 790.0, w: 150.0, h: 150.0 }); // Apps dial top node
            rects
        }
        Page::Apps => {
            let n = laptop.global::<crate::AppsPage>().get_apps().row_count().max(1);
            let mut rects: Vec<Rect> = (0..n)
                .map(|i| {
                    let ang = (i as f32 * 360.0 / n as f32 - 90.0).to_radians();
                    let cx = 960.0 + 380.0 * ang.cos();
                    let cy = 590.0 + 380.0 * ang.sin();
                    Rect { x: cx - 62.0, y: cy - 62.0, w: 124.0, h: 124.0 }
                })
                .collect();
            rects.push(Rect { x: 110.0, y: 690.0, w: 260.0, h: 46.0 }); // Adjust in Settings ›
            if laptop.global::<crate::AppsPage>().get_has_last_app() {
                rects.push(Rect { x: 1510.0, y: 440.0, w: 300.0, h: 220.0 }); // Last played app
            }
            rects
        }
        Page::Library => {
            let lib = laptop.global::<Library>();
            if lib.get_browse_all() {
                let mut rects = vec![
                    Rect { x: 110.0, y: 130.0, w: 130.0, h: 46.0 }, // 0: TV Series
                    Rect { x: 252.0, y: 130.0, w: 110.0, h: 46.0 }, // 1: Movies
                    Rect { x: 374.0, y: 130.0, w: 140.0, h: 46.0 }, // 2: Browse all
                    Rect { x: 526.0, y: 130.0, w: 160.0, h: 46.0 }, // 3: Watching now
                ];
                for i in 0..28 {
                    rects.push(Rect {
                        x: 112.0,
                        y: 200.0 + i as f32 * 30.0,
                        w: 124.0,
                        h: 80.0,
                    });
                }
                let n = laptop.global::<crate::Browse>().get_items().row_count();
                for i in 0..n {
                    rects.push(Rect {
                        x: 290.0 + (i % 6) as f32 * 266.0,
                        y: 264.0 + (i / 6) as f32 * 274.0,
                        w: 190.0,
                        h: 254.0,
                    });
                }
                return rects;
            }
            let ll = laptop.global::<crate::LibraryLanding>();
            let mut rects = vec![
                Rect { x: 110.0, y: 130.0, w: 110.0, h: 46.0 }, // 0: Movies
                Rect { x: 232.0, y: 130.0, w: 130.0, h: 46.0 }, // 1: TV Series
                Rect { x: 374.0, y: 130.0, w: 140.0, h: 46.0 }, // 2: Browse all
                if ll.get_watching_now_count() > 0 {
                    Rect { x: 526.0, y: 130.0, w: 160.0, h: 46.0 } // 3: Watching now
                } else {
                    Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 }
                },
            ];
            for (i, r) in ll.get_rail().iter().enumerate() {
                if r.enabled {
                    rects.push(Rect { x: 110.0, y: 220.0 + 130.0 * i as f32, w: 380.0, h: 116.0 });
                }
            }
            let grid_n = ll.get_grid().row_count().min(6);
            for i in 0..grid_n {
                let col = (i % 3) as f32;
                let row = (i / 3) as f32;
                rects.push(Rect { x: 545.0 + 260.0 * col, y: 242.0 + 296.0 * row, w: 190.0, h: 190.0 });
            }
            if grid_n > 0 {
                rects.push(Rect { x: 1376.0, y: 844.0, w: 84.0, h: 84.0 }); // Card Play
            }
            rects
        }
        Page::Collections => {
            let n = laptop.global::<crate::Library>().get_collections().row_count();
            (0..n)
                .map(|i| {
                    let col = (i % 6) as f32;
                    let row = (i / 6) as f32;
                    Rect { x: 110.0 + col * 284.0, y: 464.0 + row * 296.0, w: 240.0, h: 274.0 }
                })
                .collect()
        }
        Page::Movie => Vec::new(),
        Page::Show(_) => {
            let sp = laptop.global::<crate::ShowPageState>();
            let n = sp.get_episodes().row_count().min(24).max(1);
            (0..n)
                .map(|i| {
                    let ang = (i as f32 * 360.0 / n as f32 - 90.0).to_radians();
                    let cx = 1440.0 + 340.0 * ang.cos();
                    let cy = 580.0 + 340.0 * ang.sin();
                    Rect { x: cx - 38.0, y: cy - 38.0, w: 76.0, h: 76.0 }
                })
                .collect()
        }
        Page::Collection(_) => {
            let n = laptop.global::<crate::CollectionPageState>().get_items().row_count().min(12);
            (0..n)
                .map(|i| Rect { x: 750.0, y: 214.0 + i as f32 * 92.0, w: 1060.0, h: 72.0 })
                .collect()
        }
        Page::Sports => vec![
            Rect { x: 110.0, y: 130.0, w: 160.0, h: 46.0 },
            Rect { x: 282.0, y: 130.0, w: 160.0, h: 46.0 },
            Rect { x: 560.0, y: 220.0, w: 1250.0, h: 250.0 },
        ],
        Page::Search => (0..36)
            .map(|i| {
                let gx = (i % 6) as f32;
                let gy = (i / 6) as f32;
                Rect { x: 110.0 + gx * 108.0, y: 306.0 + gy * 106.0, w: 86.0, h: 86.0 }
            })
            .collect(),
        Page::DashboardPicker => {
            let n = laptop.global::<Display>().get_dashboard_choices().row_count();
            (0..n)
                .map(|i| Rect {
                    x: 96.0 + i as f32 * 436.0,
                    y: 330.0,
                    w: 420.0,
                    h: 600.0,
                })
                .collect()
        }
        Page::QuickSwitchPicker => quick_switch_rects(laptop),
        Page::Settings => (0..7)
            .map(|i| Rect { x: 110.0, y: 196.0 + i as f32 * 88.0, w: 380.0, h: 80.0 })
            .collect(),
    }
}

fn ps5_tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
    match page {
        Page::Home => vec![
            Rect { x: 96.0, y: 620.0, w: 220.0, h: 72.0 }, // 0: Primary button
            Rect { x: 332.0, y: 620.0, w: 200.0, h: 72.0 }, // 1: From start
            Rect { x: 548.0, y: 620.0, w: 72.0, h: 72.0 },  // 2: More
        ],
        Page::Apps => {
            let n = laptop.global::<crate::AppsPage>().get_apps().row_count();
            (0..n)
                .map(|i| {
                    let col = (i % 6) as f32;
                    let row = (i / 6) as f32;
                    Rect {
                        x: 96.0 + col * 272.0,
                        y: 196.0 + row * 214.0,
                        w: 248.0,
                        h: 186.0,
                    }
                })
                .collect()
        }
        Page::Library => {
            let mut rects = vec![
                Rect { x: 96.0, y: 420.0, w: 180.0, h: 72.0 }, // 0: Play
                Rect { x: 292.0, y: 420.0, w: 200.0, h: 72.0 }, // 1: Show / From start
            ];
            let ll = laptop.global::<crate::LibraryLanding>();
            for i in 0..ll.get_shelf1().row_count().min(12) {
                let col = (i % 6) as f32;
                let row = (i / 6) as f32;
                rects.push(Rect {
                    x: 96.0 + col * 292.0,
                    y: 570.0 + row * 240.0,
                    w: 268.0,
                    h: 216.0,
                });
            }
            rects
        }
        Page::Show(_) => vec![
            Rect { x: 96.0, y: 500.0, w: 260.0, h: 72.0 }, // 0: Resume / Play
            Rect { x: 372.0, y: 500.0, w: 220.0, h: 72.0 }, // 1: From S1 E1
        ],
        Page::Collections => {
            let n = laptop.global::<Library>().get_collections().row_count();
            (0..n)
                .map(|i| {
                    let col = (i % 6) as f32;
                    let row = (i / 6) as f32;
                    Rect {
                        x: 96.0 + col * 292.0,
                        y: 478.0 + row * 440.0,
                        w: 248.0,
                        h: 416.0,
                    }
                })
                .collect()
        }
        Page::Collection(_) => vec![
            Rect { x: 96.0, y: 540.0, w: 260.0, h: 72.0 }, // 0: Resume
            Rect { x: 372.0, y: 540.0, w: 200.0, h: 72.0 }, // 1: From start
        ],
        Page::Sports => {
            let groups = laptop.global::<crate::Scoreboard>().get_groups();
            let mut rects = Vec::new();
            let mut y = 320.0;
            for g in 0..groups.row_count() {
                if let Some(grp) = groups.row_data(g) {
                    let count = grp.cards.row_count();
                    for ci in 0..count {
                        let col = (ci % 3) as f32;
                        let row = (ci / 3) as f32;
                        rects.push(Rect {
                            x: 96.0 + col * 582.0,
                            y: y + row * 224.0,
                            w: 562.0,
                            h: 204.0,
                        });
                    }
                    let rows = count.div_ceil(3) as f32;
                    y += rows * 224.0 + 48.0;
                }
            }
            if rects.is_empty() {
                rects.push(Rect { x: 96.0, y: 208.0, w: 240.0, h: 56.0 });
            }
            rects
        }
        Page::DashboardPicker => {
            let n = laptop.global::<Display>().get_dashboard_choices().row_count();
            (0..n)
                .map(|i| Rect {
                    x: 96.0 + i as f32 * 436.0,
                    y: 330.0,
                    w: 420.0,
                    h: 600.0,
                })
                .collect()
        }
        Page::QuickSwitchPicker => quick_switch_rects(laptop),
        Page::Settings => (0..6)
            .map(|i| Rect {
                x: 540.0,
                y: 150.0 + i as f32 * 88.0,
                w: 1284.0,
                h: 76.0,
            })
            .collect(),
        Page::Movie => orbit_movie_rects(laptop),
        Page::Search => (0..36)
            .map(|i| {
                let col = (i % 6) as f32;
                let row = (i / 6) as f32;
                Rect {
                    x: 96.0 + col * 94.0,
                    y: 238.0 + row * 84.0,
                    w: 84.0,
                    h: 74.0,
                }
            })
            .collect(),
    }
}

fn blades_tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
    let tv = laptop.global::<TvUi>();
    match page {
        Page::Home => {
            let ox = 24.0;
            let oy = 24.0;
            let hs = laptop.global::<crate::HomeState>();
            let up_n = hs.get_up_next().row_count().min(4);
            let has_game = hs.get_next_game().visible;
            let offscreen = Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 };

            let hero_h = if up_n > 0 { 440.0 } else { 477.0 };
            let mut rects = vec![Rect { x: ox + 44.0, y: oy + 96.0, w: 848.0, h: hero_h }]; // 0: Hero

            // 1..=4: Up next cards
            for i in 0..4 {
                if i < up_n {
                    let cx = if i == 0 { 44.0 } else { 326.0 + (i - 1) as f32 * 193.0 };
                    let cw = if i == 0 { 268.0 } else { 179.0 };
                    rects.push(Rect { x: ox + cx, y: oy + 596.0, w: cw, h: 196.0 });
                } else {
                    rects.push(offscreen);
                }
            }

            // 5: Next Game card
            rects.push(if has_game {
                Rect { x: ox + 920.0, y: oy + 308.0, w: 568.0, h: 210.0 }
            } else {
                offscreen
            });

            // 6..=9: Bottom Quick actions bar (Search, Screensaver, Sleep timer, Theme)
            for i in 0..4 {
                rects.push(Rect {
                    x: ox + 44.0 + i as f32 * 365.0,
                    y: oy + 850.0,
                    w: 349.0,
                    h: 88.0,
                });
            }

            // 10..=16: Right rail Apps 2x4 stack (7 recent-first)
            let apps_y = if has_game { 534.0 } else { 308.0 };
            let apps_h = if has_game { 262.0 } else { 488.0 };
            let tile_h = (apps_h - 36.0) / 4.0;
            let bar_n = hs.get_apps_bar().row_count().min(7);
            for i in 0..7 {
                if i < bar_n {
                    rects.push(Rect {
                        x: ox + 920.0 + (i % 2) as f32 * 292.0,
                        y: oy + apps_y + (i / 2) as f32 * (tile_h + 12.0),
                        w: 276.0,
                        h: tile_h,
                    });
                } else {
                    rects.push(offscreen);
                }
            }

            // 17: All apps ›
            rects.push(Rect {
                x: ox + 920.0 + 292.0,
                y: oy + apps_y + 3.0 * (tile_h + 12.0),
                w: 276.0,
                h: tile_h,
            });
            rects
        }
        Page::Apps => {
            let ox = 92.0;
            let oy = 24.0;
            let apps_page = laptop.global::<crate::AppsPage>();
            let n = apps_page.get_apps().row_count();
            let mut rects: Vec<Rect> = (0..n)
                .map(|i| Rect {
                    x: ox + 40.0 + (i % 4) as f32 * 366.0,
                    y: oy + 200.0 + (i / 4) as f32 * 218.0,
                    w: 344.0,
                    h: 166.0,
                })
                .collect();
            rects.push(Rect { x: ox + 1272.0, y: oy + 111.0, w: 196.0, h: 48.0 });
            if apps_page.get_has_last_app() {
                rects.push(Rect { x: ox + 40.0, y: oy + 94.0, w: 440.0, h: 82.0 });
            }
            rects
        }
        Page::Library => {
            let ox = 160.0;
            let oy = 24.0;
            let lib = laptop.global::<Library>();
            let mut rects = vec![
                Rect { x: ox + 208.0, y: oy + 38.0, w: 126.0, h: 36.0 },
                Rect { x: ox + 336.0, y: oy + 38.0, w: 112.0, h: 36.0 },
                Rect { x: ox + 450.0, y: oy + 38.0, w: 132.0, h: 36.0 },
                Rect { x: ox + 610.0, y: oy + 34.0, w: 200.0, h: 44.0 },
            ];
            if lib.get_browse_all() {
                for i in 0..28 {
                    rects.push(Rect {
                        x: ox + 48.0 + i as f32 * 44.0,
                        y: oy + 106.0,
                        w: 40.0,
                        h: 36.0,
                    });
                }
                let n = lib.get_browse_items().row_count();
                for i in 0..n {
                    rects.push(Rect {
                        x: ox + 40.0 + (i % 8) as f32 * 184.0,
                        y: oy + 210.0 + (i / 8) as f32 * 316.0,
                        w: 168.0,
                        h: 248.0,
                    });
                }
            } else {
                let lib_scroll = tv.get_lib_scroll();
                let ll = laptop.global::<LibraryLanding>();
                let s1_n = ll.get_shelf1().row_count().min(4);
                let s2_n = ll.get_shelf2().row_count().min(6);
                let s3_n = ll.get_shelf3().row_count().min(6);
                let s2_y = oy + 452.0 + if s1_n > 0 { 316.0 } else { 10.0 } + 36.0;
                let s3_y = s2_y + if s2_n > 0 { 276.0 } else { 0.0 };
                for i in 0..s1_n {
                    rects.push(Rect {
                        x: ox + 40.0 + i as f32 * 384.0,
                        y: oy + 492.0 - lib_scroll,
                        w: 368.0,
                        h: 206.0,
                    });
                }
                for i in 0..s2_n {
                    rects.push(Rect {
                        x: ox + 148.0 + i as f32 * 256.0,
                        y: s2_y - lib_scroll,
                        w: 134.0,
                        h: 200.0,
                    });
                }
                for i in 0..s3_n {
                    rects.push(Rect {
                        x: ox + 148.0 + i as f32 * 256.0,
                        y: s3_y - lib_scroll,
                        w: 134.0,
                        h: 200.0,
                    });
                }
            }
            rects
        }
        Page::Collections => {
            let ox = 228.0;
            let oy = 24.0;
            let n = laptop.global::<Library>().get_collections().row_count();
            (0..n)
                .map(|i| Rect {
                    x: ox + 40.0 + (i % 7) as f32 * 218.0,
                    y: oy + 112.0 + (i / 7) as f32 * 352.0,
                    w: 198.0,
                    h: 280.0,
                })
                .collect()
        }
        Page::Movie => blades_movie_rects(laptop),
        Page::Show(_) => {
            let ox = 228.0;
            let oy = 24.0;
            let show = laptop.global::<ShowPageState>();
            let mut rects = vec![
                Rect { x: ox + 40.0, y: oy + 304.0, w: 280.0, h: 52.0 },
                Rect { x: ox + 340.0, y: oy + 304.0, w: 170.0, h: 52.0 },
            ];
            let season_n = show.get_seasons().row_count();
            for i in 0..season_n {
                rects.push(Rect {
                    x: ox + 40.0 + i as f32 * 130.0,
                    y: oy + 380.0,
                    w: 120.0,
                    h: 42.0,
                });
            }
            let ep_n = show.get_episodes().row_count();
            for i in 0..ep_n {
                rects.push(Rect {
                    x: ox + 360.0 + (i % 3) as f32 * 384.0,
                    y: oy + 446.0 + (i / 3) as f32 * 280.0,
                    w: 366.0,
                    h: 204.0,
                });
            }
            if !show.get_next_season_label().is_empty() {
                rects.push(Rect {
                    x: ox + 360.0 + (ep_n % 3) as f32 * 384.0,
                    y: oy + 446.0 + (ep_n / 3) as f32 * 280.0,
                    w: 366.0,
                    h: 204.0,
                });
            }
            if show.get_has_trailer() {
                rects.push(Rect {
                    x: ox + 530.0,
                    y: oy + 304.0,
                    w: 180.0,
                    h: 52.0,
                });
            }
            rects
        }
        Page::Collection(_) => {
            let ox = 296.0;
            let oy = 24.0;
            let col = laptop.global::<CollectionPageState>();
            let mut rects = vec![Rect { x: ox + 40.0, y: oy + 320.0, w: 220.0, h: 48.0 }];
            let n = col.get_items().row_count();
            for i in 0..n {
                rects.push(Rect {
                    x: ox + 410.0 + (i % 3) as f32 * 366.0,
                    y: oy + 124.0 + (i / 3) as f32 * 260.0,
                    w: 348.0,
                    h: 190.0,
                });
            }
            rects
        }
        Page::Search => {
            let ox = 92.0;
            let oy = 24.0;
            let mut rects = Vec::with_capacity(51);
            for i in 0..36 {
                rects.push(Rect {
                    x: ox + 40.0 + (i % 6) as f32 * 82.0,
                    y: oy + 214.0 + (i / 6) as f32 * 80.0,
                    w: 72.0,
                    h: 70.0,
                });
            }
            for i in 0..3 {
                rects.push(Rect {
                    x: ox + 40.0 + i as f32 * 166.0,
                    y: oy + 706.0,
                    w: 154.0,
                    h: 58.0,
                });
            }
            let n = laptop.global::<Watch>().get_included().row_count().min(12);
            for i in 0..n {
                rects.push(Rect {
                    x: ox + 568.0 + (i % 4) as f32 * 238.0,
                    y: oy + 172.0 + (i / 4) as f32 * 268.0,
                    w: 222.0,
                    h: 196.0,
                });
            }
            rects
        }
        Page::Sports => {
            let ox = 296.0;
            let oy = 24.0;
            let mut rects = vec![
                Rect { x: ox + 200.0, y: oy + 38.0, w: 144.0, h: 36.0 },
                Rect { x: ox + 346.0, y: oy + 38.0, w: 126.0, h: 36.0 },
            ];
            if tv.get_sports_tab() == "scoreboard" {
                let sb = laptop.global::<Scoreboard>();
                let offscreen = Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 };
                rects.push(Rect { x: ox + 44.0, y: oy + 100.0, w: 44.0, h: 44.0 });   // 2: Prev
                rects.push(Rect { x: ox + 220.0, y: oy + 100.0, w: 44.0, h: 44.0 });  // 3: Next
                rects.push(if !sb.get_is_today() {
                    Rect { x: ox + 280.0, y: oy + 96.0, w: 96.0, h: 52.0 }
                } else {
                    offscreen
                }); // 4: Today
                rects.push(Rect { x: ox + 388.0, y: oy + 96.0, w: 240.0, h: 52.0 }); // 5: Dropdown
                rects.push(Rect { x: ox + 640.0, y: oy + 96.0, w: 124.0, h: 52.0 }); // 6: Refresh

                let groups = sb.get_groups();
                let mut y = oy + 170.0;
                for gi in 0..groups.row_count() {
                    if let Some(grp) = groups.row_data(gi) {
                        y += 46.0;
                        let cards = grp.cards;
                        let n = cards.row_count();
                        for ci in 0..n {
                            if let Some(c) = cards.row_data(ci) {
                                let col = (ci % 3) as f32;
                                let row = (ci / 3) as f32;
                                let cx = ox + 40.0 + col * 490.0;
                                let cy = y + row * 236.0;
                                rects.push(Rect { x: cx + 260.0, y: cy + 174.0, w: 92.0, h: 38.0 });
                                if !c.game_url.is_empty() {
                                    rects.push(Rect { x: cx + 360.0, y: cy + 174.0, w: 92.0, h: 38.0 });
                                }
                            }
                        }
                        y += n.div_ceil(3) as f32 * 236.0 + 14.0;
                    }
                }
            }
            rects
        }
        Page::DashboardPicker => {
            let n = laptop.global::<Display>().get_dashboard_choices().row_count();
            (0..n)
                .map(|i| Rect {
                    x: 96.0 + i as f32 * 436.0,
                    y: 330.0,
                    w: 420.0,
                    h: 600.0,
                })
                .collect()
        }
        Page::QuickSwitchPicker => quick_switch_rects(laptop),
        Page::Settings => {
            let ox = 364.0;
            let oy = 24.0;
            let mut rects = (0..7)
                .map(|i| Rect {
                    x: ox + 40.0,
                    y: oy + 106.0 + i as f32 * 56.0,
                    w: 240.0,
                    h: 48.0,
                })
                .collect::<Vec<_>>();
            match tv.get_settings_tab().as_str() {
                "accounts" => {
                    let n = laptop.global::<Accounts>().get_accounts().row_count();
                    for i in 0..n {
                        rects.push(Rect {
                            x: ox + 316.0 + (i % 2) as f32 * 610.0,
                            y: oy + 106.0 + (i / 2) as f32 * 144.0,
                            w: 590.0,
                            h: 128.0,
                        });
                    }
                }
                "sites" => {
                    rects.push(Rect { x: ox + 1420.0, y: oy + 106.0, w: 140.0, h: 40.0 });
                    let n = laptop.global::<Accounts>().get_sites().row_count();
                    for i in 0..n {
                        rects.push(Rect {
                            x: ox + 316.0,
                            y: oy + 162.0 + i as f32 * 64.0,
                            w: 1232.0,
                            h: 56.0,
                        });
                    }
                }
                "server" => {
                    rects.push(Rect { x: ox + 1420.0, y: oy + 106.0, w: 140.0, h: 40.0 });
                }
                "sports" => {
                    // 7..=11: Starting soon (Off, 5, 10, 15, 30)
                    for i in 0..5 {
                        rects.push(Rect { x: ox + 1150.0 + i as f32 * 76.0, y: oy + 170.0, w: 68.0, h: 36.0 });
                    }
                    // 12..=13: Final score (On, Off)
                    rects.push(Rect { x: ox + 1380.0, y: oy + 220.0, w: 68.0, h: 36.0 });
                    rects.push(Rect { x: ox + 1460.0, y: oy + 220.0, w: 68.0, h: 36.0 });
                    // 14..=15: During video (Show, Hold)
                    rects.push(Rect { x: ox + 1380.0, y: oy + 270.0, w: 68.0, h: 36.0 });
                    rects.push(Rect { x: ox + 1460.0, y: oy + 270.0, w: 68.0, h: 36.0 });
                    // 16+: Starred games Unstar
                    let n = laptop.global::<SportsAlertsSettings>().get_starred_games().row_count();
                    for i in 0..n {
                        rects.push(Rect { x: ox + 1420.0, y: oy + 390.0 + i as f32 * 62.0, w: 96.0, h: 34.0 });
                    }
                }
                "diagnostics" | "changelog" => {}
                _ => {
                    rects.extend([
                        Rect { x: ox + 316.0, y: oy + 106.0, w: 820.0, h: 82.0 },
                        Rect { x: ox + 316.0, y: oy + 200.0, w: 820.0, h: 82.0 },
                        Rect { x: ox + 316.0, y: oy + 294.0, w: 820.0, h: 82.0 },
                        Rect { x: ox + 316.0, y: oy + 388.0, w: 820.0, h: 82.0 },
                        Rect { x: ox + 1168.0, y: oy + 486.0, w: 392.0, h: 56.0 },
                        Rect { x: ox + 1168.0, y: oy + 554.0, w: 392.0, h: 56.0 },
                        Rect { x: ox + 1168.0, y: oy + 190.0, w: 392.0, h: 254.0 },
                        Rect { x: ox + 316.0, y: oy + 482.0, w: 820.0, h: 82.0 }, // 13: Sleep warning
                    ]);
                    let places_n = laptop.global::<Display>().get_places().row_count().min(4);
                    for i in 0..places_n {
                        rects.push(Rect {
                            x: ox + 316.0,
                            y: oy + 680.0 + i as f32 * 54.0,
                            w: 820.0,
                            h: 46.0,
                        });
                    }
                    for _ in places_n..4 {
                        rects.push(Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 });
                    }
                    rects.push(Rect { x: ox + 1168.0, y: oy + 622.0, w: 392.0, h: 56.0 }); // 19: Startup animation
                    rects.push(Rect { x: ox + 1168.0, y: oy + 690.0, w: 392.0, h: 56.0 }); // 20: Preview startup animation
                    rects.push(Rect { x: ox + 1168.0, y: oy + 418.0, w: 392.0, h: 152.0 }); // 21: Quick switch
                }
            }
            rects
        }
    }
}

fn tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
    if laptop.global::<crate::WeatherCardState>().get_visible() {
        let bottom = if laptop.global::<TvUi>().get_hud_open() { 250.0 } else { 120.0 };
        let card_x = 1920.0 - 96.0 - 560.0;
        let btn_y = 1080.0 - bottom - 22.0 - 54.0;
        return vec![
            Rect { x: card_x + 24.0, y: btn_y, w: 140.0, h: 54.0 },
            Rect { x: card_x + 176.0, y: btn_y, w: 140.0, h: 54.0 },
        ];
    }
    let al = laptop.global::<SportsAlertState>();
    if al.get_visible() {
        let bottom = if laptop.global::<TvUi>().get_hud_open() { 250.0 } else { 120.0 };
        let card_x = 1920.0 - 96.0 - 560.0;
        let btn_y = 1080.0 - bottom - 22.0 - 56.0;
        return if al.get_primary().is_empty() {
            vec![Rect { x: card_x + 24.0, y: btn_y, w: 140.0, h: 56.0 }]
        } else {
            vec![
                Rect { x: card_x + 24.0, y: btn_y, w: 220.0, h: 56.0 },
                Rect { x: card_x + 256.0, y: btn_y, w: 140.0, h: 56.0 },
            ]
        };
    }
    if matches!(page, Page::QuickSwitchPicker) {
        let n = laptop.global::<crate::QuickSwitch>().get_choices().row_count();
        let mut rects = Vec::with_capacity(1 + n);
        rects.push(Rect { x: 96.0, y: 316.0, w: 1728.0, h: 92.0 });
        for i in 0..n {
            let col = (i % 5) as f32;
            let row = (i / 5) as f32;
            rects.push(Rect {
                x: 96.0 + col * 349.0,
                y: 440.0 + row * 204.0,
                w: 332.0,
                h: 186.0,
            });
        }
        return rects;
    }
    let ui = laptop.global::<Display>().get_dashboard_ui();
    if ui == "blades" {
        return blades_tile_rects(laptop, page);
    }
    if ui == "ps5" {
        return ps5_tile_rects(laptop, page);
    }
    if ui == "orbit" {
        return orbit_tile_rects(laptop, page);
    }
    let tv = laptop.global::<TvUi>();
    let dash = laptop.global::<Dash>();
    match page {
        Page::Home => {
            let hs = laptop.global::<crate::HomeState>();
            let up_n = hs.get_up_next().row_count().min(4);
            let has_game = hs.get_next_game().visible;
            let has_bottom_right = has_game || hs.get_just_added().visible;
            let offscreen = Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 };

            let mut rects = vec![Rect { x: 342.0, y: 136.0, w: 708.0, h: 590.0 }]; // 0: Hero

            // 1..=4: Up next row
            for i in 0..4 {
                if i < up_n {
                    let cx = if i == 0 { 342.0 } else { 668.0 + (i - 1) as f32 * 218.0 };
                    let cw = if i == 0 { 310.0 } else { 202.0 };
                    rects.push(Rect { x: cx, y: 742.0, w: cw, h: 230.0 });
                } else {
                    rects.push(offscreen);
                }
            }

            // 5: Next Game (or Just Added fallback) card
            rects.push(if has_bottom_right {
                Rect { x: 1338.0, y: 682.0, w: 486.0, h: 290.0 }
            } else {
                offscreen
            });

            // 6..=9: Quick tiles (Search, Screensaver, Sleep, Theme)
            rects.push(Rect { x: 1066.0, y: 136.0, w: 240.0, h: 135.0 }); // 6: Search
            rects.push(Rect { x: 1066.0, y: 287.0, w: 240.0, h: 135.0 }); // 7: Screensaver
            rects.push(Rect { x: 1066.0, y: 438.0, w: 240.0, h: 135.0 }); // 8: Sleep
            rects.push(Rect { x: 1066.0, y: 589.0, w: 240.0, h: 137.0 }); // 9: Theme

            // 10..=16: Apps 2x4 grid (7 recent-first)
            let bar_n = hs.get_apps_bar().row_count().min(7);
            for i in 0..7 {
                if i < bar_n {
                    let col = (i % 2) as f32;
                    let row = (i / 2) as f32;
                    rects.push(Rect {
                        x: 1338.0 + col * 251.0,
                        y: 176.0 + row * 126.0,
                        w: 235.0,
                        h: 112.0,
                    });
                } else {
                    rects.push(offscreen);
                }
            }

            // 17: All apps ›
            rects.push(Rect {
                x: 1338.0 + 251.0,
                y: 176.0 + 3.0 * 126.0,
                w: 235.0,
                h: 112.0,
            });
            rects
        }
        Page::Apps => {
            let apps_page = laptop.global::<crate::AppsPage>();
            let n = apps_page.get_apps().row_count();
            let mut rects: Vec<Rect> = (0..n)
                .map(|i| {
                    let col = (i % 4) as f32;
                    let row = (i / 4) as f32;
                    Rect {
                        x: 96.0 + col * 328.0,
                        y: 176.0 + row * 208.0,
                        w: 308.0,
                        h: 184.0,
                    }
                })
                .collect();
            let qs_y = if apps_page.get_has_last_app() { 432.0 } else { 176.0 };
            let slots = laptop
                .global::<crate::QuickSwitch>()
                .get_choices()
                .iter()
                .filter(|c| c.slot > 0)
                .count() as f32;
            let btn_y = qs_y + 28.0 + slots * 56.0 + if slots > 0.0 { 2.0 } else { 0.0 };
            rects.push(Rect { x: 1436.0, y: btn_y, w: 388.0, h: 52.0 });
            if apps_page.get_has_last_app() {
                rects.push(Rect { x: 1436.0, y: 176.0, w: 388.0, h: 224.0 });
            }
            rects
        }
        Page::Collections => {
            let n = laptop.global::<Library>().get_collections().row_count();
            (0..n)
                .map(|i| {
                    let col = (i % 5) as f32;
                    let row = (i / 5) as f32;
                    Rect {
                        x: 96.0 + col * 348.0,
                        y: 188.0 + row * 320.0,
                        w: 332.0,
                        h: 300.0,
                    }
                })
                .collect()
        }
        Page::Sports => {
            let mut rects = vec![
                Rect { x: 96.0, y: 122.0, w: 184.0, h: 44.0 },
                Rect { x: 316.0, y: 122.0, w: 164.0, h: 44.0 },
            ];
            if tv.get_sports_tab() == "scoreboard" {
                let sb = laptop.global::<Scoreboard>();
                let offscreen = Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 };
                rects.push(Rect { x: 96.0, y: 184.0, w: 50.0, h: 50.0 });   // 2: Prev
                rects.push(Rect { x: 478.0, y: 184.0, w: 50.0, h: 50.0 });  // 3: Next
                rects.push(if !sb.get_is_today() {
                    Rect { x: 540.0, y: 184.0, w: 100.0, h: 50.0 }
                } else {
                    offscreen
                }); // 4: Today
                rects.push(Rect { x: 652.0, y: 184.0, w: 300.0, h: 50.0 }); // 5: Dropdown
                rects.push(Rect { x: 964.0, y: 184.0, w: 128.0, h: 50.0 }); // 6: Refresh

                let groups = sb.get_groups();
                let mut y = 252.0;
                for gi in 0..groups.row_count() {
                    if let Some(grp) = groups.row_data(gi) {
                        y += 48.0;
                        let cards = grp.cards;
                        let n = cards.row_count();
                        for ci in 0..n {
                            if let Some(c) = cards.row_data(ci) {
                                let col = (ci % 2) as f32;
                                let row = (ci / 2) as f32;
                                let cx = 96.0 + col * 872.0;
                                let cy = y + row * 170.0;
                                rects.push(Rect { x: cx + 724.0, y: cy + 28.0, w: 112.0, h: 42.0 });
                                if !c.game_url.is_empty() {
                                    rects.push(Rect { x: cx + 724.0, y: cy + 78.0, w: 112.0, h: 42.0 });
                                }
                            }
                        }
                        y += n.div_ceil(2) as f32 * 170.0 + 16.0;
                    }
                }
            }
            rects
        }
        Page::Library => {
            let lib = laptop.global::<Library>();
            // 0..=3: TV Series, Movies, Browse all, Watching now pill
            let mut rects = vec![
                Rect { x: 96.0, y: 138.0, w: 140.0, h: 44.0 },
                Rect { x: 268.0, y: 138.0, w: 112.0, h: 44.0 },
                Rect { x: 416.0, y: 138.0, w: 160.0, h: 44.0 },
                Rect { x: 600.0, y: 138.0, w: 220.0, h: 44.0 },
            ];
            if lib.get_browse_all() {
                for i in 0..28 {
                    rects.push(Rect {
                        x: 96.0 + i as f32 * 62.0,
                        y: 196.0,
                        w: 58.0,
                        h: 44.0,
                    });
                }
                let n = lib.get_browse_items().row_count();
                for i in 0..n {
                    rects.push(Rect {
                        x: 96.0 + (i % 8) as f32 * 218.0,
                        y: 296.0 + (i / 8) as f32 * 316.0,
                        w: 198.0,
                        h: 300.0,
                    });
                }
            } else {
                let lib_scroll = laptop.global::<TvUi>().get_lib_scroll();
                let ll = laptop.global::<LibraryLanding>();
                let s1_n = ll.get_shelf1().row_count().min(4);
                let s2_n = ll.get_shelf2().row_count().min(6);
                let s3_n = ll.get_shelf3().row_count().min(6);
                for i in 0..s1_n {
                    rects.push(Rect {
                        x: 96.0 + i as f32 * 436.0,
                        y: 536.0 - lib_scroll,
                        w: 420.0,
                        h: 236.0,
                    });
                }
                let s2_y = if s1_n == 0 { 536.0 } else { 848.0 };
                for i in 0..s2_n {
                    rects.push(Rect {
                        x: 96.0 + i as f32 * 292.0,
                        y: s2_y - lib_scroll,
                        w: 284.0,
                        h: 225.0,
                    });
                }
                let s3_y = s2_y + if s2_n > 0 { 301.0 } else { 0.0 };
                for i in 0..s3_n {
                    rects.push(Rect {
                        x: 96.0 + i as f32 * 292.0,
                        y: s3_y - lib_scroll,
                        w: 284.0,
                        h: 225.0,
                    });
                }
            }
            rects
        }
        Page::Movie => classic_movie_rects(laptop),
        Page::Show(_) => {
            let show = laptop.global::<ShowPageState>();
            let mut rects = vec![
                Rect { x: 96.0, y: 452.0, w: 440.0, h: 120.0 },
                Rect { x: 552.0, y: 452.0, w: 260.0, h: 120.0 },
            ];
            let season_n = show.get_seasons().row_count();
            let tab_w = if season_n <= 6 {
                170.0
            } else if season_n <= 14 {
                116.0
            } else {
                84.0
            };
            let gap = if season_n <= 6 { 28.0 } else { 12.0 };
            for i in 0..season_n {
                rects.push(Rect {
                    x: 96.0 + i as f32 * (tab_w + gap),
                    y: 616.0,
                    w: tab_w,
                    h: 48.0,
                });
            }
            let ep_n = show.get_episodes().row_count();
            for i in 0..ep_n {
                rects.push(Rect {
                    x: 388.0 + (i % 3) as f32 * 484.0,
                    y: 700.0 + (i / 3) as f32 * 380.0,
                    w: 468.0,
                    h: 344.0,
                });
            }
            if !show.get_next_season_label().is_empty() {
                rects.push(Rect {
                    x: 388.0 + (ep_n % 3) as f32 * 484.0,
                    y: 700.0 + (ep_n / 3) as f32 * 380.0,
                    w: 468.0,
                    h: 263.0,
                });
            }
            if show.get_has_trailer() {
                rects.push(Rect {
                    x: 828.0,
                    y: 452.0,
                    w: 240.0,
                    h: 120.0,
                });
            }
            rects
        }
        Page::Collection(_) => {
            let col = laptop.global::<CollectionPageState>();
            let mut rects = vec![
                Rect { x: 96.0, y: 472.0, w: 520.0, h: 140.0 },
            ];
            let n = col.get_items().row_count();
            for i in 0..n {
                rects.push(Rect {
                    x: 660.0 + (i % 3) as f32 * 388.0,
                    y: 188.0 + (i / 3) as f32 * 252.0,
                    w: 372.0,
                    h: 236.0,
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
        Page::DashboardPicker => {
            let n = laptop.global::<Display>().get_dashboard_choices().row_count();
            (0..n)
                .map(|i| Rect {
                    x: 96.0 + i as f32 * 436.0,
                    y: 330.0,
                    w: 420.0,
                    h: 600.0,
                })
                .collect()
        }
        Page::QuickSwitchPicker => quick_switch_rects(laptop),
        Page::Settings => {
            // 0..=6: SubTabs (Display, Accounts, Sites, Sports, Server, Diagnostics, What's new)
            let mut rects = vec![
                Rect { x: 96.0, y: 122.0, w: 100.0, h: 44.0 },
                Rect { x: 232.0, y: 122.0, w: 128.0, h: 44.0 },
                Rect { x: 396.0, y: 122.0, w: 72.0, h: 44.0 },
                Rect { x: 504.0, y: 122.0, w: 92.0, h: 44.0 },
                Rect { x: 632.0, y: 122.0, w: 92.0, h: 44.0 },
                Rect { x: 760.0, y: 122.0, w: 152.0, h: 44.0 },
                Rect { x: 948.0, y: 122.0, w: 156.0, h: 44.0 },
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
                "sites" => {
                    rects.push(Rect { x: 1604.0, y: 180.0, w: 220.0, h: 52.0 });
                    let n = laptop.global::<Accounts>().get_sites().row_count();
                    for i in 0..n {
                        rects.push(Rect {
                            x: 96.0 + (i % 2) as f32 * 888.0,
                            y: 256.0 + (i / 2) as f32 * 144.0,
                            w: 840.0,
                            h: 128.0,
                        });
                    }
                }
                "server" => {
                    rects.push(Rect { x: 1604.0, y: 180.0, w: 220.0, h: 52.0 });
                }
                "diagnostics" | "changelog" => {}
                _ => {
                    rects.extend([
                        Rect { x: 96.0, y: 374.0, w: 840.0, h: 66.0 },   // 7: Display layout
                        Rect { x: 96.0, y: 296.0, w: 840.0, h: 66.0 },   // 8: Theme
                        Rect { x: 96.0, y: 578.0, w: 840.0, h: 66.0 },   // 9: Temperature
                        Rect { x: 96.0, y: 656.0, w: 840.0, h: 66.0 },   // 10: Clock
                        Rect { x: 984.0, y: 736.0, w: 412.0, h: 76.0 },  // 11: Recent services
                        Rect { x: 1412.0, y: 736.0, w: 412.0, h: 76.0 }, // 12: Application (Quit)
                        Rect { x: 96.0, y: 218.0, w: 840.0, h: 66.0 },   // 13: Dashboard UI
                        Rect { x: 96.0, y: 734.0, w: 840.0, h: 66.0 },   // 14: Sleep warning
                    ]);
                    let places_n = laptop.global::<Display>().get_places().row_count().min(4);
                    for i in 0..places_n {
                        rects.push(Rect {
                            x: 984.0,
                            y: 434.0 + i as f32 * 64.0,
                            w: 840.0,
                            h: 54.0,
                        });
                    }
                    for _ in places_n..4 {
                        rects.push(Rect { x: -1000.0, y: -1000.0, w: 0.0, h: 0.0 });
                    }
                    rects.push(Rect { x: 96.0, y: 452.0, w: 644.0, h: 66.0 });  // 19: Startup animation
                    rects.push(Rect { x: 752.0, y: 452.0, w: 184.0, h: 66.0 }); // 20: Preview startup animation
                    rects.push(Rect { x: 96.0, y: 812.0, w: 840.0, h: 66.0 });  // 21: Quick switch
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
    if tv.get_screen() == "library" && !laptop.global::<Library>().get_browse_all() && idx >= 4 {
        laptop.global::<LibraryLanding>().invoke_focus_changed(idx as i32);
    }
}

fn scroll_library_to_focus(laptop: &LaptopWindow, idx: usize) {
    if laptop.global::<Display>().get_dashboard_ui() == "orbit" {
        return;
    }
    let tv = laptop.global::<TvUi>();
    if idx < 4 {
        STATE.with_borrow_mut(|s| s.lib_scroll = 0.0);
        tv.set_lib_scroll(0.0);
        return;
    }
    let rects = tile_rects(laptop, &Page::Library);
    STATE.with_borrow_mut(|s| {
        let Some(&r) = rects.get(idx) else { return };
        let abs_y = r.y + s.lib_scroll;
        let vert = Rect { x: abs_y - 486.0, y: 0.0, w: r.h, h: r.w };
        s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 594.0, 16.0);
        tv.set_lib_scroll(s.lib_scroll);
    });
}

fn scroll_show_to_focus(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    let rects = tile_rects(laptop, &Page::Show(String::new()));
    STATE.with_borrow_mut(|s| {
        let Some(&r) = rects.get(s.shell.focused_tile) else { return };
        let show = laptop.global::<ShowPageState>();
        let season_n = show.get_seasons().row_count();
        let grid_base = 2 + season_n;
        if s.shell.focused_tile >= grid_base {
            show.set_scrolled(true);
            let vert = Rect { x: r.y - 244.0, y: 0.0, w: r.h, h: r.w };
            s.show_scroll = focus::panorama_scroll(vert, s.show_scroll.max(456.0), 804.0, 16.0);
        } else {
            show.set_scrolled(false);
            s.show_scroll = 0.0;
        }
        tv.set_show_scroll(s.show_scroll);
    });
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
                let lib = laptop.global::<Library>();
                let max_scroll = if lib.get_browse_all() {
                    4000.0
                } else {
                let ll = laptop.global::<LibraryLanding>();
                    let s1_n = ll.get_shelf1().row_count().min(4);
                    let s2_n = ll.get_shelf2().row_count().min(6);
                    let s3_n = ll.get_shelf3().row_count().min(6);
                    let mut h: f32 = if s1_n > 0 { 322.0 } else { 10.0 };
                    if s2_n > 0 {
                        h += 301.0;
                    }
                    if s3_n > 0 {
                        h += 300.0;
                    }
                    (h - 594.0).max(0.0)
                };
                s.lib_scroll = (s.lib_scroll + step).clamp(0.0, max_scroll);
                tv.set_lib_scroll(s.lib_scroll);
            }
            Page::Apps | Page::Collections => {
                s.lib_scroll = (s.lib_scroll + step).clamp(0.0, 4000.0);
                tv.set_lib_scroll(s.lib_scroll);
            }
            Page::Movie => {
                let rects = tile_rects(laptop, &Page::Movie);
                let max_scroll = rects.last().map(|l| (l.y + l.h - 1020.0).max(0.0)).unwrap_or(0.0);
                s.show_scroll = (s.show_scroll + step).clamp(0.0, max_scroll);
                tv.set_show_scroll(s.show_scroll);
            }
            Page::Show(_) => {
                let show = laptop.global::<ShowPageState>();
                let ep_n = show.get_episodes().row_count() + usize::from(!show.get_next_season_label().is_empty());
                let rows = ep_n.div_ceil(3);
                let bottom = 700.0 + rows.saturating_sub(1) as f32 * 380.0 + 344.0;
                let max_scroll = (bottom - 1056.0).max(0.0);
                s.show_scroll = (s.show_scroll + step).clamp(0.0, max_scroll);
                tv.set_show_scroll(s.show_scroll);
                show.set_scrolled(s.show_scroll > 40.0);
            }
            Page::Collection(_) => {
                s.col_scroll = (s.col_scroll + step).clamp(0.0, 4000.0);
                tv.set_col_scroll(s.col_scroll);
            }
            Page::Settings if tv.get_settings_tab() == "sites" => {
                let rows = laptop.global::<Accounts>().get_sites().row_count().div_ceil(2);
                let max_scroll = (rows as f32 * 144.0 + 32.0 - 836.0).max(0.0);
                s.lib_scroll = (s.lib_scroll + step).clamp(0.0, max_scroll);
                tv.set_lib_scroll(s.lib_scroll);
            }
            _ => {}
        }
    });
}

fn show_nav_override(laptop: &LaptopWindow, from: usize, dir: Dir) -> Option<usize> {
    let show = laptop.global::<ShowPageState>();
    let seasons = show.get_seasons();
    let season_n = seasons.row_count();
    let ep_n = show.get_episodes().row_count();
    let grid_base = 2 + season_n;
    let sel_key = show.get_selected_season();
    let sel_idx = (0..season_n)
        .find(|&i| seasons.row_data(i).is_some_and(|t| t.rating_key == sel_key))
        .unwrap_or(0);

    if from < 2 && dir == Dir::Down && season_n > 0 {
        return Some(2 + sel_idx);
    }
    if (2..grid_base).contains(&from) {
        match dir {
            Dir::Left if from > 2 => {
                let next = from - 1;
                if let Some(tab) = seasons.row_data(next - 2) {
                    show.invoke_select_season(tab.rating_key);
                }
                return Some(next);
            }
            Dir::Right if from + 1 < grid_base => {
                let next = from + 1;
                if let Some(tab) = seasons.row_data(next - 2) {
                    show.invoke_select_season(tab.rating_key);
                }
                return Some(next);
            }
            Dir::Down if ep_n > 0 => {
                let up_key = show.get_up_next_key();
                let eps = show.get_episodes();
                let ep_idx = (0..ep_n)
                    .find(|&i| eps.row_data(i).is_some_and(|e| e.rating_key == up_key))
                    .unwrap_or(0);
                return Some(grid_base + ep_idx);
            }
            _ => {}
        }
    }
    if from >= grid_base && (from - grid_base) < 3 && dir == Dir::Up && season_n > 0 {
        return Some(3 + sel_idx);
    }
    None
}

fn orbit_rail_focus_for_shelf(ll: &LibraryLanding, shelf_idx: usize) -> Option<usize> {
    let mut pos = 0;
    for (i, r) in ll.get_rail().iter().enumerate() {
        if r.enabled {
            if i == shelf_idx {
                return Some(4 + pos);
            }
            pos += 1;
        }
    }
    None
}

fn orbit_rail_shelf_for_focus(ll: &LibraryLanding, focus_idx: usize) -> Option<usize> {
    if focus_idx < 4 {
        return None;
    }
    let target = focus_idx - 4;
    let mut pos = 0;
    for (i, r) in ll.get_rail().iter().enumerate() {
        if r.enabled {
            if pos == target {
                return Some(i);
            }
            pos += 1;
        }
    }
    None
}

fn orbit_library_nav(laptop: &LaptopWindow, from: usize, dir: Dir) -> Option<usize> {
    let ll = laptop.global::<LibraryLanding>();
    let rail_n = ll.get_rail().iter().filter(|r| r.enabled).count();
    let grid_n = ll.get_grid().row_count().min(6);
    let grid_base = 4 + rail_n;
    let card_idx = grid_base + grid_n;
    let selected_rail = ll.get_rail_idx().max(0) as usize;
    let selected_rail_focus = orbit_rail_focus_for_shelf(&ll, selected_rail).unwrap_or(4);

    if from < 4 {
        let max_sub = if ll.get_watching_now_count() > 0 { 3 } else { 2 };
        return match dir {
            Dir::Left if from > 0 => Some(from - 1),
            Dir::Right if from < max_sub => Some(from + 1),
            Dir::Down if rail_n > 0 => Some(selected_rail_focus),
            _ => None,
        };
    }

    if from < grid_base {
        return match dir {
            Dir::Up => {
                if from > 4 {
                    if let Some(prev_shelf) = orbit_rail_shelf_for_focus(&ll, from - 1) {
                        ll.invoke_rail_selected(prev_shelf as i32);
                    }
                    Some(from - 1)
                } else {
                    Some(0)
                }
            }
            Dir::Down if from + 1 < grid_base => {
                if let Some(next_shelf) = orbit_rail_shelf_for_focus(&ll, from + 1) {
                    ll.invoke_rail_selected(next_shelf as i32);
                }
                Some(from + 1)
            }
            Dir::Right if grid_n > 0 => {
                let cell = crate::bridge::library::orbit_remembered_cell(laptop).min(grid_n - 1);
                Some(grid_base + cell)
            }
            _ => None,
        };
    }

    if from < card_idx {
        let cell = from - grid_base;
        let col = cell % 3;
        let row = cell / 3;
        return match dir {
            Dir::Left => {
                if col == 0 {
                    Some(selected_rail_focus)
                } else {
                    Some(from - 1)
                }
            }
            Dir::Right => {
                if col == 2 || cell + 1 == grid_n {
                    Some(card_idx)
                } else {
                    Some(from + 1)
                }
            }
            Dir::Up => {
                if row == 0 {
                    Some(0)
                } else {
                    Some(from - 3)
                }
            }
            Dir::Down if cell + 3 < grid_n => Some(from + 3),
            _ => None,
        };
    }

    if from == card_idx && grid_n > 0 {
        return match dir {
            Dir::Left => {
                let cell = crate::bridge::library::orbit_remembered_cell(laptop).min(grid_n - 1);
                Some(grid_base + cell)
            }
            Dir::Up => Some(0),
            _ => None,
        };
    }

    None
}

fn go_back(laptop: &LaptopWindow) -> bool {
    let is_orbit_lib = STATE.with_borrow(|s| {
        matches!(s.shell.current(), Page::Library)
            && !s.shell.bar_focused
            && laptop.global::<Display>().get_dashboard_ui() == "orbit"
            && !laptop.global::<Library>().get_browse_all()
    });
    if is_orbit_lib {
        let ll = laptop.global::<LibraryLanding>();
        let rail_n = ll.get_rail().iter().filter(|r| r.enabled).count();
        let grid_base = 4 + rail_n;
        let cur = STATE.with_borrow(|s| s.shell.focused_tile);
        if cur >= grid_base && rail_n > 0 {
            let rail_focus = orbit_rail_focus_for_shelf(&ll, ll.get_rail_idx().max(0) as usize).unwrap_or(4);
            STATE.with_borrow_mut(|s| s.shell.focused_tile = rail_focus);
            sync(laptop);
            return true;
        }
        if (4..grid_base).contains(&cur) {
            STATE.with_borrow_mut(|s| s.shell.focused_tile = 0);
            sync(laptop);
            return true;
        }
    }
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
    } else if let Some(item) = crate::bridge::selected()
        && item.kind == tvdash_core::models::MediaKind::Movie
        && item.source == tvdash_core::models::SourceKind::Plex
        && laptop.global::<Display>().get_dashboard_ui() != "ps5"
    {
        lib.invoke_close_detail();
        lib.invoke_open_movie(item.id.into());
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
        Page::Home => {
            let hs = laptop.global::<crate::HomeState>();
            match idx {
                0 => hs.invoke_activate_hero(),
                1..=4 => {
                    if let Some(card) = hs.get_up_next().row_data(idx - 1) {
                        hs.invoke_activate_up_next(card.key);
                    }
                }
                5 => {
                    let ng = hs.get_next_game();
                    if ng.visible {
                        hs.invoke_open_game(ng.event_id, ng.date);
                    } else if hs.get_just_added().visible {
                        hs.invoke_activate_just_added();
                    }
                }
                6 => {
                    STATE.with_borrow_mut(|s| s.shell.push(Page::Search));
                    sync(laptop);
                }
                7 => {
                    crate::bridge::start_ambient(laptop);
                    tv.set_ambient(true);
                }
                8 => laptop.global::<SleepState>().invoke_open_picker(),
                9 => {
                    let next = match laptop.global::<crate::Theme>().get_mode() {
                        UiTheme::Dark => UiTheme::Aubergine,
                        UiTheme::Aubergine => UiTheme::Ku,
                        UiTheme::Ku => UiTheme::Stone,
                        UiTheme::Stone => UiTheme::Mist,
                        UiTheme::Mist => UiTheme::Dark,
                    };
                    dash.invoke_set_theme(next);
                }
                10..=16 => {
                    if let Some(app) = hs.get_apps_bar().row_data(idx - 10) {
                        hs.invoke_launch_app(app.id);
                    }
                }
                17 => hs.invoke_open_apps_page(),
                _ => {}
            }
        }
        Page::Apps => {
            let apps_page = laptop.global::<crate::AppsPage>();
            let apps = apps_page.get_apps();
            if idx == apps.row_count() {
                laptop.global::<crate::QuickSwitch>().invoke_open_picker();
            } else if apps_page.get_has_last_app() && idx == apps.row_count() + 1 {
                laptop.global::<crate::HomeState>().invoke_launch_app(apps_page.get_last_app_id());
            } else if let Some(app) = apps.row_data(idx) {
                laptop.global::<crate::HomeState>().invoke_launch_app(app.id);
            }
        }
        Page::Collections => {
            let lib = laptop.global::<Library>();
            if let Some(col) = lib.get_collections().row_data(idx) {
                lib.invoke_open_collection(col.rating_key);
                STATE.with_borrow_mut(|s| s.shell.push(Page::Collection(String::new())));
                sync(laptop);
            }
        }
        Page::Library => {
            let lib = laptop.global::<Library>();
                let ll = laptop.global::<LibraryLanding>();
            if idx < 4 {
                STATE.with_borrow_mut(|s| s.lib_scroll = 0.0);
                tv.set_lib_scroll(0.0);
                let is_orbit = laptop.global::<Display>().get_dashboard_ui() == "orbit";
                match idx {
                    0 => {
                        lib.invoke_set_browse_all(false);
                        lib.invoke_set_media_tab(if is_orbit { "movies" } else { "tv" }.into());
                        STATE.with_borrow_mut(|s| s.shell.focused_tile = 4);
                        sync(laptop);
                    }
                    1 => {
                        lib.invoke_set_browse_all(false);
                        lib.invoke_set_media_tab(if is_orbit { "tv" } else { "movies" }.into());
                        STATE.with_borrow_mut(|s| s.shell.focused_tile = 4);
                        sync(laptop);
                    }
                    2 => lib.invoke_set_browse_all(!lib.get_browse_all()),
                    _ => ll.invoke_open_watching_now(),
                }
                return;
            }
            let is_tv = lib.get_media_tab() == "tv";
            if lib.get_browse_all() {
                if laptop.global::<Display>().get_dashboard_ui() == "orbit" {
                    let br = laptop.global::<crate::Browse>();
                    if idx < 32 {
                        br.invoke_set_letter((idx - 4) as i32);
                    } else if let Some(item) = br.get_items().row_data(idx - 32) {
                        br.invoke_activate(item.key);
                    }
                    return;
                }
                if idx < 32 {
                    let li = idx - 4;
                    let letter = match li {
                        0 => String::new(),
                        27 => "#".to_string(),
                        n => ((b'A' + (n as u8 - 1)) as char).to_string(),
                    };
                    STATE.with_borrow_mut(|s| s.lib_scroll = 0.0);
                    tv.set_lib_scroll(0.0);
                    lib.invoke_set_browse_letter(letter.into());
                } else {
                    activate_library_selection(laptop, &lib, "browse", idx - 32);
                }
                return;
            }

            if laptop.global::<Display>().get_dashboard_ui() == "orbit" {
                let rail_n = ll.get_rail().iter().filter(|r| r.enabled).count();
                let grid_n = ll.get_grid().row_count().min(6);
                let grid_base = 4 + rail_n;
                let card_idx = grid_base + grid_n;
                if (4..grid_base).contains(&idx) {
                    if let Some(shelf_idx) = orbit_rail_shelf_for_focus(&ll, idx) {
                        ll.invoke_rail_selected(shelf_idx as i32);
                    }
                } else if (grid_base..card_idx).contains(&idx) {
                    ll.invoke_grid_activated((idx - grid_base) as i32);
                } else if idx == card_idx && grid_n > 0 {
                    let key = ll.get_focus().rating_key;
                    if !key.is_empty() {
                        lib.invoke_play_key(key, !is_tv);
                    }
                }
                return;
            }

            let item_idx = idx - 4;
            let s1_n = ll.get_shelf1().row_count().min(4);
            let s2_n = ll.get_shelf2().row_count().min(6);
            if item_idx < s1_n {
                if let Some(row) = ll.get_shelf1().row_data(item_idx) {
                    if is_tv {
                        lib.invoke_play_key(row.rating_key, true);
                    } else {
                        lib.invoke_open_movie(row.rating_key);
                    }
                }
            } else if item_idx < s1_n + s2_n {
                let s2_idx = item_idx - s1_n;
                if let Some(row) = ll.get_shelf2().row_data(s2_idx) {
                    if is_tv {
                        lib.invoke_open_show(row.rating_key);
                        STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
                        sync(laptop);
                    } else {
                        lib.invoke_open_movie(row.rating_key);
                    }
                }
            } else {
                let s3_idx = item_idx - s1_n - s2_n;
                if let Some(row) = ll.get_shelf3().row_data(s3_idx) {
                    if is_tv {
                        lib.invoke_open_show(row.rating_key);
                        STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
                        sync(laptop);
                    } else {
                        lib.invoke_open_movie(row.rating_key);
                    }
                }
            }
        }
        Page::Movie => movie_activate_tile(laptop, idx),
        Page::Show(_) => {
            let show = laptop.global::<ShowPageState>();
            let season_n = show.get_seasons().row_count();
            let ep_n = show.get_episodes().row_count();
            let grid_base = 2 + season_n;
            match idx {
                0 => {
                    let key = show.get_up_next_key();
                    if !key.is_empty() {
                        show.invoke_play_episode(key, true);
                    }
                }
                1 => show.invoke_play_from_start(),
                i if (2..grid_base).contains(&i) => {
                    if let Some(tab) = show.get_seasons().row_data(i - 2) {
                        show.invoke_select_season(tab.rating_key);
                    }
                }
                i if i >= grid_base && i < grid_base + ep_n => {
                    if let Some(ep) = show.get_episodes().row_data(i - grid_base) {
                        show.invoke_play_episode(ep.rating_key, true);
                    }
                }
                i if i == grid_base + ep_n && !show.get_next_season_key().is_empty() => {
                    let next_key = show.get_next_season_key();
                    show.invoke_select_season(next_key);
                    STATE.with_borrow_mut(|s| s.shell.focused_tile = grid_base);
                    sync(laptop);
                }
                i if show.get_has_trailer()
                    && i == grid_base + ep_n + usize::from(!show.get_next_season_label().is_empty()) =>
                {
                    show.invoke_play_trailer();
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
                i if i >= 1 => {
                    if let Some(item) = col.get_items().row_data(i - 1) {
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
        Page::DashboardPicker => {
            let disp = laptop.global::<Display>();
            if let Some(ch) = disp.get_dashboard_choices().row_data(idx) {
                disp.invoke_choose_dashboard(ch.id);
            }
        }
        Page::QuickSwitchPicker => {
            let qs = laptop.global::<crate::QuickSwitch>();
            if idx == 0 {
                qs.invoke_set_automatic(!qs.get_automatic());
            } else if !qs.get_automatic() {
                if let Some(ch) = qs.get_choices().row_data(idx - 1) {
                    qs.invoke_toggle(ch.id);
                }
            }
        }
        Page::Settings => {
            if idx < 7 {
                let next_tab = match idx {
                    1 => "accounts",
                    2 => "sites",
                    3 => "sports",
                    4 => "server",
                    5 => "diagnostics",
                    6 => "changelog",
                    _ => "display",
                };
                tv.set_confirm_quit(false);
                tv.set_settings_tab(next_tab.into());
                if next_tab != "accounts" {
                    laptop.global::<Accounts>().set_editing("".into());
                }
                if next_tab == "sports" {
                    laptop.global::<SportsAlertsSettings>().invoke_refresh_follows();
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
                    if let Some(a) = acc.get_accounts().row_data(idx - 7) {
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
                "sites" => {
                    let ed = laptop.global::<SiteEditor>();
                    if idx == 7 {
                        ed.invoke_add();
                    } else if let Some(s) = laptop.global::<Accounts>().get_sites().row_data(idx - 8) {
                        ed.invoke_edit(s.index);
                    }
                }
                "sports" => {
                    let sp = laptop.global::<SportsAlertsSettings>();
                    match idx {
                        7 => sp.invoke_set_soon_minutes(0),
                        8 => sp.invoke_set_soon_minutes(5),
                        9 => sp.invoke_set_soon_minutes(10),
                        10 => sp.invoke_set_soon_minutes(15),
                        11 => sp.invoke_set_soon_minutes(30),
                        12 => sp.invoke_set_final_on(true),
                        13 => sp.invoke_set_final_on(false),
                        14 => sp.invoke_set_during_video("show".into()),
                        15 => sp.invoke_set_during_video("hold".into()),
                        other if other >= 16 => {
                            if let Some(g) = sp.get_starred_games().row_data(other - 16) {
                                sp.invoke_unstar_game(g.event_id);
                            }
                        }
                        _ => {}
                    }
                }
                "server" => {
                    if idx == 7 {
                        laptop.global::<Server>().invoke_scan_all();
                    }
                }
                "diagnostics" | "changelog" => {}
                _ => {
                    let disp = laptop.global::<Display>();
                    let item = idx - 7;
                    if item != 5 {
                        tv.set_confirm_quit(false);
                    }
                    match item {
                        0 => {
                            disp.set_layout_setting("tv".into());
                            disp.invoke_set_layout_setting("tv".into());
                        }
                        1 => {
                            let next = match laptop.global::<crate::Theme>().get_mode() {
                                UiTheme::Dark => UiTheme::Aubergine,
                                UiTheme::Aubergine => UiTheme::Ku,
                                UiTheme::Ku => UiTheme::Stone,
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
                        6 => {
                            let cur = tvdash_core::config::DashboardUi::parse(&disp.get_dashboard_ui());
                            let cur_idx = crate::bridge::display::enabled_uis()
                                .iter()
                                .position(|&u| u == cur)
                                .unwrap_or(0);
                            STATE.with_borrow_mut(|s| {
                                s.shell.push(Page::DashboardPicker);
                                s.shell.focused_tile = cur_idx;
                            });
                            sync(laptop);
                        }
                        7 => {
                            let sl = laptop.global::<SleepState>();
                            let next = match sl.get_warn_before().as_str() {
                                "30s" => "60s",
                                "60s" => "2m",
                                _ => "30s",
                            };
                            sl.invoke_set_warn_before(next.into());
                        }
                        8..=11 => disp.invoke_pick_place((item - 8) as i32),
                        12 => {
                            let intro = laptop.global::<crate::Intro>();
                            intro.invoke_set_enabled(!intro.get_enabled());
                        }
                        13 => laptop.global::<crate::Intro>().invoke_preview(),
                        14 => laptop.global::<crate::QuickSwitch>().invoke_open_picker(),
                        _ => {}
                    }
                }
            }
        }
        Page::Sports => {
            if idx < 2 {
                let next_tab = if idx == 1 { "scoreboard" } else { "competitions" };
                tv.set_sports_tab(next_tab.into());
                if next_tab == "scoreboard" {
                    let sb = laptop.global::<Scoreboard>();
                    if sb.get_rows().row_count() == 0 && !sb.get_loading() && sb.get_error().is_empty() {
                        sb.invoke_load(sb.get_day(), sb.get_show(), sb.get_final_only());
                    }
                }
            } else if tv.get_sports_tab() == "scoreboard" {
                let sb = laptop.global::<Scoreboard>();
                match idx {
                    2 => sb.invoke_prev_day_clicked(),
                    3 => sb.invoke_next_day_clicked(),
                    4 => sb.invoke_today_clicked(),
                    5 => {
                        let choices = sb.get_choices();
                        let cur = sb.get_show();
                        let pos = (0..choices.row_count())
                            .find(|&i| choices.row_data(i).is_some_and(|c| c.value == cur))
                            .unwrap_or(0);
                        let next_idx = if choices.row_count() > 0 {
                            (pos + 1) % choices.row_count()
                        } else {
                            0
                        };
                        if let Some(ch) = choices.row_data(next_idx) {
                            sb.invoke_set_filter(ch.value);
                        }
                    }
                    6 => sb.invoke_refresh(),
                    other if other >= 7 => {
                        let groups = sb.get_groups();
                        'outer: for gi in 0..groups.row_count() {
                            if let Some(grp) = groups.row_data(gi) {
                                let cards = grp.cards;
                                for ci in 0..cards.row_count() {
                                    if let Some(c) = cards.row_data(ci) {
                                        if c.star_focus_id == other as i32 {
                                            if !c.following && !c.event_id.is_empty() {
                                                sb.invoke_toggle_star(c.event_id);
                                            }
                                            break 'outer;
                                        } else if c.game_focus_id == other as i32 && !c.game_url.is_empty() {
                                            sb.invoke_open_url(
                                                format!("{} vs {}", c.away.full, c.home.full).into(),
                                                c.game_url,
                                            );
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
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

fn minimize_to_home(laptop: &LaptopWindow) {
    let tv = laptop.global::<TvUi>();
    let dash = laptop.global::<Dash>();
    tv.set_hud_open(false);
    tv.set_video_full(false);
    if !dash.get_tv().paused {
        dash.invoke_transport("play-pause".into());
    }
}

pub fn key(laptop: &LaptopWindow, text: &str) -> bool {
    if crate::intro::is_blocking_input(laptop) {
        crate::intro::skip(laptop);
        return true;
    }
    if laptop.global::<SiteEditor>().get_open() || laptop.global::<Artwork>().get_open() {
        return false;
    }
    let tv = laptop.global::<TvUi>();
    STATE.with_borrow_mut(|s| s.last_key = Instant::now());

    let wa = laptop.global::<crate::WeatherAlertState>();
    if wa.get_visible() {
        if is_back(text) {
            wa.invoke_got_it();
            return true;
        }
        match decode_dir(text) {
            Some(Dir::Left) => {
                wa.set_focus(0);
                return true;
            }
            Some(Dir::Right) if wa.get_can_resume() => {
                wa.set_focus(1);
                return true;
            }
            _ => {}
        }
        if is_ok(text) {
            if wa.get_focus() == 1 && wa.get_can_resume() {
                wa.invoke_resume();
            } else {
                wa.invoke_got_it();
            }
            return true;
        }
        return true;
    }

    let wc = laptop.global::<crate::WeatherCardState>();
    if wc.get_visible() {
        if is_back(text) {
            wc.invoke_dismiss();
            return true;
        }
        match decode_dir(text) {
            Some(Dir::Left) => {
                wc.set_focus(0);
                return true;
            }
            Some(Dir::Right) => {
                wc.set_focus(1);
                return true;
            }
            _ => {}
        }
        if is_ok(text) {
            if wc.get_focus() == 1 {
                wc.invoke_details();
            } else {
                wc.invoke_dismiss();
            }
            return true;
        }
        return true;
    }

    let alert_open = laptop.global::<SportsAlertState>().get_visible();
    if tv.get_ambient() && !alert_open {
        tv.set_ambient(false);
        laptop.global::<Dash>().set_screensaver_open(false);
        return true;
    }

    let dash = laptop.global::<Dash>();
    if dash.get_next_ep_open() {
        if is_back(text) {
            dash.invoke_dismiss_next_ep();
            sync(laptop);
            return true;
        }
        match decode_dir(text) {
            Some(Dir::Left) => {
                tv.set_focused_tile(0);
                return true;
            }
            Some(Dir::Right) => {
                tv.set_focused_tile(1);
                return true;
            }
            _ => {}
        }
        if is_ok(text) {
            if tv.get_focused_tile() == 1 {
                dash.invoke_dismiss_next_ep();
            } else {
                dash.invoke_play_next_ep();
            }
            sync(laptop);
            return true;
        }
        return true;
    }

    let sl = laptop.global::<SleepState>();
    if sl.get_picker_open() {
        if is_back(text) {
            sl.invoke_close_picker();
            return true;
        }
        let end_ok = sl.get_end_of_item_ok();
        let cur = sl.get_focused_tile().clamp(0, 8);
        match decode_dir(text) {
            Some(Dir::Left) => {
                let next = match cur {
                    0 | 4 | 8 => cur,
                    7 if !end_ok => 5,
                    _ => cur - 1,
                };
                sl.set_focused_tile(next);
                return true;
            }
            Some(Dir::Right) => {
                let next = match cur {
                    3 | 7 | 8 => cur,
                    5 if !end_ok => 7,
                    _ => cur + 1,
                };
                sl.set_focused_tile(next);
                return true;
            }
            Some(Dir::Up) => {
                let next = match cur {
                    0..=3 => 8,
                    4..=7 => cur - 4,
                    _ => 8,
                };
                sl.set_focused_tile(next);
                return true;
            }
            Some(Dir::Down) => {
                let next = match cur {
                    8 => 3,
                    0..=3 => {
                        let c = cur + 4;
                        if c == 6 && !end_ok { 7 } else { c }
                    }
                    _ => cur,
                };
                sl.set_focused_tile(next);
                return true;
            }
            None => {}
        }
        if is_ok(text) {
            match cur {
                0 => sl.invoke_set_minutes(15),
                1 => sl.invoke_set_minutes(30),
                2 => sl.invoke_set_minutes(45),
                3 => sl.invoke_set_minutes(60),
                4 => sl.invoke_set_minutes(90),
                5 => sl.invoke_set_minutes(120),
                6 => sl.invoke_set_end_of_item(),
                7 => {
                    sl.invoke_off();
                    sl.invoke_close_picker();
                }
                _ => sl.invoke_close_picker(),
            }
        }
        return true;
    }

    if sl.get_phase() == "warn" && !sl.get_held() {
        let ep_fin = sl.get_episode_finished();
        if is_back(text) {
            if ep_fin {
                sl.invoke_play_next();
            } else {
                sl.invoke_extend();
            }
            return true;
        }
        let cur = sl.get_warn_focus().clamp(0, 2);
        match decode_dir(text) {
            Some(Dir::Left) => {
                sl.set_warn_focus((cur - 1).max(0));
                return true;
            }
            Some(Dir::Right) => {
                sl.set_warn_focus((cur + 1).min(2));
                return true;
            }
            _ => {}
        }
        if is_ok(text) {
            match cur {
                0 => {
                    if ep_fin {
                        sl.invoke_play_next();
                    } else {
                        sl.invoke_extend();
                    }
                }
                1 => sl.invoke_sleep_now(),
                2 => sl.invoke_off(),
                _ => {}
            }
        }
        return true;
    }

    let al = laptop.global::<SportsAlertState>();
    if al.get_visible() {
        if is_back(text) {
            al.invoke_dismiss_clicked();
            return true;
        }
        let has_primary = !al.get_primary().is_empty();
        match decode_dir(text) {
            Some(Dir::Left) => {
                if has_primary {
                    al.set_focus(0);
                }
                crate::bridge::alerts::reset_countdown(laptop);
                return true;
            }
            Some(Dir::Right) => {
                if has_primary {
                    al.set_focus(1);
                }
                crate::bridge::alerts::reset_countdown(laptop);
                return true;
            }
            Some(Dir::Up) | Some(Dir::Down) => {
                crate::bridge::alerts::reset_countdown(laptop);
                return true;
            }
            None => {}
        }
        if is_ok(text) {
            if has_primary && al.get_focus() == 0 {
                al.invoke_primary_clicked();
            } else {
                al.invoke_dismiss_clicked();
            }
        }
        return true;
    }

    let ll = laptop.global::<LibraryLanding>();
    if ll.get_watching_now_open() {
        if is_back(text) {
            ll.invoke_close_watching_now();
            return true;
        }
        let n = ll.get_watching_now_rows().row_count() as i32;
        let cur = tv.get_focused_tile();
        match decode_dir(text) {
            Some(Dir::Up) if n > 0 => {
                tv.set_focused_tile((cur - 1).max(0));
                return true;
            }
            Some(Dir::Down) if n > 0 => {
                tv.set_focused_tile((cur + 1).min(n - 1));
                return true;
            }
            _ => {}
        }
        if is_ok(text) && n > 0 && cur >= 0 && cur < n {
            if let Some(row) = ll.get_watching_now_rows().row_data(cur as usize) {
                let is_tv = row.is_tv;
                ll.invoke_select_watching_now(cur);
                if is_tv {
                    STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
                    sync(laptop);
                }
            }
            return true;
        }
        return true;
    }

    if tv.get_video_on() && tv.get_video_full() {
        if is_back(text) {
            if tv.get_hud_open() {
                tv.set_hud_open(false);
            } else {
                minimize_to_home(laptop);
            }
            return true;
        }
        if tv.get_hud_open() {
            let cur = tv.get_focused_tile().clamp(0, 9);
            match decode_dir(text) {
                Some(Dir::Left) => {
                    tv.set_focused_tile((cur - 1).max(0));
                    return true;
                }
                Some(Dir::Right) => {
                    tv.set_focused_tile((cur + 1).min(9));
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
                    4 => minimize_to_home(laptop),
                    5 => dash.invoke_transport("sub".into()),
                    6 => dash.invoke_transport("audio".into()),
                    7 => laptop.global::<SleepState>().invoke_open_picker(),
                    8 => dash.invoke_transport("next-ep".into()),
                    9 => dash.invoke_transport("restart".into()),
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
        if matches!(page, Page::Library)
            && laptop.global::<Display>().get_dashboard_ui() == "orbit"
            && !laptop.global::<Library>().get_browse_all()
        {
            if let Some(next_idx) = orbit_library_nav(laptop, from, dir) {
                STATE.with_borrow_mut(|s| s.shell.focused_tile = next_idx);
                sync(laptop);
                return true;
            }
            if from < 4 && dir == Dir::Up {
                STATE.with_borrow_mut(|s| s.shell.bar_focused = true);
                sync(laptop);
                return true;
            }
            return true;
        }
        if matches!(page, Page::Show(_))
            && let Some(next_idx) = show_nav_override(laptop, from, dir)
            && rects.get(next_idx).is_some_and(|r| r.w > 0.0)
        {
            STATE.with_borrow_mut(|s| s.shell.focused_tile = next_idx);
            scroll_show_to_focus(laptop);
            sync(laptop);
            return true;
        }
        if let Some(next_idx) = focus::next(&rects, from, dir) {
            if rects[next_idx].w > 0.0 {
                STATE.with_borrow_mut(|s| s.shell.focused_tile = next_idx);
                if matches!(page, Page::Show(_)) {
                    scroll_show_to_focus(laptop);
                } else if matches!(page, Page::Library) && !laptop.global::<Library>().get_browse_all() {
                    scroll_library_to_focus(laptop, next_idx);
                }
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
                dashboard_ui: tvdash_core::config::DashboardUi::Classic,
                tv_connected: true,
                sites: ["YouTube TV", "YouTube", "Disney+", "Hulu", "Netflix"]
                    .into_iter()
                    .map(site)
                    .collect(),
                browse_sort_tv: "title_asc".into(),
                browse_sort_movies: "title_asc".into(),
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
        assert_eq!(tv.get_focused_tile(), 0); // Hero

        // Up from Hero focuses TabBar; Right switches to Apps, then Library; Back returns to Home
        assert!(key(&l, &SharedString::from(Key::UpArrow)));
        assert!(tv.get_bar_focused());
        assert!(key(&l, &SharedString::from(Key::RightArrow)));
        assert_eq!(tv.get_screen(), "apps");
        assert!(key(&l, &SharedString::from(Key::RightArrow)));
        assert_eq!(tv.get_screen(), "library");
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Command::LoadLibraryShelves { is_tv: false, .. }
        ));
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert_eq!(tv.get_screen(), "home");

        // Right from Hero lands on Sleep (tile 8), Sleep (tile 8) opens picker, app tile (10) opens it
        assert!(key(&l, &SharedString::from(Key::RightArrow)));
        assert_eq!(tv.get_focused_tile(), 8);
        activate_tile(&l, 8);
        assert!(l.global::<SleepState>().get_picker_open());
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert!(!l.global::<SleepState>().get_picker_open());
        activate_tile(&l, 10);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Command::OpenSite(_)));
    }

    #[test]
    fn search_settings_hud_and_ambient() {
        let (l, rx) = test_window();
        let tv = l.global::<TvUi>();
        let w = l.global::<Watch>();

        // Open Search (tile 6 on Home) and type via OK + physical key
        activate_tile(&l, 6);
        assert_eq!(tv.get_screen(), "search");
        assert!(key(&l, "\n")); // 'A'
        assert!(key(&l, "b"));  // physical 'b'
        assert_eq!(w.get_query(), "ab");
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert_eq!(tv.get_screen(), "home");

        // Switch to Settings via TabBar and toggle Celsius (tile 9; 0..=6 are SubTabs)
        assert!(key(&l, &SharedString::from(Key::UpArrow)));
        for _ in 0..5 {
            assert!(key(&l, &SharedString::from(Key::RightArrow)));
        }
        assert_eq!(tv.get_screen(), "settings");
        activate_tile(&l, 9);
        assert!(l.global::<Display>().get_celsius());
        activate_tile(&l, 1);
        assert_eq!(tv.get_settings_tab(), "accounts");
        activate_tile(&l, 2);
        assert_eq!(tv.get_settings_tab(), "sites");
        activate_tile(&l, 3);
        assert_eq!(tv.get_settings_tab(), "sports");
        activate_tile(&l, 5);
        assert_eq!(tv.get_settings_tab(), "diagnostics");
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
