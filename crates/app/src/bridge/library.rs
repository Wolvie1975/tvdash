//! The Library: smart shelves (laptop), Browse all with its detail panel, and
//! the TV layout's Continue Watching / Recently Added rows.

use std::cell::RefCell;

use chrono::Utc;
use slint::{ComponentHandle, ModelRc, VecModel};
use tokio::sync::mpsc::UnboundedSender;
use tvdash_core::{
    Command,
    config::SortOrder,
    models::{AccountStatus, Collection, LibraryData, MediaItem, MediaKind, SourceKind},
};

use super::{STATE, UiState, home::cards_for, in_order};
use crate::{
    Browse, CollectionCard, CollectionMovie, DetailData, GenreTile, LaptopWindow, Library, PickData, ShelfItem, format,
    shelves::{self, BoxItem, CollectionOrder, Tab, UpNext},
    tint::{self, Hue},
};

/// Posters asked for at once for the Browse grid.
const BROWSE_POSTERS: usize = 120;
/// Posters on a shelf.
const SHELF_POSTERS: usize = 6;
/// Genre tile hues: blue, amber, violet, red, green, teal.
const GENRE_HUES: [f32; 6] = [215.0, 45.0, 265.0, 355.0, 140.0, 195.0];

thread_local! {
    /// For poster requests (shelves and the grid only fetch what they show).
    static COMMANDS: RefCell<Option<UnboundedSender<Command>>> = const { RefCell::new(None) };
}

pub fn set_commands(tx: UnboundedSender<Command>) {
    COMMANDS.set(Some(tx));
}

/// Send a command from the UI side (show pages ask for their seasons).
pub(super) fn send(cmd: Command) {
    COMMANDS.with_borrow(|tx| {
        if let Some(tx) = tx {
            let _ = tx.send(cmd);
        }
    });
}

pub(super) fn want(items: Vec<MediaItem>) {
    want_posters(items);
}

fn want_posters(items: Vec<MediaItem>) {
    let items: Vec<MediaItem> = items.into_iter().filter(|i| i.poster.is_some()).collect();
    if !items.is_empty() {
        COMMANDS.with_borrow(|tx| {
            if let Some(tx) = tx {
                let _ = tx.send(Command::Posters(items));
            }
        });
    }
}

/// Library state on the UI thread.
#[derive(Default)]
pub struct LibraryUi {
    /// `None` until the library has been read (and after sign-out).
    pub data: Option<LibraryData>,
    pub tab: Option<Tab>,
    /// Tonight's pick per tab (kept until Shuffle or it stops qualifying).
    pick_tv: Option<String>,
    pick_movies: Option<String>,
    pub pick: Option<MediaItem>,
    pub boxed: Vec<BoxItem>,
    pub added: Vec<MediaItem>,
    pub again: Vec<MediaItem>,
    added_cards: std::rc::Rc<VecModel<crate::PosterCardData>>,
    again_cards: std::rc::Rc<VecModel<crate::PosterCardData>>,
    // Browse all
    filter: String,
    pub browse: Vec<MediaItem>,
    browse_cards: std::rc::Rc<VecModel<crate::PosterCardData>>,
    /// Order per tab (saved in the config).
    pub sort_movies: SortOrder,
    pub sort_tv: SortOrder,
    /// The Collections row, in its order.
    pub collections: Vec<Collection>,
    /// The open collection page: the collection, its order, Hide watched, what's shown.
    coll: Option<Collection>,
    coll_order: Option<CollectionOrder>,
    coll_hide: bool,
    coll_list: Vec<(usize, MediaItem)>,
}

/// Poster and backdrop keys for a collection (they travel as pseudo items).
fn coll_poster(c: &Collection) -> MediaItem {
    MediaItem { id: format!("coll:{}", c.id), poster: c.poster.clone(), ..Default::default() }
}

