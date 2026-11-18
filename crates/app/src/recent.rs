//! Last-3 recently opened streaming services (TV-only v2 §3.3).

pub use tvdash_core::config::RecentService as Recent;

/// Record a successful service launch: move `id` to the front, dedupe, cap at 3.
pub fn push_recent(list: &mut Vec<Recent>, id: &str, now: i64) {
    list.retain(|r| r.id != id);
    list.insert(0, Recent { id: id.to_owned(), at: now });
    list.truncate(3);
}

/// Home's three service tiles: recent first (that still exist in `fixed`),
/// padded from the fixed order up to 3.
pub fn home_three<'a>(recent: &[Recent], fixed: &'a [String]) -> Vec<&'a str> {
    let mut out: Vec<&str> = recent
        .iter()
        .filter_map(|r| fixed.iter().find(|f| **f == r.id).map(String::as_str))
        .collect();
    for f in fixed {
        if out.len() == 3 {
            break;
        }
        if !out.contains(&f.as_str()) {
            out.push(f.as_str());
        }
    }
    out
}

/// Human-readable relative time since `at` (Unix seconds).
pub fn ago(at: i64, now: i64) -> String {
    let m = (now - at).max(0) / 60;
    match m {
        0..=59 => format!("{m} min ago"),
        60..=1439 => format!("{} h ago", m / 60),
        1440..=2879 => "Yesterday".into(),
        _ => "Earlier".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed() -> Vec<String> {
        ["youtube-tv", "youtube", "disney-plus", "hulu", "netflix"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn push_recent_dedupes_moves_to_front_and_caps_at_three() {
        let mut list = Vec::new();
        push_recent(&mut list, "netflix", 100);
        push_recent(&mut list, "youtube", 200);
        push_recent(&mut list, "hulu", 300);
        push_recent(&mut list, "disney-plus", 400);
        assert_eq!(
            list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["disney-plus", "hulu", "youtube"]
        );

        // Re-opening an existing item moves it to the front without duplicating.
        push_recent(&mut list, "youtube", 500);
        assert_eq!(
            list.iter().map(|r| (r.id.as_str(), r.at)).collect::<Vec<_>>(),
            [("youtube", 500), ("disney-plus", 400), ("hulu", 300)]
        );
    }

    #[test]
    fn home_three_uses_recent_then_pads_from_fixed() {
        let fixed = fixed();
        assert_eq!(home_three(&[], &fixed), ["youtube-tv", "youtube", "disney-plus"]);

        let recent = vec![
            Recent { id: "removed".into(), at: 300 },
            Recent { id: "netflix".into(), at: 200 },
        ];
        assert_eq!(home_three(&recent, &fixed), ["netflix", "youtube-tv", "youtube"]);
    }

    #[test]
    fn ago_formats_buckets() {
        let now = 1_000_000;
        assert_eq!(ago(now - 5 * 60, now), "5 min ago");
        assert_eq!(ago(now - 3 * 3600, now), "3 h ago");
        assert_eq!(ago(now - 30 * 3600, now), "Yesterday");
        assert_eq!(ago(now - 72 * 3600, now), "Earlier");
    }
}
