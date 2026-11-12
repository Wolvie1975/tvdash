//! Watch on TV logos, bundled with the app (`assets/logos/`). Each app has a
//! version for the light themes and one for Dark (light text on dark tiles).

use std::{cell::RefCell, collections::HashMap};

use slint::Image;
use tracing::warn;

macro_rules! logos {
    ($($id:literal),* $(,)?) => {
        /// (light, dark) SVG source for an app id.
        fn svg(id: &str) -> Option<(&'static [u8], &'static [u8])> {
            match id {
                $($id => Some((
                    include_bytes!(concat!("../../../assets/logos/", $id, ".svg")),
                    include_bytes!(concat!("../../../assets/logos/", $id, "-dark.svg")),
                )),)*
                _ => None,
            }
        }
    };
}

logos!(
    "youtube-tv",
    "youtube",
    "disney-plus",
    "hulu",
    "apple-tv",
    "netflix",
    "fandango-at-home",
    "prime-video",
    "crunchyroll",
    "tubi",
);

thread_local! {
    static CACHE: RefCell<HashMap<(&'static str, bool), Option<Image>>> = RefCell::default();
}

/// The logo for app `id` (see `tvdash_core::config::APPS`), for a dark or light
/// background. `None` if the app has no logo or it doesn't parse.
pub fn logo(id: &'static str, dark: bool) -> Option<Image> {
    CACHE.with_borrow_mut(|cache| {
        cache
            .entry((id, dark))
            .or_insert_with(|| {
                let (light_svg, dark_svg) = svg(id)?;
                Image::load_from_svg_data(if dark { dark_svg } else { light_svg })
                    .inspect_err(|_| warn!(id, dark, "couldn't load a bundled logo"))
                    .ok()
            })
            .clone()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_app_has_both_logos() {
        for app in tvdash_core::config::APPS {
            for dark in [false, true] {
                let img = super::logo(app.id, dark).unwrap_or_else(|| panic!("{} (dark: {dark})", app.id));
                let size = img.size();
                assert!(size.width > size.height, "{} is a wordmark: {size:?}", app.id);
            }
        }
        assert!(super::logo("nope", true).is_none());
    }
}
