//! Ten streaming service brand fills, wordmarks, and bundled SVG logos.

use slint::{Color, Image, SharedString};

use crate::Brand;

fn rgb(hex: u32) -> Color {
    Color::from_rgb_u8(((hex >> 16) & 0xFF) as u8, ((hex >> 8) & 0xFF) as u8, (hex & 0xFF) as u8)
}

/// Normalize a display name (e.g. `"Disney+"`, `"Fandango at Home"`) to its slug ID.
pub fn service_id(name_or_id: &str) -> String {
    let mut out = String::with_capacity(name_or_id.len());
    let mut prev_dash = false;
    for ch in name_or_id.trim().chars() {
        match ch {
            'a'..='z' | '0'..='9' => {
                out.push(ch);
                prev_dash = false;
            }
            'A'..='Z' => {
                out.push(ch.to_ascii_lowercase());
                prev_dash = false;
            }
            '+' => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                }
                out.push_str("plus");
                prev_dash = false;
            }
            _ => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                    prev_dash = true;
                }
            }
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

fn bundled_svg(id: &str) -> Option<&'static [u8]> {
    match id {
        "youtube-tv" => Some(include_bytes!("../../../assets/logos/youtube-tv.svg")),
        "youtube-music" => Some(include_bytes!("../../../assets/logos/youtube-music.svg")),
        "youtube" => Some(include_bytes!("../../../assets/logos/youtube.svg")),
        "disney-plus" => Some(include_bytes!("../../../assets/logos/disney-plus.svg")),
        "hulu" => Some(include_bytes!("../../../assets/logos/hulu.svg")),
        "apple-tv" => Some(include_bytes!("../../../assets/logos/apple-tv.svg")),
        "netflix" => Some(include_bytes!("../../../assets/logos/netflix.svg")),
        "fandango-at-home" => Some(include_bytes!("../../../assets/logos/fandango-at-home.svg")),
        "prime-video" => Some(include_bytes!("../../../assets/logos/prime-video.svg")),
        "crunchyroll" => Some(include_bytes!("../../../assets/logos/crunchyroll.svg")),
        "tubi" => Some(include_bytes!("../../../assets/logos/tubi.svg")),
        _ => None,
    }
}

/// Brand styling for a service ID or display name. Falls back to neutral surface for custom sites.
pub fn brand_for(id_or_name: &str, display_name: &str) -> Brand {
    brand_for_with_logo(id_or_name, display_name, None)
}

/// Brand styling with an optional downloaded TMDB logo overriding the bundled SVG fallback.
pub fn brand_for_with_logo(id_or_name: &str, display_name: &str, tmdb_logo: Option<Image>) -> Brand {
    let id = service_id(id_or_name);
    let (fill, fill_end, ink, wordmark, sub, sub_ink): (Color, Color, Color, &str, &str, Color) =
        match id.as_str() {
            "youtube-tv" => (rgb(0xF1F1F1), rgb(0xF1F1F1), rgb(0xD50000), "YouTube", "TV", rgb(0x1F1F1F)),
            "youtube-music" => (rgb(0x181818), rgb(0x181818), rgb(0xFFFFFF), "YouTube", "Music", rgb(0xFFFFFF)),
            "youtube" => (rgb(0xF1F1F1), rgb(0xF1F1F1), rgb(0xD50000), "YouTube", "", rgb(0x1F1F1F)),
            "disney-plus" => (rgb(0x2A74D6), rgb(0x0B2E6B), rgb(0xFFFFFF), "Disney+", "", rgb(0xFFFFFF)),
            "hulu" => (rgb(0x16191E), rgb(0x16191E), rgb(0x1CE783), "hulu", "", rgb(0x1CE783)),
            "apple-tv" => (rgb(0x1C1C1E), rgb(0x1C1C1E), rgb(0xFFFFFF), "Apple TV", "", rgb(0xFFFFFF)),
            "netflix" => (rgb(0x121212), rgb(0x121212), rgb(0xE50914), "NETFLIX", "", rgb(0xE50914)),
            "fandango-at-home" => (rgb(0x1B2A5C), rgb(0x1B2A5C), rgb(0xFF8A2A), "FANDANGO", "at home", rgb(0xFFFFFF)),
            "prime-video" => (rgb(0x1A8CFF), rgb(0x1A8CFF), rgb(0xFFFFFF), "prime video", "", rgb(0xFFFFFF)),
            "crunchyroll" => (rgb(0xF47521), rgb(0xF47521), rgb(0xFFFFFF), "crunchyroll", "", rgb(0xFFFFFF)),
            "tubi" => (rgb(0x7408FF), rgb(0x7408FF), rgb(0xFFF200), "tubi", "", rgb(0xFFF200)),
            _ => (rgb(0x1B1F26), rgb(0x1B1F26), rgb(0xE8EAED), display_name, "", rgb(0xE8EAED)),
        };
    let logo = tmdb_logo.or_else(|| bundled_svg(&id).and_then(|bytes| Image::load_from_svg_data(bytes).ok()));
    let has_logo = logo.is_some();

    Brand {
        id: SharedString::from(id),
        name: SharedString::from(display_name),
        fill,
        fill_end,
        ink,
        wordmark: SharedString::from(wordmark),
        sub: SharedString::from(sub),
        sub_ink,
        logo: logo.unwrap_or_default(),
        has_logo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_id_normalizes_all_ten_names() {
        assert_eq!(service_id("YouTube TV"), "youtube-tv");
        assert_eq!(service_id("YouTube"), "youtube");
        assert_eq!(service_id("Disney+"), "disney-plus");
        assert_eq!(service_id("Hulu"), "hulu");
        assert_eq!(service_id("Apple TV"), "apple-tv");
        assert_eq!(service_id("Netflix"), "netflix");
        assert_eq!(service_id("Fandango at Home"), "fandango-at-home");
        assert_eq!(service_id("Prime Video"), "prime-video");
        assert_eq!(service_id("Crunchyroll"), "crunchyroll");
        assert_eq!(service_id("Tubi"), "tubi");
    }
}
