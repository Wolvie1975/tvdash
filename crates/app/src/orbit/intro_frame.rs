//! Frame-by-frame math for Orbit's Ring ripple startup animation (§4.1).

use std::f32::consts::PI;

pub const LOGO: f32 = 1.8;
pub const REVEAL: f32 = 1.9;
pub const SLOTS: usize = 4;
const FIRST: f32 = 0.2;
const EVERY: f32 = 0.6;
const LIFE: f32 = 1.8;
pub const CENTRE: (f32, f32) = (960.0, 540.0);
pub const BADGE: (f32, f32, f32) = (1745.0, 125.0, 190.0); // spec default (x, y, diameter)

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ring {
    pub d: f32,
    pub opacity: f32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Frame {
    pub rings: [Ring; SLOTS],
    pub word_opacity: f32,
    pub word_scale: f32,
    pub bg_opacity: f32,
    pub trav_x: f32,
    pub trav_y: f32,
    pub trav_d: f32,
    pub trav_opacity: f32,
    pub land: f32,
    pub echo_d: f32,
    pub echo_opacity: f32,
}

fn cl(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn out(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

fn in_out(x: f32) -> f32 {
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

fn lerp(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

/// `t`: seconds since `logo` began. `reveal_at`: value of `t` when `reveal` began (`None` before).
pub fn frame(t: f32, reveal_at: Option<f32>) -> Frame {
    frame_to(t, reveal_at, BADGE)
}

/// Same as [`frame`], targeting an explicit `(cx, cy, diameter)` clock badge.
pub fn frame_to(t: f32, reveal_at: Option<f32>, badge: (f32, f32, f32)) -> Frame {
    let mut f = Frame {
        word_scale: 1.0,
        bg_opacity: 1.0,
        ..Default::default()
    };
    let r = reveal_at.map(|s| t - s);

    // Ripple rings: spawn every EVERY s from FIRST until reveal starts.
    let spawn_until = t.min(reveal_at.map_or(f32::INFINITY, |s| s - 1e-3));
    if spawn_until >= FIRST {
        let hi = ((spawn_until - FIRST) / EVERY).floor() as i32;
        let fade = r.map_or(1.0, |r| 1.0 - cl(r / 0.5));
        for n in (hi - 3).max(0)..=hi {
            let age = t - (FIRST + EVERY * n as f32);
            if !(0.0..=LIFE).contains(&age) {
                continue;
            }
            let q = age / LIFE;
            f.rings[n as usize % SLOTS] = Ring {
                d: 60.0 + 700.0 * out(q),
                opacity: 0.8 * (1.0 - q) * fade,
            };
        }
    }

    match r {
        None => {
            let w = out(cl((t - 0.4) / 0.7));
            f.word_opacity = w;
            f.word_scale = 0.94 + 0.06 * w;
        }
        Some(r) => {
            f.word_opacity = 1.0 - cl(r / 0.4);
            f.word_scale = 1.0 + 0.04 * cl(r / 0.4);
            f.bg_opacity = 1.0 - out(cl((r - 0.3) / 0.5));

            let p = in_out(cl(r / 1.0));
            f.trav_x = lerp(CENTRE.0, badge.0, p);
            f.trav_y = lerp(CENTRE.1, badge.1, p);
            f.trav_d = 120.0 + (badge.2 - 120.0) * p + 140.0 * (PI * p).sin();
            f.trav_opacity = 1.0;
            f.land = out(cl((r - 0.85) / 0.4));

            let e = cl((r - 1.0) / 0.7);
            f.echo_d = badge.2 + 150.0 * out(e);
            f.echo_opacity = if e > 0.0 && e < 1.0 { 0.6 * (1.0 - e) } else { 0.0 };
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t0_is_blank() {
        let f = frame(0.0, None);
        assert!(f.rings.iter().all(|r| r.opacity == 0.0));
        assert_eq!(f.word_opacity, 0.0);
    }

    #[test]
    fn t1_has_two_live_rings_and_wordmark() {
        let f = frame(1.0, None);
        let live = f.rings.iter().filter(|r| r.opacity > 0.0).count();
        assert_eq!(live, 2);
        assert!(f.word_opacity > 0.0);
    }

    #[test]
    fn long_hold_caps_at_four_rings() {
        let f = frame(5.0, None);
        let live = f.rings.iter().filter(|r| r.opacity > 0.0).count();
        assert!(live > 0 && live <= 4);
    }

    #[test]
    fn no_new_rings_spawn_after_reveal() {
        let rs = 1.8;
        let f = frame(rs + 0.2, Some(rs));
        // Any live ring must have spawned before rs, so at rs + 0.2 its age >= 0.2 => d > 60 + 700*out(0.2/1.8)
        let min_d = 60.0 + 700.0 * out(0.2 / LIFE);
        for r in f.rings {
            if r.opacity > 0.0 {
                assert!(r.d >= min_d - 0.5, "ring too young: d={}", r.d);
            }
        }
    }

    #[test]
    fn traveller_lands_on_badge_at_reveal_plus_one() {
        let rs = 1.8;
        let f = frame(rs + 1.0, Some(rs));
        assert!((f.trav_x - 1745.0).abs() < 0.5);
        assert!((f.trav_y - 125.0).abs() < 0.5);
        assert!((f.trav_d - 190.0).abs() < 0.5);
    }

    #[test]
    fn settled_at_reveal_plus_125() {
        let rs = 1.8;
        let f = frame(rs + 1.25, Some(rs));
        assert!((f.land - 1.0).abs() < 1e-4);
        assert_eq!(f.word_opacity, 0.0);
        assert_eq!(f.bg_opacity, 0.0);
    }

    #[test]
    fn all_ripples_done_at_end_of_reveal() {
        let rs = 1.8;
        let f = frame(rs + 1.9, Some(rs));
        assert!(f.rings.iter().all(|r| r.opacity == 0.0));
        assert_eq!(f.echo_opacity, 0.0);
    }

    #[test]
    fn skip_during_logo_keeps_frames_finite_and_lands() {
        let rs = 0.9;
        let f = frame(rs + 1.0, Some(rs));
        assert!(f.trav_x.is_finite() && f.trav_y.is_finite() && f.trav_d.is_finite());
        assert!((f.trav_x - 1745.0).abs() < 0.5);
        assert!((f.trav_y - 125.0).abs() < 0.5);
        assert!((f.trav_d - 190.0).abs() < 0.5);
    }
}
