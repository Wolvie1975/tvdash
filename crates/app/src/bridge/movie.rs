//! Bridge state and event handlers for `MoviePageState`.

use std::{rc::Rc, time::Duration};

use chrono::Local;
use slint::{Color, ComponentHandle, Image, Model, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};
use tvdash_core::{
    config::ClockFormat,
    events::{CollectionLink, ExtraItem, ExtraKind, MovieDetail, Person},
    models::{MediaItem, RgbaImage, SourceKind},
};

use super::{LaptopWindow, STATE, UiState, to_image};
use crate::{
    ExtraData, MoviePageState, PersonData, TvUi,
    tint,
    movie::{play_label, runtime_short},
    orbit::rings::{gap_for_count, ring_paths},
};

const DISC_TINTS: &[u32] = &[
    0x3B5B7C, 0x6B4C7A, 0x3D6E5C, 0x7A5638, 0x4A5B82, 0x784458, 0x3E6A75, 0x6A613B,
];

pub(super) struct MovieUi {
    pub detail: Option<MovieDetail>,
    pub extras: Vec<ExtraItem>,
    pub collection_link: Option<CollectionLink>,
    pub crew_model: Rc<VecModel<PersonData>>,
    pub cast_model: Rc<VecModel<PersonData>>,
    pub extras_model: Rc<VecModel<ExtraData>>,
    pub timer: Option<Timer>,
}

impl Default for MovieUi {
    fn default() -> Self {
        Self {
            detail: None,
            extras: Vec::new(),
            collection_link: None,
            crew_model: Rc::new(VecModel::default()),
            cast_model: Rc::new(VecModel::default()),
            extras_model: Rc::new(VecModel::default()),
            timer: None,
        }
    }
}

pub(super) fn init(s: &UiState, laptop: &LaptopWindow) {
    let mp = laptop.global::<MoviePageState>();
    mp.set_crew(ModelRc::from(s.movie.crew_model.clone()));
    mp.set_cast(ModelRc::from(s.movie.cast_model.clone()));
    mp.set_extras(ModelRc::from(s.movie.extras_model.clone()));
}

pub fn reset_loading(laptop: &LaptopWindow, rating_key: &str) {
    STATE.with_borrow_mut(|s| {
        s.movie.detail = None;
        s.movie.extras.clear();
        s.movie.collection_link = None;
        s.movie.timer = None;
        s.movie.crew_model.set_vec(Vec::<PersonData>::new());
        s.movie.cast_model.set_vec(Vec::<PersonData>::new());
        s.movie.extras_model.set_vec(Vec::<ExtraData>::new());
    });
    let mp = laptop.global::<MoviePageState>();
    mp.set_rating_key(rating_key.into());
    mp.set_title("".into());
    mp.set_kicker("".into());
    mp.set_meta_line("".into());
    mp.set_chips(ModelRc::default());
    mp.set_tech_chips(ModelRc::default());
    mp.set_summary("".into());
    mp.set_backdrop(Image::default());
    mp.set_has_backdrop(false);
    mp.set_play_label("Play".into());
    mp.set_play_meta("".into());
    mp.set_progress(0.0);
    mp.set_show_from_start(false);
    mp.set_has_trailer(false);
    mp.set_watched(false);
    mp.set_in_collection(false);
    mp.set_collection_title("".into());
    mp.set_collection_position("".into());
    mp.set_collection_total(0);
    mp.set_collection_watched(0);
    mp.set_collection_next("".into());
    mp.set_collection_poster(Image::default());
    mp.set_tab("cast".into());
    mp.set_cast_count("0".into());
    mp.set_loading(true);
}

