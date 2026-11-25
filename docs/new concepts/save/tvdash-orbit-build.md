# tvdash — Orbit Dashboard UI — Build spec (rev. 2026-11-23, for v0.1.68)

This spec applies to **TV-only** tvdash. It adds an **Orbit** Dashboard UI (`orbit`), the "less square" one, next to **Classic** (`ui/tv/`), **Blades** (`ui/v3/`) and **PS5** (`tvdash-ps5-a-build.md`). Every screen is designed: Home, Apps, Library, Show page, Sports (Scoreboard + Competitions), Collections (landing + collection page), Search, Settings (including Quit) and the startup animation.

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **Orbit (less square)**. Each screen has one approved board.

| Board | Section |
|---|---|
| Orbit · Home | §3 Home |
| Apps · Clock face | §4 Apps |
| Library · Dial | §5 Library |
| Show page · Episode ring · 22 seasons · 22-episode season | §6 Show page |
| Sports · Scoreboard · Stack | §7.2 Scoreboard |
| Sports · Competitions · Type bubbles | §7.3 Competitions |
| Collections · Bubble grid | §8.1 Collections landing |
| Collection page · Split + Timeline | §8.2 Collection page |
| Search · Key bubbles | §9 Search |
| Settings · Section bubbles · Quit tvdash | §10 Settings |
| Startup · Play-ring iris | §11 Startup animation |

All sizes are in **1920 × 1080 design units**, scaled by `TvUi.u`. The safe area is 110 / 34 (Orbit sits a little wider than PS5). Titles, teams and scores on the canvas are placeholders.

---

## 0. Decisions and rules (apply to every screen)

- **Option, not replacement.** `dashboard_ui = "orbit"` sits beside `classic`, `blades` and `ps5`. Classic stays the default.
- **Parity.** Features, links and tiles stay **close to 1:1** with the other UIs. Orbit binds the **same globals** (`HomeState`, `AppsPage`, `Library`, `LibraryLanding`, `ShowPageState`, `CollectionPageState`, `Scoreboard`, `Sports`, `Watch`, `SleepState`, `Display` …), fed by the same Rust. **Nothing is computed per UI**, apart from the ring paths (§1.5), which are display-only.
- **Hover = focus.** Anything the air mouse can hover gets the **same hover colour effect as remote focus**: the focus ring plus the focused fill or scale. Hover moves focus, and click = OK. This covers every button, pill, bubble, ring node, row and key.
- **Vertical scrolling only.** Long lists scroll ↑ ↓, and nothing scrolls sideways. The only ← → movement is on **dials and rings** (Home apps dial, Library Start watching dial, Apps clock face, Show episode ring). These turn rather than scroll, and they hold a fixed number of slots.
- **Rings.** One ring segment per item **up to 24 items**. Above 24, the ring is **one continuous arc** for the share watched, and labels switch from counts to `%`. This applies to collections, seasons and filter bubbles. The episode ring pages at 24 (§6.3).
- **Progress math in Rust, cached.** Watched ÷ total is computed in Rust and passed as `progress: float` plus `segments: int`. Watched counts are cached and refreshed on events, never while drawing (§1.6).
- **No Shuffle, no Star.** Sports uses read-only **★ Following** (`tvdash-follows-replace-star.md`).
- **One focus ring at a time**, drawn as a sibling **behind** the element (see the PS5 spec, §1.3 note).
- **Not v2.** Orbit is a dashboard UI, not part of the Leaf / Multiview 2.0 work.

---

## 1. Shared foundation

### 1.1 Tokens (`ui/orbit/theme.slint`, `export global OrbitTheme`)

| Token | Value | Use |
|---|---|---|
| `bg` | `#07070A` | Behind everything |
| `text` / `sub` / `muted` | `#EEF1F4` / `#B7BEC7` / `#8E95A0` | Text |
| `accent` | `Theme.accent` (Home default `#8FC1FF`; Collections gold `#F2C66B` and Search green `#7EE0B0` are section tints over the same token, see §13 Q1) | Kickers, lit ring segments, NOW / next markers, toggles |
| `live` | `#FF6B6B` | Live status, live ring segments, Quit |
| `glass` | `#0B0A12B3` + 1 px `#FFFFFF1A` | Pill nav, clock badge, quick circles |
| `row` | `#0D1017CC` + 1 px `#FFFFFF14` | List rows, cards |
| `row-focus` | `#FFFFFF1F` + 1 px `#FFFFFF55` | Focused or selected row |
| `track` | `#FFFFFF2E` | Unlit ring segments and progress tracks |
| `focus` | ring 4 px `text`, **5 px gap** in `bg`; large items get shadow `0 20 50 #000A` | Focus and hover |
| `fade` | `bg` → transparent over 100–140 px | Bottom of every scrolling list |
| `scrim-left` / `scrim-bottom` | `bg` F0 → A6 38% → 0 66% / `bg` → CC 25% → 0 55% | Text over backdrops |

**Radii:** circles (radius = ½ size) for anything that represents an item. Pills and rows use radius = ½ height. Panels (Sports card, Collections card, Quit dialog) use 48–125 px. **There are no square corners anywhere.**

