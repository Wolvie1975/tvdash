//! UI-thread side of search (the dashboard's slide-in panel): turns ranked hits
//! into its groups and buttons. Only touched from the Slint event loop.

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, VecModel};
use tvdash_core::models::{MediaItem, MediaKind, RgbaImage, WatchHit, WatchTier};

use crate::{LaptopWindow, Watch, WatchCard};

const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Default)]
struct WatchUi {
    query: String,
    hits: Vec<WatchHit>,
    posters: HashMap<(bool, u32), Image>,
    debounce: Option<Timer>,
    /// Episodes in progress (Continue Watching), to resume a library show.
    in_progress: Vec<MediaItem>,
}

thread_local! {
    static W: RefCell<WatchUi> = RefCell::new(WatchUi::default());
}

/// Continue Watching changed: shows found in search resume these episodes.
pub fn set_in_progress(items: &[MediaItem]) {
    W.with_borrow_mut(|s| s.in_progress = items.iter().filter(|i| i.show_id.is_some()).cloned().collect());
}

/// The episode in progress for a show, if any.
pub fn resume_for_show(show_id: &str) -> Option<MediaItem> {
    W.with_borrow(|s| s.in_progress.iter().find(|i| i.show_id.as_deref() == Some(show_id)).cloned())
}

pub fn key(is_tv: bool, id: u32) -> String {
    format!("{}:{id}", if is_tv { "tv" } else { "movie" })
}

/// Start a search now. Returns false for an empty query.
pub fn start(laptop: &LaptopWindow, query: &str) -> bool {
    let q = query.trim().to_owned();
    if q.is_empty() {
        return false;
    }
    let w = laptop.global::<Watch>();
    w.set_searching(true);
    w.set_message("".into());
    W.with_borrow_mut(|s| {
        s.query = q;
        s.hits.clear();
        s.debounce = None;
    });
    rebuild(laptop);
    true
}

/// Search once typing pauses for 300 ms. An emptied field clears the results.
pub fn debounce(laptop: &LaptopWindow, query: String, run: impl Fn(String) + 'static) {
    if query.trim().is_empty() {
        W.with_borrow_mut(|s| {
            s.query.clear();
            s.hits.clear();
            s.debounce = None;
        });
        let w = laptop.global::<Watch>();
        w.set_searching(false);
        w.set_message("".into());
        rebuild(laptop);
        return;
    }
    let weak = laptop.as_weak();
    let timer = Timer::default();
    timer.start(slint::TimerMode::SingleShot, DEBOUNCE, move || {
        if let Some(l) = weak.upgrade()
            && start(&l, &query)
        {
            run(query.trim().to_owned());
        }
    });
    W.with_borrow_mut(|s| s.debounce = Some(timer)); // replacing cancels the previous one
}

pub fn on_results(laptop: &LaptopWindow, query: String, hits: Vec<WatchHit>, error: Option<String>) {
    let current = W.with_borrow(|s| s.query == query);
    if !current {
        return; // results for an older query
    }
    let w = laptop.global::<Watch>();
    w.set_searching(false);
    let empty = hits.is_empty();
    W.with_borrow_mut(|s| s.hits = hits);
    w.set_message(match error {
        Some(e) => e.into(),
        None if empty => format!("Nothing on TMDB matches “{query}”.").into(),
        None => "".into(),
    });
    rebuild(laptop);
}

pub fn on_poster(laptop: &LaptopWindow, tmdb_id: u32, is_tv: bool, image: RgbaImage) {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&image.pixels, image.width, image.height);
    W.with_borrow_mut(|s| s.posters.insert((is_tv, tmdb_id), Image::from_rgba8(buffer)));
    rebuild(laptop);
}

/// The hit behind a card key.
pub fn hit(key_: &str) -> Option<WatchHit> {
    W.with_borrow(|s| s.hits.iter().find(|h| key(h.is_tv, h.tmdb_id) == key_).cloned())
}

/// "Movie · 1999 · included with Netflix", "TV series · 2011 · in Plex".
fn meta(h: &WatchHit) -> String {
    let mut parts = vec![if h.is_tv { "TV series".to_owned() } else { "Movie".to_owned() }];
    parts.extend(h.year.map(|y| y.to_string()));
    parts.push(line(h));
    parts.join(" · ")
}

fn line(h: &WatchHit) -> String {
    match h.tier {
        WatchTier::Library => match &h.library {
            Some(item) => format!("in {}", item.source.label()),
            None => "in your library".into(),
        },
        WatchTier::Included => {
            format!("included with {}", h.services.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", "))
        }
        WatchTier::CostsExtra => h.elsewhere.first().cloned().unwrap_or_else(|| "costs extra".into()),
        WatchTier::NotStreaming => "not available in the US".into(),
        WatchTier::Checking => "checking where it streams…".into(),
    }
}

/// What a result's button does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Play this library item (a movie, or the show's episode in progress).
    Play(MediaItem),
    /// Play a library show's next episode.
    PlayShow(MediaItem),
    /// Open the title on app `site`.
    Open { site: usize, title: String },
}

