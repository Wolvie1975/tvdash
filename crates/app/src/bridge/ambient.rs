//! TV Screensaver (`TvAmbient`) — Poster wall pre-renderer + Up next + library footer.

use rand::seq::SliceRandom;
use slint::{ComponentHandle, Image, Model, Rgba8Pixel, SharedPixelBuffer};
use tvdash_core::models::{MediaItem, PlayState, SourceKind};

use super::{STATE, UiState};
use crate::{AmbientState, LaptopWindow, Server, format, tint};

/// Render at 0.5× of 2700×2560 (12 cols × 8 rows, 2× repeat height for seamless vertical wrap).
const COLS: usize = 12;
const ROWS: usize = 8;
const TILE_W: f32 = 103.0;
const TILE_H: f32 = 150.0;
const GAP: f32 = 10.0;
const STEP_X: f32 = TILE_W + GAP;
const STEP_Y: f32 = TILE_H + GAP;
const REPEAT_H: u32 = (ROWS as f32 * STEP_Y) as u32; // 1280
const WALL_W: u32 = (COLS as f32 * STEP_X) as u32; // 1356
const WALL_H: u32 = REPEAT_H * 2; // 2560

fn image_to_pixmap(img: &Image) -> Option<tiny_skia::Pixmap> {
    let buf = img.to_rgba8()?;
    let w = buf.width();
    let h = buf.height();
    let size = tiny_skia::IntSize::from_wh(w, h)?;
    let mut data = buf.as_bytes().to_vec();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u16;
        if a < 255 {
            px[0] = ((px[0] as u16 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u16 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u16 * a + 127) / 255) as u8;
        }
    }
    tiny_skia::Pixmap::from_vec(data, size)
}

fn build_poster_wall(s: &UiState) -> Option<Image> {
    if s.posters.is_empty() {
        return None;
    }
    let mut imgs: Vec<Image> = s.posters.values().cloned().collect();
    let mut rng = rand::thread_rng();
    imgs.shuffle(&mut rng);
    imgs.truncate(COLS * ROWS);

    let pixmaps: Vec<tiny_skia::Pixmap> = imgs.iter().filter_map(image_to_pixmap).collect();
    if pixmaps.is_empty() {
        return None;
    }

    let mut wall = tiny_skia::Pixmap::new(WALL_W, WALL_H)?;
    wall.fill(tiny_skia::Color::from_rgba8(5, 6, 8, 255));

    let paint = tiny_skia::PixmapPaint {
        opacity: 0.42,
        quality: tiny_skia::FilterQuality::Bilinear,
        ..Default::default()
    };

    for row in 0..ROWS {
        for col in 0..COLS {
            let idx = (row * COLS + col) % pixmaps.len();
            let src = &pixmaps[idx];
            let sx = TILE_W / src.width() as f32;
            let sy = TILE_H / src.height() as f32;
            let dx = col as f32 * STEP_X;
            let dy = row as f32 * STEP_Y;
            for copy in 0..2 {
                let y_off = dy + copy as f32 * REPEAT_H as f32;
                let xf = tiny_skia::Transform::from_scale(sx, sy).post_translate(dx, y_off);
                wall.draw_pixmap(0, 0, src.as_ref(), &paint, xf, None);
            }
        }
    }

    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(wall.data(), WALL_W, WALL_H);
    Some(Image::from_rgba8(buf))
}

fn format_mm_ss(ms: u64) -> String {
    let secs = ms / 1000;
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}

fn apply_up_next_item(
    s: &UiState,
    amb: &AmbientState,
    label: &str,
    item: &MediaItem,
    line: String,
    progress: Option<f32>,
) {
    let key = (item.source, item.id.clone());
    let hue = s.hues.get(&key).copied().unwrap_or(tint::DEFAULT);
    let t = tint::card_tint(hue, tvdash_core::config::ThemeMode::Dark);
    let poster = s.posters.peek(&key);
    amb.set_up_next_label(label.into());
    amb.set_up_next_title(item.title.as_str().into());
    amb.set_up_next_line(line.into());
    amb.set_up_next_accent(t.accent);
    amb.set_up_next_has_poster(poster.is_some());
    amb.set_up_next_poster(poster.cloned().unwrap_or_default());
    if let Some(p) = progress {
        amb.set_up_next_has_bar(true);
        amb.set_up_next_progress(p.clamp(0.0, 1.0));
    } else {
        amb.set_up_next_has_bar(false);
        amb.set_up_next_progress(0.0);
    }
}