fn coll_art(c: &Collection) -> MediaItem {
    let art = c.art.clone().or_else(|| c.movies.first().and_then(|m| m.art.clone()));
    MediaItem { id: format!("art:coll:{}", c.id), poster: art, ..Default::default() }
}

pub(super) fn init(s: &UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    lib.set_continue_watching(ModelRc::from(s.lib_continue_cards.clone()));
    lib.set_recently_added(ModelRc::from(s.lib_recent_cards.clone()));
    lib.set_added(ModelRc::from(s.lib.added_cards.clone()));
    lib.set_again(ModelRc::from(s.lib.again_cards.clone()));
    laptop.global::<Browse>().set_cards(ModelRc::from(s.lib.browse_cards.clone()));
}

fn tab(s: &UiState) -> Tab {
    s.lib.tab.unwrap_or(Tab::Tv)
}

fn sort_of(s: &UiState) -> SortOrder {
    match tab(s) {
        Tab::Movies => s.lib.sort_movies,
        Tab::Tv => s.lib.sort_tv,
    }
}

/// The saved orders (from the config), before the first library read.
pub(super) fn set_sorts(s: &mut UiState, laptop: &LaptopWindow, movies: SortOrder, tv: SortOrder) {
    s.lib.sort_movies = movies;
    s.lib.sort_tv = tv;
    laptop.global::<Library>().set_sort(sort_of(s).id().into());
}

/// Everything that depends on Continue Watching, Recently Added or the theme.
pub(super) fn rebuild(s: &mut UiState, laptop: &LaptopWindow) {
    s.lib_continue = in_order(&s.continue_watching);
    s.lib_recent = in_order(&s.recently_added);
    s.lib_continue_cards.set_vec(cards_for(s, &s.lib_continue));
    s.lib_recent_cards.set_vec(cards_for(s, &s.lib_recent));
    laptop.global::<Library>().set_state(state(s).into());
    show_shelves(s, laptop);
    s.lib.browse_cards.set_vec(cards_for(s, &s.lib.browse));
}

pub(super) fn state(s: &UiState) -> &'static str {
    match (&s.lib.data, s.accounts.get(&SourceKind::Plex)) {
        (Some(_), _) => "ready",
        (None, Some(AccountStatus::NotSetUp)) => "signin",
        _ => "loading",
    }
}

/// The library was read (or emptied by a sign-out).
pub(super) fn on_library(s: &mut UiState, laptop: &LaptopWindow, data: LibraryData) {
    let empty = data == LibraryData::default();
    s.lib.data = (!empty || s.accounts.get(&SourceKind::Plex) == Some(&AccountStatus::Connected)).then_some(data);
    work_out_shelves(s, laptop);
    if !s.lib.filter.is_empty() {
        refilter(s, laptop);
    }
}

