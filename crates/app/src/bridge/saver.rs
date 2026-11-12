//! The screensaver: the time and weather as sentences, and what's on the TV.

use chrono::TimeDelta;
use slint::ComponentHandle;
use tvdash_core::{
    config::{ClockFormat, ThemeMode},
    models::{MediaKind, PlayState},
};

use super::{UiState, home};
use crate::{LaptopWindow, Saver, format, tint};

/// Burn-in: where the text block sits, one step per minute (px from its home).
const DRIFT: [(f32, f32); 8] =
    [(0.0, 0.0), (6.0, -4.0), (10.0, 3.0), (4.0, 8.0), (-3.0, 5.0), (-6.0, -2.0), (-2.0, -7.0), (5.0, -6.0)];

/// "It's 9:04 PM on Thursday." and "Outside, 51° and partly sunny."
pub(super) fn sentences(s: &UiState, laptop: &LaptopWindow) {
    let saver = laptop.global::<Saver>();
    let now = format::now_local(s.tz);
    saver.set_time(format::hm(now, s.clock.unwrap_or(ClockFormat::H12)).into());
    saver.set_day(now.format("%A").to_string().into());
    match &s.weather {
        Some((w, _)) => {
            saver.set_has_weather(true);
            saver.set_temp(format!("{}°", w.temperature.round() as i64).into());
            saver.set_condition(w.condition.to_lowercase().into());
        }
        None => saver.set_has_weather(false),
    }
}

/// Move the text a step (once a minute).
pub(super) fn drift(s: &mut UiState, laptop: &LaptopWindow) {
    s.saver_step = (s.saver_step + 1) % DRIFT.len();
    let (x, y) = DRIFT[s.saver_step];
    let saver = laptop.global::<Saver>();
    saver.set_drift_x(x);
    saver.set_drift_y(y);
}

/// The On the TV status block, accented from the poster (dark theme).
pub(super) fn status(s: &UiState, laptop: &LaptopWindow, hue: tint::Hue) {
    let saver = laptop.global::<Saver>();
    saver.set_accent(tint::card_tint(hue, ThemeMode::Dark).accent);
    saver.set_show_progress(false);
    if let Some(st) = &s.player {
        let item = home::with_details(s, &st.item);
        let state = if st.state == PlayState::Paused { "PAUSED" } else { "PLAYING" };
        saver.set_label(format!("ON THE TV · {state} · {}", item.source.label().to_uppercase()).into());
        saver.set_title(item.title.as_str().into());
        let line = match item.kind {
            MediaKind::Episode => format::episode_line(&item),
            _ => format::facts(&item),
        };
        saver.set_line(line.into());
        if let Some(dur) = st.duration_ms.filter(|&d| d > 0) {
            let left = dur.saturating_sub(st.position_ms);
            let ends = format::now_local(s.tz) + TimeDelta::milliseconds(left as i64);
            saver.set_show_progress(true);
            saver.set_progress(format::fraction(st));
            saver.set_at(format!("{} / {}", format::duration(st.position_ms), format::duration(dur)).into());
            saver.set_ends(format!("Ends at {}", format::hm(ends, s.clock.unwrap_or(ClockFormat::H12))).into());
        }
        return;
    }
    if let Some(site) = &s.browser {
        saver.set_label("ON THE TV · STREAMING".into());
        saver.set_title(site.as_str().into());
        saver.set_line("Full screen on the TV".into());
        return;
    }
    saver.set_label("ON THE TV · NOTHING PLAYING".into());
    match s.rows.first() {
        Some(item) => {
            saver.set_title(format!("Up next: {}", item.title).into());
            let mut parts: Vec<String> = Vec::new();
            if item.kind == MediaKind::Episode {
                match (item.season, item.episode) {
                    (Some(se), Some(e)) => parts.push(format!("Season {se} · Episode {e}")),
                    _ => parts.extend(item.episode_label.clone()),
                }
            }
            parts.extend(format::time_left(item));
            saver.set_line(parts.join(" · ").into());
        }
        None => {
            saver.set_title("Nothing playing".into());
            saver.set_line("".into());
        }
    }
}
