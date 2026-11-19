# tvdash — Library landing (Option C) + Show page (vertical) + Screensaver (Poster wall) — Build spec

Applies to the **TV-only (One-screen) Tile Hub**, from `v0.1.32` (branch `tv-tile-hub-v2`). Replaces the current `TvLibrary` smart-shelves landing, the `TvShow` layout and the `TvAmbient` screensaver. Browse All, Collections, Search and Settings are unchanged.

**Design canvas:** https://claude.ai/artifact/6gc5fgjMpZS4kXkGzMkiXV (**Chosen** page for Library + Show; **Screensaver** page, board 3, for the screensaver)

| Board | What it shows |
|---|---|
| Library · TV Series (Option C) | Focus backdrop, Finish the season, Start watching |
| Library · Movies (Option C) | Focus backdrop, Continue watching, Recently added |
| Show page · whole page | Full vertical page with the 1080 fold marked |
| Show page · scrolled to up next | Sticky bar, season column showing the focused episode |
| Screensaver 3 · Poster wall | Drifting poster wall, large thin clock, Up next block |

All sizes are **1920 × 1080 design units**, scaled by `TvUi.u` as everywhere else. Safe area 96 / 54, gutters 16, tile radius 6, focus = 4 px `text-primary` outline + `0 20 60 #0009` shadow.

---

## 0. Decisions (2026-11-19)

- **Landing = Option C "Shaped shelves"** for both tabs: a backdrop + text block at the top that **follow focus**, then two shelves with different shapes.
- **TV Series:** Finish the season (16:9 tiles) → **Start watching** (6 unstarted shows, numbered, **reshuffled every Monday**).
- **Movies:** Continue watching (16:9 tiles) → **Recently added** (6 newest, numbered).
- **Watching now** leaves the shelf list and becomes a small live **pill** next to the sub-tabs.
- **The text block always reserves 3 summary lines**, so the shelves never move. On the numbered rows it's the only place to read what a title is about, and that's intentional.
- **Show page = Show 1, vertical**: backdrop top, action tiles, season tabs, then a **season column with the season's own poster** beside a **3-column episode grid** that scrolls down. No sideways scrolling anywhere.
- **Screensaver = Poster wall** with the **Up next block** added (poster, label, title, episode line, progress bar), bottom right opposite the clock.

---

## 1. Shared pieces

### 1.1 `FocusBackdrop` (new component)

The big artwork at the top right of the Library landing. It shows the **focused item's backdrop**, not its poster.

