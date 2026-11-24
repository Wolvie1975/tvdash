# tvdash — Orbit · Library › Browse all — Build spec (rev. 2026-11-23, for v0.1.68)

An addendum to the Orbit UI (`tvdash-orbit-build.md`), written after the initial Orbit buildout. It adds the **Browse all** sub-tab of Library. It reuses the Orbit pieces already built: `OrbitTheme`, `OrbitShell`, `OrbitRing` + `rings.rs`, `OrbitPill`, `SlotList`, `FocusBackdrop`, and the watched-count cache.

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **Orbit (less square)**, boards **Browse all · TV Series** and **Browse all · Movies**.

Orbit rules apply as usual: **hover = focus**, **vertical scrolling only**, **ring segments ≤ 24, then a single % arc**, and progress is computed in Rust and cached.

---

## 1. Data rules (unchanged, all UIs)

From `tvdash-blades-ui.md`:

- **One library at a time**: TV Series or Movies, never mixed.
- **All** returns only the **first 10 titles A–Z**, limited in the Plex query rather than just in the view, with the line `First 10 of N · pick a letter to see the rest`.
- A letter (or `#`) loads **every** title under it.
- Sort uses `titleSort`, so "The …" files like Plex does: **A–Z** (`titleSort`), **Z–A** (`titleSort:desc`), **Recently added** (`addedAt:desc`). Sort is remembered per library in `config.toml`.

---

## 2. Layout

| Part | Spec |
|---|---|
| Header row | Sub-tabs (Browse all active) · divider · library switch **TV Series · Movies** (filled pill = active) · **Sort · A–Z** pill (A–Z / Z–A / Recently added, `titleSort`, remembered per library) · **All genres** pill (genre filter) |
| Letter column | x 112, y 200: 124-wide `glass` strip (radius 62), full height. Letters **All · # · A–Z** as circles; the focused letter **100 px** `text` fill / `bg` ink + `focus`, with the title count under it (`14`); neighbours 70 → 54 px, fading 15% per step; **9 visible**, the strip scrolls ↑ ↓ (`SlotList`, focus stays in the middle) |
| Label | x 290, y 214: `S · 14 SHOWS` / `S · 22 MOVIES` + `A–Z · rings show …` |
| Grid | x 290, y 260, **6 across**, 190 px `OrbitRing` posters, column gap 76, row gap 26; title 19 / 600; meta 14 `sub`. Rows scroll ↑ ↓ under the bottom `fade` |
| TV rings | Segments = episodes (≤ 24), else % arc; meta `3 seasons · 2017 · 14 of 30` / `% watched`; fully watched dimmed to 70% |
| Movie rings | Full ring = watched (dimmed 70%), arc = part-watched share (+ `40 min left`), faint = unwatched; meta `2019 · 1 h 52 m` |
| Backdrop | Focused title's art (fallback: blue glow) |
| Hint | `↑ ↓ letters · → into the grid · All shows the first 10 A–Z · a letter loads everything under it` |

Focus: the default is the **All** letter (or the last letter used, per library) · ↑ ↓ moves letters, and the grid reloads after a 300 ms pause · → into the grid · grid ← → / ↑ ↓; ← from the first column → letter column · ↑ from the letter column's top → header pills · OK on a show → Show page; OK on a movie → Resume / Play (existing movie action) · Back: grid → letter column → sub-tabs.

```rust
// src/orbit/letters.rs
pub const LETTERS: [&str; 28] = ["All", "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L",
                                 "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"];
/// First of the 9 visible letter slots, keeping `focused` in the middle where possible.
pub fn letter_window(focused: usize) -> usize { focused.saturating_sub(4).min(LETTERS.len() - 9) }
```

Tests: focus 0 → window 0; focus 20 (S) → 16; focus 27 (Z) → 19.

---

## 3. State (Slint)

Use the existing Browse all view model if Classic and Blades already have one. Otherwise, these names are a proposal:

```slint
// ui/state.slint
export struct BrowseItem { key: string, title: string, meta: string, status: string, poster: image,
                           ring-lit: string, ring-unlit: string, watched: bool, is-movie: bool }
export global Browse {
    in property <string> library;          // "tv" | "movies"
    in property <int> letter;              // index into LETTERS
    in property <[int]> letter-counts;     // titles per letter (for the focused letter's count)
    in property <[BrowseItem]> items;
    in property <string> sort-label;       // "A–Z"
    in property <string> genre-label;      // "All genres"
    in property <string> note;             // "First 10 of 214 · pick a letter to see the rest"
    callback set-library(string);
    callback set-letter(int);              // debounced 300 ms in Rust
    callback cycle-sort();
    callback pick-genre();
    callback activate(string);             // key
}
```

The ring path strings come from `rings::ring_paths` (`tvdash-orbit-build.md` §1.5). TV items use episodes watched out of total; movies use a single segment, either full or a part-watched arc.

---

## 4. Files

```text
crates/app/
├─ src/orbit/letters.rs            # LETTERS, letter_window() + tests
├─ src/bridge/browse.rs            # shared query (first 10 for All, full letter), rings, labels
└─ ui/orbit/browse.slint           # letter column + grid
```

## 5. Done when

- [ ] Browse all opens from Library's sub-tabs in Orbit; the TV Series / Movies switch shows one library at a time.
- [ ] The letter column scrolls ↑ ↓ with the focused letter in the middle; nothing scrolls sideways.
- [ ] **All** shows the first 10 A–Z with the "First 10 of N" note; a letter loads all its titles; `#` covers titles starting with a digit or symbol.
- [ ] TV rings show episodes (≤ 24 segments, else a % arc); movie rings show watched / part-watched / unwatched, with time left.
- [ ] Sort and genre pills work and are remembered per library.
- [ ] Hover gives the focus effect; OK opens the Show page or plays the movie; works by remote alone at 1080p / 1440p / 4K.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-23 | Created as a separate addendum to the Orbit spec: letter column + bubble grid for TV Series and Movies |
