# tvdash — Bound the image caches (memory leak fix) (2026-11-22)

**Problem:** tvdash's memory only grows. Every poster, backdrop and logo it decodes is kept as an uncompressed `slint::Image` in a `HashMap` that is never trimmed. Over a long session (Library browsing, Home/Library shelves refreshing, Where to watch searches) this reaches GBs, Fedora starts swapping, and the desktop freezes ("crashes with barely any apps open"). On integrated graphics Slint's GPU texture copies come out of the same RAM, so the real cost is roughly double.

**Goal:** tvdash's resident memory stays flat (a few hundred MB) no matter how long it runs or how much is browsed, with no visible change in artwork.

Related: `tvdash-code-audit-findings.md` (Maintainability: "Poster cache never shrinks"; this spec replaces that line). The audit's Pass 4 memory check was never run; §6 below is that check.

---

## 0. For the coding agent: confirm these first

Write the answers here and commit them with the first change.

| # | Question | Answer |
|---|---|---|
| C1 | Baseline: VmRSS after startup, and after 10 min of scrolling Library (Movies + TV) and opening a few Show pages. Use §6 | _pending_ |
| C2 | Every place that **reads** `s.posters`, `s.backdrops`, `s.backdrop_hues`, `s.hues`, `s.sports_logos`, `s.logos` (≈24 call sites: `bridge/home.rs`, `bridge/library.rs`, `bridge/mod.rs`, show/collection code) and what it does on a miss (request again? show a placeholder?) | _pending_ |
| C3 | Whether anything other than these maps keeps decoded pixels alive long-term (e.g. `RgbaImage` kept in `UiState`, controller-side caches, `Vec<MediaItem>` with embedded bytes) | _pending_ |
| C4 | Is RSS still growing after §1–§3 with images capped? If yes, find the next grower before closing (heaptrack or `valgrind --tool=massif`) | _pending_ |

**Stop condition:** if C1 shows RSS **flat** while browsing, this isn't the leak. Report back with the numbers and the next suspect instead of making these changes.

---

## 1. What's unbounded today

| Map (`bridge::UiState` unless noted) | Filled from | Decoded size | Trimmed? |
|---|---|---|---|
| `backdrops: HashMap<(SourceKind, String), Image>` | `AppEvent::Backdrop` → `library.rs::on_backdrop`. Plex backdrop at **1300×660**, or a blurred 450 px poster | **~3.4 MB** each | Never |
| `backdrop_hues` | Same | tiny | Never |
| `posters: HashMap<(SourceKind, String), Image>` | `AppEvent::Poster` → `home.rs` (~line 146). `POSTER_HEIGHT = 480` (`controller.rs:46`) | **~0.6 MB** each | Never |
| `hues` | Same | tiny | Never |
| `sports_logos: HashMap<String, Image>` | `AppEvent::SportsLogo` (`controller.rs` ~1170), ESPN logos decoded at **source size** | 0.1–1 MB | Never |
| `logos: HashMap<String, Image>` | Service/app logos | small, fixed set | Fine as is |
| `watch_ui.rs` `posters: HashMap<(bool, u32), Image>` | TMDB `w185` posters for Where to watch | ~0.15 MB | Never |
| Controller `posters_requested`, `backdrops_requested`, `sports_logos_requested: HashSet` | Every request ever made | tiny | Never (see §3, this matters for refetch) |

Library focus (`library.rs::on_library_focus_changed`) waits 300 ms, then loads the focused item's backdrop plus its two neighbours, so backdrops pile up at ~3 per pause while browsing.

---

## 2. Changes

### 2.1 A small LRU for decoded images

Add `crates/app/src/image_cache.rs` (no new dependency needed; `lru` crate is fine too if preferred):

```rust
/// Least-recently-used map of decoded images with a fixed capacity.
pub struct ImageCache<K> {
    map: HashMap<K, (slint::Image, u64)>, // value + last-used tick
    tick: u64,
    cap: usize,
}

impl<K: Eq + Hash + Clone> ImageCache<K> {
    pub fn new(cap: usize) -> Self;
    pub fn get(&mut self, k: &K) -> Option<slint::Image>;   // bumps last-used
    pub fn peek(&self, k: &K) -> Option<&slint::Image>;     // no bump (for contains-style checks)
    pub fn contains(&self, k: &K) -> bool;
    /// Inserts and returns the keys it evicted (oldest first), so callers can
    /// clear their "requested" sets.
    pub fn insert(&mut self, k: K, v: slint::Image) -> Vec<K>;
    pub fn len(&self) -> usize;
    pub fn clear(&mut self) -> Vec<K>;
}
```

- Eviction when `len > cap`: remove the smallest tick. O(n) scan is fine at these sizes (≤ 400).
- `slint::Image` is reference-counted. Evicting from the cache does **not** blank anything already on screen: models that still hold a clone keep it alive until the row is replaced. That's the intended behavior.