**Font:** **Outfit** 300 / 400 / 500 / 600 / 700 / 800 (OFL; bundle the `.ttf`s). Use tabular numerals for the clock, scores and times.

### 1.2 Type scale

| Role | Size / weight |
|---|---|
| Hero title | 84–92 / 700, −2 tracking (Library 80, Collections 78, Collection 54) |
| Big app name (Apps) | 62 / 700 |
| Score (Sports card disc) | 28–38 / 700 |
| Kicker | 16–18 / 600 uppercase, 2.5 tracking, `accent` |
| Section label | 15–17 / 600 uppercase, 2.5 tracking, `sub` / `muted` |
| Pill nav tab | 22 / 400 (active 600, filled `text`, ink `bg`) |
| Row title | 20–22 / 600 (focused 25–26) |
| Meta | 15–19 `sub` |
| Summary | 18–20, line-height 27–30, `sub`, **fixed line count** per screen |

### 1.3 Shell (`ui/orbit/shell.slint`)

`OrbitShell` owns the backdrop, the pill nav, the clock badge and the page area. Pages switch on `TvUi.screen` / `TvUi.active-tab`, as in the other shells.

| Element | Spec |
|---|---|
| Pill nav | Centred, y 40, padding 6, gap 6, `glass` capsule. Tabs h52, padding 0 26: **Home · Apps · Library · Collections · Sports · Settings**. Active = `text` fill / `bg` ink. Sub-pages keep the parent tab active (Search: none active) |
| Clock badge | Top-right, right 80, top 30, **150 px** circle (Home: **190 px**), `glass`: time 44 / 300 + `AM · ☀ 34°F` 15 `sub`. It is the target of the startup blink (§11) |
| Back pill | Sub-pages: `‹ Library` / `‹ Collections` / `‹ Back` at x 110, y 130, h46 |
| Quick circles | Home and Library only: bottom-left (x 110, bottom 34), **58 px** `glass` circles, gap 12: **Search · Screensaver · Sleep timer** (focus shows the label pill, e.g. `Sleep · Off`) |
| Focus | ↑ from any page's top row goes to the pill nav. In the nav, ← → moves between tabs (switch immediately) and ↓ goes to the page's default focus |

### 1.4 Components

| Component | Spec |
|---|---|
| `OrbitRing` | Circle with a ring around an inner image or text. Props: `size`, `thickness`, `segments`, `lit`, `progress`, `lit-color`, `inner` (image / text). Draws two `Path`s (lit + unlit) from §1.5. **Slint has no conic gradient; rings are always Paths** |
| `OrbitPlay` | Round play button: 104 px (Library / Show), ring 9 px = item or show progress; inner 86 px `text` disc with a `bg` ▶ |
| `OrbitBubble` | Filter / section bubble: 80–128 px circle, big count or icon, segmented ring; row label to the right; selected row gets a `#FFFFFF14` pill behind |
| `OrbitRow` | List row h64–80 (focused grows to h96–138 with extra content), radius ½ height, `row` / `row-focus`, round thumb or node at the left |
| `OrbitPill` | h40–56 pill. Primary = `text` fill / `bg` ink 600; secondary = 1 px `#FFFFFF38` outline; filter pill shows a count at 70% opacity |
| `OrbitToggle` | 92 × 50 track; on = `accent`, `bg` knob 40 |
| `SlotList` | Fixed-slot vertical list (§1.7) used by every scrolling list |

### 1.5 Ring paths (Rust → Slint)

Rust builds the SVG path strings once per data change. Slint binds them to `Path { commands: … }`.

```rust
// src/orbit/rings.rs
/// Lit and unlit path strings for a ring of `n` segments with `lit` lit (n > 24 → one arc).
pub struct RingPaths { pub lit: String, pub unlit: String }

pub const SEG_MAX: usize = 24;

pub fn ring_paths(n: usize, lit: usize, r: f32, gap_deg: f32) -> RingPaths {
    let mut out = RingPaths { lit: String::new(), unlit: String::new() };
    if n == 0 { return out; }
    if n > SEG_MAX {
        let p = lit as f32 / n as f32;
        out.lit = arc(r, 0.0, 360.0 * p);
        out.unlit = arc(r, 360.0 * p, 360.0);
        return out;
    }
    let step = 360.0 / n as f32;
    for i in 0..n {
        let seg = arc(r, i as f32 * step, (i + 1) as f32 * step - gap_deg);
        if i < lit { out.lit.push_str(&seg) } else { out.unlit.push_str(&seg) }
    }
    out
}

/// One arc from `a0`..`a1` degrees, clockwise from 12 o'clock, centred on (r, r).
fn arc(r: f32, a0: f32, a1: f32) -> String {
    if a1 - a0 <= 0.0 { return String::new(); }
    let pt = |a: f32| { let t = (a - 90.0).to_radians(); (r + r * t.cos(), r + r * t.sin()) };
    let (x0, y0) = pt(a0);
    let (x1, y1) = pt(a1.min(a0 + 359.9));
    let large = if a1 - a0 > 180.0 { 1 } else { 0 };
    format!("M {x0:.1} {y0:.1} A {r} {r} 0 {large} 1 {x1:.1} {y1:.1} ")
}
```

