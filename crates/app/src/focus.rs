//! Spatial focus navigation over mixed-size tile rectangles (TV-only v2 §5).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// Pick the nearest tile in `dir` from `tiles[from]`, preferring tiles that
/// overlap on the orthogonal axis. Returns `None` if `from` is out of bounds
/// or no tile lies in that direction (e.g. `Up` from the top row → focus TabBar).
pub fn next(tiles: &[Rect], from: usize, dir: Dir) -> Option<usize> {
    let a = *tiles.get(from)?;
    let (acx, acy) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
    tiles
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != from)
        .filter_map(|(i, b)| {
            let (bcx, bcy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            let (primary, ortho) = match dir {
                Dir::Right if b.x >= a.x + a.w - 1.0 => (b.x - (a.x + a.w), (bcy - acy).abs()),
                Dir::Left if b.x + b.w <= a.x + 1.0 => (a.x - (b.x + b.w), (bcy - acy).abs()),
                Dir::Down if b.y >= a.y + a.h - 1.0 => (b.y - (a.y + a.h), (bcx - acx).abs()),
                Dir::Up if b.y + b.h <= a.y + 1.0 => (a.y - (b.y + b.h), (bcx - acx).abs()),
                _ => return None,
            };
            let overlap = match dir {
                Dir::Left | Dir::Right => b.y < a.y + a.h && b.y + b.h > a.y,
                Dir::Up | Dir::Down => b.x < a.x + a.w && b.x + b.w > a.x,
            };
            let score = primary + ortho * if overlap { 0.5 } else { 2.0 };
            Some((i, score))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
}

/// Adjust horizontal panorama scroll so `tile` stays at least `margin` px
/// inside `[0, viewport_w]`.
pub fn panorama_scroll(tile: Rect, current_scroll: f32, viewport_w: f32, margin: f32) -> f32 {
    if tile.x - current_scroll < margin {
        (tile.x - margin).max(0.0)
    } else if tile.x + tile.w - current_scroll > viewport_w - margin {
        tile.x + tile.w - (viewport_w - margin)
    } else {
        current_scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_tiles() -> Vec<Rect> {
        // 0: Clock (178 x 496)
        // 1: Hero (760 x 496)
        // 2: Search (240 x 154)
        // 3: Library (240 x 154)
        // 4: Screensaver (240 x 154)
        // 5..8: 3 recent services + All 10 (232 x 160 each under clock+hero)
        vec![
            Rect { x: 96.0, y: 180.0, w: 178.0, h: 496.0 },
            Rect { x: 290.0, y: 180.0, w: 760.0, h: 496.0 },
            Rect { x: 1066.0, y: 180.0, w: 240.0, h: 154.0 },
            Rect { x: 1066.0, y: 350.0, w: 240.0, h: 154.0 },
            Rect { x: 1066.0, y: 520.0, w: 240.0, h: 156.0 },
            Rect { x: 96.0, y: 692.0, w: 226.0, h: 160.0 },
            Rect { x: 338.0, y: 692.0, w: 226.0, h: 160.0 },
            Rect { x: 580.0, y: 692.0, w: 226.0, h: 160.0 },
            Rect { x: 822.0, y: 692.0, w: 228.0, h: 160.0 },
        ]
    }

    #[test]
    fn home_navigation_matches_spec() {
        let t = home_tiles();
        // Right from tall hero picks the vertically-centered tile next to it (Library)
        assert_eq!(next(&t, 1, Dir::Right), Some(3));
        // Up from Library goes to Search; Left from Search goes back to hero
        assert_eq!(next(&t, 3, Dir::Up), Some(2));
        assert_eq!(next(&t, 2, Dir::Left), Some(1));
        // Down from hero goes to a service tile below it
        assert_eq!(next(&t, 1, Dir::Down), Some(7));
        // Down from Clock goes to first service tile
        assert_eq!(next(&t, 0, Dir::Down), Some(5));
        // Up from top row returns None (signals TabBar focus)
        assert_eq!(next(&t, 1, Dir::Up), None);
        assert_eq!(next(&t, 2, Dir::Up), None);
    }

    #[test]
    fn out_of_bounds_returns_none() {
        assert_eq!(next(&home_tiles(), 99, Dir::Right), None);
    }

    #[test]
    fn panorama_scroll_keeps_margin() {
        let off_right = Rect { x: 1900.0, y: 180.0, w: 384.0, h: 240.0 };
        let s = panorama_scroll(off_right, 0.0, 1920.0, 96.0);
        assert_eq!(s, 1900.0 + 384.0 - (1920.0 - 96.0));

        let off_left = Rect { x: 100.0, y: 180.0, w: 384.0, h: 240.0 };
        assert_eq!(panorama_scroll(off_left, 400.0, 1920.0, 96.0), 4.0);
    }
}