### 2.2 Caps

| Cache | Cap | Worst case |
|---|---|---|
| `backdrops` (+ `backdrop_hues` evicted together) | **40** | ~136 MB |
| `posters` (+ `hues` evicted together) | **300** | ~180 MB |
| `sports_logos` | **200**, and decode resized (§2.3) | ~13 MB |
| `watch_ui` posters | **120** | ~18 MB |

Put the caps in one place (`const` in `image_cache.rs`). No config option.

### 2.3 Decode at display size

- **Sports logos:** after `image::load_from_memory`, resize to fit **128×128** (`image::imageops::FilterType::Triangle`) before `to_rgba8()`. The largest logo on screen is the Blades badge (30 px × `TvUi.u`, ≤ 60 px at 4K), so 128 is plenty.
- **Backdrops:** keep 1300×660 (it's the size the FocusBackdrop draws). Don't change the Plex request.
- **Posters:** keep 480 px tall.

### 2.4 Replace the maps

- `UiState.posters`, `backdrops`, `sports_logos` become `ImageCache<…>`; `hues`/`backdrop_hues` are removed on the same evicted keys.
- Reads go through `get` (bumps recency) where the image is about to be shown, `contains`/`peek` where code only checks presence (e.g. the prefetch filter in `on_library_focus_changed`).
- Same for `watch_ui.rs`.

---

## 3. Keep refetching working

The controller skips any key already in `posters_requested` / `backdrops_requested` / `sports_logos_requested`. Once an image is evicted it would never load again (blank artwork). Fix:

- On every eviction, the bridge sends `Command::ForgetImages { posters: Vec<(SourceKind, String)>, backdrops: Vec<(SourceKind, String)>, logos: Vec<String> }` (batched per event, skip when empty).
- The controller removes those keys from the three `*_requested` sets.
- Also remove a key from its `*_requested` set when a fetch **fails** (today a failed fetch is never retried). Optional but cheap.
- C2's call sites: on a cache miss where an image is needed, they must request it (most already send `FetchPosters` / `PrefetchBackdrop`; fill any gaps found in C2).

---

## 4. Don't evict what's on screen

LRU order already protects anything just shown. Two extra rules:

- `on_backdrop` and the Library focus path call `get` on the focused item's key every focus change, so the current backdrop is always the most recent.
- Home's hero/TV card (`home.rs::refresh_tv_card`) reads its backdrop with `get` on each refresh.

No page-leave clearing: Home and Library share backdrops, and the cap already bounds memory.

---

## 5. Tests

- [ ] `ImageCache`: insert past cap evicts the least recently used; `get` bumps recency; `peek`/`contains` don't; `insert` of an existing key updates the value and recency without evicting; returned evicted keys are in oldest-first order.
- [ ] Bridge: inserting 41 backdrops leaves 40, removes the matching `backdrop_hues` entry, and sends one `ForgetImages` with the evicted key.
- [ ] Controller: `ForgetImages` removes keys from the requested sets, so a second `PrefetchBackdrop` / `FetchPosters` for that item fetches again (use the existing fake Plex server harness).
- [ ] Sports logo decode: a 500×500 PNG comes out ≤ 128×128.

---

## 6. Memory check (before and after)

Run on the laptop:

```bash
# Live RSS of tvdash every 5 s
watch -n 5 'ps -o pid,rss,cmd -C tvdash | awk "NR==1 || /tvdash/ {printf \"%s  %.0f MB  %s\n\", \$1, \$2/1024, \$3}"'
```

Script for C1 and the after numbers:

1. Start tvdash, wait for Home to settle. Note RSS.
2. Library › Movies: hold → for 2 minutes across both shelves and Browse all. Note RSS.
3. Library › TV: same, open 5 Show pages. Note RSS.
4. Sports › Scoreboard on a busy day, then Where to watch with 5 searches. Note RSS.
5. Leave it on Home for 30 min (shelves refresh). Note RSS.

After a freeze, check whether the kernel or `systemd-oomd` killed anything:

```bash
journalctl -b -1 | grep -iE "oom|out of memory|killed process" | tail
```

---

## 7. Done when

- [ ] §6 steps 2–5 end within **+150 MB** of step 1, and repeating step 2 three times doesn't grow RSS further.
- [ ] No blank posters or backdrops after browsing far away and coming back (refetch works).
- [ ] Sports logos look the same at 1080p and 4K.
- [ ] C1–C4 answered in §0; before/after RSS numbers added to the change log.
- [ ] `cargo test` passes; changelog entry: "Fixed memory growth: image caches are now bounded."

## 8. Change log

| Date | Change |
|---|---|
| 2026-11-22 | Written. LRU caps for posters (300), backdrops (40), sports logos (200, decoded at 128 px), Where to watch posters (120); `ForgetImages` keeps refetch working; memory check script |