```slint
// ui/orbit/ring.slint
export component OrbitRing inherits Rectangle {
    in property <length> size: 108px;
    in property <length> thickness: 6px;
    in property <string> lit-path;     // from RingPaths.lit
    in property <string> unlit-path;   // from RingPaths.unlit
    in property <brush> lit-color: OrbitTheme.accent;
    width: size; height: size;
    Path { commands: unlit-path; stroke: OrbitTheme.track; stroke-width: thickness;
           viewbox-width: size / 1px; viewbox-height: size / 1px; }
    Path { commands: lit-path; stroke: lit-color; stroke-width: thickness; stroke-line-cap: round;
           viewbox-width: size / 1px; viewbox-height: size / 1px; }
    @children   // inner disc: inset by thickness + 3 px, border-radius 50%, clip
}
```

| Gap per segment | Value |
|---|---|
| ≤ 12 segments | 4.3° (≈ 1.2%) |
| 13–24 segments | 2.9° (≈ 0.8%) |

Tests: `n = 0` → empty; `n = 8, lit = 5` → 5 lit arcs, 3 unlit; `n = 25` → single arcs; `lit = n` → no unlit; `lit = 0` → no lit.

**Cost.** Arcs are short `Path`s, at most about 300 on any screen. They are rebuilt only when the data changes, and Slint only redraws on change.

### 1.6 Watched counts and caching

| Rule | Spec |
|---|---|
| Source | Plex children of the collection / show (watched state per item) |
| Check first | Whether the Plex collection list already returns a watched total (like `viewedLeafCount` on shows). If it does, use it and skip the per-item fetch |
| Cache | `orbit_counts` in the existing state store: `{ key → (total, watched, fetched_at) }`. Screens read the cache first and show it immediately |
| Refresh | After playback in that collection or show ends · library-changed event · slow timer (30 min) · never during a draw |
| Big collections | Fine: one request per collection, off the UI thread |

### 1.7 `SlotList` (every vertical list)

The list shows a fixed number of slots. Rust (or a Slint `property <int> top`) shifts which items fill them. Moving focus animates the slots (spring, ~220 ms), and the list never jumps.

| Rule | Spec |
|---|---|
| Focus position | Stays in view; lists with a hero row keep focus about ⅓ down |
| Ends | The last slot fades (`fade`) when there is more below; the first one fades (40% opacity) when there is more above |
| Counter | Lists over 20 items show `N of M` in the hint line (e.g. `143 of 380`) |
| Hold ↓ | Accelerates after 600 ms (3 / s → 12 / s) |

### 1.8 Backdrop

Reuse **`FocusBackdrop`** (two layers, 250 ms crossfade, 300 ms debounce) through the bounded **`ImageCache`** (`BACKDROP_CAP = 40`). The backdrop is full screen with `scrim-left` + `scrim-bottom`. Sports, Search and Settings use fixed glows (§7.1, §9, §10).

---

## 2. Plumbing (fourth Dashboard UI)

Build on the PS5 three-way switch (`tvdash-ps5-a-build.md` §2). If PS5 isn't built yet, do its §2.2 table first and add both values at once.

```rust
// crates/core/src/config.rs
pub enum DashboardUi { #[default] Classic, Blades, Ps5, Orbit }

impl DashboardUi {
    pub fn as_str(self) -> &'static str {
        match self { Self::Classic => "classic", Self::Blades => "blades", Self::Ps5 => "ps5", Self::Orbit => "orbit" }
    }
    pub fn parse(s: &str) -> Self {
        match s { "blades" => Self::Blades, "ps5" => Self::Ps5, "orbit" => Self::Orbit, _ => Self::Classic }
    }
    pub fn next(self) -> Self {
        match self { Self::Classic => Self::Blades, Self::Blades => Self::Ps5, Self::Ps5 => Self::Orbit, Self::Orbit => Self::Classic }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic · top tabs", Self::Blades => "Blades · side tabs",
            Self::Ps5 => "PS5 · header tabs · app row", Self::Orbit => "Orbit · pill nav · dials · rings",
        }
    }
}
```

```toml
[display]
dashboard_ui = "orbit"   # "classic" | "blades" | "ps5" | "orbit"
```

| File | Change |
|---|---|
| `ui/state.slint` | `Display.dashboard-ui` accepts `"orbit"` |
| `ui/laptop.slint` | add `if Display.dashboard-ui == "orbit" : OrbitShell {}`; bg + font from `OrbitTheme` |
| `ui/shared/sports_alert.slint` | Orbit branch: `row` bg, radius ½ height, `accent` |
| `src/tv_nav.rs` `tile_rects()` | `DashboardUi::Orbit => orbit_tile_rects(laptop, page)` (ring and dial nodes are rects around each circle) |
| `src/callbacks.rs` intro preview | `Skin::from(DashboardUi::Orbit)` |
| `src/intro.rs` | `Skin::Orbit` (§11) |
| Settings pickers (all UIs) | one card per `DashboardUi` value |

