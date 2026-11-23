//! Colour from poster art (the redesign's "Option B"): the On the TV card is
//! tinted with the hue of what's playing (or what's up next); everything else
//! stays neutral. Also the hues for streaming tiles.
//!
//! Colours are built in HSL from a hue and a saturation, with fixed lightness
//! per theme, so text on every tint passes 4.5:1 (checked in the tests).

use slint::Color;
use tvdash_core::{config::ThemeMode, models::RgbaImage};

use crate::Tint;

/// Dominant hue of an image: degrees and how colourful it is (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hue {
    pub degrees: f32,
    pub saturation: f32,
}

/// The tint used when nothing has art: the design's "up next" blue.
pub const DEFAULT: Hue = Hue { degrees: 214.0, saturation: 0.6 };

/// Saturation-weighted mean hue of the image (sampled), ignoring near-black,
/// near-white and grey pixels. Mostly grey art gives a low saturation, which
/// keeps its tint muted.
pub fn poster_hue(img: &RgbaImage) -> Hue {
    let (mut x, mut y, mut weight, mut n) = (0.0f32, 0.0f32, 0.0f32, 0u32);
    let step = ((img.width * img.height) as usize / 4000).max(1);
    for px in img.pixels.as_chunks::<4>().0.iter().step_by(step) {
        let (h, s, l) = rgb_to_hsl(px[0], px[1], px[2]);
        n += 1;
        if !(0.12..=0.88).contains(&l) || s < 0.15 {
            continue;
        }
        // Mid-lightness, saturated pixels carry the colour.
        let w = s * (1.0 - (2.0 * l - 1.0).abs());
        x += w * h.to_radians().cos();
        y += w * h.to_radians().sin();
        weight += w;
    }
    if n == 0 || weight / (n as f32) < 0.02 {
        return Hue { degrees: DEFAULT.degrees, saturation: 0.0 };
    }
    let degrees = y.atan2(x).to_degrees().rem_euclid(360.0);
    // How strongly the colourful pixels agree (vector length) × how many there are.
    let agreement = (x * x + y * y).sqrt() / weight;
    let coverage = (weight / n as f32 * 4.0).min(1.0);
    Hue { degrees, saturation: (agreement * coverage).clamp(0.0, 1.0) }
}

/// Colours for the On the TV card in this theme.
pub fn card_tint(hue: Hue, mode: ThemeMode) -> Tint {
    let h = hue.degrees;
    let k = hue.saturation.clamp(0.0, 1.0);
    if mode.is_light() {
        let (raised, sec) = match mode {
            ThemeMode::Stone => (rgb(0xE3DFD7), rgb(0x47433D)),
            _ => (rgb(0xDEE3E8), rgb(0x3F464F)),
        };
        Tint {
            card: hsl(h, 0.08 + 0.17 * k, 0.77),
            poster: hsl(h, 0.12 + 0.25 * k, 0.62),
            accent: hsl(h, 0.2 + 0.4 * k, 0.2),
            sec,
            track: hsl(h, 0.1 + 0.15 * k, 0.69),
            btn: raised,
            edge: hsl(h, 0.1 + 0.15 * k, 0.66),
            on_accent: rgb(0xFFFFFF),
        }
    } else if mode == ThemeMode::Ku {
        Tint {
            card: rgb(0x0D203B),
            poster: rgb(0x142D52),
            accent: rgb(0xFF4D5A),
            sec: rgb(0xB8CBE6),
            track: rgb(0x163259),
            btn: rgb(0x142D52),
            edge: rgb(0x21467A),
            on_accent: rgb(0x0B0D10),
        }
    } else if mode == ThemeMode::KuLight {
        Tint {
            card: rgb(0xE2E5E7),
            poster: rgb(0xD3D7DA),
            accent: rgb(0xFFC82D),
            sec: rgb(0x0051BA),
            track: rgb(0xC2C7CA),
            btn: rgb(0xF2F4F5),
            edge: rgb(0xB7BCBF),
            on_accent: rgb(0x0B192E),
        }
    } else {
        Tint {
            card: hsl(h, 0.12 + 0.33 * k, 0.11),
            poster: hsl(h, 0.15 + 0.3 * k, 0.2),
            accent: hsl(h, 0.3 + 0.5 * k, 0.72),
            sec: hsl(h, 0.1 + 0.2 * k, 0.74),
            track: hsl(h, 0.12 + 0.3 * k, 0.18),
            btn: hsl(h, 0.12 + 0.3 * k, 0.15),
            edge: hsl(h, 0.12 + 0.28 * k, 0.24),
            on_accent: rgb(0x0B0D10),
        }
    }
}