/// Work the shelves out for the current tab, keep or choose Tonight's pick, and show them.
fn work_out_shelves(s: &mut UiState, laptop: &LaptopWindow) {
    laptop.global::<Library>().set_state(state(s).into());
    let Some(data) = s.lib.data.as_ref() else { return };
    let tab = tab(s);
    let since = shelves::week_start(s.tz, Utc::now());
    let candidates = shelves::pick_candidates(data, tab);
    let kept = match tab {
        Tab::Tv => &s.lib.pick_tv,
        Tab::Movies => &s.lib.pick_movies,
    };
    let pick = kept
        .as_ref()
        .and_then(|id| candidates.iter().find(|c| c.id == *id))
        .or_else(|| candidates.get(shelves::random(candidates.len())))
        .map(|m| (*m).clone());
    let id = pick.as_ref().map(|p| p.id.clone());
    match tab {
        Tab::Tv => s.lib.pick_tv = id,
        Tab::Movies => s.lib.pick_movies = id,
    }
    s.lib.pick = pick;
    s.lib.boxed = match tab {
        Tab::Tv => shelves::finish_a_season(data),
        Tab::Movies => shelves::movies_in_progress(data),
    };
    let order = sort_of(s);
    s.lib.added = shelves::added_since(data, tab, since);
    s.lib.again = shelves::watch_again(data, tab);
    shelves::sort(&mut s.lib.added, order);
    shelves::sort(&mut s.lib.again, order);
    let genres = shelves::genres(data, tab);
    s.lib.collections = shelves::collection_row(data).into_iter().cloned().collect();

    let lib = laptop.global::<Library>();
    let (title, rule, empty) = match tab {
        Tab::Tv => ("FINISH A SEASON", "Shows with 3 or fewer episodes left", "No season is close to done."),
        Tab::Movies => ("CONTINUE WATCHING", "Movies you started and didn't finish", "No movies in progress."),
    };
    lib.set_box_title(title.into());
    lib.set_box_rule(rule.into());
    lib.set_box_empty(empty.into());
    lib.set_added_rule(
        match tab {
            Tab::Tv => "New shows and new episodes since Monday",
            Tab::Movies => "New movies in your Plex library since Monday",
        }
        .into(),
    );
    lib.set_again_rule(
        match tab {
            Tab::Tv => "Shows you've watched all the way through",
            Tab::Movies => "Movies you've played more than once",
        }
        .into(),
    );
    lib.set_added_count(s.lib.added.len() as i32);
    lib.set_again_count(s.lib.again.len() as i32);
    let tiles: Vec<GenreTile> = genres
        .iter()
        .zip(GENRE_HUES)
        .map(|((name, n), deg)| {
            let t = tint::card_tint(Hue { degrees: deg, saturation: 0.6 }, s.theme);
            GenreTile { name: name.as_str().into(), count: n.to_string().into(), bg: t.card, edge: t.edge, sec: t.sec }
        })
        .collect();
    lib.set_genres(ModelRc::new(VecModel::from(tiles)));
    lib.set_collection_count(s.lib.collections.len() as i32);
    lib.set_sort(order.id().into());

    let mut wanted: Vec<MediaItem> = s.lib.pick.iter().cloned().collect();
    wanted.extend(s.lib.boxed.iter().map(|b| b.item.clone()));
    wanted.extend(s.lib.added.iter().take(SHELF_POSTERS).cloned());
    wanted.extend(s.lib.again.iter().take(SHELF_POSTERS).cloned());
    if tab == Tab::Movies {
        wanted.extend(s.lib.collections.iter().map(coll_poster));
    }
    want_posters(wanted);
    show_shelves(s, laptop);
}

