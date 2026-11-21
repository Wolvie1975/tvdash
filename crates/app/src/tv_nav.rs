//! Focus navigation for the TV-only v2 Tile Hub (arrows + OK + Back + mouse).

use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use slint::{ComponentHandle, Model, SharedString, platform::Key};
use tvdash_core::config::Layout;

use crate::{
    Accounts, Artwork, CollectionPageState, Dash, Display, LaptopWindow, Library, LibraryLanding, Nav, Scoreboard,
    Server, ShowPageState, SiteEditor, SleepState, Sports, SportsAlertState, SportsAlertsSettings, TvUi,
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
        Page::Collections => "collections",
        Page::Sports => "sports",
        Page::Search => "search",
        Page::Settings => "settings",
    }
}

fn hint_for(page: &Page) -> &'static str {
    match page {
        Page::Home => "OK Select · Back Home · F12 Return from service",
        Page::Collections => "OK Open collection · Back Home",
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
        crate::bridge::start_ambient(laptop);
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
                    if browse && s.shell.focused_tile >= 32 {
                        let vert = Rect { x: r.y - 256.0, y: 0.0, w: r.h, h: r.w };
                        s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 748.0, 16.0);
                    } else if browse {
                        s.lib_scroll = 0.0;
                    }
                }
                Page::Collections => {
                    let vert = Rect { x: r.y - 180.0, y: 0.0, w: r.h, h: r.w };
                    s.lib_scroll = focus::panorama_scroll(vert, s.lib_scroll, 824.0, 16.0);
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
                _ => {}
            }
        }
        s.cw_scroll = 0.0;
        if !matches!(s.shell.current(), Page::Library | Page::Collections) {
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
        let lib = laptop.global::<Library>();
        if lib.get_collections().row_count() == 0 && lib.get_media_tab() != "movies" {
            lib.invoke_set_media_tab("movies".into());
        }
    }
}

fn blades_tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
    let tv = laptop.global::<TvUi>();
    match page {
        Page::Home => {
            let ox = 24.0;
            let oy = 24.0;
            let mut rects = vec![
                Rect { x: ox + 40.0, y: oy + 96.0, w: 900.0, h: 470.0 },  // 0: Hero
                Rect { x: ox + 40.0, y: oy + 96.0, w: 900.0, h: 470.0 },  // 1: Hero
                Rect { x: ox + 978.0, y: oy + 492.0, w: 572.0, h: 50.0 }, // 2: Search
                Rect { x: ox + 978.0, y: oy + 545.0, w: 572.0, h: 50.0 }, // 3: Library
                Rect { x: ox + 978.0, y: oy + 598.0, w: 572.0, h: 50.0 }, // 4: Screensaver
                Rect { x: ox + 968.0, y: oy + 96.0, w: 592.0, h: 204.0 }, // 5: Clock/Weather
                Rect { x: ox + 968.0, y: oy + 318.0, w: 592.0, h: 146.0 }, // 6: Last Played
                Rect { x: ox + 978.0, y: oy + 704.0, w: 572.0, h: 50.0 }, // 7: Google Meet
                Rect { x: ox + 978.0, y: oy + 757.0, w: 572.0, h: 50.0 }, // 8: Theme
                Rect { x: ox + 978.0, y: oy + 651.0, w: 572.0, h: 50.0 }, // 9: Sleep timer
            ];
            let n = laptop.global::<TvUi>().get_services().row_count().min(10);
            for i in 0..n {
                rects.push(Rect {
                    x: ox + 40.0 + (i % 5) as f32 * 182.0,
                    y: oy + 626.0 + (i / 5) as f32 * 102.0,
                    w: 170.0,
                    h: 88.0,
                });
            }
            rects
        }
        Page::Library => {
            let ox = 92.0;
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
                        x: ox + 40.0 + (i % 8) as f32 * 192.0,
                        y: oy + 210.0 + (i / 8) as f32 * 316.0,
                        w: 174.0,
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
            let ox = 160.0;
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
        Page::Show(_) => {
            let ox = 160.0;
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
            rects
        }
        Page::Collection(_) => {
            let ox = 228.0;
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
            let ox = 228.0;
            let oy = 24.0;
            vec![
                Rect { x: ox + 200.0, y: oy + 38.0, w: 144.0, h: 36.0 },
                Rect { x: ox + 346.0, y: oy + 38.0, w: 126.0, h: 36.0 },
            ]
        }
        Page::Settings => {
            let ox = 296.0;
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
                }
            }
            rects
        }
    }
}