pub(super) fn on_movie_detail(s: &mut UiState, laptop: &LaptopWindow, detail: MovieDetail) {
    let mp = laptop.global::<MoviePageState>();
    let h24 = s.clock == Some(ClockFormat::H24);
    let pl = play_label(&detail, Local::now(), h24);

    let res_hdr = match (&detail.resolution, &detail.hdr) {
        (Some(r), Some(h)) => Some(format!("{r} · {h}")),
        (Some(r), None) => Some(r.clone()),
        (None, Some(h)) => Some(h.clone()),
        (None, None) => None,
    };
    let runtime = if detail.duration_ms > 0 { Some(runtime_short(detail.duration_ms)) } else { None };
    let score = detail.audience_rating.map(|r| format!("★ {r:.1}"));

    // Orbit kicker: "MOVIE · 2019 · 1 H 52 M · PG-13"
    let mut kicker_parts: Vec<String> = vec!["MOVIE".into()];
    if let Some(y) = detail.year {
        kicker_parts.push(y.to_string());
    }
    if let Some(rt) = &runtime {
        kicker_parts.push(rt.to_uppercase());
    }
    if let Some(cr) = &detail.content_rating {
        kicker_parts.push(cr.to_uppercase());
    }

    // Classic meta_line: "2019 · Drama · Thriller · PG-13 · 1 h 52 m · ★ 7.8 · 4K · Dolby Vision"
    let mut meta_parts: Vec<String> = Vec::new();
    if let Some(y) = detail.year {
        meta_parts.push(y.to_string());
    }
    for g in detail.genres.iter().take(2) {
        meta_parts.push(g.clone());
    }
    if let Some(cr) = &detail.content_rating {
        meta_parts.push(cr.clone());
    }
    if let Some(rt) = &runtime {
        meta_parts.push(rt.clone());
    }
    if let Some(sc) = &score {
        meta_parts.push(sc.clone());
    }
    if let Some(rh) = &res_hdr {
        meta_parts.push(rh.clone());
    }
    if let Some(aud) = &detail.audio {
        meta_parts.push(aud.clone());
    }

    // Blades chips: ["2019", "PG-13", "Drama", "Thriller", "1 h 52 m", "★ 7.8", "4K · Dolby Vision", "5.1"]
    let mut chips: Vec<SharedString> = Vec::new();
    if let Some(y) = detail.year {
        chips.push(y.to_string().into());
    }
    if let Some(cr) = &detail.content_rating {
        chips.push(cr.as_str().into());
    }
    for g in detail.genres.iter().take(2) {
        chips.push(g.as_str().into());
    }
    if let Some(rt) = &runtime {
        chips.push(rt.as_str().into());
    }
    if let Some(sc) = &score {
        chips.push(sc.as_str().into());
    }
    if let Some(rh) = &res_hdr {
        chips.push(rh.as_str().into());
    }
    if let Some(aud) = &detail.audio {
        chips.push(aud.as_str().into());
    }

    // Orbit tech_chips: ["Drama", "Thriller", "★ 7.8", "4K · Dolby Vision", "5.1"]
    let mut tech_chips: Vec<SharedString> = Vec::new();
    for g in detail.genres.iter().take(2) {
        tech_chips.push(g.as_str().into());
    }
    if let Some(sc) = &score {
        tech_chips.push(sc.as_str().into());
    }
    if let Some(rh) = &res_hdr {
        tech_chips.push(rh.as_str().into());
    }
    if let Some(aud) = &detail.audio {
        tech_chips.push(aud.as_str().into());
    }

    let key = (SourceKind::Plex, detail.item.id.clone());
    if let Some(art) = s.backdrops.get(&key).or_else(|| s.posters.get(&key)) {
        mp.set_backdrop(art);
        mp.set_has_backdrop(true);
    } else {
        mp.set_backdrop(Image::default());
        mp.set_has_backdrop(false);
    }
    let hue = s.backdrop_hues.get(&key).or_else(|| s.hues.get(&key)).copied().unwrap_or(tint::DEFAULT);
    let card = tint::card_tint(hue, s.effective_theme());
    let (tint, on_tint) = (card.accent, card.on_accent);

    let crew_rows: Vec<PersonData> = detail.crew.iter().map(|p| person_row(s, p)).collect();
    let cast_rows: Vec<PersonData> = detail.cast.iter().map(|p| person_row(s, p)).collect();
    let cast_count = crew_rows.len() + cast_rows.len();

    s.movie.crew_model.set_vec(crew_rows);
    s.movie.cast_model.set_vec(cast_rows);

    mp.set_rating_key(detail.item.id.as_str().into());
    mp.set_title(detail.item.title.as_str().into());
    mp.set_kicker(kicker_parts.join(" · ").into());
    mp.set_meta_line(meta_parts.join(" · ").into());
    mp.set_chips(ModelRc::new(VecModel::from(chips)));
    mp.set_tech_chips(ModelRc::new(VecModel::from(tech_chips)));
    mp.set_summary(detail.summary.as_str().into());
    mp.set_tint(tint);
    mp.set_on_tint(on_tint);
    mp.set_play_label(pl.label.into());
    mp.set_play_meta(pl.meta.into());
    mp.set_progress(pl.progress);
    let play_ring = ring_paths(100, (pl.progress * 100.0).round() as usize, 47.5, 0.0);
    mp.set_play_ring_lit(play_ring.lit.into());
    mp.set_play_ring_unlit(play_ring.unlit.into());
    mp.set_show_from_start(pl.show_from_start);
    mp.set_watched(detail.view_count > 0 && detail.view_offset_ms == 0);
    mp.set_has_trailer(
        detail.primary_extra_key.is_some()
            || s.movie.extras.iter().any(|e| e.kind == ExtraKind::Trailer),
    );
    mp.set_cast_count(cast_count.to_string().into());
    mp.set_loading(false);

    s.movie.detail = Some(detail);
    start_countdown_timer(s, laptop.as_weak());
}