/// Fill the shelf models from the worked-out lists (posters and theme colours too).
fn show_shelves(s: &UiState, laptop: &LaptopWindow) {
    let lib = laptop.global::<Library>();
    let hue_of = |i: &MediaItem| s.hues.get(&(i.source, i.id.clone())).copied().unwrap_or(tint::DEFAULT);
    let poster_of = |i: &MediaItem| s.posters.get(&(i.source, i.id.clone())).cloned();

    lib.set_has_pick(s.lib.pick.is_some());
    let pick_hue = s.lib.pick.as_ref().map(hue_of).unwrap_or(tint::DEFAULT);
    lib.set_pick_tint(tint::card_tint(pick_hue, s.theme));
    if let Some(p) = &s.lib.pick {
        let poster = poster_of(p);
        let (meta, why, action) = match p.kind {
            MediaKind::Series => {
                let n = p.leaf_count.unwrap_or(0);
                (
                    format!("Unwatched series · {n} episode{} · Plex", if n == 1 { "" } else { "s" }),
                    "A show you haven't started yet.".to_owned(),
                    "Start S1 · E1 on TV",
                )
            }
            _ => {
                let mut m = vec!["Unwatched · Movie".to_owned()];
                m.extend(p.duration_ms.map(format::length));
                m.push("Plex".into());
                (m.join(" · "), "Picked at random from movies you haven't watched.".to_owned(), "Play on TV")
            }
        };
        lib.set_pick(PickData {
            title: p.title.as_str().into(),
            meta: meta.into(),
            why: why.into(),
            action: action.into(),
            has_poster: poster.is_some(),
            poster: poster.unwrap_or_default(),
        });
    }

    let boxed: Vec<ShelfItem> = s
        .lib
        .boxed
        .iter()
        .map(|b| {
            let t = tint::card_tint(hue_of(&b.item), s.theme);
            let poster = poster_of(&b.item);
            ShelfItem {
                title: b.item.title.as_str().into(),
                line: b.left.as_str().into(),
                has_poster: poster.is_some(),
                poster: poster.unwrap_or_default(),
                card: t.card,
                art: t.poster,
                accent: t.accent,
                sec: t.sec,
                segments: b.segments as i32,
                done: b.done as i32,
                progress: b.progress,
            }
        })
        .collect();
    lib.set_box_items(ModelRc::new(VecModel::from(boxed)));
    s.lib.added_cards.set_vec(cards_for(s, &s.lib.added[..s.lib.added.len().min(SHELF_POSTERS)]));
    s.lib.again_cards.set_vec(cards_for(s, &s.lib.again[..s.lib.again.len().min(SHELF_POSTERS)]));

    let cards: Vec<CollectionCard> = s
        .lib
        .collections
        .iter()
        .map(|c| {
            let key = coll_poster(c);
            let t = tint::card_tint(hue_of(&key), s.theme);
            let poster = poster_of(&key);
            let st = shelves::stats(c);
            let mut line = format!("{} movie{}", st.total, if st.total == 1 { "" } else { "s" });
            line += &if st.total > 0 && st.watched == st.total {
                " · all watched".to_owned()
            } else {
                format!(" · {} watched", st.watched)
            };
            if c.smart {
                line += " · smart";
            }
            CollectionCard {
                title: c.title.as_str().into(),
                line: line.into(),
                has_poster: poster.is_some(),
                poster: poster.unwrap_or_default(),
                card: t.card,
                art: t.poster,
                accent: t.accent,
                sec: t.sec,
                track: t.track,
                progress: if st.total == 0 { 0.0 } else { st.watched as f32 / st.total as f32 },
            }
        })
        .collect();
    lib.set_collections(ModelRc::new(VecModel::from(cards)));
}

/// A poster arrived: show it wherever the Library uses that item.
pub(super) fn on_poster(s: &mut UiState, laptop: &LaptopWindow, key: &(SourceKind, String)) {
    let is = |i: &MediaItem| i.source == key.0 && i.id == key.1;
    let on_shelves = s.lib.pick.as_ref().is_some_and(is)
        || s.lib.boxed.iter().any(|b| is(&b.item))
        || s.lib.added.iter().take(SHELF_POSTERS).any(is)
        || s.lib.again.iter().take(SHELF_POSTERS).any(is);
    let collection = key.1.strip_prefix("coll:").or_else(|| key.1.strip_prefix("art:coll:"));
    if on_shelves || collection.is_some() {
        show_shelves(s, laptop);
    }
    let open = s.lib.coll.as_ref().map(|c| c.id.as_str());
    if collection.is_some_and(|id| Some(id) == open) || s.lib.coll_list.iter().any(|(_, m)| is(m)) {
        fill_collection(s, laptop);
    }
    if let Some(i) = s.lib.browse.iter().position(is) {
        if let Some(card) = cards_for(s, &s.lib.browse[i..=i]).pop() {
            use slint::Model;
            s.lib.browse_cards.set_row_data(i, card);
        }
        let b = laptop.global::<Browse>();
        if b.get_detail_open() && b.get_selected() == i as i32 {
            b.set_detail(detail(s, &s.lib.browse[i]));
        }
    }
}

// ── Actions ─────────────────────────────────────────────────────────

