//! Fixed-capacity LRU cache for decoded `slint::Image` values.

use std::{
    collections::HashMap,
    hash::Hash,
    ops::{Deref, DerefMut},
};

pub const BACKDROP_CAP: usize = 40;
pub const POSTER_CAP: usize = 300;
pub const SPORTS_LOGO_CAP: usize = 200;
pub const WATCH_POSTER_CAP: usize = 120;

/// Least-recently-used map of decoded images with a fixed capacity.
pub struct ImageCache<K> {
    map: HashMap<K, (slint::Image, u64)>,
    tick: u64,
    cap: usize,
}

impl<K: Eq + Hash + Clone> ImageCache<K> {
    pub fn new(cap: usize) -> Self {
        Self { map: HashMap::new(), tick: 0, cap }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    /// Returns a clone of the cached image and marks it most-recently-used.
    pub fn get(&mut self, k: &K) -> Option<slint::Image> {
        let t = self.next_tick();
        if let Some((img, last)) = self.map.get_mut(k) {
            *last = t;
            Some(img.clone())
        } else {
            None
        }
    }

    /// Returns a reference without updating recency.
    pub fn peek(&self, k: &K) -> Option<&slint::Image> {
        self.map.get(k).map(|(img, _)| img)
    }

    pub fn contains(&self, k: &K) -> bool {
        self.map.contains_key(k)
    }

    /// Inserts `k -> v` and returns any evicted keys (oldest first).
    pub fn insert(&mut self, k: K, v: slint::Image) -> Vec<K> {
        let t = self.next_tick();
        self.map.insert(k, (v, t));
        let mut evicted = Vec::new();
        while self.map.len() > self.cap {
            let Some(oldest) = self.map.iter().min_by_key(|(_, (_, tick))| *tick).map(|(key, _)| key.clone())
            else {
                break;
            };
            self.map.remove(&oldest);
            evicted.push(oldest);
        }
        evicted
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &slint::Image> {
        self.map.values().map(|(img, _)| img)
    }

    pub fn clear(&mut self) -> Vec<K> {
        let mut entries: Vec<(K, u64)> = self.map.drain().map(|(k, (_, t))| (k, t)).collect();
        entries.sort_by_key(|(_, t)| *t);
        entries.into_iter().map(|(k, _)| k).collect()
    }
}

/// `ImageCache` with its capacity baked into the type so `#[derive(Default)]` works.
pub struct FixedImageCache<K, const CAP: usize>(pub ImageCache<K>);

impl<K: Eq + Hash + Clone, const CAP: usize> Default for FixedImageCache<K, CAP> {
    fn default() -> Self {
        Self(ImageCache::new(CAP))
    }
}

impl<K, const CAP: usize> Deref for FixedImageCache<K, CAP> {
    type Target = ImageCache<K>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, const CAP: usize> DerefMut for FixedImageCache<K, CAP> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::{Rgba8Pixel, SharedPixelBuffer};

    fn dummy_img(r: u8) -> slint::Image {
        let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&[r, 0, 0, 255], 1, 1);
        slint::Image::from_rgba8(buf)
    }

    #[test]
    fn evicts_least_recently_used_and_respects_get_and_peek() {
        let mut cache = ImageCache::new(3);
        assert!(cache.insert("a", dummy_img(1)).is_empty());
        assert!(cache.insert("b", dummy_img(2)).is_empty());
        assert!(cache.insert("c", dummy_img(3)).is_empty());
        assert_eq!(cache.len(), 3);

        // `get` bumps "a" so "b" becomes oldest.
        assert!(cache.get(&"a").is_some());
        // `peek` and `contains` do NOT bump "b".
        assert!(cache.peek(&"b").is_some());
        assert!(cache.contains(&"b"));

        let evicted = cache.insert("d", dummy_img(4));
        assert_eq!(evicted, vec!["b"]);
        assert_eq!(cache.len(), 3);
        assert!(!cache.contains(&"b"));
        assert!(cache.contains(&"a"));
        assert!(cache.contains(&"c"));
        assert!(cache.contains(&"d"));

        // Updating an existing key updates recency without evicting.
        assert!(cache.insert("c", dummy_img(33)).is_empty());
        assert_eq!(cache.len(), 3);

        // Next insert evicts "a" (now oldest after "c" was updated).
        let evicted2 = cache.insert("e", dummy_img(5));
        assert_eq!(evicted2, vec!["a"]);

        // Clear returns remaining keys in oldest-first order: d, c, e.
        assert_eq!(cache.clear(), vec!["d", "c", "e"]);
        assert!(cache.is_empty());
    }
}