pub(super) fn on_movie_extras(
    s: &mut UiState,
    laptop: &LaptopWindow,
    movie_key: String,
    extras: Vec<ExtraItem>,
) {
    if s.movie.detail.as_ref().is_none_or(|d| d.item.id != movie_key) {
        return;
    }
    let rows: Vec<ExtraData> = extras
        .iter()
        .map(|e| {
            let thumb = s
                .posters
                .peek(&(SourceKind::Plex, e.item.id.clone()))
                .cloned()
                .unwrap_or_default();
            ExtraData {
                key: e.item.id.as_str().into(),
                title: e.item.title.as_str().into(),
                kind: e.kind.label().into(),
                duration: e.duration_ms.map(runtime_short).unwrap_or_default().into(),
                thumb,
                is_trailer: e.kind == ExtraKind::Trailer,
            }
        })
        .collect();
    let has_trailer = s.movie.detail.as_ref().is_some_and(|d| d.primary_extra_key.is_some())
        || extras.iter().any(|e| e.kind == ExtraKind::Trailer);
    s.movie.extras = extras;
    s.movie.extras_model.set_vec(rows);
    laptop.global::<MoviePageState>().set_has_trailer(has_trailer);
}

pub(super) fn on_movie_collection(
    s: &mut UiState,
    laptop: &LaptopWindow,
    movie_key: String,
    link: Option<CollectionLink>,
) {
    if s.movie.detail.as_ref().is_none_or(|d| d.item.id != movie_key) {
        return;
    }
    let mp = laptop.global::<MoviePageState>();
    s.movie.collection_link = link.clone();
    match link {
        Some(l) => {
            let poster = s
                .posters
                .peek(&(SourceKind::Plex, l.key.clone()))
                .cloned()
                .unwrap_or_default();
            mp.set_in_collection(true);
            mp.set_collection_title(l.title.into());
            mp.set_collection_position(format!("#{} of {}", l.position, l.total).into());
            mp.set_collection_total(l.total as i32);
            mp.set_collection_watched(l.watched as i32);
            mp.set_collection_next(l.next_unwatched.unwrap_or_default().into());
            mp.set_collection_poster(poster);
            let col_ring = ring_paths(l.total, l.watched, 33.0, gap_for_count(l.total));
            mp.set_col_ring_lit(col_ring.lit.into());
            mp.set_col_ring_unlit(col_ring.unlit.into());
        }
        None => {
            mp.set_in_collection(false);
        }
    }
}

