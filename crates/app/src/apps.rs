//! Home Apps bar and Apps page data helpers (§7, §9).

use std::collections::HashMap;

use slint::Image;
use tvdash_core::config::StreamingSite;

use crate::{AppTile, brand};

/// First `n` app ids for the Home bar: recent first, then config order, no duplicates.
pub fn apps_bar(recent: &[String], all: &[String], n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(n);
    for id in recent.iter().chain(all.iter()) {
        if out.len() == n {
            break;
        }
        if all.contains(id) && !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// Build an [`AppTile`] for a configured streaming site.
pub fn app_tile(site: &StreamingSite, logos: &HashMap<String, Image>) -> AppTile {
    let id = brand::service_id(&site.name);
    let b = brand::brand_for_with_logo(&id, &site.name, logos.get(&site.name).cloned());
    let logo_wide = site
        .logo_box
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("wide"))
        .unwrap_or_else(|| matches!(id.as_str(), "tubi" | "disney-plus" | "crunchyroll"));
    AppTile {
        id: b.id,
        name: b.name,
        logo: b.logo,
        has_logo: b.has_logo,
        logo_wide,
        color: b.fill,
        text_color: b.ink,
    }
}

/// All visible streaming sites sorted A–Z by display name (case-insensitive) for the Apps page (§9).
pub fn apps_page_tiles(sites: &[StreamingSite], logos: &HashMap<String, Image>) -> Vec<AppTile> {
    let mut visible: Vec<&StreamingSite> = sites.iter().filter(|s| !s.hidden).collect();
    visible.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    visible.into_iter().map(|s| app_tile(s, logos)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str) -> StreamingSite {
        StreamingSite {
            name: name.into(),
            url: format!("https://{}.example", brand::service_id(name)),
            ..Default::default()
        }
    }

    #[test]
    fn apps_bar_recent_first_no_dups_skips_unknown() {
        let all: Vec<String> = [
            "youtube-tv",
            "youtube",
            "disney-plus",
            "hulu",
            "apple-tv",
            "netflix",
            "fandango-at-home",
            "prime-video",
            "tubi",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let recent = vec![
            "removed-app".into(),
            "netflix".into(),
            "tubi".into(),
            "netflix".into(),
        ];
        assert_eq!(
            apps_bar(&recent, &all, 7),
            [
                "netflix",
                "tubi",
                "youtube-tv",
                "youtube",
                "disney-plus",
                "hulu",
                "apple-tv",
            ]
        );
    }

    #[test]
    fn apps_page_order_is_case_insensitive_a_to_z() {
        let sites = vec![
            site("YouTube TV"),
            site("netflix"),
            site("YouTube Music"),
            site("YouTube"),
            site("Apple TV"),
        ];
        let tiles = apps_page_tiles(&sites, &HashMap::new());
        let names: Vec<&str> = tiles.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            ["Apple TV", "netflix", "YouTube", "YouTube Music", "YouTube TV"]
        );
    }
}