Tests: `parse("orbit")`, TOML round-trip, `next()` cycles all four, and `rg -n '"blades"|DashboardUi::|Skin::' crates/` shows no non-exhaustive match.

---

## 3. Home (board "Orbit · Home")

| Element | Spec |
|---|---|
| Backdrop | What's On art, `scrim-left` + `scrim-bottom` |
| Clock badge | **190 px** (Home only) |
| What's On hero | x 110, top ~230: kicker `WHAT'S ON · <source>` · title 92 · meta · **2-line** plot · `OrbitPlay` (ring = `whats-on.progress`) + `Resume` label + status (`Ends at …`) · pills **From start** · **···** (Stop · Audio · Subtitles) |
| Up next | Right side: 4 round posters 108 px with progress rings, along a curve (left indents **120 / 70 / 40 / 30**), title + meta to the right |
| Next Game | Capsule under Up next (round team circles, score / time, status): OK → `open-game`. If there's no game → **Just Added** capsule |
| Apps dial | Circle `cx 960, cy 1770, r 950`; apps from `HomeState.apps-bar` + **All apps** at **9.5°** steps; focused app at the top, **150 px**, label above at y 904; neighbours 104 px, fading |
| Quick circles | Bottom-left (§1.3) |

Focus: hero Play is the default · → hero pills · ↓ to the dial (focused = most recent app) · dial ← → turns · ↑ from the dial → hero · Up next is reached with → from the hero pills (↑ ↓ along the curve) · Back → hero Play. OK on an app launches it + `push_recent`.

---

## 4. Apps (board "Apps · Clock face")

| Element | Spec |
|---|---|
| Face | Centre (960, 590), r 380, outer guide ring 760 (1 px `#FFFFFF24`), inner disc 500 `glass` |
| Apps | `AppsPage.apps` **A–Z** clockwise from 12 o'clock, one per "hour"; circles **124 px** in brand `color`, name (or logo) in `text-color`; focused **176 px** + `focus` |
| Hand | 8 px `accent`, lit only for the outer 50 px (r 262 → 312), rotates to the focused app (spring 250 ms) |
| Centre | `APPS · N` 16 `muted` · name 62 / 700 · `Opened yesterday` (from `recent.rs`, else nothing) · **OK · Open** pill |
| Left | `Apps` 56 / 700 + 2-line hint |
| Background | Faint glow in the focused app's `text-color` (≈ 20%) |

Navigation: **→ / ↓** = next app clockwise; **← / ↑** = previous. Hover and click work on any circle. OK launches + `push_recent`.

**Count:** the face spaces any count evenly (`360 / N`). Today N = 12. If N > 16, circles shrink to 96 px (§13 Q4).

---

## 5. Library (board "Library · Dial")

| Element | Spec |
|---|---|
| Sub-tabs | y 130: pills **TV Series · Movies · Browse all** + **Watching now · N** green pill (`LibraryLanding.watching-now-count`) |
| Hero | x 110, y 230, w880: kicker (e.g. `FINISH THE SEASON · S2 · E6 NEXT · 3 LEFT`) · title 80 · **3-line** summary · `OrbitPlay` + `Play S2 · E6` / `5 of 8 watched` · **Show page** pill |
| Finish the season | Right column (right 70, top 230): 4 round posters 108 px with **segmented rings** (one segment per episode in the season, watched lit), on a curve (indents 130 / 70 / 36 / 24) |
| Start watching | Bottom dial `cx 960, cy 1780, r 950`, 6 posters at slots `[0, 1, −1, 2, −2, 3] × 10.5°`; #1 focused 160 px, others 120; accent number badges 40 px; label `START WATCHING · NEW PICKS EVERY MONDAY` at y 700; guide circle at y 830 |
| More shelves | `↓ Added this week · Popular on Republic City`: ↓ from the dial swaps the hero area and dial to the next shelf pair (same layout); this is a page step, not a sideways scroll |
| Movies tab | Same layout: **Recently added** on the dial, **Continue watching** on the curve (rings = playback progress) |

Shelf data is shared (`bridge/library.rs`). Focus: dial ← → · ↑ to the hero Play · → from the hero to the curve (↑ ↓) · ↓ from the dial to the next shelf pair · Back → sub-tabs.

---

## 6. Show page (boards "Show page · Episode ring", "22 seasons", "22-episode season")

### 6.1 Hero (left)

`‹ Library` pill · kicker `TV SERIES · 2019 · 4 SEASONS · TV-14` · title 84 · **3-line** summary · `OrbitPlay` (ring = **show** progress) + `Play S2 · E6` / `13 of 32 episodes watched` · pills **From start** · **···**. Quick circles are hidden on this page.

### 6.2 Season list (under the hero)