/// The button for a hit: (label, primary, action). Library first; then your apps;
/// "costs extra" and "not streaming" have none.
pub fn action(h: &WatchHit, resume_show: impl Fn(&str) -> Option<MediaItem>) -> Option<(String, bool, Action)> {
    if let Some(item) = &h.library {
        return Some(match item.kind {
            MediaKind::Series => match resume_show(&item.id) {
                Some(ep) => ("Resume on TV".into(), true, Action::Play(ep)),
                None => ("Play on TV".into(), true, Action::PlayShow(item.clone())),
            },
            _ if item.resume_ms.is_some() => ("Resume on TV".into(), true, Action::Play(item.clone())),
            _ => ("Play on TV".into(), true, Action::Play(item.clone())),
        });
    }
    let (site, _) = h.services.first()?;
    Some(("Open on TV".into(), false, Action::Open { site: *site, title: h.title.clone() }))
}

fn card(h: &WatchHit, posters: &HashMap<(bool, u32), Image>, in_progress: &[MediaItem]) -> WatchCard {
    let poster = posters.get(&(h.is_tv, h.tmdb_id));
    let resume = |id: &str| in_progress.iter().find(|i| i.show_id.as_deref() == Some(id)).cloned();
    let (label, primary) = action(h, resume).map(|(l, p, _)| (l, p)).unwrap_or_default();
    WatchCard {
        key: key(h.is_tv, h.tmdb_id).into(),
        title: h.title.as_str().into(),
        meta: meta(h).into(),
        action: label.into(),
        primary,
        has_poster: poster.is_some(),
        poster: poster.cloned().unwrap_or_default(),
    }
}

fn rebuild(laptop: &LaptopWindow) {
    let w = laptop.global::<Watch>();
    W.with_borrow(|s| {
        let group = |t: WatchTier| -> ModelRc<WatchCard> {
            let cards: Vec<WatchCard> =
                s.hits.iter().filter(|h| h.tier == t).map(|h| card(h, &s.posters, &s.in_progress)).collect();
            ModelRc::from(Rc::new(VecModel::from(cards)))
        };
        w.set_library(group(WatchTier::Library));
        w.set_included(group(WatchTier::Included));
        w.set_extra(group(WatchTier::CostsExtra));
        w.set_none(group(WatchTier::NotStreaming));
        w.set_checking(group(WatchTier::Checking));
    });
}

/// Close the panel: it slides out and the query clears.
pub fn close(laptop: &LaptopWindow) {
    W.with_borrow_mut(|s| {
        s.query.clear();
        s.hits.clear();
        s.debounce = None;
    });
    let w = laptop.global::<Watch>();
    w.set_query("".into());
    w.set_searching(false);
    w.set_message("".into());
    rebuild(laptop);
    laptop.global::<crate::Dash>().set_search_open(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvdash_core::models::SourceKind;

    fn hit(tier: WatchTier) -> WatchHit {
        WatchHit {
            tmdb_id: 603,
            is_tv: false,
            title: "The Matrix".into(),
            year: Some(1999),
            overview: String::new(),
            tier,
            services: vec![(3, "Hulu".into()), (5, "Netflix".into())],
            elsewhere: vec!["Rent · Apple TV".into()],
            library: None,
            link: Some("https://tmdb".into()),
        }
    }

    #[test]
    fn lines_and_actions() {
        let none = |_: &str| None;
        assert_eq!(meta(&hit(WatchTier::Included)), "Movie · 1999 · included with Hulu, Netflix");
        assert_eq!(line(&hit(WatchTier::CostsExtra)), "Rent · Apple TV");
        assert_eq!(line(&hit(WatchTier::NotStreaming)), "not available in the US");

        let (label, primary, a) = action(&hit(WatchTier::Included), none).unwrap();
        assert_eq!((label.as_str(), primary), ("Open on TV", false));
        assert_eq!(a, Action::Open { site: 3, title: "The Matrix".into() });
        let extra = WatchHit { services: vec![], ..hit(WatchTier::CostsExtra) };
        assert!(action(&extra, none).is_none(), "costs extra: no button");

        let film = MediaItem {
            source: SourceKind::Plex,
            id: "1".into(),
            kind: MediaKind::Movie,
            title: "The Matrix".into(),
            resume_ms: Some(5),
            ..Default::default()
        };
        let lib = WatchHit { library: Some(film.clone()), ..hit(WatchTier::Library) };
        assert_eq!(line(&lib), "in Plex");
        let (label, primary, a) = action(&lib, none).unwrap();
        assert_eq!((label.as_str(), primary, a), ("Resume on TV", true, Action::Play(film.clone())));

        // A show: resume its episode in progress, else start its next one.
        let show = MediaItem { kind: MediaKind::Series, resume_ms: None, ..film };
        let lib = WatchHit { library: Some(show.clone()), is_tv: true, ..hit(WatchTier::Library) };
        assert_eq!(action(&lib, none).unwrap().2, Action::PlayShow(show));
        let ep = MediaItem { id: "e".into(), kind: MediaKind::Episode, ..Default::default() };
        let (label, _, a) = action(&lib, |id| (id == "1").then(|| ep.clone())).unwrap();
        assert_eq!((label.as_str(), a), ("Resume on TV", Action::Play(ep)));
    }
}
