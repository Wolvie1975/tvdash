//! Shuffled, repeat-free rotation through a Plex library section.
//!
//! Stores `{ total, step, offset, next_i }` where `gcd(step, total) == 1`, so
//! `pos(i) = (offset + step * i) mod total` visits every index `0..total`
//! exactly once per cycle before any repeats.

use rand::Rng;
use serde::{Deserialize, Serialize};
use tvdash_core::state::BrowseRotation;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rotation {
    pub total: u32,
    pub step: u32,
    pub offset: u32,
    pub next_i: u32,
}

impl From<BrowseRotation> for Rotation {
    fn from(r: BrowseRotation) -> Self {
        Self {
            total: r.total,
            step: r.step,
            offset: r.offset,
            next_i: r.next_i,
        }
    }
}

impl From<&Rotation> for BrowseRotation {
    fn from(r: &Rotation) -> Self {
        Self {
            total: r.total,
            step: r.step,
            offset: r.offset,
            next_i: r.next_i,
        }
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Rotation {
    pub fn new(total: u32, rng: &mut impl Rng) -> Self {
        let t = total.max(1) as u64;
        let step = loop {
            let s = rng.gen_range(1..=t.max(2) - 1).max(1);
            if gcd(s, t) == 1 {
                break s as u32;
            }
        };
        Self {
            total,
            step,
            offset: rng.gen_range(0..t) as u32,
            next_i: 0,
        }
    }

    /// Next `n` positions (fewer if the cycle ends; caller starts a new cycle for the rest).
    pub fn take(&mut self, n: u32) -> Vec<u32> {
        let t = self.total.max(1) as u64;
        let mut out = Vec::new();
        while out.len() < n as usize && self.next_i < self.total {
            out.push(((self.offset as u64 + self.step as u64 * self.next_i as u64) % t) as u32);
            self.next_i += 1;
        }
        out
    }

    pub fn exhausted(&self) -> bool {
        self.next_i >= self.total
    }
}

/// Take up to `n` positions from `rot`, starting a fresh cycle if needed and
/// skipping any positions already picked on this visit.
pub fn take_visit_positions(
    rot: &mut Option<Rotation>,
    total: u32,
    n: u32,
    rng: &mut impl Rng,
) -> Vec<u32> {
    if total == 0 {
        *rot = None;
        return Vec::new();
    }
    let need_new = rot
        .as_ref()
        .is_none_or(|r| r.total != total || r.exhausted());
    if need_new {
        *rot = Some(Rotation::new(total, rng));
    }
    let want = n.min(total);
    let mut picked = rot.as_mut().expect("rotation").take(want);
    if picked.len() < want as usize {
        let mut fresh = Rotation::new(total, rng);
        while picked.len() < want as usize && !fresh.exhausted() {
            let next = fresh.take(1);
            if let Some(&p) = next.first()
                && !picked.contains(&p)
            {
                picked.push(p);
            }
        }
        *rot = Some(fresh);
    }
    picked
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn rotation_visits_every_position_once_per_cycle() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        for total in [1u32, 2, 10, 1900, 1901] {
            let mut rot = Rotation::new(total, &mut rng);
            let mut seen = Vec::new();
            while !rot.exhausted() {
                seen.extend(rot.take(10));
            }
            assert!(rot.exhausted());
            assert_eq!(seen.len(), total as usize, "total={total}");
            seen.sort_unstable();
            assert_eq!(seen, (0..total).collect::<Vec<_>>(), "total={total}");
        }
    }

    #[test]
    fn changed_total_starts_new_cycle() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut rot = Some(Rotation::new(100, &mut rng));
        let first = take_visit_positions(&mut rot, 100, 10, &mut rng);
        assert_eq!(first.len(), 10);
        assert_eq!(rot.as_ref().unwrap().next_i, 10);

        let after_resize = take_visit_positions(&mut rot, 105, 10, &mut rng);
        assert_eq!(after_resize.len(), 10);
        assert_eq!(rot.as_ref().unwrap().total, 105);
        assert_eq!(rot.as_ref().unwrap().next_i, 10);
    }
}