| Rule | Spec |
|---|---|
| Position | x 110, y 588, w600; header `SEASONS` + (long shows) `Season 9 of 22` pill + hint `↑ ↓ seasons · → the ring` |
| Rows | `OrbitRow` h72: 64 px `OrbitRing` (segments = episodes in that season, inner `S2`) · `Season 2` 21 · meta `Watched` / `5 of 8` / `10 episodes` |
| Selected | `row-focus` fill (selected, not focused); the white focus stays on the ring |
| Window | **5 slots** around the current season (`start = clamp(cur − 2, 0, total − 5)`); end rows at 40% when more exist past them |

### 6.3 Episode ring (right)

| Rule | Spec |
|---|---|
| Ring | Centre (1440, 580), r 340; track 680 px, 8 px thick: `accent` arc for watched ÷ total in the season, `track` for the rest |
| Nodes | Clockwise from 12 o'clock at `360 / N` |
| ≤ 12 episodes | Nodes **76 px** labelled `E1`; next episode **132 px** with its thumbnail |
| 13–24 | Nodes **54 px** labelled `14` (number only); next **112 px** labelled `E14` |
| 25+ | **Pages of 24**: the ring shows E1–24, and a `1 of 3` pill sits over the ring. Moving past the last node turns to the next page (spring rotate + crossfade) |
| Node states | Watched = `accent` fill + `bg` text; next = thumbnail + `focus`; unwatched = `#1A1624` + 2 px `#FFFFFF38` |
| Centre | `NEXT UP · S2 · E6` (or `S2 · E7` when focused elsewhere) · title 40 · `44 min · aired Mar 2020` · 2-line summary |

Focus: the default is the next episode on the ring · ← → (and ↑ ↓) move around the ring · ← from the left-most node goes to the season list · in the list, ↑ ↓ picks a season (the ring redraws with that season, focus on its next unwatched episode or E1) and → goes back to the ring · OK on a node plays it (`play-item`) · Back → Library.

```rust
// src/orbit/episode_ring.rs
pub const PAGE: usize = 24;
pub struct RingPage { pub first: usize, pub count: usize, pub page: usize, pub pages: usize }
pub fn page_for(total: usize, focused: usize) -> RingPage {
    let pages = total.div_ceil(PAGE).max(1);
    let page = (focused / PAGE).min(pages - 1);
    let first = page * PAGE;
    RingPage { first, count: (total - first).min(PAGE), page, pages }
}
pub fn node_size(count: usize, next: bool) -> f32 {
    match (count > 12, next) { (false, false) => 76.0, (false, true) => 132.0, (true, false) => 54.0, (true, true) => 112.0 }
}
```

Tests: 8 → 1 page of 8; 22 → 1 page, dense; 50 with focus 30 → page 2 of 3, first 24, count 24; 50 with focus 49 → page 3, count 2.

---

## 7. Sports

### 7.1 Shell

The fixed backdrop is `bg` + two glows (`#0051BA40` top-left, `#6B2C9133` top-right). Sub-switch pills at y 130: **Competitions · Scoreboard** (`TvUi.sports-tab`). **Both tabs use the same layout:** filter bubbles on the left, the focused item's card top-right, and a vertical list below it.

### 7.2 Scoreboard (board "Sports · Scoreboard · Stack")

Bound to the shared `Scoreboard` view model (`tvdash-scoreboard-build.md` §10).

| Part | Spec |
|---|---|
| League bubbles | x 110, y 210, w380: `LEAGUE` label; **All leagues** + one bubble per league with games in the window, **A–Z**; 96 px `OrbitBubble`: count, ring segments = games (**live segments in `live`**), meta `1 live · 1 more` (live in `live`) or `2 games`. ↑ ↓ picks; changes apply immediately. More than 7 → `SlotList` |
| Game card | x 560, y 220, 1250 × 250, radius 125, `row`: 222 px score disc (team-colour split under a `#06080B8C` veil, both scores, ring = **game clock** in `live` for live games, full `#FFFFFF55` for final, faint for upcoming) · kicker `● LIVE · COLLEGE FOOTBALL · 3RD 3:38 · ★ FOLLOWING` · matchup 42 · period pills (current period tinted `live` at 18%) · venue · TV · **OK · Open game** + **Watch on <network>** |
| List | x 560, y 500, 1250 × 560: sections **Live · N** / **Upcoming · N** / **Final · N** (dot + label); rows h80: away circle 60 · away score · – · home score · home circle · league + status · ★ Following · network; leader bold, trailer `sub`; within a section, sorted by league then start time. Empty sections are hidden. The first live game sits in the card, not the list |
| Data age | Hint line: `Updates ~10m · last 9:31 AM` (stale → warning tint) |
| Date | §13 Q2 |

Game clock share (Rust, in the Scoreboard VM): `elapsed ÷ regulation`, where regulation comes from the sport's period table (Scoreboard spec §7). Halftime / intermissions keep the last value; OT / extra time = 100%.

Focus: bubbles ↑ ↓ · → to the card's **Open game** · ↓ from the card into the list · ↑ ↓ through the list · OK on a row moves that game into the card (a second OK opens it) · ← from anywhere on the right → bubbles · Back → bubbles, then All leagues, then the nav.