pub(super) fn on_poster(
    s: &mut UiState,
    laptop: &LaptopWindow,
    source: SourceKind,
    item_id: &str,
    image: &RgbaImage,
) {
    if source != SourceKind::Plex {
        return;
    }
    if let Some(url) = item_id.strip_prefix("person:")
        && let Some(detail) = &s.movie.detail
    {
        let img = to_image(image);
        for (idx, p) in detail.crew.iter().enumerate() {
            if p.thumb.as_deref() == Some(url)
                && let Some(mut row) = s.movie.crew_model.row_data(idx)
            {
                row.photo = img.clone();
                row.has_photo = true;
                s.movie.crew_model.set_row_data(idx, row);
            }
        }
        for (idx, p) in detail.cast.iter().enumerate() {
            if p.thumb.as_deref() == Some(url)
                && let Some(mut row) = s.movie.cast_model.row_data(idx)
            {
                row.photo = img.clone();
                row.has_photo = true;
                s.movie.cast_model.set_row_data(idx, row);
            }
        }
        return;
    }
    if let Some(idx) = s.movie.extras.iter().position(|e| e.item.id == item_id)
        && let Some(mut row) = s.movie.extras_model.row_data(idx)
    {
        row.thumb = to_image(image);
        s.movie.extras_model.set_row_data(idx, row);
    }
    if s.movie.collection_link.as_ref().is_some_and(|l| l.key == item_id) {
        laptop.global::<MoviePageState>().set_collection_poster(to_image(image));
    }
    if s.movie.detail.as_ref().is_some_and(|d| d.item.id == item_id) {
        let mp = laptop.global::<MoviePageState>();
        if !mp.get_has_backdrop() {
            mp.set_backdrop(to_image(image));
            mp.set_has_backdrop(true);
        }
    }
}

pub(super) fn on_backdrop(
    s: &mut UiState,
    laptop: &LaptopWindow,
    source: SourceKind,
    item_id: &str,
    image: &RgbaImage,
) {
    if source == SourceKind::Plex && s.movie.detail.as_ref().is_some_and(|d| d.item.id == item_id) {
        let mp = laptop.global::<MoviePageState>();
        mp.set_backdrop(to_image(image));
        mp.set_has_backdrop(true);
    }
}

pub fn current_item() -> Option<MediaItem> {
    STATE.with_borrow(|s| s.movie.detail.as_ref().map(|d| d.item.clone()))
}

pub fn first_trailer_key() -> Option<String> {
    STATE.with_borrow(|s| {
        s.movie
            .extras
            .iter()
            .find(|e| e.kind == ExtraKind::Trailer)
            .map(|e| e.item.id.clone())
            .or_else(|| s.movie.detail.as_ref().and_then(|d| d.primary_extra_key.clone()))
    })
}

pub fn collection_target() -> Option<(String, String)> {
    STATE.with_borrow(|s| {
        let link = s.movie.collection_link.as_ref()?;
        let movie_id = s.movie.detail.as_ref()?.item.id.clone();
        Some((link.key.clone(), movie_id))
    })
}

pub fn toggle_watched_args() -> Option<(String, bool)> {
    STATE.with_borrow(|s| {
        let d = s.movie.detail.as_ref()?;
        let currently_watched = d.view_count > 0 && d.view_offset_ms == 0;
        Some((d.item.id.clone(), !currently_watched))
    })
}

fn start_countdown_timer(s: &mut UiState, weak: Weak<LaptopWindow>) {
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(60), move || {
        let Some(l) = weak.upgrade() else { return };
        if l.global::<TvUi>().get_screen() != "movie" {
            return;
        }
        STATE.with_borrow(|st| {
            let Some(detail) = st.movie.detail.as_ref() else { return };
            let h24 = st.clock == Some(ClockFormat::H24);
            let pl = play_label(detail, Local::now(), h24);
            l.global::<MoviePageState>().set_play_meta(pl.meta.into());
        });
    });
    s.movie.timer = Some(timer);
}

fn person_row(s: &UiState, p: &Person) -> PersonData {
    let (photo, has_photo) = p
        .thumb
        .as_deref()
        .and_then(|url| s.posters.peek(&(SourceKind::Plex, format!("person:{url}"))))
        .map(|img| (img.clone(), true))
        .unwrap_or_else(|| (Image::default(), false));
    PersonData {
        name: p.name.as_str().into(),
        role: p.role.as_str().into(),
        initials: initials_for(&p.name).into(),
        photo,
        has_photo,
        tint: disc_tint_for(&p.name),
    }
}

fn initials_for(name: &str) -> String {
    let mut parts = name.split_whitespace();
    let first = parts.next().and_then(|w| w.chars().next());
    let last = parts.next_back().and_then(|w| w.chars().next());
    match (first, last) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

fn disc_tint_for(name: &str) -> Color {
    let hash = name.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let rgb = DISC_TINTS[(hash as usize) % DISC_TINTS.len()];
    Color::from_rgb_u8(((rgb >> 16) & 0xFF) as u8, ((rgb >> 8) & 0xFF) as u8, (rgb & 0xFF) as u8)
}
