# tvdash — Browse all: rotating 10 by default, load by letter — Change spec (rev. 2026-11-24 b, for v0.1.72)

An agent-ready change for **Library › Browse all**, in every dashboard UI (data is shared; Orbit's view is in `tvdash-orbit-browse-all.md`).

**Why:** today `controller.rs` (≈ line 1308) loads the section with a hard-coded `section_items(&sec.key, sort, None, 1000)`, and `bridge/browse.rs` filters that list by letter in memory. Libraries over 1,000 titles (e.g. 1,900 movies) lose everything past item 1,000, and the "First 10 of N" count is wrong.

**New behaviour:**

| Letter | What loads | Requests | Items in memory |
|---|---|---|---|
| **All** (default) | **The next 10 titles of a shuffled rotation through the whole section**, shown **A–Z / 0–9** by `titleSort`. Every title appears once per cycle before any repeats | 1 count + 10 tiny | 10 |
| `#`, `A`–`Z` | Every title under that letter, in the chosen Sort | 1 per 200 titles | that letter only |

No counts are hard-coded anywhere: totals come from Plex's `totalSize`.

---

## 1. `crates/media/src/plex.rs`

### 1.1 Read `totalSize` on normal responses

```rust
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct MediaContainer {
    metadata: Vec<Metadata>,
    hub: Vec<Hub>,
    directory: Vec<Directory>,
    #[serde(rename = "totalSize")]
    total_size: Option<u32>,      // sent on paged responses
}

/// One page plus Plex's total for the whole (filtered) query.
async fn metadata_page(&self, path: &str, query: &[(&str, String)], context: &str)
    -> Result<(Vec<Metadata>, Option<u32>)>
{
    let r: Container = send_json(self.request(Method::GET, path).query(query), context).await?;
    Ok((r.media_container.metadata, r.media_container.total_size))
}
```

### 1.2 Rotating 10 (the All default)

**Goal:** every title bubbles up in turn. Plain `sort=random` can repeat titles and leave others unseen, so All walks a **shuffled permutation of the whole section, without repeats**, storing only five small numbers.

How it works:

1. Sort the section by `titleSort`, so each title has a stable position `0 … total−1`.
2. Pick a random `step` with `gcd(step, total) == 1`. Then `pos(i) = (offset + step × i) mod total` visits **every** position exactly once as `i` goes `0 … total−1`, in a scrambled order.
3. Each visit takes the next 10 values of `i` and fetches each title directly with `X-Plex-Container-Start=pos&X-Plex-Container-Size=1`.
4. Save `{ section, total, step, offset, next_i }` in the state store. When `next_i` reaches `total`, the cycle is done: pick a new `step` / `offset` and start again.
5. If `total` changes (titles added or removed), start a new cycle immediately, so new titles get a fair chance and positions stay valid.

```rust
// crates/app/src/browse_rotation.rs  (pure logic, unit-tested)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Rotation { pub total: u32, pub step: u32, pub offset: u32, pub next_i: u32 }

fn gcd(mut a: u64, mut b: u64) -> u64 { while b != 0 { (a, b) = (b, a % b); } a }

impl Rotation {
    pub fn new(total: u32, rng: &mut impl rand::Rng) -> Self {
        let t = total.max(1) as u64;
        let step = loop {
            let s = rng.gen_range(1..=t.max(2) - 1).max(1);
            if gcd(s, t) == 1 { break s as u32; }
        };
        Rotation { total, step, offset: rng.gen_range(0..t) as u32, next_i: 0 }
    }
    /// The next `n` positions (fewer if the cycle ends; the caller then starts a new cycle for the rest).
    pub fn take(&mut self, n: u32) -> Vec<u32> {
        let t = self.total.max(1) as u64;
        let mut out = Vec::new();
        while out.len() < n as usize && self.next_i < self.total {
            out.push(((self.offset as u64 + self.step as u64 * self.next_i as u64) % t) as u32);
            self.next_i += 1;
        }
        out
    }
    pub fn exhausted(&self) -> bool { self.next_i >= self.total }
}
```

```rust
// crates/media/src/plex.rs
/// The title at `pos` in `titleSort` order (one tiny request).
pub async fn section_item_at(&self, section_key: &str, pos: u32, genre_key: Option<&str>) -> Result<Option<MediaItem>> {
    let mut q = vec![
        ("sort", "titleSort".to_owned()),
        ("X-Plex-Container-Start", pos.to_string()),
        ("X-Plex-Container-Size", "1".into()),
    ];
    if let Some(g) = genre_key { q.push(("genre", g.to_owned())); }
    let (meta, _) = self.metadata_page(&format!("/library/sections/{section_key}/all"), &q, "browse item").await?;
    Ok(Self::items(meta).into_iter().next())
}
```

The controller, on **All**:

1. Read `total` (the existing `section_count`, i.e. `Container-Size=0` → `totalSize`).
2. Load the saved `Rotation` for the section. If there isn't one, or its `total` differs, or it's `exhausted()`, create a new one.
3. `take(10)`; if fewer than 10 came back, start a new cycle and take the rest, skipping titles already picked this visit.
4. Fetch the positions **concurrently** (`futures::future::join_all`, at most 4–5 in flight) with `section_item_at`.
5. Save the rotation, then sort the 10 for display (§2).

> **Genre filter:** when a genre is selected, All rotates within that genre: use a separate `Rotation` per `(section, genre)` and the genre-filtered count from `metadata_page` (`section_count` ignores genre).
>
> **Cost:** 1 count + 10 single-item requests per visit, about 2 KB each; nothing kept but the five numbers. A 1,900-title library cycles fully every 190 visits.

### 1.3 One letter, paged to its own total

```rust
/// Every title whose first character is `ch` ("#" = digits and symbols), in `sort` order.
pub async fn section_letter(&self, section_key: &str, ch: &str, sort: LibrarySort, genre_key: Option<&str>)
    -> Result<Vec<MediaItem>>
{
    const PAGE: u32 = 200;
    let path = format!("/library/sections/{section_key}/firstCharacter/{}", urlencode(ch));
    let mut out = Vec::new();
    let mut start = 0u32;
    loop {
        let mut q = vec![
            ("sort", sort.plex_sort().to_owned()),
            ("X-Plex-Container-Start", start.to_string()),
            ("X-Plex-Container-Size", PAGE.to_string()),
        ];
        if let Some(g) = genre_key { q.push(("genre", g.to_owned())); }
        let (meta, total) = self.metadata_page(&path, &q, "browse letter").await?;
        let got = meta.len() as u32;
        if got == 0 { break; }                              // never loop forever
        start += got;
        out.extend(Self::items(meta));
        let done = match total {
            Some(t) => start >= t,                          // Plex told us the letter's total
            None => got < PAGE,                             // no total: a short page is the last one
        };
        if done { break; }
    }
    Ok(out)
}
```

> **Check first on the real server:** `GET /library/sections/{id}/firstCharacter` lists the letters with a `size` per letter (useful for the focused letter's count in Orbit's letter column), and `GET /library/sections/{id}/firstCharacter/S` returns the S titles and honours `sort`, `genre` and the `X-Plex-Container-*` paging. Plex files digits and symbols under `#`. If the per-letter path doesn't page or sort, fall back to `/all` with `sort=titleSort` and stop paging once titles pass the letter.

---

## 2. Sort the random 10 for display (`crates/app/src/bridge/browse.rs`)

Reuse the existing `browse_sort_key` (it mirrors Plex's `titleSort`: leading "The / A / An" dropped, case-folded):

```rust
// After the 10 rotation picks load: display order is alphabetical / numerical, whatever the Sort pill says.
items.sort_by(|a, b| browse_sort_key(&a.title).cmp(&browse_sort_key(&b.title)));
```

- Numbers sort before letters (`"1917"` before `"Alien"`). Compare digit runs numerically if the sort key doesn't already (`"2 Fast"` before `"10 Things"`).
- The **Sort** pill (A–Z / Z–A / Recently added) applies to **letters only**. On All, the hint reads `10 random picks · A–Z`.
- Note line on All: `10 of 1,900 · new picks every visit · pick a letter to see the rest` (total from the count).

---

## 3. Controller and state (`crates/app/src/controller.rs`, `bridge/library.rs`, `bridge/browse.rs`)

| Today | Change |
|---|---|
| `shelves.browse_items = plex.section_items(&sec.key, sort, None, 1000)` when the Library loads | **Remove.** Nothing loads the whole section any more |
| Letter change filters `lib_browse_all` in memory | Letter change sends a new command, e.g. `Command::BrowseLetter { section, letter, sort, genre }`, which runs the rotation (All, §1.2) or `section_letter`, then pushes `AppEvent::BrowseItems { letter, items, total }` |
| — | **Debounce** letter changes 300 ms (already in the Orbit spec) and **drop stale replies**: keep a request counter and ignore any reply that isn't the latest |
| — | **Cache per letter** for the visit (`HashMap<(section, letter, sort, genre), Vec<MediaItem>>`), cleared when Browse all closes or the library changes. Re-visiting S is instant. **All** keeps its 10 while you stay in Browse all (so leaving a letter and coming back to All doesn't skip titles); the next visit advances the rotation |
| Poster prefetch takes the first 48 of `browse_items` | Prefetch posters for the items just loaded (the 10, or the letter's first 48) |

### Things that used `browse_items`

`crate::bridge::start_watching(&shelves.browse_items)` (the Library's **Start watching** shelf: 6 random unstarted shows, reseeded each Monday) used the full list. Give it its own small query instead of the whole library:

```rust
// Unstarted shows, picked by Plex, filtered in Rust; cache the 6 keys with the ISO week number.
// GET /library/sections/{id}/all?type=2&unwatched=1&sort=random&X-Plex-Container-Size=40
// keep items with viewedLeafCount == 0, take 6
```

- Store `{ week, keys }` in the existing state store. Same week → reuse the keys (`/library/metadata/{key}` per key or one `?id=` list); new week → run the query again.
- Search the code for any other use: `rg -n "browse_items|lib_browse_all" crates/` and switch each to its own small query or the per-letter results.

---

## 4. Tests

- `Rotation`: for totals 1, 2, 10, 1900 and 1901, taking 10 at a time visits every position exactly once per cycle (collect all positions, assert sorted == 0..total); `exhausted()` after `total` picks; a changed `total` starts a new cycle.
- `section_item_at` builds `sort=titleSort`, `Container-Start=pos`, `Container-Size=1` (use the existing fake Plex server / JSON fixtures).
- `section_letter` pages: fixture with `totalSize=450` → 3 requests (200, 200, 50), 450 items; empty page → stops.
- Display sort of the random 10: `["The Zoo", "10 Things", "2 Fast", "alien", "Brave"]` → `["2 Fast", "10 Things", "alien", "Brave", "The Zoo"]`.
- Stale reply: request S then T quickly; only T's items are shown.
- Start watching: same week → same 6; next week → new query.

Run per crate, lightest first: `cargo test -j 2 -p tvdash-media`, then `cargo test -j 2 -p tvdash -- --test-threads=1` (`GEMINI.md`). Live test with `cargo run -j 2 -- --windowed --layout tv` before the version bump.

## 5. Done when

- [ ] Browse all opens on **10 titles in A–Z / 0–9 order**, different on each visit, with the real total; over repeated visits **every title shows up once before any repeats**.
- [ ] Choosing a letter loads every title under it, whatever the library size (tested with a section over 1,000).
- [ ] No hard-coded section limits remain (`rg -n "1000" crates/app/src/controller.rs` clean for library loading).
- [ ] Opening the Library no longer loads the whole section; memory stays flat as libraries grow.
- [ ] Start watching still shows 6 unstarted shows that change on Mondays.
- [ ] Works the same in Classic, Blades, PS5 and Orbit.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-24 b | All now walks a shuffled rotation (every title once per cycle) instead of `sort=random`; `Rotation` logic + `section_item_at` |
| 2026-11-24 | Created: All = 10 random (Plex `sort=random`), displayed alphabetically; letters load on demand with paging to Plex's `totalSize`; removed the 1,000-item full-section load |