### 7.3 Competitions (board "Sports · Competitions · Type bubbles")

Bound to the `Sports` global (schedule rows, `open-url`).

| Part | Spec |
|---|---|
| Header extra | `SCH` school chip + `[School] · ★ Following` next to the sub pills |
| Type bubbles | x 110, y 214: `TYPE` label; **All types** + one bubble per event type with events in the window, **A–Z** (Football · Men's basketball · Volleyball · Women's basketball …); **128 px**: count + ring segments = events; label + meta (`2 events`, All: `6 events · sorted by type`). Types with no events are dropped. More than 6 → `SlotList` |
| Event card | x 560, y 220, 1250 × 250, radius 125: 222 px disc (school / opponent colour split, `SCH vs OP1`, time 52, `TODAY · PM`; ring **accent = home**, `#FFFFFF40` = away / neutral) · kicker `NEXT UP · MEN'S BASKETBALL · HOME` · title 46 · venue · TV · link pills **Stream · Live stats · Audio · Details** (only those with URLs; Stream primary) |
| List | x 560, y 500: **sorted by type (A–Z), then date**; with All types, a small type header per group; rows h80: 64 px disc (time; ring = home / away) · day · `vs / at [Opponent]` · Home / Away / Neutral site · TV |
| Paging | The list is continuous: when focus reaches the last loaded row, request the next `Sports` page (§13 Q3) |

Focus: bubbles ↑ ↓ · → to the card's link pills (← → between pills) · ↓ into the list · OK on a row moves it into the card · Back: list → bubbles → All types → nav.

---

## 8. Collections

### 8.1 Landing (board "Collections · Bubble grid")

| Part | Spec |
|---|---|
| Backdrop | Focused collection's art (fallback: first movie's art), gold-tinted glow |
| Hero | x 110, y 160, w900: kicker `COLLECTION · SMART · 8 MOVIES · 3 WATCHED` (over 24: `380 MOVIES · 37% WATCHED`) · title 78 · **2-line** summary. **No Resume area** |
| Grid | Label `ALL COLLECTIONS` + count at y 410; grid from y 456, **6 across**: 200 px `OrbitRing` covers (segments ≤ 24, otherwise % arc), name 20 / 600, detail `8 movies · 3 watched` / `380 movies · 37% watched` / `all watched` / `unstarted` |
| Order | Existing rule (something left to watch first, then most recently added) |
| Scroll | `SlotList` by rows; two full rows visible; `fade` at the bottom |

Focus: grid ← → within a row, ↑ ↓ between rows; ↑ from row 1 → nav. OK → collection page.

### 8.2 Collection page (board "Collection page · Split + Timeline")

Bound to `CollectionPageState`.

| Part | Spec |
|---|---|
| Header row | `‹ Collections` · `ORDER` **Collection order · Release date · A–Z** (`set-order`) · **Hide watched** pill (`set-hide-watched`) |
| Left (fixed) | x 110, y 220, w520: 280 px disc (ring = % watched; centre `37%` / `142 of 380 watched`) · kicker `COLLECTION · PLEX` · title 54 · `380 movies · 1972–2025 · 6 decades` · **3-line** summary |
| Timeline | Vertical line at x 790 (4 px), **lit in `accent` from the top to the focused movie** |
| Decades | Pills on the line (120 × 44): `2010s`, with the current decade in `accent`; summary to the right (`118 movies · 12% watched`). ↓ on a decade pill jumps to the next decade |
| Year ticks | 15 / 600 `muted`, left of the line, only where the year changes |
| Movie rows | Node on the line (40 px poster circle + ring: watched = full `accent`, part-watched = arc, unwatched = faint; focused 64 px + `focus`) · row from x 850, h64: number · title · year · runtime · status (`Watched` / `40 min left`) |
| Focused row | Grows to h138: 2-line summary + **OK · Resume** / **OK · Play** |
| Open at | First unwatched movie (`143 of 380` in the hint) |
| Other orders | **Collection order / A–Z:** decades and ticks hide; same rows as a plain `SlotList` (no line) |

Focus: default = the focused row · ↑ ↓ movies · ↑ from the top → ORDER pills · OK → `play-item(key, resume)` · Back → Collections.

---

## 9. Search (board "Search · Key bubbles")

Opened from the Search quick circle or Home's empty hero. Bound to `Watch`.

| Part | Spec |
|---|---|
| Backdrop | Top result's art, else a green glow (`#3D5A34` → `bg`) |
| Query | x 110, y 196, 650 × 76 capsule (2 px `#FFFFFF38`): icon · `Watch.query` 32 / 500 + `accent` caret · `N results` |
| Keyboard | x 110, y 310: **6 × 6 round keys, 86 px, gap 22**, a–z then 1–0; focused key `text` fill / `bg` ink + `focus` |
| Actions | y 970: **Space · Delete · Clear** pills (200 × 56) |
| Filter | **Only what I can watch now** `OrbitToggle` (`Watch.only-now`) in the results header |
| Results | x 860, y 214, w950, `SlotList`: groups (hidden when empty) **IN YOUR LIBRARY** `Plays on Plex` · **INCLUDED WITH YOUR APPS** `Opens full screen on the TV` · **COSTS EXTRA** `Rent or buy` · **NOT STREAMING**. Rows h88 (focused h112): round poster 72 · title · `2019 · Movie` · chip (Plex `#E5A00D`, included `#30D158`, extra `text`, none `#3A3F46`) |
| Detail | OK on a result → the existing detail panel as a rounded (radius 48) sheet over the results |