/// Progress-bar colour on a poster card (always drawn on a dark scrim).
pub fn progress_color(hue: Hue) -> Color {
    hsl(hue.degrees, 0.3 + 0.5 * hue.saturation, 0.7)
}

/// Hues of the default streaming sites (from the Color board); other sites
/// take the next unused hue in order.
pub fn site_hue(name: &str, index: usize) -> f32 {
    let known =
        [("netflix", 338.0), ("disney", 202.0), ("hulu", 94.0), ("prime", 23.0), ("apple", 262.0), ("youtube", 7.0)];
    let lower = name.to_lowercase();
    if let Some((_, h)) = known.iter().find(|(k, _)| lower.contains(k)) {
        return *h;
    }
    const SPARE: [f32; 6] = [175.0, 48.0, 300.0, 130.0, 232.0, 280.0];
    SPARE[index % SPARE.len()]
}

/// (background, foreground) of a streaming tile's monogram.
pub fn site_colors(hue: f32, mode: ThemeMode) -> (Color, Color) {
    if mode.is_light() {
        (hsl(hue, 0.25, 0.8), hsl(hue, 0.6, 0.23))
    } else {
        (hsl(hue, 0.35, 0.16), hsl(hue, 0.8, 0.74))
    }
}

fn rgb(hex: u32) -> Color {
    Color::from_rgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// HSL (degrees, 0–1, 0–1) to a colour.
pub fn hsl(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let u = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_rgb_u8(u(r), u(g), u(b))
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color) -> f64 {
        let f = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.red()) + 0.7152 * f(c.green()) + 0.0722 * f(c.blue())
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    fn solid(r: u8, g: u8, b: u8) -> RgbaImage {
        RgbaImage { width: 10, height: 10, pixels: [r, g, b, 255].repeat(100) }
    }

    #[test]
    fn hsl_round_trip() {
        assert_eq!(hsl(0.0, 1.0, 0.5), Color::from_rgb_u8(255, 0, 0));
        assert_eq!(hsl(120.0, 1.0, 0.25), Color::from_rgb_u8(0, 128, 0));
        let (h, s, l) = rgb_to_hsl(0x5B, 0x9D, 0xFF);
        assert!((h - 215.0).abs() < 1.0 && s > 0.99 && (l - 0.68).abs() < 0.01);
    }

    #[test]
    fn hue_of_art() {
        let teal = poster_hue(&solid(30, 140, 140));
        assert!((teal.degrees - 180.0).abs() < 2.0 && teal.saturation > 0.8, "{teal:?}");
        let grey = poster_hue(&solid(120, 120, 120));
        assert_eq!(grey.saturation, 0.0);
        // Half red, half black: still red, less certain than solid red.
        let mut half = solid(200, 30, 30);
        half.pixels[..200].fill(0);
        let h = poster_hue(&half);
        assert!(h.degrees < 2.0 || h.degrees > 358.0, "{h:?}");
    }

    #[test]
    fn text_on_every_tint_passes_aa() {
        let text = |m: ThemeMode| match m {
            ThemeMode::Dark | ThemeMode::Aubergine | ThemeMode::Ku | ThemeMode::FollowSystem => rgb(0xE8EAED),
            ThemeMode::Stone => rgb(0x1F1D1A),
            ThemeMode::Mist => rgb(0x1A1E23),
            ThemeMode::KuLight => rgb(0x0B192E),
        };
        for mode in [ThemeMode::Dark, ThemeMode::Aubergine, ThemeMode::Ku, ThemeMode::KuLight, ThemeMode::Stone, ThemeMode::Mist] {
            for deg in (0..360).step_by(10) {
                for sat in [0.0, 0.5, 1.0] {
                    let t = card_tint(Hue { degrees: deg as f32, saturation: sat }, mode);
                    let ctx = format!("{mode:?} {deg}° s{sat}");
                    assert!(contrast(t.accent, t.card) >= 4.5, "accent {ctx}: {}", contrast(t.accent, t.card));
                    assert!(contrast(t.sec, t.card) >= 4.5, "secondary {ctx}");
                    assert!(contrast(text(mode), t.card) >= 4.5, "text {ctx}");
                    assert!(contrast(t.on_accent, t.accent) >= 4.5, "button label {ctx}");
                    assert!(contrast(text(mode), t.btn) >= 4.5, "button text {ctx}");
                }
                let (bg, fg) = site_colors(deg as f32, mode);
                assert!(contrast(fg, bg) >= 4.5, "tile {mode:?} {deg}°: {}", contrast(fg, bg));
            }
        }
    }

    #[test]
    fn site_hues() {
        assert_eq!(site_hue("Netflix", 0), 338.0);
        assert_eq!(site_hue("Disney+", 1), 202.0);
        assert_eq!(site_hue("Max", 6), site_hue("Peacock", 12));
    }
}
