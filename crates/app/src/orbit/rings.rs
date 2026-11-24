//! SVG arc path generator for Orbit segmented and continuous rings (§1.5).

/// Lit and unlit SVG path strings for a ring of `n` segments with `lit` lit.
/// Above `SEG_MAX` (24), falls back to one continuous arc for the watched share.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RingPaths {
    pub lit: String,
    pub unlit: String,
}

pub const SEG_MAX: usize = 24;

/// Default gap between segments in degrees (`4.3°` for `<= 12`, `2.9°` for `13..=24`).
pub fn gap_for_count(n: usize) -> f32 {
    if n <= 12 { 4.3 } else { 2.9 }
}

/// Build `lit` and `unlit` SVG path strings centred on `(r, r)` with radius `r`.
pub fn ring_paths(n: usize, lit: usize, r: f32, gap_deg: f32) -> RingPaths {
    let mut out = RingPaths::default();
    if n == 0 || r <= 0.0 {
        return out;
    }
    let lit = lit.min(n);
    if n > SEG_MAX {
        let p = lit as f32 / n as f32;
        if lit > 0 {
            out.lit = arc(r, 0.0, 360.0 * p);
        }
        if lit < n {
            out.unlit = arc(r, 360.0 * p, 360.0);
        }
        return out;
    }
    let step = 360.0 / n as f32;
    for i in 0..n {
        let seg = arc(r, i as f32 * step, (i + 1) as f32 * step - gap_deg);
        if i < lit {
            out.lit.push_str(&seg);
        } else {
            out.unlit.push_str(&seg);
        }
    }
    out
}

/// One SVG arc from `a0`..`a1` degrees, clockwise from 12 o'clock, centred on `(r, r)`.
fn arc(r: f32, a0: f32, a1: f32) -> String {
    if a1 - a0 <= 0.0 {
        return String::new();
    }
    let pt = |a: f32| {
        let t = (a - 90.0).to_radians();
        (r + r * t.cos(), r + r * t.sin())
    };
    let (x0, y0) = pt(a0);
    let (x1, y1) = pt(a1.min(a0 + 359.9));
    let large = if a1 - a0 > 180.0 { 1 } else { 0 };
    format!("M {x0:.1} {y0:.1} A {r} {r} 0 {large} 1 {x1:.1} {y1:.1} ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring_has_no_paths() {
        let rp = ring_paths(0, 0, 50.0, 4.3);
        assert!(rp.lit.is_empty());
        assert!(rp.unlit.is_empty());
    }

    #[test]
    fn segmented_ring_counts_lit_and_unlit() {
        let rp = ring_paths(8, 5, 50.0, 4.3);
        assert_eq!(rp.lit.matches("M ").count(), 5);
        assert_eq!(rp.unlit.matches("M ").count(), 3);
    }

    #[test]
    fn over_24_uses_single_continuous_arcs() {
        let rp = ring_paths(25, 10, 50.0, 2.9);
        assert_eq!(rp.lit.matches("M ").count(), 1);
        assert_eq!(rp.unlit.matches("M ").count(), 1);
    }

    #[test]
    fn all_lit_has_no_unlit_and_zero_lit_has_no_lit() {
        let full = ring_paths(12, 12, 50.0, 4.3);
        assert_eq!(full.lit.matches("M ").count(), 12);
        assert!(full.unlit.is_empty());

        let empty = ring_paths(12, 0, 50.0, 4.3);
        assert!(empty.lit.is_empty());
        assert_eq!(empty.unlit.matches("M ").count(), 12);
    }
}