Physical typing works too; edits are debounced (`Watch.edited`). Focus: keys in a grid · → from the right-hand key column into the results · ← from the results → keyboard · Back closes the detail, then Search.

---

## 10. Settings (boards "Settings · Section bubbles", "Settings · Quit tvdash")

| Part | Spec |
|---|---|
| Backdrop | Glow `#2A3550` top-right |
| Section column | x 110, y 196, w380: `SETTINGS` label; **80 px** round icon bubbles + labels, rows h80 gap 8: **Display · Accounts · Sites · Sports · Server · Diagnostics · What's new** (`TvUi.settings-tab` keys `display`, `accounts`, `sites`, `sports`, `server`, `diagnostics`, `changelog`); selected = `text` fill bubble + `#FFFFFF14` row |
| Quit | After a 1 px divider: **Quit tvdash**: 84 px bubble `#3A1418` + 2 px `#FF6B6B66`, power icon `#FF8A8A`, label `#FF9A9A`, hint `Stops playback · closes Brave · OK asks first` |
| Content | x 560, w1250: section title 44 / 700, then rows h74 radius 37 (`row`): name 21 / 600 + hint 14 `muted` · value pill `Kansas Dark ›` / `OrbitToggle` / button pill |

### 10.1 Display

| Row | Control | Binds |
|---|---|---|
| **Dashboard UI** | 150 px round previews (ring `accent` on the selected one, ✓ after the name): **Classic** · **Blades** · **PS5** · **Orbit** (`Pill nav · dials · rings`). macOS Stage is shelved and not listed | `Display.set-dashboard-ui` |
| Theme | `Kansas Dark ›` (hint `Accent colour in Orbit`) | existing cycle |
| Time format | `12h ›` | existing |
| Weather location | `[City] ›` | existing |
| Weather units | `°F ›` | existing |
| Startup animation | toggle + **Preview** | `Intro.preview` |
| Clear recent apps | **Clear** (hint `Resets the order on Home`) | existing |

Other sections have the same content as the PS5 spec §9.3, as Orbit rows.

### 10.2 Quit

OK on **Quit tvdash** dims the screen (`#06070AE0`) and opens a 640-wide dialog (radius 60, `#121319`): 120 px power bubble · **Quit tvdash?** 44 · what stops (`[Show] S2 · E6 stops and your place is saved to Plex. Brave closes too.`, or `Nothing is playing.`) · **Cancel** (focused first) · **Quit** (`live` fill). Back = Cancel.

Quit runs the existing path: `callbacks::request_quit` → `Command::Quit` → `release_tv()` (stop + report Stopped, quit mpv, close Brave, release the inhibitor, clear runtime state) → end the event loop, with the 8 s `QUIT_FALLBACK`. **No new quit logic.**

---

## 11. Startup animation (board "Startup · Play-ring iris")

`Skin::Orbit` plugs into the existing `IntroGate` (`logo → hold → reveal → done`). **These are the approved timings (8.25 s board loop, scaled from 7 s).**

| Stage | Time | What moves |
|---|---|---|
| Logo | 0 → 0.7 s | Black. Play button (150 px: ring track + `text` disc + ▶) above the wordmark (`tvdash` 104 / 700, `ORBIT` 18 / 600 6-tracking `accent`); both fade 0 → 1, scale 0.94 → 1 |
| Ring fill | 0.35 → 2.1 s | `accent` ring fills clockwise around the play button (`OrbitRing` with `progress` animated 0 → 1) |
| Hold | Only while Home data isn't ready (max `startup.max_hold_secs`) | Ring stops at 90% and breathes (opacity 1 ↔ 0.7, 1.2 s); finishes to 100% when ready |
| Iris | 2.25 → 3.2 s | Home is revealed through a **growing circle** from the centre (clip `Rectangle`, `border-radius = size / 2`, size 0 → 2300 px, ease (.6, 0, .2, 1)); logo and play button fade out over the first 0.4 s |
| Clock blink | 3.65 → 4.95 s | **Decorative.** A ring over the clock badge (4 px `#FFFFFFCC` + 40 px `#8FC1FF88` glow, `#FFFFFF22` fill) blinks twice: opacity 0 → 1 → 0 → 1 → 0, scale 1 → 1.14. Home is usable during it |

```rust
// src/intro.rs
Skin::Orbit => (Duration::from_millis(2250), Duration::from_millis(2700)), // logo_len (to iris start), reveal_len (iris + blink)
```

- All movement is opacity / scale / rotation and one clip size, using `animate` (no blur, no 3D).
- **Preview** in Settings replays it; Startup animation **Off** goes straight to Home (no blink).