| Rule | Value |
|---|---|
| Area | x `620`, y `0`, `1300 × 660` (bleeds off the right and top edges) |
| Image | Plex `art` (16:9), via `/photo/:/transcode?width=1300&height=660&url=…` |
| Fallback 1 | No `art` → the poster (`thumb`), `image-fit: cover`, heavily blurred |
| Fallback 2 | No poster → a gradient in the item's tint (`poster` → `card`) |
| Scrims | Left: `bg` 0% → `bg @ 80%` 22% → transparent 60%. Bottom: `bg` → transparent over the lower 55% |
| Debounce | Swap **300 ms after focus settles** (holding an arrow doesn't flicker through every title) |
| Crossfade | **250 ms** ease-in-out between two image layers |
| Prefetch | Load the backdrops of the focused tile's left and right neighbours |
| Focus leaves the shelves | (tabs, sub-tabs, pill) → keep the last item, don't blank |
| Tint | The sub-tab underline and kicker colour follow the focused item's tint |

```slint
// ui/tv/focus_backdrop.slint  (sketch)
export component FocusBackdrop inherits Rectangle {
    in property <image> image-a;
    in property <image> image-b;
    in property <bool> show-b;          // Rust flips this to crossfade
    clip: true;

    Image {
        source: root.image-a; width: 100%; height: 100%; image-fit: cover;
        opacity: root.show-b ? 0 : 1;
        animate opacity { duration: 250ms; easing: ease-in-out; }
    }
    Image {
        source: root.image-b; width: 100%; height: 100%; image-fit: cover;
        opacity: root.show-b ? 1 : 0;
        animate opacity { duration: 250ms; easing: ease-in-out; }
    }
    // left scrim
    Rectangle {
        background: @linear-gradient(90deg, Theme.bg 0%, Theme.bg.with-alpha(0.8) 22%, transparent 60%);
    }
    // bottom scrim
    Rectangle {
        y: parent.height * 0.45; height: parent.height * 0.55;
        background: @linear-gradient(0deg, Theme.bg 0%, transparent 100%);
    }
}
```

```rust
// Debounce: restart a single-shot timer on every focus change.
// Keep `backdrop_timer` alive (e.g. in the Library controller struct).
fn on_library_focus_changed(&self, item: FocusItem) {
    let weak = self.ui.as_weak();
    self.backdrop_timer.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_millis(300),
        move || {
            if let Some(ui) = weak.upgrade() {
                // 1. resolve image (art → blurred thumb → tint gradient), from cache if present
                // 2. write it into the hidden layer (image-a or image-b)
                // 3. flip show-b; update kicker / title / meta / summary / tint
            }
        },
    );
}
```

> **Note:** Text (kicker, title, meta, summary) updates on the same debounced tick as the image, so words and picture always match.

### 1.2 `FocusInfo` text block (top left)

| Element | Spec |
|---|---|
| Position | x `96`, y `222`, width `820` |
| Kicker | 20 px, weight 500, uppercase, 1.2 px tracking, tint `accent` |
| Title | 76 px, weight 400, line-height 1, −1 px tracking, one line, elide |
| Meta | 24 px, `#C9CDD3`-ish (`text-secondary` on art), one line |
| Summary | 22 px, line-height 1.45, `text-secondary`, max width 720, **fixed height = 3 lines (~96 px)**, word-wrap + elide |
| Gaps | 14 px between rows |

```slint
Text {
    text: root.summary;
    font-size: 22px;                 // design px; scaled by TvUi.u like everything else
    wrap: word-wrap;
    overflow: elide;
    vertical-alignment: top;
    height: 3 * 22px * 1.45;         // reserve 3 lines even when the summary is short
}
```

> **Check:** confirm multi-line `elide` with `word-wrap` behaves on Slint 1.18; if not, trim the string to ~3 lines in Rust before binding.

**Kicker text by shelf:**

| Shelf | Kicker | Meta |
|---|---|---|
| Finish the season | `Season 3 · Episode 7 next` | `2 episodes left · Western · TV-14 · 2023` |
| Start watching | `Start watching · 3 seasons` | `Year · genre · rating · N episodes` |
| Continue watching | `35 min left` | `2022 · Thriller · 1 h 58 m · R` |
| Recently added | `Added 2 days ago` | `Year · genre · runtime · rating` |

**No summary?** Show the genre and the first two cast names in the summary space (`Drama · with A. Name, B. Name`). Never leave it empty.

---

## 2. Library landing (`TvLibrary`, Browse All **off**)

### 2.1 Layout (first screen)

| Element | Position / size |
|---|---|
| Top chrome | Unchanged (`TabBar` left, weather + clock right). Give the weather/clock a `bg @ 60%` chip so it reads over art |
| Sub-tabs | y `138`: **TV Series · Movies** · **Browse all** pill · **Watching now** pill |
| FocusBackdrop | §1.1 |
| FocusInfo | §1.2 |
| Shelf 1 label | y `496`, 22 px uppercase `text-secondary`; right side count, 20 px `text-muted` |
| Shelf 1 tiles | y `536`, **4 × `420 × 236`**, gap 16 (16:9 wide tiles) |
| Shelf 2 label | y `808` |
| Shelf 2 row | y `848`, **6 items × `284` wide**, gap 8 (numbered, §2.4) |

### 2.2 Watching now pill

- Next to Browse all: 44 px tall, radius 22, `bg @ 70%` + 1 px `border`, green dot (`ok`) + `2 watching now`.
- When nothing is playing: **grey dot + `Nothing streaming`**. The pill never hides, so the sub-tab row doesn't shift.
- Data: `GET /status/sessions` every **10 s while Library is open** (existing poller).
- OK → **Watching now overlay** (§2.2a). With nothing streaming, OK does nothing (or shows the overlay's empty state).

### 2.2a Watching now overlay

- Centred panel over `scrim`, `surface`, radius 6, width `960`, max 6 rows then scroll inside the panel.
- Header: green dot + **WATCHING NOW** (22 px uppercase) · right `3 streams · all sections`.
- One **84 px row per stream** (same content as the laptop design, `tvdash-laptop-current.md` §4):
  - poster `44 × 66`, title + `S2 · E7` or `Movie`;
  - line 2: player + app (`Living room TV · tvdash (mpv)`) and chips **This TV** (tint fill) · **Remote** · **Direct play** / **Transcode 720p**;
  - right: **Playing** / **Paused** + time left; 5 px progress bar along the bottom.
- Rows are focusable: OK → that title's Show page (or movie). No remote control of other devices.
- Back / Esc closes it and returns focus to the pill. Refreshes with the 10 s poller while open.
- Empty: `Nothing is streaming from Republic City`, grey dot.

### 2.3 Shelf 1 — 16:9 progress tiles

| | TV Series: **Finish the season** | Movies: **Continue watching** |
|---|---|---|
| Rule | Season `leafCount − viewedLeafCount ≤ 3` and `> 0` (existing) | Movies with `viewOffset > 0` (existing) |
| Order | Most recently watched first | `lastViewedAt` desc |
| Tile art | Show `art` (16:9) | Movie `art` |
| Bottom-left | Title, 26 px | Title, 26 px |
| Bottom-right | `2 left`, 18 px | `35 min left`, 18 px |
| Bar | One segment per episode in the season, 5 px, gap 4 | One continuous 5 px bar |
| Scrim | Bottom: `bg @ 90%` → transparent at 60% | Same |
| OK | Plays the next episode (as today) | Resumes from `viewOffset` |

### 2.4 Shelf 2 — numbered row

Each item is `284 × 225`:

- **Numeral** `1`–`6`: 230 px, weight 600, line-height 1, `color: transparent` with a **3 px stroke `#3A4049`**, −12 px tracking, anchored bottom-left (overhangs 30 px below the row).
- **Poster** 2:3 `150 × 225`, right-aligned over the numeral, radius 6, shadow `-10 0 24 bg` so it separates from the digit. Title bottom-left on the poster (18 px, 600) only if no artwork.

> **Slint stroke text (decided: native).** Slint's `Text` has `stroke`, `stroke-width` and `stroke-style`, so no SVGs are needed:
>
> ```slint
> Text {
>     text: rank;                 // "1" … "6"
>     font-size: 230px;
>     font-weight: 600;
>     letter-spacing: -12px;
>     color: transparent;         // outline only
>     stroke: #3A4049;
>     stroke-width: 3px;
>     stroke-style: outside;
> }
> ```
>
> **Check first:** draw one stroked digit and look at it on the TV with the renderer tvdash actually uses (FemtoVG or Skia). If the outline is missing or jagged there, fall back to SVG digits (`assets/rank/1.svg` … `6.svg`). Don't fake it with a filled grey digit; the outline is the look.

| | TV Series: **Start watching** | Movies: **Recently added** |
|---|---|---|
| Rule | Shows with `viewedLeafCount == 0` | All movies |
| Pick | **Random 6, reshuffled every Monday** (§2.5) | **6 newest** by `addedAt` desc |
| Label right | `Not started yet · new picks every Monday` | `Newest first · 9 this week` |
| OK | Opens `TvShow` | Starts playback (as today) |
| Request | From the cached show list | `/library/sections/{movies}/all?type=1&sort=addedAt:desc&X-Plex-Container-Start=0&X-Plex-Container-Size=6` |

### 2.5 Start watching — weekly seeded shuffle

```toml
# Cargo.toml
[dependencies]
rand = "0.8"
rand_chacha = "0.3"
chrono = "0.4"
```

```rust
use chrono::{Datelike, Local};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Same 6 shows all week; changes on Monday. No state in config.toml.
pub fn start_watching(mut pool: Vec<Show>) -> Vec<Show> {
    pool.retain(|s| s.viewed_leaf_count == 0);
    pool.sort_by_key(|s| s.rating_key);                     // stable base order
    let w = Local::now().iso_week();
    let seed = ((w.year() as u64) << 8) | w.week() as u64;
    pool.shuffle(&mut ChaCha8Rng::seed_from_u64(seed));
    pool.truncate(6);
    pool
}
```

- Starting a show mid-week drops it from the pool; the next show in that week's order fills the slot.
- Fewer than 6 unstarted → show what there is. None → hide the shelf.
- `rand_chacha` (not `StdRng`) so a `rand` upgrade can't change the order mid-week.

### 2.6 Focus & navigation

| From | Key | To |
|---|---|---|
| Page open | — | First tile of Shelf 1 (backdrop shows it) |
| Shelf 1 | ↑ | Sub-tabs |
| Sub-tabs row | → past Browse all | Watching now pill |
| Shelf 1 | ↓ | Same column in Shelf 2 (clamp to nearest) |
| Shelf 2 | ↑ | Shelf 1 |
| Any shelf | ← / → | Neighbour; no wrap |
| Switch tab (TV ↔ Movies) | — | Focus resets to Shelf 1, first tile |

### 2.7 Empty states

| Case | Behaviour |
|---|---|
| Shelf 1 empty | Hide it; Shelf 2 moves up to y `536` |
| Both empty | FocusInfo shows `Nothing in progress` + `Browse all 214 shows` button tile |
| Item has no `art` | §1.1 fallbacks |

### 2.8 Removed from the landing (data layer stays)

- Watching now shelf (→ pill), Recently added on TV, Popular on Republic City, Watch again.
- **Decided:** the landing stays at **two shelves**, with nothing below the fold. Keep these queries in Rust only if something else uses them (e.g. Popular for Settings › Server); otherwise remove them.
- **Decided:** no Collections teaser on Movies. Collections stays its own tab.

---

## 3. Show page (`TvShow`)

### 3.1 Layout

| Element | Position / size |
|---|---|
| Top chrome | `← Back` pill + tabs (existing) |
| Backdrop | x `560`, y `0`, `1360 × 720`, show `art`; left scrim (`bg` → `bg @ 82%` 25% → transparent 62%) and bottom scrim from y `380` |
| Kicker | y `160`: `TV series · Plex`, 20 px tint |
| Title | 88 px, weight 400, −1.5 px tracking |
| Meta | 24 px: `2022–2025 · Drama · TV-14 · 2 seasons · 18 of 20 watched` |
| Summary | 22 px / 1.45, max 780 wide, 3 lines |
| Action row | y `452`, height 120: **Resume** `440` (tint, 5 px progress along the bottom) · **From S1 · E1** `260` · **Shuffle** `220`, gap 16 |
| Season tabs | y `620`, 26 px; **existing selector rules** (tabs 1–6 seasons, pills 7–14, picker 15+; Specials last, never "S0"); right side `10 episodes · 7 h 52 m` |
| Season column | x `96`, y `700`, width `260` (§3.2) |
| Episode grid | x `388`, y `700`, width `1436`, **3 columns**, col gap 16, row gap 40 (§3.3) |

Initial focus: **Resume**. The page opens on the season that holds the up-next episode (existing rule).

### 3.2 Season column (new)

| Element | Spec |
|---|---|
| Season poster | `260 × 390`, radius 6, shadow `0 16 40 #0008` |
| Name | `Season 2`, 28 px |
| Meta | `2024 · 10 episodes · 7 h 52 m`, 18 px |
| Progress | 5 px bar + `8 of 10`, 17 px |
| Summary | 18 px / 1.45, `text-muted`, clamp 4 lines |
| Changing season | The whole column swaps to the new season |

**Season poster data**

| Need | Source |
|---|---|
| Poster | Season `thumb` from `GET /library/metadata/{show}/children` |
| Fallback | If season `thumb` **equals the show's `thumb`**, or is missing → show poster with a `SEASON N` label over a bottom scrim |
| Year | Season `year`; if missing, the year of its first episode's `originallyAvailableAt` |
| Summary | Season `summary`; if empty, hide the line |
| Runtime | Sum of episode `duration` |

### 3.3 Episode grid

Tile `468 × 263` (16:9), then text below:

| Element | Spec |
|---|---|
| Watched | Green tick in a 34 px dark circle, top right |
| Up next | `UP NEXT` chip top left (tint fill, dark text) + 2 px tint outline when not focused |
| Progress | 5 px tint bar along the bottom (started episodes) |
| Line 1 | Number 22 px `text-muted` (`09`; specials `S1`, `S2`…) + title 24 px |
| Line 2 | `48 min · 22 min left` / `· watched` / `· season finale`, 18 px |
| Last slot | Dashed `Continue to Season 3 ›` (or `Specials ›`); OK switches season and focuses its first episode |
| OK | Play / resume that episode (existing mpv path) |

### 3.4 Scrolled state

Triggered when focus moves **down into the grid**:

1. Backdrop + top block fade out (opacity → 0, 250 ms); a faint tint glow stays top right.
2. **Sticky bar** appears under the tabs at y `132`, `1728 × 76`, `surface @ 95%`, 1 px `border`, radius 6:
   `Show title (28)` · divider · **mini season poster `36 × 54`** · compact season tabs (22 px) · **Resume S2 · E9** button right (tint, 52 tall).
3. **Season column sticks** at y `244`. Under the poster it now shows the **focused episode**: kicker `Episode 9` (tint), title 26 px, meta 18 px, summary 18 px.
4. Scroll so the focused row's top is always ≥ 32 px below the sticky bar.
5. Focus back up to the first grid row → ↑ goes to the season tabs; the page scrolls to the top and the backdrop fades back in.

### 3.5 Focus & navigation

| From | Key | To |
|---|---|---|
| Resume / From S1·E1 / Shuffle | ← → | Each other |
| Action row | ↓ | Season tabs (current season) |
| Season tabs | ← → | Switch season (column + grid update immediately) |
| Season tabs | ↓ | Up-next episode if it's in this season, else the first episode |
| Grid | ← → ↑ ↓ | Neighbours, 3 across; ↑ from row 1 → season tabs |
| Sticky bar | Reachable with ↑ from the top visible row once scrolled | Tabs / Resume |

### 3.6 Removed

- The left hero info card (`520 × 320`) and the right-side `372 × 236` 3-column grid.

---

## 4. Screensaver (`TvAmbient`) — Poster wall

### 4.1 Behaviour (unchanged rules)

| Rule | Value |
|---|---|
| Start | **5 quiet minutes on Home**, or OK on the Home **Screensaver** tile |
| Never starts | While video plays full screen in mpv, or while a streaming app (Brave) is open |
| Wake | Any remote button, key or mouse move. **The waking press does nothing else.** Returns to where you were |
| Theme | **Always dark** (`#050608`), whatever the theme |
| Burn-in | The foreground (clock, Up next, footer) moves on a slow random walk, up to **±8 px**, one step per minute. The wall is always moving anyway |

### 4.2 Layout

| Layer | Spec |
|---|---|
| Poster wall | **12 columns** of 2:3 posters, about `206 × 300`, gap 20, rotated **−10°**, **opacity 0.42**, oversized (`2700 × 2300`) so the edges never show |
| Wall motion | Drifts **upward ~12 px/s**, looping seamlessly (rows that leave the top re-enter at the bottom) |
| Vignette 1 | Radial, `1300 × 900` at 22% / 82%: `#050608F2` → `B0` 45% → `40` (behind the clock) |
| Vignette 2 | Radial, `1000 × 700` at 80% / 86%: `#050608E0` → `80` 50% → transparent (behind Up next) |
| Bottom fade | Linear: `#050608` → transparent over the bottom 34% |
| Clock | x `120`, bottom `150`: **260 px, weight 200**, −8 px tracking, tabular numerals; AM/PM 52 px weight 300 `text-secondary`. Follows Settings › Clock (12 h / 24 h; no AM/PM in 24 h) |
| Date + weather | Under the clock, 32 px weight 300 `#C9CDD3`: `Thursday, November 19 · 58° and clear` (units follow Settings › Temperature) |
| Up next block | Right `120`, bottom `150`, lined up with the bottom of the clock (§4.3) |
| Footer | Left `128` / right `120`, bottom `64`: 1 px `#FFFFFF14` hairline, then right-aligned `214 shows · 486 movies on Republic City`, 20 px `#6E747D` |

### 4.3 Up next block

| Element | Spec |
|---|---|
| Poster | `120 × 180`, radius 6, shadow `0 16 40 #000A` |
| Label | Dot + text, 20 px uppercase, 1.4 px tracking, weight 500, **tint** colour |
| Title | 56 px, weight 400, `text-primary` |
| Line | 26 px, `text-secondary` |
| Bar | `560 × 5`, track `#FFFFFF1A`, fill **tint** |
| Gap | 32 px between poster and text; 12 px between text rows |

**States**

| State | Label | Title | Line | Bar |
|---|---|---|---|---|
| Playback paused behind Home | `Paused · Plex` | Show / movie | `S2 · E9 — The Breakwater · stopped at 26:10` | Yes |
| Something in progress (top of Continue watching) | `Up next · Plex` | Show / movie | `S2 · E9 — The Breakwater · 22 min left` | Yes |
| Nothing in progress | `Start watching` | One of this week's six (§2.5), rotating each time the screensaver starts | `3 seasons · Drama · 2023` | No |

The tint comes from that item's poster (same Okhsl tint as everywhere else).

### 4.4 Data & refresh

| What | Source | Refresh |
|---|---|---|
| Wall posters | Random mix of show + movie `thumb`s from the cached library, **96 unique**, reshuffled each time the screensaver starts | On start |
| Poster images | `plex_image(thumb, 206, 300)` (×2 at 4K) | Cached |
| Clock | Local time | Every minute, 400 ms crossfade on the digits |
| Weather | Existing Open-Meteo data | Every 15 min (existing poller) |
| Up next | mpv state + `/hubs/continueWatching` | On start, then every 5 min |
| Library count | Section `totalSize` | On start |

### 4.5 Performance note

Don't animate 96 separate rotated images every frame on the laptop's GPU. **Render the wall once into a single image in Rust** (e.g. with `tiny-skia`: draw the posters, rotate, apply the 0.42 opacity), make it taller than the screen by one full repeat, and animate only its `y` in Slint, wrapping when one repeat has scrolled past. If Slint 1.18's `transform-rotation` on a `Rectangle` performs well on the real hardware, the live version is fine too. Test both at 4K.

```slint
// Sketch: animate a pre-rendered wall image
export component PosterWall inherits Rectangle {
    in property <image> wall;          // pre-rendered, rotated, 2 × repeat-height tall
    in property <length> repeat-height;
    property <length> offset: 0px;
    clip: true;
    background: #050608;

    Image {
        source: root.wall;
        y: -mod(root.offset, root.repeat-height);
        width: parent.width; height: root.repeat-height * 2;
        image-fit: cover;
    }
    Timer {
        interval: 33ms;                // ~30 fps is plenty at 12 px/s
        running: true;
        triggered => { root.offset += 0.4px; }
    }
}
```

> **Check:** the `Timer` element exists from Slint 1.6; if you'd rather drive it from Rust, update `offset` from a `slint::Timer` instead. If `mod()` won't take lengths, wrap `offset` back to 0 in the timer once it passes `repeat-height`.

### 4.6 Removed

- The old `TvAmbient` layout (dimmed single backdrop + 180 px clock).

---

## 5. Rust / Slint changes

### 5.1 Models (Plex fields to make sure you have)

| Struct | Fields |
|---|---|
| `Show` | `rating_key`, `title`, `year`, `end_year`, `summary`, `thumb`, `art`, `genres`, `content_rating`, `child_count`, `leaf_count`, `viewed_leaf_count`, `added_at` |
| `Season` | `rating_key`, `index`, `title`, `thumb`, `summary`, `year`, `leaf_count`, `viewed_leaf_count` |
| `Episode` | `rating_key`, `index`, `title`, `summary`, `thumb`, `duration`, `view_offset`, `view_count`, `originally_available_at` |
| `Movie` | `rating_key`, `title`, `year`, `summary`, `thumb`, `art`, `duration`, `view_offset`, `last_viewed_at`, `added_at`, `genres`, `content_rating` |

### 5.2 Slint state

```slint
// Library landing
export struct FocusInfo {
    kicker: string, title: string, meta: string, summary: string,
    tint-accent: color,
}
export global LibraryLanding {
    in-out property <FocusInfo> focus;
    in property <image> backdrop-a;
    in property <image> backdrop-b;
    in property <bool> backdrop-show-b;
    in property <int> watching-now-count;
    in property <[MediaTileData]> shelf1;      // Finish the season / Continue watching
    in property <[RankedTileData]> shelf2;     // Start watching / Recently added
    callback focus-changed(string /* rating-key */);
    callback open-watching-now();
}

// Show page additions
export struct SeasonInfo {
    name: string, meta: string, summary: string,
    progress: float, progress-label: string,
    poster: image, poster-is-fallback: bool,
}
// ShowPageState gains:
//   in property <SeasonInfo> season;
//   in property <bool> scrolled;              // drives fade + sticky bar
//   in property <EpisodeData> focused-episode;
```

### 5.3 Images

- One helper: `plex_image(path, w, h) -> Url` using `/photo/:/transcode?width=…&height=…&minSize=1&upscale=1&url=…&X-Plex-Token=…`.
- Sizes: backdrop `1300×660`, Shelf 1 tile `420×236`, ranked poster `150×225`, season poster `260×390`, episode still `468×263`, mini poster `36×54`. Request at **×2 when `TvUi.u ≥ 1.5`** (1440p / 4K).
- LRU cache by `(path, w, h)`; prefetch neighbours on focus.

---

## 6. Task list

### Phase 1 — Shared
- [ ] `plex_image()` helper + LRU image cache + neighbour prefetch
- [ ] `FocusBackdrop` component (two layers, crossfade, scrims)
- [ ] Debounced focus → backdrop/text update (300 ms `slint::Timer`)
- [ ] `FocusInfo` block with fixed 3-line summary + "no summary" fallback
- [ ] Stroke numerals with native `Text` stroke; test one digit on the TV first

### Phase 2 — Library landing
- [ ] New `TvLibrary` landing layout (§2.1); keep Browse All untouched
- [ ] Watching now pill wired to the sessions poller (grey `Nothing streaming` state)
- [ ] Watching now overlay (§2.2a): 84 px rows, focusable, Back closes
- [ ] Shelf 1: Finish the season (TV) / Continue watching (Movies) as 16:9 tiles
- [ ] Shelf 2: Start watching (weekly seeded shuffle) / Recently added (6 newest)
- [ ] Kicker/meta text per shelf (§1.2 table)
- [ ] Focus map (§2.6) + reset on tab switch
- [ ] Empty states (§2.7)
- [ ] Remove old shelves from the landing view (keep queries)

### Phase 3 — Show page
- [ ] New top block: backdrop, title, meta, summary, action row
- [ ] Season column + season poster with fallback rule
- [ ] 3-column episode grid with watched / up next / progress / "Continue to…" slot
- [ ] Scrolled state: fade, sticky bar, sticky column with focused episode
- [ ] Focus map (§3.5); open on up-next season, focus Resume
- [ ] Remove old hero card + 372 × 236 grid

### Phase 4 — Screensaver
- [ ] `PosterWall`: pre-render the rotated wall in Rust, animate `y` in Slint, seamless loop (§4.5)
- [ ] Vignettes + bottom fade
- [ ] Clock (260 px / 200 weight, 12 h / 24 h) + date + weather line
- [ ] Up next block with its three states (§4.3)
- [ ] Footer with library count
- [ ] Burn-in random walk (±8 px, once a minute)
- [ ] Start / wake rules; waking press swallowed; never starts during playback or Brave
- [ ] Remove the old `TvAmbient` layout

### Phase 5 — Polish
- [ ] Stone and Mist: check scrims, pill and sticky bar against `tokens` (art stays dark in all themes)
- [ ] 1080p / 1440p / 4K check via `TvUi.u`
- [ ] Hold-arrow test: no backdrop flicker, no shelf jump
- [ ] Changelog entry in `Dash.changelog`

---

## 7. Done when

- [ ] Moving focus across either landing shelf swaps the backdrop and text once focus settles, with a smooth crossfade and no layout jump.
- [ ] Start watching shows the same 6 shows all week and a new set after Monday; starting one replaces it with the next.
- [ ] Recently added (Movies) shows the 6 newest movies.
- [ ] Watching now pill shows the live count (or grey `Nothing streaming`) and updates every 10 s; OK opens the overlay and Back closes it.
- [ ] Show page opens on the up-next season with Resume focused; OK resumes from `viewOffset`.
- [ ] Each season shows its own poster, or the show poster with a `SEASON N` label when it has none.
- [ ] Moving into the episode grid fades the backdrop, shows the sticky bar, and the column shows the focused episode.
- [ ] Nothing on either page scrolls sideways.
- [ ] After 5 quiet minutes on Home the Poster wall starts, drifts smoothly at 4K without stutter, and any button wakes it without doing anything else.
- [ ] The Up next block shows Paused / Up next / Start watching correctly, with the item's tint.
- [ ] Works in Dark, Stone and Mist at 1080p, 1440p and 4K.

---

## 8. Settled questions (2026-11-19)

| Question | Decision |
|---|---|
| Below the fold on the landing | **Two shelves only**; nothing below the fold |
| Watching now pill → OK | **Overlay list** (§2.2a) |
| Watching now when idle | **Grey `Nothing streaming` pill** (never hidden) |
| Collections teaser on Movies | **No**; Collections stays its own tab |
| Stroke numerals | **Native Slint `Text` stroke**; SVG digits only if it looks wrong on the TV's renderer |

No open questions remain for this spec.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-19 | Library landing Option C chosen (TV: Finish the season + Start watching; Movies: Continue watching + Recently added). Start watching = random 6, weekly. Fixed 3-line summary block. Show page = Show 1 vertical with season poster column and sticky bar |
| 2026-11-19 | Open questions settled: two shelves only; Watching now overlay; grey `Nothing streaming` pill; no Collections teaser; native Slint text stroke for numerals |
| 2026-11-19 | Screensaver: Poster wall chosen, with the Up next block added (§4). Old `TvAmbient` layout retired |