pub fn set_tab(laptop: &LaptopWindow, name: &str) {
    laptop.global::<Library>().set_tab(name.into());
    STATE.with_borrow_mut(|s| {
        s.lib.tab = Some(Tab::parse(name));
        work_out_shelves(s, laptop);
    });
}

pub fn shuffle(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        let Some(data) = s.lib.data.as_ref() else { return };
        let tab = tab(s);
        let current = s.lib.pick.as_ref().map(|p| p.id.clone());
        let others: Vec<String> = shelves::pick_candidates(data, tab)
            .into_iter()
            .filter(|c| Some(&c.id) != current.as_ref())
            .map(|c| c.id.clone())
            .collect();
        if others.is_empty() {
            return;
        }
        let id = Some(others[shelves::random(others.len())].clone());
        match tab {
            Tab::Tv => s.lib.pick_tv = id,
            Tab::Movies => s.lib.pick_movies = id,
        }
        work_out_shelves(s, laptop);
    });
}

/// What playing a library item means: a movie or episode plays; a show plays
/// its episode in progress, else its next one.
pub fn play_command(item: &MediaItem) -> Command {
    match item.kind {
        MediaKind::Series => match crate::watch_ui::resume_for_show(&item.id) {
            Some(ep) => Command::Play(ep),
            None => Command::PlayShow(item.clone()),
        },
        _ => Command::Play(item.clone()),
    }
}

/// A poster on the "added" or "again" shelf.
pub fn shelf_item(shelf: &str, index: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| if shelf == "added" { &s.lib.added } else { &s.lib.again }.get(index).cloned())
}

/// An item in the Browse all grid.
pub fn browse_item(index: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| s.lib.browse.get(index).cloned())
}

pub fn pick() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.lib.pick.clone())
}

pub fn box_item(index: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| s.lib.boxed.get(index).map(|b| b.item.clone()))
}

/// Open Browse all on `filter` ("all", "added", "again", "genre:<name>").
pub fn browse(laptop: &LaptopWindow, filter: &str) {
    STATE.with_borrow_mut(|s| {
        s.lib.filter = filter.to_owned();
        let b = laptop.global::<Browse>();
        b.set_unwatched_only(false);
        b.set_detail_open(false);
        b.set_selected(-1);
        refilter(s, laptop);
    });
    laptop.global::<crate::Nav>().set_page("browse".into());
}

/// A shelf poster: Browse that shelf with it selected.
pub fn open_item(laptop: &LaptopWindow, shelf: &str, index: usize) {
    let id = STATE.with_borrow(|s| match shelf {
        "added" => s.lib.added.get(index).map(|i| i.id.clone()),
        _ => s.lib.again.get(index).map(|i| i.id.clone()),
    });
    browse(laptop, shelf);
    STATE.with_borrow_mut(|s| {
        if let Some(i) = id.and_then(|id| s.lib.browse.iter().position(|x| x.id == id)) {
            browse_select(s, laptop, i);
        }
    });
}

pub fn set_unwatched(laptop: &LaptopWindow, on: bool) {
    laptop.global::<Browse>().set_unwatched_only(on);
    STATE.with_borrow_mut(|s| refilter(s, laptop));
}

