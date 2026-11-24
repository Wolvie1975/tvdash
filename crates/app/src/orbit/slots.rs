//! Fixed-slot vertical list windowing helpers (§1.7).

/// Compute the starting index for a fixed-slot window of `slots` items
/// around `focused` in a list of `total` items.
/// If `hero_third` is true, keeps `focused` roughly 1/3 down the window.
pub fn window_start(total: usize, slots: usize, focused: usize, hero_third: bool) -> usize {
    if total <= slots || slots == 0 {
        return 0;
    }
    let anchor = if hero_third { slots / 3 } else { slots / 2 };
    focused.saturating_sub(anchor).min(total - slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_list_starts_at_zero() {
        assert_eq!(window_start(3, 5, 2, false), 0);
    }

    #[test]
    fn long_list_clamps_window() {
        assert_eq!(window_start(22, 5, 0, false), 0);
        assert_eq!(window_start(22, 5, 8, false), 6);
        assert_eq!(window_start(22, 5, 21, false), 17);
    }
}
