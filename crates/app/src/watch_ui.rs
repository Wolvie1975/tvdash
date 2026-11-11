//! UI-thread side of Where to watch: turns ranked hits into the page's groups
//! and detail panel. Only touched from the Slint event loop.

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, VecModel};
use tvdash_core::models::{MediaKind, RgbaImage, WatchHit, WatchTier};

use crate::{LaptopWindow, Watch, WatchAction, WatchCard};

const DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Default)]
struct WatchUi {
    query: String,
    hits: Vec<WatchHit>,
    posters: HashMap<(bool, u32), Image>,
    debounce: Option<Timer>,
}

thread_local! {
    static W: RefCell<WatchUi> = RefCell::new(WatchUi::default());
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
    w.set_detail_open(false);
    W.with_borrow_mut(|s| {
        s.query = q;
        s.hits.clear();
        s.debounce = None;
    });
    rebuild(laptop);
    true
}

/// Search once typing pauses for 300 ms.
pub fn debounce(laptop: &LaptopWindow, query: String, run: impl Fn(String) + 'static) {
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
    // Keep the open detail panel current as availability arrives.
    if w.get_detail_open() {
        select(laptop, &w.get_selected());
    }
}

pub fn on_poster(laptop: &LaptopWindow, tmdb_id: u32, is_tv: bool, image: RgbaImage) {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&image.pixels, image.width, image.height);
    W.with_borrow_mut(|s| s.posters.insert((is_tv, tmdb_id), Image::from_rgba8(buffer)));
    rebuild(laptop);
    let w = laptop.global::<Watch>();
    if w.get_detail_open() && w.get_selected() == key(is_tv, tmdb_id).as_str() {
        select(laptop, &w.get_selected());
    }
}

/// The hit behind a card key.
pub fn hit(key_: &str) -> Option<WatchHit> {
    W.with_borrow(|s| s.hits.iter().find(|h| key(h.is_tv, h.tmdb_id) == key_).cloned())
}

fn meta(h: &WatchHit) -> String {
    let kind = if h.is_tv { "Show" } else { "Movie" };
    match h.year {
        Some(y) => format!("{y} · {kind}"),
        None => kind.into(),
    }
}

fn line(h: &WatchHit) -> String {
    match h.tier {
        WatchTier::Library => match &h.library {
            Some(item) => format!("In {}", item.source.label()),
            None => "In your library".into(),
        },
        WatchTier::Included => {
            format!("On {}", h.services.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", "))
        }
        WatchTier::CostsExtra => h.elsewhere.first().cloned().unwrap_or_else(|| "Costs extra".into()),
        WatchTier::NotStreaming => "Not streaming in the US".into(),
        WatchTier::Checking => "Checking…".into(),
    }
}

fn tier_id(t: WatchTier) -> &'static str {
    match t {
        WatchTier::Library => "library",
        WatchTier::Included => "included",
        WatchTier::CostsExtra => "extra",
        WatchTier::NotStreaming => "none",
        WatchTier::Checking => "checking",
    }
}

fn card(h: &WatchHit, posters: &HashMap<(bool, u32), Image>) -> WatchCard {
    let poster = posters.get(&(h.is_tv, h.tmdb_id));
    WatchCard {
        key: key(h.is_tv, h.tmdb_id).into(),
        title: h.title.as_str().into(),
        meta: meta(h).into(),
        line: line(h).into(),
        tier: tier_id(h.tier).into(),
        has_poster: poster.is_some(),
        poster: poster.cloned().unwrap_or_default(),
    }
}

fn rebuild(laptop: &LaptopWindow) {
    let w = laptop.global::<Watch>();
    W.with_borrow(|s| {
        let group = |t: WatchTier| -> ModelRc<WatchCard> {
            let cards: Vec<WatchCard> = s.hits.iter().filter(|h| h.tier == t).map(|h| card(h, &s.posters)).collect();
            ModelRc::from(Rc::new(VecModel::from(cards)))
        };
        w.set_library(group(WatchTier::Library));
        w.set_included(group(WatchTier::Included));
        w.set_extra(group(WatchTier::CostsExtra));
        w.set_none(group(WatchTier::NotStreaming));
        w.set_checking(group(WatchTier::Checking));
    });
}

