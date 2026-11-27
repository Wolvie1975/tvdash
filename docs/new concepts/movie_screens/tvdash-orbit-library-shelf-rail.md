# tvdash — Orbit · Library landing: Shelf rail — Build spec (rev. 2026-11-27, for v1.1.0)

This replaces the Orbit **Library landing** (Movies and TV Series tabs) described in `tvdash-orbit-build.md` §5. The hero + right curve + bottom dial layout is retired **for Library only**. It looked too much like Orbit Home, which uses the same three parts and many of the same titles.

The new landing has **three columns**:

1. a **shelf rail** on the left
2. a **3 × 2 poster grid** in the middle
3. a **detail card** on the right

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **Concept 2C · Library: Shelf rail**, boards **2C · Movies** and **2C · TV Series**.

**Scope**

- Orbit only. Classic, Blades and PS5 keep their Library landings and the current `LibraryLanding.shelf1/2/3` data exactly as it is.
- Home, Browse all, the Show page, the Movie page and the Watching now overlay don't change.

The usual Orbit rules apply:

- **Hover = focus**: hover + click behaves the same as focus + OK.
- **Vertical scrolling only.**
- **Ring segments ≤ 24, then a single % arc.**
- One focus ring: 4 px `text` + 5 px `bg` gap.
- `OrbitTheme` tokens and the Outfit font.

---

## 1. Shelves (data)

Each tab has **exactly three shelves**. Browse all is **not** in the rail, because it stays in the sub-tabs.