fn refilter(s: &mut UiState, laptop: &LaptopWindow) {
    let b = laptop.global::<Browse>();
    let tab = tab(s);
    let since = shelves::week_start(s.tz, Utc::now());
    s.lib.browse = s
        .lib
        .data
        .as_ref()
        .map(|d| shelves::browse(d, tab, &s.lib.filter, since, b.get_unwatched_only(), sort_of(s)))
        .unwrap_or_default();
    // Genre chips: on the full list and genre views.
    let genres: Vec<slint::SharedString> = match (&s.lib.data, s.lib.filter.as_str()) {
        (Some(d), f) if f == "all" || f.starts_with("genre:") => {
            shelves::all_genres(d, tab).into_iter().map(Into::into).collect()
        }
        _ => Vec::new(),
    };
    b.set_genres(ModelRc::new(VecModel::from(genres)));
    b.set_genre(s.lib.filter.strip_prefix("genre:").unwrap_or_default().into());
    let what = if tab == Tab::Movies { "movies" } else { "shows" };
    let title = match s.lib.filter.as_str() {
        "added" => "Added this week".to_owned(),
        "again" => "Watch again".to_owned(),
        f => match f.strip_prefix("genre:") {
            Some(g) => format!("{g} · {what}"),
            None => format!("All {what}"),
        },
    };
    b.set_title(title.into());
    let n = s.lib.browse.len();
    b.set_count_text(format!("{n} title{}", if n == 1 { "" } else { "s" }).into());
    s.lib.browse_cards.set_vec(cards_for(s, &s.lib.browse));
    b.set_detail_open(false);
    b.set_selected(-1);
    want_posters(s.lib.browse.iter().take(BROWSE_POSTERS).cloned().collect());
}

/// Browse all's genre chips ("" = every genre).
pub fn set_genre(laptop: &LaptopWindow, genre: &str) {
    STATE.with_borrow_mut(|s| {
        s.lib.filter = if genre.is_empty() { "all".into() } else { format!("genre:{genre}") };
        refilter(s, laptop);
    });
}

/// The Sort menu: order the shelves and Browse all, and remember it for this tab.
pub fn set_sort(laptop: &LaptopWindow, id: &str) {
    let Some(order) = SortOrder::parse(id) else { return };
    let lib = laptop.global::<Library>();
    lib.set_sort_open(false);
    let movies = STATE.with_borrow_mut(|s| {
        match tab(s) {
            Tab::Movies => s.lib.sort_movies = order,
            Tab::Tv => s.lib.sort_tv = order,
        }
        work_out_shelves(s, laptop);
        if !s.lib.filter.is_empty() {
            refilter(s, laptop);
        }
        tab(s) == Tab::Movies
    });
    lib.set_sort(order.id().into());
    COMMANDS.with_borrow(|tx| {
        if let Some(tx) = tx {
            let _ = tx.send(Command::SetLibrarySort { movies, order });
        }
    });
}

// ── Collection page ────────────────────────────────────────────────

pub fn open_collection(laptop: &LaptopWindow, index: usize) {
    let opened = STATE.with_borrow_mut(|s| {
        let Some(c) = s.lib.collections.get(index).cloned() else { return false };
        let mut wanted: Vec<MediaItem> = vec![coll_poster(&c), coll_art(&c)];
        wanted.extend(c.movies.iter().cloned());
        want_posters(wanted);
        s.lib.coll = Some(c);
        s.lib.coll_order = Some(CollectionOrder::Collection);
        s.lib.coll_hide = false;
        let page = laptop.global::<crate::Collection>();
        page.set_detail_open(false);
        fill_collection(s, laptop);
        true
    });
    if opened {
        laptop.global::<crate::Nav>().set_page("collection".into());
    }
}

pub fn set_collection_order(laptop: &LaptopWindow, order: &str) {
    STATE.with_borrow_mut(|s| {
        s.lib.coll_order = Some(CollectionOrder::parse(order));
        fill_collection(s, laptop);
    });
}

pub fn set_collection_hide(laptop: &LaptopWindow, hide: bool) {
    STATE.with_borrow_mut(|s| {
        s.lib.coll_hide = hide;
        fill_collection(s, laptop);
    });
}

/// The collection's big button: resume, else the first unwatched, else from the start.
pub fn collection_up_next() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.lib.coll.as_ref().and_then(shelves::up_next).map(|u| u.item().clone()))
}

pub fn collection_shuffle() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.lib.coll.as_ref().and_then(|c| shelves::shuffle_unwatched(c, None)))
}

pub fn collection_movie(index: usize) -> Option<MediaItem> {
    STATE.with_borrow(|s| s.lib.coll_list.get(index).map(|(_, m)| m.clone()))
}