/// What the detail panel offers for a hit, best first.
pub fn actions(h: &WatchHit) -> Vec<WatchAction> {
    let mut out = Vec::new();
    if let Some(item) = &h.library {
        let (label, kind) = match item.kind {
            MediaKind::Movie if item.resume_ms.is_some() => (format!("Resume on TV · {}", item.source.label()), "play"),
            MediaKind::Movie => (format!("Play on TV · {}", item.source.label()), "play"),
            _ => (format!("Show in Library · {}", item.source.label()), "library"),
        };
        out.push(WatchAction { label: label.into(), kind: kind.into(), site: -1, primary: true });
    }
    for (i, name) in &h.services {
        out.push(WatchAction {
            label: format!("Open on {name}").into(),
            kind: "open".into(),
            site: *i as i32,
            primary: out.is_empty(),
        });
    }
    if h.link.is_some() {
        out.push(WatchAction { label: "All options on TMDB".into(), kind: "tmdb".into(), site: -1, primary: false });
    }
    out
}

pub fn select(laptop: &LaptopWindow, key_: &str) {
    let Some(h) = hit(key_) else { return };
    let w = laptop.global::<Watch>();
    let posters = W.with_borrow(|s| s.posters.get(&(h.is_tv, h.tmdb_id)).cloned());
    w.set_detail(WatchCard {
        has_poster: posters.is_some(),
        poster: posters.unwrap_or_default(),
        ..card(&h, &HashMap::new())
    });
    w.set_detail_overview(h.overview.as_str().into());
    let model = |v: Vec<slint::SharedString>| ModelRc::from(Rc::new(VecModel::from(v)));
    w.set_detail_yours(model(h.services.iter().map(|(_, n)| n.as_str().into()).collect()));
    let elsewhere = match h.tier {
        WatchTier::Checking => vec!["Checking where it streams…".into()],
        WatchTier::NotStreaming => vec!["Not streaming in the US right now".into()],
        _ => h.elsewhere.iter().map(|e| e.as_str().into()).collect(),
    };
    w.set_detail_elsewhere(model(elsewhere));
    w.set_detail_actions(ModelRc::from(Rc::new(VecModel::from(actions(&h)))));
    w.set_selected(key_.into());
    w.set_detail_open(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvdash_core::models::{MediaItem, SourceKind};

    fn hit(tier: WatchTier) -> WatchHit {
        WatchHit {
            tmdb_id: 603,
            is_tv: false,
            title: "The Matrix".into(),
            year: Some(1999),
            overview: String::new(),
            tier,
            services: vec![(2, "Hulu".into()), (0, "Netflix".into())],
            elsewhere: vec!["Rent · Apple TV".into()],
            library: None,
            link: Some("https://tmdb".into()),
        }
    }

    #[test]
    fn lines_and_actions() {
        assert_eq!(meta(&hit(WatchTier::Included)), "1999 · Movie");
        assert_eq!(line(&hit(WatchTier::Included)), "On Hulu, Netflix");
        assert_eq!(line(&hit(WatchTier::CostsExtra)), "Rent · Apple TV");
        assert_eq!(line(&hit(WatchTier::NotStreaming)), "Not streaming in the US");

        let a = actions(&hit(WatchTier::Included));
        let got: Vec<(&str, &str, i32, bool)> =
            a.iter().map(|x| (x.label.as_str(), x.kind.as_str(), x.site, x.primary)).collect();
        assert_eq!(
            got,
            [
                ("Open on Hulu", "open", 2, true),
                ("Open on Netflix", "open", 0, false),
                ("All options on TMDB", "tmdb", -1, false)
            ]
        );

        let mut lib = hit(WatchTier::Library);
        lib.library = Some(MediaItem {
            source: SourceKind::Plex,
            id: "1".into(),
            kind: MediaKind::Movie,
            title: "The Matrix".into(),
            subtitle: None,
            episode_label: None,
            duration_ms: None,
            resume_ms: Some(5),
            poster: None,
        });
        assert_eq!(line(&lib), "In Plex");
        let a = actions(&lib);
        assert_eq!((a[0].label.as_str(), a[0].kind.as_str(), a[0].primary), ("Resume on TV · Plex", "play", true));
        assert!(!a[1].primary, "services follow the library action");
        lib.library.as_mut().unwrap().kind = MediaKind::Series;
        assert_eq!(actions(&lib)[0].kind, "library");
    }
}
