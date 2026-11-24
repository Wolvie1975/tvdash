//! Letter-rail helpers for Orbit Library › Browse all.

pub const LETTERS: [&str; 28] = [
    "All", "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L",
    "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];

/// First of the 9 visible letter slots, keeping `focused` in the middle where possible.
pub fn letter_window(focused: usize) -> usize {
    focused.saturating_sub(4).min(LETTERS.len() - 9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_window_keeps_focus_centered_and_clamped() {
        assert_eq!(letter_window(0), 0);
        assert_eq!(letter_window(20), 16);
        assert_eq!(letter_window(27), 19);
    }
}