fn tile_rects(laptop: &LaptopWindow, page: &Page) -> Vec<Rect> {
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
    if laptop.global::<Display>().get_dashboard_ui() == "blades" {
        return blades_tile_rects(laptop, page);
    }
    let tv = laptop.global::<TvUi>();
    let dash = laptop.global::<Dash>();
    match page {
        Page::Home => {
            // 0..=4: Clock, Hero, Search, Library, Screensaver
            // 5: Weather (210 x 230), 6: Last Played (356 x 230), 7: Settings (356 x 230), 8: Theme (240 x 230)
            // 9..19: 2x5 Streaming Services grid (235 x 146)
            let mut rects = vec![
                Rect { x: 96.0, y: 136.0, w: 210.0, h: 590.0 },   // 0: Clock + Plex info
                Rect { x: 322.0, y: 136.0, w: 728.0, h: 590.0 },  // 1: Hero
                Rect { x: 1066.0, y: 136.0, w: 240.0, h: 135.0 }, // 2: Search
                Rect { x: 1066.0, y: 287.0, w: 240.0, h: 135.0 }, // 3: Library
                Rect { x: 1066.0, y: 438.0, w: 240.0, h: 135.0 }, // 4: Screensaver
                Rect { x: 96.0, y: 742.0, w: 210.0, h: 230.0 },   // 5: Weather
                Rect { x: 322.0, y: 742.0, w: 356.0, h: 230.0 },  // 6: Last Played
                Rect { x: 694.0, y: 742.0, w: 356.0, h: 230.0 },  // 7: Google Meet
                Rect { x: 1066.0, y: 742.0, w: 240.0, h: 230.0 }, // 8: Theme
                Rect { x: 1066.0, y: 589.0, w: 240.0, h: 135.0 }, // 9: Sleep
            ];
            let svcs_n = tv.get_services().row_count().min(10);
            for i in 0..svcs_n {
                let col = (i % 2) as f32;
                let row = (i / 2) as f32;
                rects.push(Rect {
                    x: 1338.0 + col * 251.0,
                    y: 176.0 + row * 162.0,
                    w: 235.0,
                    h: 146.0,
                });
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
        Page::Sports => vec![
            Rect { x: 96.0, y: 122.0, w: 184.0, h: 44.0 },
            Rect { x: 316.0, y: 122.0, w: 164.0, h: 44.0 },
        ],
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
                        y: 296.0 + (i / 8) as f32 * 276.0,
                        w: 198.0,
                        h: 256.0,
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
                            x: 96.0,
                            y: 252.0 + i as f32 * 96.0,
                            w: 1728.0,
                            h: 84.0,
                        });
                    }
                }
                "server" => {
                    rects.push(Rect { x: 1604.0, y: 180.0, w: 220.0, h: 52.0 });
                }
                "diagnostics" | "changelog" => {}
                _ => {
                    rects.extend([
                        Rect { x: 96.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 528.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 960.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 1392.0, y: 184.0, w: 416.0, h: 220.0 },
                        Rect { x: 96.0, y: 420.0, w: 416.0, h: 220.0 },
                        Rect { x: 528.0, y: 420.0, w: 416.0, h: 220.0 },
                        Rect { x: 96.0, y: 656.0, w: 416.0, h: 220.0 },
                        Rect { x: 528.0, y: 656.0, w: 416.0, h: 220.0 }, // 13: Sleep warning
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
    if tv.get_screen() == "library" && !laptop.global::<Library>().get_browse_all() && idx >= 4 {
        laptop.global::<LibraryLanding>().invoke_focus_changed(idx as i32);
    }
}

fn scroll_library_to_focus(laptop: &LaptopWindow, idx: usize) {
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
            Page::Collections => {
                s.lib_scroll = (s.lib_scroll + step).clamp(0.0, 4000.0);
                tv.set_lib_scroll(s.lib_scroll);
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
                    if dash.get_tv().paused {
                        dash.invoke_transport("play-pause".into());
                    }
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
            4 => {
                crate::bridge::start_ambient(laptop);
                tv.set_ambient(true);
            }
            5 => {} // Weather tile
            6 => tv.invoke_play_last_played(),
            7 => {
                laptop.global::<Sports>().invoke_open_url("Google Meet".into(), "https://meet.google.com".into());
            }
            8 => {
                let next = match laptop.global::<crate::Theme>().get_mode() {
                    UiTheme::Dark => UiTheme::Aubergine,
                    UiTheme::Aubergine => UiTheme::Ku,
                    UiTheme::Ku => UiTheme::Stone,
                    UiTheme::Stone => UiTheme::Mist,
                    UiTheme::Mist => UiTheme::Dark,
                };
                dash.invoke_set_theme(next);
            }
            9 => laptop.global::<SleepState>().invoke_open_picker(),
            i if i >= 10 => {
                let svcs = tv.get_services();
                if let Some(svc) = svcs.row_data(i - 10) {
                    dash.invoke_open_site(svc.index);
                }
            }
            _ => {}
        },
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
                match idx {
                    0 => {
                        lib.set_browse_all(false);
                        lib.invoke_set_media_tab("tv".into());
                        STATE.with_borrow_mut(|s| s.shell.focused_tile = 4);
                        sync(laptop);
                    }
                    1 => {
                        lib.set_browse_all(false);
                        lib.invoke_set_media_tab("movies".into());
                        STATE.with_borrow_mut(|s| s.shell.focused_tile = 4);
                        sync(laptop);
                    }
                    2 => lib.set_browse_all(!lib.get_browse_all()),
                    _ => ll.invoke_open_watching_now(),
                }
                return;
            }
            let is_tv = lib.get_media_tab() == "tv";
            if lib.get_browse_all() {
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

            let item_idx = idx - 4;
            let s1_n = ll.get_shelf1().row_count().min(4);
            let s2_n = ll.get_shelf2().row_count().min(6);
            if item_idx < s1_n {
                if let Some(row) = ll.get_shelf1().row_data(item_idx) {
                    lib.invoke_play_key(row.rating_key, true);
                }
            } else if item_idx < s1_n + s2_n {
                let s2_idx = item_idx - s1_n;
                if let Some(row) = ll.get_shelf2().row_data(s2_idx) {
                    if is_tv {
                        lib.invoke_open_show(row.rating_key);
                        STATE.with_borrow_mut(|s| s.shell.push(Page::Show(String::new())));
                        sync(laptop);
                    } else {
                        lib.invoke_play_key(row.rating_key, true);
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
                        lib.invoke_play_key(row.rating_key, true);
                    }
                }
            }
        }
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
                            let next = if disp.get_dashboard_ui() == "blades" { "classic" } else { "blades" };
                            disp.invoke_set_dashboard_ui(next.into());
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
    if laptop.global::<SiteEditor>().get_open() || laptop.global::<Artwork>().get_open() {
        return false;
    }
    let tv = laptop.global::<TvUi>();
    STATE.with_borrow_mut(|s| s.last_key = Instant::now());

    let alert_open = laptop.global::<SportsAlertState>().get_visible();
    if tv.get_ambient() && !alert_open {
        tv.set_ambient(false);
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
            let cur = tv.get_focused_tile().clamp(0, 8);
            match decode_dir(text) {
                Some(Dir::Left) => {
                    tv.set_focused_tile((cur - 1).max(0));
                    return true;
                }
                Some(Dir::Right) => {
                    tv.set_focused_tile((cur + 1).min(8));
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
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Command::LoadLibraryShelves { is_tv: true, .. }
        ));
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert_eq!(tv.get_screen(), "home");

        // Down from Hero lands on Last Played (tile 6), Sleep (tile 9) opens picker, service tile (10..20) opens it
        assert!(key(&l, &SharedString::from(Key::DownArrow)));
        assert_eq!(tv.get_focused_tile(), 6);
        activate_tile(&l, 9);
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

        // Open Search (tile 2 on Home) and type via OK + physical key
        activate_tile(&l, 2);
        assert_eq!(tv.get_screen(), "search");
        assert!(key(&l, "\n")); // 'A'
        assert!(key(&l, "b"));  // physical 'b'
        assert_eq!(w.get_query(), "ab");
        assert!(key(&l, &SharedString::from(Key::Escape)));
        assert_eq!(tv.get_screen(), "home");

        // Switch to Settings via TabBar and toggle Celsius (tile 9; 0..=6 are SubTabs)
        assert!(key(&l, &SharedString::from(Key::UpArrow)));
        for _ in 0..4 {
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
