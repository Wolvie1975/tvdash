/// Season strip stacking helper (PS5 §5.1):
/// Rows of 4, max 2 rows. When `total > 8`, shows the first 7 seasons
/// (replacing slot 6 with `selected` if `selected >= 7`) plus `AllSeasons(total)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Season(usize),
    AllSeasons(usize),
}

pub fn strip(total: usize, selected: usize) -> Vec<Slot> {
    if total <= 8 {
        return (0..total).map(Slot::Season).collect();
    }
    let mut slots: Vec<Slot> = (0..7).map(Slot::Season).collect();
    if selected >= 7 && selected < total {
        slots[6] = Slot::Season(selected);
    }
    slots.push(Slot::AllSeasons(total));
    slots
}

pub fn rows(slots: usize) -> usize {
    slots.div_ceil(4)
}

pub fn grid_top(slots: usize) -> f32 {
    let r = rows(slots) as f32;
    244.0 + r * 116.0 + (r - 1.0) * 18.0 + 40.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_and_rows_match_spec() {
        assert_eq!(rows(3), 1);
        assert_eq!(rows(6), 2);
        assert_eq!(strip(8, 0).len(), 8);

        let s12 = strip(12, 9);
        assert_eq!(s12.len(), 8);
        assert_eq!(s12[6], Slot::Season(9));
        assert_eq!(s12[7], Slot::AllSeasons(12));
    }
}