| Tab | Rail order | Source | Ring on posters |
|---|---|---|---|
| Movies | **Continue Watching** | `s.lib_continue` (movies, in-progress), max 6 | % watched (`ring_paths(100, pct, …)`) |
| Movies | **Recently Added** | `s.lib_added_week`, falling back to `s.lib_recent`, max 6 (same as today's shelf 2) | none |
| Movies | **Recently Released** | `s.lib_recently_released` (`originallyAvailableAt:desc`), max 6 | none, or % if part-watched |
| TV Series | **Continue Watching** | Shows in progress, see §1.1, max 6 | Episodes this season (segments), see §1.1 |
| TV Series | **Start Watching** | `s.lib_start_watching` (weekly random picks, unchanged), max 6 | none; keep the 1–6 rank badge |
| TV Series | **Recently Released** | `s.lib_recently_released` (shows), max 6 | none |

All three shelves are already fetched by `load_library_shelves`, so **no new Plex queries are needed**.

### 1.1 TV · Continue Watching

Today TV shelf 1 is *Finish the season* when that list is non-empty. That list only holds shows with 1–3 episodes left, and it falls back to Continue watching otherwise. Orbit drops that switch. **Continue Watching** is built as follows:

1. Start from the TV items in `s.lib_continue` (on-deck episodes, in Plex order), one entry per **show** (group by the show's rating key, and keep the first).
2. **Ring:**
   - If the show also appears in `s.lib_finish_season`, use segments: `total_episodes` / `watched_episodes`, `gap_for_count`.
   - Otherwise use the on-deck episode's % watched.
   - If that is 0, draw a faint full ring.
3. **Meta line** under the poster:
   - Part-watched: `S2 · E13 · 31 min left`
   - Next unwatched episode: `S2 · E13 next`
   - Add `· 2 left` when the season count is known and ≤ 3.
4. **Poster:** the show poster, not the episode thumbnail.

Put this in a pure function, `orbit::library_rail::tv_continue(…) -> Vec<RailGridItem>`, with tests (§7).

### 1.2 Counts

The rail circle shows the number of items in that shelf (0–6).

An **empty shelf** stays in the rail:

- It is drawn at 50% opacity with a count of `0`.
- It is skipped by ↑ ↓.
- Hovering it does nothing.

If all three shelves are empty, the grid shows `Nothing here yet` and the card shows the `LIBRARY` fallback that `rebuild_landing` already builds.

---

## 2. Layout (1920 × 1080 base, × `TvUi.u`)

| Part | Spec |
|---|---|
| Sub-tabs | Unchanged: y 130, **Movies · TV Series · Browse all** + **Watching now · N** |
| Backdrop | Unchanged: the focused item's art, crossfading (`backdrop-a/b`), with the same left and bottom scrims |
| **Rail** | x 110, y 220. Three rows of 116 px with 14 px gaps (y 220 / 350 / 480). Each row is a 108 px circle holding the count (38 px / 700) and a 21 px label. **Selected shelf:** `text`-filled circle with `bg` ink, an accent focus ring (5 px `bg` + 3 px accent) and a `glass` pill behind the row (radius 58). **Others:** `glass` circle with a 1 px `glass-border`, label `sub` colour |
| **Grid header** | x 520, y 200: `CONTINUE WATCHING · rings = progress` (16 px / 600, letter-spacing 3, accent). The note after `·` per shelf: `rings = progress`, `newest first`, `newest release dates`, `rings = episodes this season`, `new picks every Monday` |
| **Grid** | x 520, y 238, w 760, **3 across × 2 rows**, column gap 20, row gap 30. Cell w 240: `OrbitRing` poster **190 px** (focused **210 px** + focus ring), title 19 / 600 (one line, ellipsis, max 230), meta 15 `sub`. Unused cells are empty, with no placeholders |
| **Detail card** | x 1340 → right 70, y 200, h 760, radius 44, `glass` fill + 1 px `glass-border`, padding 36 / 36 / 32. Top to bottom: <br>• round poster 180 px <br>• kicker (15 / 600, accent) <br>• title 42 / 700, up to 2 lines <br>• meta 17 <br>• summary 17 / 26, **5 lines reserved** (130 px) so nothing moves <br>• pinned to the bottom: `OrbitPlay` 84 px (ring = the item's ring) + play label 20 / 600 + one sub-line 14 |
| Hint | x 520, bottom 40: `↑ ↓ shelves · → into the grid · → again to the card · Browse all stays in the sub-tabs` |
| Quick circles | Unchanged |

**Card text:** reuse `focus_info_for` / `FocusInfoData`, with these changes for Orbit:

| Item | Kicker | Play label | Sub-line |
|---|---|---|---|
| Movie, Continue Watching | `CONTINUE WATCHING` | `Resume` | `Stopped at 2:22:14 · 1 h 36 min left` |
| Movie, other shelves | `ADDED 3 DAYS AGO` or `RELEASED <Mon D, YYYY>` | `Play` | `Never started` (or time left) |
| TV, Continue Watching | `CONTINUE WATCHING · S2 · E13 · 10 LEFT IN SEASON` | `Resume S2 · E13` or `Play S2 · E13` | `31 min left · 12 of 22 watched this season` |
| TV, Start Watching | `START WATCHING · 3 SEASONS` | `Play S1 · E1` | `Never started` |
| TV, Recently Released | `RELEASED <date>` | `Play` (resumes if started) | Season / episode count |

Add `sub-line: string` to `FocusInfoData`. The other UIs ignore it.

---

## 3. Focus and navigation

There are three zones: **rail → grid → card**. ← → moves between zones, and ↑ ↓ moves within a zone.

| From | Key | Goes to |
|---|---|---|
| Rail | ↑ ↓ | Previous / next non-empty shelf. The **grid and card switch at once** (no OK needed), and grid focus resets to item 1 |
| Rail | → | Grid, on the last-focused cell for this shelf (default: item 1) |
| Rail top | ↑ | Sub-tabs |
| Grid | ← → ↑ ↓ | Moves through the 3 × 2 cells. ← from column 1 goes back to the rail. → from column 3 goes to the card |
| Grid | OK | Movie → **Movie page** (`open-movie`). Show → **Show page** (`open-show`). Same as today |
| Card | OK on Play | Movie → resume/play (`play-key(key, true)`). Show → plays the episode named on the button |
| Card | ← | Grid, on the cell the card was showing |
| Any | Back | Grid / card → rail. Rail → sub-tabs. Sub-tabs → Home (unchanged) |

- **The card always shows the focused grid item.** While the rail has focus, the card shows item 1 of the selected shelf.
- Focus changes call `LibraryLanding.focus-changed(...)` as today, so backdrops and tints keep working. Use the 300 ms settle delay that is already in place.
- **Remember** the selected shelf and the grid cell per tab (Movies, TV) in `UiState`, for the session only. Coming back to Library restores them. A fresh start opens on **Continue Watching**, or on the first non-empty shelf.

**Hover:**

- Hovering a rail row selects that shelf, the same as ↑ ↓.
- Hovering a grid cell focuses it.
- Hovering the card's Play focuses it.
- Click = OK everywhere.

### 3.1 Focus rects (`tv_nav.rs`, `orbit_tile_rects` → `Page::Library`, landing branch only)

| Index | Rect (x, y, w, h) |
|---|---|
| 0–3 | Sub-tabs + Watching now: unchanged |
| 4–6 | Rail rows: `(110, 220 + 130·i, 380, 116)` |
| 7–12 | Grid cells: `(545 + 260·col, 238 + 280·row, 190, 190)` (circle area) |
| 13 | Card Play: `(1376, 844, 84, 84)` |

Rects for empty grid cells and empty shelves are **not** added, so indices after them shift (the same "only when visible" pattern as the Movie page).

The **Browse all** branch of `orbit_tile_rects` is unchanged.

---

## 4. Files

| File | Change |
|---|---|
| `crates/app/ui/orbit/library.slint` | Replace the landing branch (hero, curve, dial, `shelf-pair`, `dial-idx`) with `OrbitShelfRail`, `OrbitLibraryGrid` and `OrbitDetailCard`. The Browse all branch and the sub-tabs stay. Delete the dial/curve code from this file once the new branch works |
| `crates/app/ui/orbit/controls.slint` | Add the `OrbitRailRow` component (count circle + label + selected pill). Reuse `OrbitRing` / `OrbitPlay` for the grid and card |
| `crates/app/ui/state.slint` | `LibraryLanding` gets Orbit-only properties: <br>• `rail: [RailShelfData]` (title, count, enabled) <br>• `rail-idx: int` <br>• `grid: [RailGridItem]` (rating-key, title, meta, poster, has-poster, ring-lit, ring-unlit, badge) <br>• `grid-header: string` <br>Plus `sub-line` on `FocusInfoData` |
| `crates/app/src/orbit/library_rail.rs` (new) | Pure helpers: <br>• `rail_for(is_tv, shelves)` <br>• `tv_continue(continue, finish_season)` <br>• `grid_for(shelf, …)` <br>• `meta_line(item, …)` <br>Unit tests here |
| `crates/app/src/bridge/library.rs` | `rebuild_landing`: when `DashboardUi::Orbit` is active, also fill `rail`, `grid` and `grid-header` from the helpers above. **Leave `shelf1/2/3` untouched** for the other UIs. New `select_rail_shelf(idx)` swaps the grid and calls `apply_focus_item` for item 1 |
| `crates/app/src/tv_nav.rs` | Library landing rects (§3.1), zone navigation (§3), Back order, and the remembered shelf/cell per tab |
| `crates/app/src/callbacks.rs` | Wire `rail-selected(int)` and `grid-activated(int)`, reusing `open-movie` / `open-show` / `play-key` |
| `docs/status.md`, `CHANGELOG.md` | Note the new Orbit Library landing |

The `ui-orbit` feature stub (`ui/stubs/orbit.slint`) needs the new globals and properties added, so builds without Orbit still compile.

---

## 5. Commits (in order)

1. `Orbit Library: rail and grid data helpers + tests` (`orbit/library_rail.rs`, state structs)
2. `Orbit Library: shelf rail, grid and detail card layout` (Slint)
3. `Orbit Library: focus rects, zone navigation, hover`
4. `Orbit Library: TV Continue Watching with season rings`
5. `Orbit Library: remove hero/curve/dial landing`
6. Version bump to **1.1.0** (a feature change), per `GEMINI.md`

---

## 6. Things not to break

- **Classic, Blades and PS5 Library landings:** their shelf titles and data must stay the same as now. TV shelf 1 there is still Finish the season when it is non-empty.
- **Watching now:** the pill, count and overlay.
- **Browse all:** the letter column, grid, sort, `lib_scroll`.
- **The Home → Library → Movie/Show page → Back chain:** Back returns to the same shelf and cell.
- **Backdrop crossfade and tint updates on focus.**

---

## 7. Tests

`orbit/library_rail.rs`:

- `rail_for` gives the right three titles and order per tab, and counts match the shelf lengths (max 6).
- `tv_continue`:
  - gives one entry per show (two on-deck episodes of the same show → one tile)
  - uses segments when the show is in `finish_season`, else %
  - draws a faint ring at 0%
- Meta lines:
  - `S2 · E13 · 31 min left`
  - `S2 · E13 next`
  - `S2 · E13 next · 2 left`
  - movie `1 h 36 min left`
- Empty shelves are `enabled = false`. The first enabled shelf is the default.
- Rings switch to a single % arc above 24 episodes.

Per `GEMINI.md`, the user runs:

- `cargo test -j 2 -p tvdash-core`
- `cargo test -j 2 -p tvdash -- --test-threads=1`
- Live: `cargo run -j 2 -p tvdash --no-default-features --features ui-orbit -- --windowed --layout tv`
- Final live test with default features.

---

## 8. Done when

- [ ] Orbit Library › Movies shows the rail **Continue Watching · Recently Added · Recently Released**, the 3 × 2 grid and the detail card.
- [ ] Orbit Library › TV Series shows the rail **Continue Watching · Start Watching · Recently Released**.
- [ ] TV Continue Watching shows one tile per show, with episode-segment rings where the season is known.
- [ ] ↑ ↓ on the rail swaps the grid and card immediately. ← → moves rail ↔ grid ↔ card. Back goes card/grid → rail → sub-tabs.
- [ ] Hover and click work on every rail row, grid cell and the Play button.
- [ ] OK on a movie opens the Movie page, OK on a show opens the Show page, and Play in the card plays or resumes.
- [ ] Empty shelves are dimmed and skipped. All-empty shows `Nothing here yet`.
- [ ] Leaving and coming back restores the shelf and cell per tab.
- [ ] Nothing scrolls sideways. Nothing jumps when focus changes (the summary is fixed at 5 lines, the title at up to 2).
- [ ] Classic, Blades and PS5 Library landings are unchanged.
- [ ] Looks right at 1080p, 1440p and 4K in Dark, Stone and Mist.

---

## 9. Open questions (defaults in bold)

| # | Question | Default |
|---|---|---|
| 1 | More than 6 items per shelf (grid rows scrolling ↑ ↓)? | **No, 6 max**, matching today's fetch limits; Browse all covers the rest |
| 2 | Should the card also have a `Movie page` / `Show page` pill? | **No**: OK on the grid poster already opens it |
| 3 | Remember the shelf across restarts (`config.toml`)? | **No**, session only |
| 4 | Home's Up next and Library's Continue Watching show the same titles | **Accepted**: the layouts are different now |

---

## Change log

| Date | Change |
|---|---|
| 2026-11-27 | Concept 2C chosen over 2A (shelf stack) and 2B (bubble wall). Rail shelves set by Jason: Movies = Continue Watching, Recently Added, Recently Released; TV = Continue Watching, Start Watching, Recently Released |