pub fn start_ambient(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        s.ambient_starts = s.ambient_starts.wrapping_add(1);
        let amb = laptop.global::<AmbientState>();

        // 1. Poster wall
        if let Some(wall) = build_poster_wall(s) {
            amb.set_has_wall(true);
            amb.set_wall(wall);
            amb.set_repeat_height((REPEAT_H * 2) as f32);
        } else {
            amb.set_has_wall(false);
        }

        // 2. Up next block (3 states)
        if let Some(p) = &s.player
            && p.state == PlayState::Paused
        {
            let mut parts = Vec::new();
            if let Some(ep) = &p.item.episode_label {
                parts.push(ep.clone());
            }
            if let Some(sub) = &p.item.subtitle {
                parts.push(sub.clone());
            }
            parts.push(format!("stopped at {}", format_mm_ss(p.position_ms)));
            let prog = match p.duration_ms {
                Some(d) if d > 0 => p.position_ms as f32 / d as f32,
                _ => 0.0,
            };
            let title = p.item.subtitle.as_deref().unwrap_or(&p.item.title);
            let mut item = p.item.clone();
            item.title = title.to_string();
            apply_up_next_item(s, &amb, "PAUSED · PLEX", &item, parts.join(" · "), Some(prog));
        } else if let Some(item) = s
            .lib_continue
            .iter()
            .chain(s.lib_shelf1_items.iter())
            .find(|i| i.resume_ms.is_some())
            .or_else(|| s.lib_continue.first())
        {
            let mut parts = Vec::new();
            if let Some(ep) = &item.episode_label {
                parts.push(ep.clone());
            }
            if let Some(left) = format::time_left(item) {
                parts.push(left);
            }
            let prog = item
                .duration_ms
                .filter(|&d| d > 0)
                .and_then(|d| item.resume_ms.map(|r| r as f32 / d as f32));
            let label = match item.source {
                SourceKind::Jellyfin => "UP NEXT · JELLYFIN",
                _ => "UP NEXT · PLEX",
            };
            apply_up_next_item(s, &amb, label, item, parts.join(" · "), prog);
        } else if !s.lib_shelf2_items.is_empty() {
            let idx = s.ambient_starts % s.lib_shelf2_items.len();
            let item = &s.lib_shelf2_items[idx];
            apply_up_next_item(s, &amb, "START WATCHING", item, format::item_meta(item), None);
        } else {
            amb.set_up_next_label("".into());
            amb.set_up_next_title("".into());
            amb.set_up_next_line("".into());
            amb.set_up_next_has_bar(false);
            amb.set_up_next_has_poster(false);
        }

        // 3. Footer library count
        let libs = laptop.global::<Server>().get_libraries();
        let mut shows = 0i32;
        let mut movies = 0i32;
        for i in 0..libs.row_count() {
            if let Some(row) = libs.row_data(i) {
                let lower = row.title.to_lowercase();
                if lower.contains("tv") || lower.contains("show") || lower.contains("series") {
                    shows += row.count;
                } else if lower.contains("movie") || lower.contains("film") {
                    movies += row.count;
                }
            }
        }
        let server_name = s
            .server_summary
            .as_ref()
            .map(|x| x.name.as_str())
            .filter(|n| !n.is_empty())
            .or_else(|| s.servers.get(&SourceKind::Plex).map(|s| s.as_str()))
            .unwrap_or("Plex");
        if shows > 0 || movies > 0 {
            amb.set_footer_line(format!("{shows} shows · {movies} movies on {server_name}").into());
        } else {
            amb.set_footer_line(format!("Connected to {server_name}").into());
        }
    });
}