/// Hold a movie: the Browse detail panel over the page.
pub fn collection_details(laptop: &LaptopWindow, index: usize) {
    STATE.with_borrow_mut(|s| {
        let Some((_, item)) = s.lib.coll_list.get(index).cloned() else { return };
        laptop.global::<Browse>().set_detail(detail(s, &item));
        laptop.global::<crate::Collection>().set_detail_open(true);
        s.selected = Some(item);
    });
}

fn fill_collection(s: &mut UiState, laptop: &LaptopWindow) {
    let Some(c) = s.lib.coll.clone() else { return };
    let page = laptop.global::<crate::Collection>();
    let order = s.lib.coll_order.unwrap_or(CollectionOrder::Collection);
    let st = shelves::stats(&c);
    let hue = s.hues.get(&(SourceKind::Plex, format!("coll:{}", c.id))).copied().unwrap_or(tint::DEFAULT);
    let t = tint::card_tint(hue, s.theme);
    page.set_tint(t.clone());
    page.set_crumb(format!("Movies › Collections › {}", c.title).into());
    page.set_title(c.title.as_str().into());
    let mut meta = vec![format!("{} movie{}", st.total, if st.total == 1 { "" } else { "s" })];
    match st.years {
        Some((a, b)) if a != b => meta.push(format!("{a}–{b}")),
        Some((a, _)) => meta.push(a.to_string()),
        None => {}
    }
    if st.runtime_ms > 0 {
        meta.push(format!("{} in total", format::length(st.runtime_ms)));
    }
    page.set_meta(meta.join(" · ").into());
    page.set_progress(if st.total == 0 { 0.0 } else { st.watched as f32 / st.total as f32 });
    page.set_watched_text(format!("{} of {} watched", st.watched, st.total).into());
    page.set_summary(c.summary.clone().unwrap_or_default().into());
    let (up, action) = match shelves::up_next(&c) {
        Some(UpNext::Resume(m)) => {
            let left = format::time_left(&m).map(|l| format!(" · {l}")).unwrap_or_default();
            (format!("Up next: {}{left}", m.title), format!("Resume {} on TV", m.title))
        }
        Some(UpNext::Play(m)) => (format!("Up next: {}", m.title), format!("Play {} on TV", m.title)),
        Some(UpNext::FromStart(m)) => {
            (format!("All watched. Start again with {}", m.title), "Play from the start".into())
        }
        None => (String::new(), String::new()),
    };
    page.set_up_next(up.into());
    page.set_action(action.into());
    page.set_can_shuffle(c.movies.iter().any(|m| m.view_count == 0));
    let art = s.posters.get(&(SourceKind::Plex, format!("art:coll:{}", c.id))).cloned();
    page.set_has_backdrop(art.is_some());
    page.set_backdrop(art.unwrap_or_default());
    page.set_order(
        match order {
            CollectionOrder::Collection => "collection",
            CollectionOrder::Release => "release",
            CollectionOrder::Az => "az",
        }
        .into(),
    );
    page.set_hide_watched(s.lib.coll_hide);

    s.lib.coll_list = shelves::collection_movies(&c, order, s.lib.coll_hide);
    let movies: Vec<CollectionMovie> = s
        .lib
        .coll_list
        .iter()
        .map(|(n, m)| {
            let key = (m.source, m.id.clone());
            let hue = s.hues.get(&key).copied().unwrap_or(tint::DEFAULT);
            let poster = s.posters.get(&key).cloned();
            let year = m.year.map(|y| y.to_string()).unwrap_or_default();
            let state = match format::time_left(m) {
                Some(left) if m.resume_ms.is_some() => left,
                _ if m.view_count > 0 => "watched".into(),
                _ => "unwatched".into(),
            };
            CollectionMovie {
                position: if order == CollectionOrder::Collection { format!("#{n}") } else { year.clone() }.into(),
                title: m.title.as_str().into(),
                line: [year, state].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ").into(),
                has_poster: poster.is_some(),
                poster: poster.unwrap_or_default(),
                placeholder: tint::card_tint(Hue { saturation: hue.saturation.max(0.3), ..hue }, s.theme).poster,
                accent: tint::progress_color(hue),
                progress: m.progress().unwrap_or(0.0),
                watched: m.view_count > 0 && m.resume_ms.is_none(),
            }
        })
        .collect();
    page.set_movies(ModelRc::new(VecModel::from(movies)));
}