---

## 12. Files

```text
crates/core/src/config.rs        # DashboardUi::Orbit, next(), label() + tests
crates/app/
├─ src/intro.rs                  # Skin::Orbit + timings (§11)
├─ src/tv_nav.rs                 # Orbit dispatch; orbit_tile_rects() per page (dial / ring nodes)
├─ src/callbacks.rs              # intro preview skin; dashboard label
├─ src/orbit/{mod.rs, rings.rs, episode_ring.rs, slots.rs, counts.rs}
└─ ui/orbit/
   ├─ theme.slint  shell.slint  intro.slint
   ├─ ring.slint  play.slint  bubble.slint  row.slint  pill.slint  toggle.slint  slot_list.slint
   ├─ home.slint  apps.slint  library.slint  show.slint
   ├─ scoreboard.slint  competitions.slint
   ├─ collections.slint  collection.slint
   ├─ search.slint  settings.slint  quit_dialog.slint
```

---

## 13. Open questions

| # | Question | Default |
|---|---|---|
| 1 | Section tints (Collections gold, Search green) or one accent everywhere? | **Theme accent everywhere**; the tints only colour the backdrop glows |
| 2 | Scoreboard date stepper / Refresh: where do they go? | Small pill next to the sub-switch: `‹ Mon, Nov 23 ›` + `↻`; Today appears when not on today |
| 3 | Competitions paging | Continuous list, next page loads at the last row |
| 4 | Apps clock with more than 16 apps | Circles shrink to 96 px; still one ring |
| 5 | Library ↓ to the next shelf pair, or keep only two shelves? | Shelf pairs (Start watching / Finish the season, then Added this week / Popular) |
| 6 | Quit also on Home as a fourth quick circle? | **No**, Settings only |

---

## 14. Task list

1. **Plumbing:** `DashboardUi::Orbit` + tests · switch table (§2) · `OrbitTheme`, `OrbitShell`, pill nav, clock badge · Outfit fonts.
2. **Rings + lists:** `rings.rs` (+ tests), `OrbitRing`, `OrbitPlay`, `OrbitBubble`, `OrbitRow`, `SlotList`, watched-count cache (§1.5–§1.7).
3. **Home** (§3) and **Apps** clock face (§4).
4. **Library** dial (§5).
5. **Show page:** season list, episode ring, paging (§6).
6. **Sports:** Scoreboard stack, game clock share, Competitions type bubbles (§7).
7. **Collections:** landing grid, Split + Timeline page (§8).
8. **Search** (§9).
9. **Settings** + Quit dialog (§10).
10. **Startup animation** + Preview (§11).
11. **Polish & release:** hover = focus on every target (air mouse); 1080p / 1440p / 4K; hold-arrow test on every `SlotList`, dial and ring (no flicker, no jumps); toasts / Sleep warning / Up Next modal readable over backdrops; tests per crate, lightest first (`cargo test -j 2 -p tvdash-core`, …, `cargo test -j 2 -p tvdash -- --test-threads=1`); version bump in `Cargo.toml`, `CHANGELOG.md`, `docs/fedora-install.txt`, `docs/status.md`, `releases/README.md`; the user runs the release build and RPM (`GEMINI.md`).

## 15. Done when

- [ ] Settings › Display switches between Classic, Blades, PS5 and Orbit from every UI; the choice survives a restart.
- [ ] Every tab and sub-page opens in the Orbit look; nothing falls back to another UI.
- [ ] Orbit Home shows the same What's On, Up next, Next Game / Just Added and apps as the other UIs at the same moment.
- [ ] Every hoverable element shows the hover colour effect, and hover + click behaves like focus + OK.
- [ ] Nothing scrolls sideways; dials and rings turn with ← → and keep their slot count.
- [ ] Rings show one segment per item up to 24, then a single % arc; labels switch to %; the 380-movie collection draws without lag.
- [ ] The episode ring handles 8, 22 and 50 episodes (pages of 24); long shows scroll the season list five at a time.
- [ ] Scoreboard: league filter works, live rings follow the game clock, ★ Following shows, OK opens the game. Competitions: type filter works, list sorted by type then date, links open in the TV browser.
- [ ] Collection page opens at the first unwatched movie; decades jump with ↓; other orders show a plain list.
- [ ] Quit asks first (Cancel focused) and exits cleanly through `Command::Quit`.
- [ ] The Orbit intro plays with the §11 timings, holds only while loading, blinks the clock twice, and Preview replays it.
- [ ] Looks right at 1080p, 1440p and 4K; works by remote alone.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-23 | Spec created: Home, Apps (clock face), Library (dial), Show page (episode ring + season list, 24-episode pages), Sports (Scoreboard stack + Competitions type bubbles), Collections (bubble grid + Split + Timeline), Search (key bubbles), Settings (section bubbles + Quit), startup animation (play-ring iris + clock blink, 8.25 s board timing). Rules: hover = focus, vertical scroll only, ring segments ≤ 24, progress computed in Rust and cached |