pub fn select(laptop: &LaptopWindow, index: usize) {
    STATE.with_borrow_mut(|s| browse_select(s, laptop, index));
}

fn browse_select(s: &mut UiState, laptop: &LaptopWindow, index: usize) {
    let Some(item) = s.lib.browse.get(index).cloned() else { return };
    let b = laptop.global::<Browse>();
    b.set_detail(detail(s, &item));
    b.set_selected(index as i32);
    b.set_detail_open(true);
    want_posters(vec![item.clone()]);
    s.selected = Some(item);
}

fn detail(s: &UiState, item: &MediaItem) -> DetailData {
    let poster = s.posters.get(&(item.source, item.id.clone())).cloned();
    let mut meta = vec![if item.kind == MediaKind::Series { "TV series" } else { "Movie" }.to_owned()];
    meta.extend(item.year.map(|y| y.to_string()));
    if item.kind != MediaKind::Series {
        meta.extend(item.duration_ms.filter(|&d| d > 0).map(format::length));
    }
    meta.extend(item.genres.first().cloned());
    let (extra, action, can_resume) = match item.kind {
        MediaKind::Series => {
            let (all, seen) = (item.leaf_count.unwrap_or(0), item.viewed_leaf_count.unwrap_or(0));
            let resume = crate::watch_ui::resume_for_show(&item.id);
            let action = match &resume {
                Some(ep) => format!("Resume {} on TV", ep.episode_label.clone().unwrap_or_default()),
                None if seen == 0 => "Start S1 · E1 on TV".into(),
                None => "Play next episode on TV".into(),
            };
            (format!("{seen} of {all} episodes watched"), action, false)
        }
        _ => {
            let extra = match item.view_count {
                0 => String::new(),
                1 => "Watched once".into(),
                n => format!("Watched {n} times"),
            };
            let action = if item.resume_ms.is_some() { "Resume on TV" } else { "Play on TV" };
            (extra, action.into(), item.resume_ms.is_some())
        }
    };
    DetailData {
        title: item.title.as_str().into(),
        subtitle: format::time_left(item).unwrap_or_default().into(),
        badge: item.source.label().to_uppercase().into(),
        meta: meta.join(" · ").into(),
        extra: extra.into(),
        overview: item.summary.clone().unwrap_or_default().into(),
        action: action.into(),
        has_poster: poster.is_some(),
        poster: poster.unwrap_or_default(),
        progress: item.progress().unwrap_or(0.0),
        can_resume,
    }
}

/// TV layout: a Continue Watching or Recently Added card.
pub fn select_row(section: &str, index: usize) {
    STATE.with_borrow_mut(|s| {
        let list = if section == "continue" { &s.lib_continue } else { &s.lib_recent };
        s.selected = list.get(index).cloned();
    });
}

/// The item picked in Browse (or the TV layout).
pub fn selected() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.selected.clone())
}

/// Items on the Library's shelves (for tests).
#[cfg(test)]
pub fn shelf_ids() -> (Option<String>, Vec<String>, std::collections::HashSet<String>) {
    STATE.with_borrow(|s| {
        (
            s.lib.pick.as_ref().map(|p| p.id.clone()),
            s.lib.boxed.iter().map(|b| b.item.id.clone()).collect(),
            s.lib.added.iter().map(|i| i.id.clone()).collect(),
        )
    })
}
