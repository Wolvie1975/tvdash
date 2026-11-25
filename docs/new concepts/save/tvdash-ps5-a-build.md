# tvdash — PS5 Dashboard UI — Build spec (rev. 2026-11-23 b, for v0.1.68)

Applies to **TV-only** tvdash. Adds a **third Dashboard UI**, `ps5`, next to **Classic** (tile hub, `ui/tv/`) and **Blades** (`ui/v3/`). Every section is designed: Home, Apps, Library, Show page, Collections, Sports, Search, Settings and the startup animation. A fourth UI, macOS Stage, has its own spec (`tvdash-macos-stage-build.md`) and reuses §2 of this one.

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **PlayStation-inspired**

| Board | Section |
|---|---|
| Home A — Apps row | §3 Home (**layout only**: row and cards are superseded by §3.2–§3.4) |
| Home D — today's sketch | Header tabs (§2.3), Up next / Next Game card row |
| Library A — Shelf grids | §4 Library |
| Show A — Hub · Show A — Episodes | §5 Show page |
| Sports · Scoreboard · Sports · Competitions | §6 Sports |
| Collections · landing · Collections · collection page | §7 Collections |
| Search | §8 Search |
| Settings · Display | §9 Settings |
| Startup animation storyboard · Startup animation · animated | §10 Startup animation |

All sizes are **1920 × 1080 design units**, scaled by `TvUi.u` (×1.333 at 1440p, ×2 at 4K). Safe area 96 / 54. Sample titles, teams and scores on the canvas are placeholders.

---

## 0. Decisions

- **Look:** PS5-inspired, not copied. No Sony logos, glyphs, icons or sounds.
- **Option, not replacement:** `dashboard_ui = "ps5"` sits beside `classic` and `blades` (and later `macos`). Classic stays the default.
- **Parity:** features, links and tiles stay **close to 1:1** with Classic and Blades. PS5 binds the **same globals** (`HomeState`, `AppsPage`, `Library`, `LibraryLanding`, `ShowPageState`, `CollectionPageState`, `Scoreboard`, `Sports`, `Watch`, `SleepState`, `Display` …), fed by the same Rust. **Nothing is computed per UI.**
- **Scrolling:** vertical only, except the Home row (one short sideways row).
- **Hero art** is always the backdrop (`art`, 16:9); season posters live on season cards.
- **No Shuffle, no Star.** Show actions are **Resume** and **From S1 · E1**. Sports uses **★ Following** (read-only) from the MyNewsFeed follows (`tvdash-follows-replace-star.md`).
- **Season stacking:** rows of 4, max 2 rows; 9+ seasons → 7 cards + **All N seasons**.
- **Not v2:** PS5 is a dashboard UI, not part of the Leaf / Multiview 2.0 work.

---

## 1. Shared foundation

### 1.1 Tokens (`ui/ps5/theme.slint`, `export global PsTheme`)

Theme colours read `Theme` (`accent`, `on-accent`); the rest are fixed because PS5 is always dark over backdrops (§11 Q1).

| Token | Value | Use |
|---|---|---|
| `radius-tile` | 22 (focused row tile) · 18 (row tile) · 20 (shelf / poster) · 16 (cards) | Rounded squares |
| `radius-pill` | height ÷ 2 | All buttons, chips, segmented controls |
| `focus` | 4 px `text` outline, **5 px offset**, shadow `0 24 60 #000A` | One focus at a time |
| `scrim-left` | `#050608F0` 0% → `#050608B3` 38% → transparent 72% | Text over backdrops |
| `scrim-bottom` | `#050608F7` 0% → `#0506088C` 40% → transparent 62% | Rows and cards over backdrops |
| `glass` | `#E8EAED29` | Secondary buttons, unselected chips, header icon buttons, keys |
| `card` | `#14171CDB` + 1 px `#E8EAED1A` | Info cards (Next Game, Scoreboard, Competitions rows, Settings rows) |
| `bg` | `#050608` | Behind everything |
| `text` / `sub` / `muted` | `#E8EAED` / `#C9CDD3` / `#8A9099` | Text |
| `accent` | `Theme.accent` (kicker ink: a light tint ≥ 4.5:1) | Kickers, UP NEXT chip, upcoming status |
| `live` | `#FF4D4D` (text `#FF6B6B`) | Live dot and live status only |
| `fade` | `#050608` → transparent over 110–140 px | Bottom of every scrolling page |

Fonts: **Inter** 300 / 400 / 500 / 600 / 800 (bundle the `.ttf`s, import in Slint); tabular numerals for clock, scores and times.

### 1.2 Type scale

| Role | Size / weight |
|---|---|
| Header tab (active / other) | 30 / 600 `text` + 3 px underline / 30 / 300 `text` @ 70% |
| Hero title | 92–96 / 600, −2 tracking, line-height 1 |
| Library / Collections hero title | 72–80 / 600 |
| Sticky title (episodes) | 44 / 600 |
| Row label (focused tile) | 36 / 600 + 24 / 400 `sub` |
| Kicker | 22 / 600 uppercase, 1.4 tracking, `accent` |
| Section / group label | 22 / 600 uppercase, 1.2 tracking, `sub` (Sports groups: 26 / 600 sentence case + count 20 / 400 `muted`) |
| Meta | 24–28 / 400 `sub` |
| Summary | 23–24, line-height 34, `sub` |
| Card title | 20–26 / 500–600 |
| Card meta | 15–20 `muted` |

### 1.3 Components

| Component | Spec |
|---|---|
| `PsHeader` | §2.3 |
| `PsRowTile` | Normal 132 × 132 r18 `y: 42`; focused 216 × 216 r22 + `focus`; 160 ms ease-out. App tiles: `AppTile.logo` `contain` 70% × 60% (88% × 66% when `logo-wide`), else name 22 / 800 in `text-color` |
| `PsRowLabel` | Right of the focused tile, 260–300 wide, 70 from top |
| `PsPillButton` | Primary h72, `#E8EAED` fill, `#0B0D10` 30 / 600 · secondary `glass` 28 / 500 · round "more" 72 × 72 · compact h56–60 |
| `PsIconButton` | 52 × 52 `glass` circle, 26 px stroke icon; focused: `focus` + label pill below (`Sleep · Off`) |
| `PsSegmented` | `glass` track (padding 4–5, pill), options h44–46; selected = `#E8EAED` fill + `#0B0D10` 600 |
| `PsCard` | r16; art cards (art + bottom scrim, kicker, title, 5 px progress) and info cards (`card`) |
| `PsShelfTile` | Square or 2:3, r20, art + scrim, title; optional rank numeral / chip |
| `PsBoxScore` | §6.4 |
| `PsToggle` | 56–64 × 32–36 track `glass`, knob `#E8EAED`; on = track `accent` |
| `HintBar` | Existing bottom-right pill |

```slint
// ui/ps5/row_tile.slint
import { Theme } from "../theme.slint";

export component PsRowTile inherits Rectangle {
    in property <bool> focused;
    in property <brush> fill;
    callback activated();
    width: focused ? 216px : 132px;
    height: self.width;
    y: focused ? 0px : 42px;
    border-radius: focused ? 22px : 18px;
    background: fill;
    clip: true;
    drop-shadow-blur: focused ? 60px : 0px;
    drop-shadow-offset-y: focused ? 24px : 0px;
    drop-shadow-color: #000000A6;
    animate width, y, border-radius, drop-shadow-blur { duration: 160ms; easing: ease-out; }
    @children
    TouchArea { clicked => { root.activated(); } }
}
```

> Draw focus rings as a **sibling behind** the element (`x: -9px`, size + 18, radius + 9): `clip: true` hides children outside the tile.

### 1.4 Backdrop

Reuse **`FocusBackdrop`** (`tvdash-library-show-v2-build.md` §1.1: two layers, 250 ms crossfade, 300 ms debounce, prefetch, fallbacks) through the existing **bounded `ImageCache`** (`BACKDROP_CAP = 40`).

| Rule | PS5 |
|---|---|
| Area | **Full screen** 1920 × 1080 |
| Scrims | `scrim-left` + `scrim-bottom`; Home adds a top fade (`#050608BF` → transparent over 360) |
| Fallbacks | No `art` → blurred poster → tint gradient; app tile → brand gradient from `AppTile.color`; Sports / Search / Settings → fixed glows (§6.1, §8.1, §9.1) |

---

## 2. Plumbing (third Dashboard UI)

### 2.1 Config (`crates/core/src/config.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardUi {
    #[default]
    Classic,
    Blades,
    Ps5,
}

impl DashboardUi {
    pub fn as_str(self) -> &'static str {
        match self { Self::Classic => "classic", Self::Blades => "blades", Self::Ps5 => "ps5" }
    }
    pub fn parse(s: &str) -> Self {
        match s { "blades" => Self::Blades, "ps5" => Self::Ps5, _ => Self::Classic }
    }
    /// Settings cycle order.
    pub fn next(self) -> Self {
        match self { Self::Classic => Self::Blades, Self::Blades => Self::Ps5, Self::Ps5 => Self::Classic }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Classic => "Classic · top tabs", Self::Blades => "Blades · side tabs", Self::Ps5 => "PS5 · header tabs · app row" }
    }
}
```

```toml
[display]
dashboard_ui = "ps5"   # "classic" | "blades" | "ps5"
```

Tests: `parse("ps5")`, TOML round-trip, unknown → Classic, `next()` cycles all values.

### 2.2 Every place that branches on `"blades"` today

| File | Today | Change |
|---|---|---|
| `ui/state.slint` `Display.dashboard-ui` | `"classic" \| "blades"` | add `"ps5"`; add `in property <string> dashboard-ui-label` |
| `ui/laptop.slint` | `TvShell` if not blades / `V3Shell` if blades; bg + font from `V3Theme` or `Theme` | three `if`s (`TvShell` only for `"classic"`) + `Ps5Shell`; `PsTheme.bg` / `PsTheme.font` for `"ps5"` |
| `ui/shared/sports_alert.slint` | Blades vs Classic toast colours | PS5 branch: `PsTheme.card` bg, `#E8EAED1A` border, `Theme.accent` |
| `ui/tv/settings.slint` ~188 | Blades or Classic text | show `Display.dashboard-ui-label` |
| `ui/v3/settings.slint` ~496–610 | two option cards | one card per `DashboardUi` value (§9.3 previews) |
| `src/tv_nav.rs` ~1596 | toggles Blades ↔ Classic | `DashboardUi::parse(..).next()` |
| `src/tv_nav.rs` ~711 `tile_rects()` | `if blades { blades_tile_rects }` | `match` → `ps5_tile_rects(laptop, page)` |
| `src/callbacks.rs` ~444 | parses two values | push the label too |
| `src/callbacks.rs` ~1231 intro preview | Blades or Tiles | `Skin::from(DashboardUi::parse(..))` |
| `src/intro.rs` `Skin` | `Tiles`, `Blades` | add `Ps5` (§10) |
| test configs (`bridge/tests.rs`, `tv_nav.rs` ~2112) | `Classic` | add a PS5 case where per-UI rects are tested |

`rg -n '"blades"|DashboardUi::|Skin::' crates/` must show no two-way check left.

### 2.3 Shell and header (`ui/ps5/shell.slint`)

`Ps5Shell` owns the full-screen backdrop, the header and the page area; pages switch on `TvUi.screen` / `TvUi.active-tab` exactly as in `TvShell` / `V3Shell` (`shell_tv.rs` `Tab` / `Page` unchanged).

| Element | Spec |
|---|---|
| Header | y 44, x 96–1824, h56 |
| Left | **Home · Apps · Library · Collections · Sports · Settings** (`Tab` order), gap 44; active underlined. Sub-pages (Show, Collection, Search) put a `‹ Back` / `‹ Collections` pill left of the tabs and keep the parent tab underlined (Search underlines nothing and highlights the Search icon) |
| Right | Quick actions as `PsIconButton`s, gap 18: **Search · Screensaver · Sleep timer · Theme** (Home + Apps spec §6.2 hints), then weather 26 / 500 and clock 30 / 500 |
| Focus | One strip: ← → across tabs (switch immediately, `ShellState::switch_tab`) then quick actions; ↓ to the page's default focus |

---

## 3. Home

### 3.1 Layout

| Element | Position |
|---|---|
| Backdrop | art of the **focused row tile** |
| Row | x 96, y 140, gap 16 |
| Hero block | x 96, y 420, width 980 |
| Cards | x 96, y 790: label `Up next`, gap 16, cards h190 |

### 3.2 Row — 9 tiles from `HomeState`

| # | Tile | Content |
|---|---|---|
| 1 | **What's On** | `whats-on.art` crop (brand colour when `is-app`), 6 px progress |
| 2–8 | **Apps bar** (`apps-bar`, 7, recent first via `apps::apps_bar`) | `AppTile` logo |
| 9 | **All apps** | `glass` + 9-dot icon → Apps tab |

Width: 216 + 8 × 132 + 8 × 16 + label 280 = 1680 ≤ 1728. Default focus: What's On. Launching an app calls `push_recent`.

### 3.3 Hero (follows the focused tile)

| Focused | Kicker | Title | Meta | Buttons |
|---|---|---|---|---|
| What's On (media) | `WHAT'S ON · <source>` | `whats-on.title` | subtitle; status + 560 × 6 progress; plot 2 lines | **Resume / Play** · **From start** · ⋯ |
| What's On (mpv loaded) | same | same | `Ends at …` / `Paused at …` | **Full screen** · From start · ⋯ |
| What's On (app open) | `WHAT'S ON · NETFLIX` | logo 110 | `Open on the TV` | **Return to app** |
| What's On (empty) | `WHAT'S ON` | `Nothing on right now` | `Search or open the Library` | **Search** |
| App | `App · recent` | logo 110 | `Opens full screen on the TV` | **Open on TV** |
| All apps | `Apps` | `All N apps` | `A–Z` | **Open** |

OK → `HomeState.activate-hero()`; From start → existing play-from-start; ⋯ → Stop · Audio · Subtitles.

### 3.4 Cards

4 **Up next** (`HomeState.up-next`; first 420 wide with **UP NEXT** chip, others 300) + one 340-wide `card`: **Next Game** when visible (OK → `open-game`), else **Just Added** (OK → `activate-just-added`), else Up next spreads. Never the same title twice.

### 3.5 Focus

Row ← → (no wrap) · ↑ header · ↓ hero primary · hero ↓ first card · cards ↑ hero · Back → What's On.

### 3.6 Apps tab

`AppsPage.apps`, **A–Z**, no sort / filter / recency. Label `Apps · N`; 6-column grid of 248 × 140 r20 brand tiles + name 22 / 600 below, gap 24 × 32, vertical scroll; focused tile scales 1.08 + `focus`. OK → launch + `push_recent`.

---

## 4. Library (board "Library A")

| Element | Spec |
|---|---|
| Sub-tabs | y 124: **TV Series · Movies · Browse all** (`PsSegmented` or underlined 28 px) + **Watching now** pill (`LibraryLanding.watching-now-count`) |
| Hero | x 96, y 190, w1000: kicker · title 72 · meta · summary **fixed 2 lines** · Play / Show page |
| On this page | Right-aligned at 1824, y 190: shelf list, current shelf `text`, others `muted` |
| Shelves | 6-column wrapping grids, gap 24; 64 between shelves; page scrolls |

| TV Series | Movies | Rule | Extras |
|---|---|---|---|
| **Start watching** | **Recently added** | 6 random unstarted (reseeded Mondays) / 6 newest | Rank 1–6 |
| **Added this week** | **Continue watching** | episodes added ≤ 7 days, max 12 / `viewOffset > 0` | progress |
| **Finish a season** | — | 1–3 episodes left | segment bar |
| **Popular on Republic City** | **Popular on Republic City** | Tautulli top 30 days, 12 | rank chip |

Cap 12 per shelf (12th → **See all ›**); empty shelves hidden. Shelf data computed in `bridge/library.rs` for all UIs. Focus: grid ← → no wrap; ↓ same column into next shelf; ↑ from first row → hero Play → sub-tabs → header. Scroll keeps the focused row 120 below the header / 96 above the bottom, 200 ms.

---

## 5. Show page (boards "Show A")

One page, two states (`scrolled`), bound to `ShowPageState` (+ `play-from-start`).

**Hub:** kicker `Up next · S1 · E3 — Title · 18 min left` · title 96 · meta · summary **3 lines** · **Resume S1 · E3** + **From S1 · E1** · `Seasons` row of hub cards 400 × 180 (max 4; 5+ → 3 + `+N more ↓`) · cue `↓ Season 1 episodes`. Initial focus Resume.

**Episodes:** heavier scrim · sticky bar (title 44, up next, compact buttons) · season strip 4-col 360 × 116 (§5.1) · label `Season 1 · 8 episodes` + `2 watched · 6 left · 5 h 52 m` · 3-col episode grid (still 16:9 h320, UP NEXT chip, watched tick, progress, 2-line summary, last slot **Continue to Season 2 ›**).

### 5.1 Season stacking

| Seasons | Strip |
|---|---|
| 1–4 | 1 row |
| 5–8 | 2 rows |
| 9+ | first 7 + **All N seasons** (selected season takes slot 7 if needed) → season picker overlay (6-across 220 × 330 posters, `8 of 10 watched`) |

```rust
// src/ps5/seasons.rs
pub enum Slot { Season(usize), AllSeasons(usize) }

pub fn strip(total: usize, selected: usize) -> Vec<Slot> {
    if total <= 8 { return (0..total).map(Slot::Season).collect(); }
    let mut slots: Vec<Slot> = (0..7).map(Slot::Season).collect();
    if selected >= 7 { slots[6] = Slot::Season(selected); }
    slots.push(Slot::AllSeasons(total));
    slots
}
pub fn rows(slots: usize) -> usize { slots.div_ceil(4) }
pub fn grid_top(slots: usize) -> f32 {
    let r = rows(slots) as f32;
    244.0 + r * 116.0 + (r - 1.0) * 18.0 + 40.0
}
```

Tests: 3 → 1 row; 6 → 2 rows; 8 → 8 slots; 12 with selected 9 → slot 6 = season 9, slot 7 = All 12.

Focus: hub buttons ↓ season cards ↓ episodes state (up-next focused); strip ↑ sticky bar ↑ hub; Back → Library tile you came from.

---

## 6. Sports (boards "Sports · Scoreboard / Competitions")

### 6.1 Shell

- Header: **Sports** underlined. Fixed backdrop: `bg` + two soft glows (KU blue `#0051BA40` top-left, red `#E8000D2E` top-right). No focus backdrop.
- Sub-switch y 130: `PsSegmented` **Competitions | Scoreboard** (`TvUi.sports-tab`).

### 6.2 Scoreboard

Bound to the shared `Scoreboard` view model (`tvdash-scoreboard-build.md` §10) with the follows changes (`tvdash-follows-replace-star.md`).

| Part | Spec |
|---|---|
| Controls (y 208) | date stepper in a `glass` pill (‹ 48 · `Mon, Nov 23` 22 / 600 · › 48) · **Today** (hidden on today) · `All sports ▾` · `↻ Refresh` (h56 `glass` pills) · right: `Updates ~10m · last 9:31 AM` 18 `muted` (stale → `Data is 32 min old` in warn) |
| Groups | **Live** (live dot + 6 px halo) · **Upcoming** · **Final** · **Postponed & cancelled**; heading 26 / 600 + count 20 / 400 `muted`; start-time order; empty groups hidden |
| Grid | 3 columns, gap 20 |
| Card | `card`, r18, padding 16 × 20, gap 10: league 14 / 700 uppercase `muted` + status right (§6.3) · box score (§6.4) · footer: `Venue · TV` 15 `muted` · **★ Following** chip (`glass`, 14 / 600) when followed · **Game ↗** pill (h36; `Recap ↗` when final) |
| Focus | One target per card. Focused card: `focus` ring and the Game pill turns `#E8EAED` / `#0B0D10`. **OK opens Game ↗** (`SourceUrl`); empty URL → nothing |
| Page | Scrolls group by group under the bottom `fade` |

### 6.3 Status text

As Scoreboard spec §3: `3rd · 3:38`, `Halftime`, `2nd Int`, `End 1st`, `OT · 4:12`, `LIVE` (live colour + dot) · `Tonight · 7:00 PM` / `Tomorrow · 6:30 PM` / `Time TBD` (accent tint) · `Final`, `Final/OT` (`muted`) · `Postponed` etc. (`muted`).

### 6.4 `PsBoxScore`

Grid columns `34 (rank) · 30 (badge) · 1fr (name) · N × 26 (periods) · 44 (T)`, gaps 8. Header row 13 / 700 `muted`. Team row: rank 15 / 800 `muted` · badge 30 r8 (logo else initials on team colour) · **short** name 20 · periods 16 / 600 `muted` · total 26. Leader 800 `text`, other 600 `sub`; tie / upcoming 800 `text`. Upcoming: no period columns, T column shows `Away` / `Home`. Columns per sport from Scoreboard spec §7.

### 6.5 Competitions

Bound to the existing `Sports` global (`rows` of kind `day` / `league` / `game`, `choices`, `page`, `total-pages`, `last-updated`, `open-url`).

| Part | Spec |
|---|---|
| Controls | `All schedules ▾` (`Sports.schedule-label`) · `Schedule last updated 9:12 AM · 14 events` 18 `muted` · right: **← Earlier** · `Page 1 of 2` · **Later →** (disabled `glass` @ 50%) |
| Day row | 26 / 600 (`Today · Monday, November 23`) |
| League row | 16 / 700 uppercase `muted` |
| Game row | `card` r18, h92: overlapping badges 44 × 44 (school + opponent) · title 24 / 600 (`[School] vs / at [Opponent]`) + `Home / Away / Neutral · City, St. / Venue` 17 `muted` · time 24 / 500 tabular (150 wide; `Time TBD`) · TV / radio 18 (170) · link pills **Stream · Live stats · Audio · Details** (only those with URLs; h40) |
| Focus | Pills are the targets; ← → along a row's pills, ↓ ↑ same pill index in the next row (clamped); focused row gets `focus`, focused pill turns `#E8EAED` |

---

## 7. Collections (boards "Collections · landing / collection page")

### 7.1 Landing

Bound to the existing collections list (`CollectionCardData`: title, detail, progress, smart, poster).

| Part | Spec |
|---|---|
| Backdrop | focused collection's `art` (fallback: first movie's art) |
| Hero (x 96, y 150, w1000) | kicker `COLLECTION · PLEX · SMART` · title 72 · `8 movies · 1998–2024 · 3 watched · up next <title>` 24 · **Open collection** + **▶ Resume <title>** |
| Grid | label `ALL COLLECTIONS` + `12 collections · something left to watch first`; 6-column 2:3 posters r20, `Smart` chip, 6 px progress, name 22 / 600, detail 18 `muted`; focused scales 1.03 + `focus` |
| Order | collections with something left first, then most recently added movie (existing rule) |

### 7.2 Collection page

Bound to `CollectionPageState`.

| Part | Spec |
|---|---|
| Header | `‹ Collections` pill + tabs (Collections underlined) |
| Left (w760) | kicker · title 80 · `meta` · 420 × 8 progress + `progress-text` · summary **3 lines** · `Up next · <title> · 1 h 12 min left` · **▶ Resume <title> on TV** (or `up-next-label`) + **From start**. **No Shuffle** |
| Right | `PsSegmented` **Collection order · Release date · A–Z** (`set-order`) · **Hide watched** `PsToggle` (`set-hide-watched`) · 4-column 2:3 posters r18 with number chip, watched tick (watched dimmed 70%), resume bar, title 20 / 600 + status 16 (`2009 · 1 h 12 min left` in accent) |
| Focus | Default: Resume. ↓ / → into the grid; grid OK → `play-item(key, resume)` |

---

## 8. Search (board "Search")

Opened from the header Search icon (or Home's empty hero). Bound to `Watch`.

### 8.1 Layout

| Part | Spec |
|---|---|
| Header | `‹ Back` pill + tabs (none underlined) · Search icon shown active |
| Backdrop | top result's art (else a dark green/blue glow) |
| Left (w560) | query field h76 pill `glass` with icon + caret (`Watch.query`, physical typing too) · **6 × 6 keyboard** a–z 0–9 (keys h74 r18 `glass`; focused key `#E8EAED`) · **Space · Delete · Clear** row · **Only what I can watch now** `PsToggle` (`Watch.only-now`) |
| Right | Groups, each hidden when empty: **IN YOUR LIBRARY** (`Plays on Plex`) · **INCLUDED WITH YOUR APPS** (`Opens full screen on the TV`) · **COSTS EXTRA** (`Rent or buy`) · **NOT STREAMING**; 4-column 16:9 tiles r16 with a chip (Plex amber `#E5A00D`, included green `#30D158`, extra `#E8EAED`, none dark) + title 18 / 600 + `2019 · Movie` 15 `muted` |
| Status | `SEARCHING…` / `Watch.message` in place of the groups |
| Detail | OK on a tile → existing detail panel (`Watch.detail`, overview, yours / elsewhere, `detail-actions`) as a right-side `card` sheet over the results |

Typing is debounced (`Watch.edited`). Focus: keyboard grid ↔ results (→ from the keyboard's right column), Back closes the detail, then Search.

---

## 9. Settings (board "Settings · Display")

### 9.1 Layout

- Header: **Settings** underlined; backdrop glow `#2A3550` top-right.
- Left (w380): sections as h74 pills with icons — **Display · Accounts · Sites · Sports · Server · Diagnostics · What's new** (`TvUi.settings-tab` keys `display`, `accounts`, `sites`, `sports`, `server`, `diagnostics`, `changelog`). Selected = `#E8EAED` fill, `#0B0D10` text.
- Right: the section's content; rows are `card` r18 h72: name 22 / 600 + hint 15 `muted` · value 20 `sub` · optional pill button · `›`.

### 9.2 Display

| Row | Control | Binds |
|---|---|---|
| **Dashboard UI** | 4 preview cards (16:9 r16, name 21 / 600, sub 16 `muted`, ✓ on the selected one): **Classic** (top tabs · tile hub) · **Blades** (side tabs) · **PS5** (header tabs · app row) · **macOS Stage** (menu bar · dock; only once that UI exists) | `Display.set-dashboard-ui` |
| Theme | value `Kansas Dark` (hint `Accent colour in PS5`), OK cycles | existing theme cycle |
| Time format | `12h` / `24h` | existing |
| Weather location | value, OK edits (City or ZIP) | existing |
| Weather units | `°F` / `°C` | existing |
| Startup animation | `On` / `Off` + **Preview** pill | `Intro.preview` |
| Clear recent services | OK clears (hint `Resets the app row order`) | existing |

### 9.3 Other sections

Same content as the Blades Settings, restyled as PS5 rows: **Accounts** (Plex / Jellyfin / Tautulli sign-in rows) · **Sites** (streaming sites list + `+ Add site`, Edit) · **Sports** (**Following**: `Teams & conferences — alerts on` / `Leagues — scores only`, footer `Edit follows in MyNewsFeed › Admin › Results`; alert toggles **Starting soon**, **Final score**, **During video**) · **Server** · **Diagnostics** (Service health, `Alerts: N teams · N conferences · N leagues · checked …`, Scan all) · **What's new** (changelog list).

---

## 10. Startup animation (boards "Startup animation storyboard / animated")

`Skin::Ps5` plugs into the existing `IntroGate` (`logo → hold → reveal → done`); the gate itself doesn't change. **Timings below are the approved (slowed, ×1.25) ones.**

| Stage | Time | What moves |
|---|---|---|
| Logo | 0 → **2.0 s** (`logo_len`) | Black. Wordmark (`tv` 300 + `dash` 800 in accent, 150 px) fades 0 → 1 over 0.5 s and scales 0.96 → 1 over 0.75 s; soft accent glow behind |
| Hold | only while Home data isn't ready (max `startup.max_hold_secs`, 6 s) | 340 × 5 track under the logo with a 30% shimmer sliding across every 1.0 s |
| Reveal | **2.125 s** (`reveal_len`) from reveal start | 0 → 0.75 s: backdrop 0 → 1; logo moves toward the header (translate −760, −470) scaling to 0.35 and fading out · 0.25 → 0.75 s: header fades in · 0.75 s + 50 ms × i: row tile *i* (0–8) rises 40 px → 0 and fades in over 0.44 s · 1.375 → 1.75 s: hero text fades in (+12 px rise) · 1.5 → 1.875 s: Up next cards fade in · 1.875 → 2.125 s: focus ring on What's On |

- All moves are opacity / translate / scale with `animate` (ease-out); no blur.
- **Preview** in Settings replays it; Startup animation **Off** goes straight to Home.

```rust
// src/intro.rs
Skin::Ps5 => (Duration::from_millis(2000), Duration::from_millis(2125)), // logo_len, reveal_len
```

---

## 11. Open questions

| # | Question | Default |
|---|---|---|
| 1 | Should Theme restyle PS5 surfaces or only the accent? | **Accent only**; PS5 stays dark |
| 2 | Library shelf count | **Four** for PS5 |
| 3 | Hub season cards with 5+ seasons | 3 + `+N more ↓` |
| 4 | Season glow follows the focused season's poster? | No |
| 5 | Control Center (Home C) / Plex-Streaming switch (Home B) | Not in this spec |
| 6 | Update the Home A board to the new row and cards | This spec is the source of truth until then |

---

## 12. Files

```text
crates/core/src/config.rs        # DashboardUi::Ps5, next(), label() + tests
crates/app/
├─ src/intro.rs                  # Skin::Ps5 + timings (§10)
├─ src/tv_nav.rs                 # three-way dispatch; ps5_tile_rects() for every page
├─ src/callbacks.rs              # intro preview skin; dashboard label
├─ src/ps5/{mod.rs, seasons.rs, shelves.rs}
├─ src/bridge/library.rs         # Added this week / Popular (shared)
└─ ui/
   ├─ laptop.slint, state.slint, shared/sports_alert.slint, tv/settings.slint, v3/settings.slint
   └─ ps5/
      ├─ theme.slint  shell.slint  intro.slint
      ├─ row_tile.slint  buttons.slint  cards.slint  controls.slint   # segmented, toggle
      ├─ home.slint  apps.slint  library.slint  show.slint  season_picker.slint
      ├─ sports.slint  box_score.slint  competitions.slint
      ├─ collections.slint  collection.slint
      ├─ search.slint  settings.slint
```

---

## 13. Task list

1. **Plumbing:** `DashboardUi::Ps5` + tests · three-way switch table (§2.2) · `PsTheme`, `Ps5Shell`, header · `FocusBackdrop` full-screen variant · fonts.
2. **Home + Apps:** row, hero, cards, focus map (§3).
3. **Library:** sub-tabs, hero, 4 shelves, On this page, Browse all, scroll (§4).
4. **Show:** hub, episodes, stacking + picker (§5).
5. **Sports:** Scoreboard cards + box score, Competitions rows, focus (§6).
6. **Collections:** landing, collection page (§7).
7. **Search:** keyboard, groups, detail sheet (§8).
8. **Settings:** sections, Display picker, other sections (§9).
9. **Startup animation** + Preview (§10).
10. **Polish & release:** 1080p / 1440p / 4K; hold-arrow test (no flicker / jumps); toasts, Sleep warning and Up Next modal readable over backdrops; mouse hover / wheel / click; tests per crate, lightest first (`cargo test -j 2 -p tvdash-core`, …, `cargo test -j 2 -p tvdash -- --test-threads=1`); version bump in `Cargo.toml`, `CHANGELOG.md`, `docs/fedora-install.txt`, `docs/status.md`, `releases/README.md`; the user runs the release build and RPM (`GEMINI.md`).

## 14. Done when

- [ ] Settings › Display switches between Classic, Blades and PS5 from every UI; the choice survives a restart.
- [ ] Every tab and sub-page opens in the PS5 look; nothing falls back to Classic.
- [ ] PS5 Home shows the same What's On, Up next, Next Game / Just Added and 7 apps as Classic and Blades at the same moment.
- [ ] Library and Show never scroll sideways; shelves wrap at 6; season stacking matches §5.1.
- [ ] Scoreboard: box scores per sport, leader bold, red only for live, ★ Following on followed games, OK opens the game page; Competitions links open in the TV browser.
- [ ] Collections: order and Hide watched work; Resume plays the up-next movie; no Shuffle anywhere.
- [ ] Search finds library and streaming results with the right chips; the filter hides Costs extra / Not streaming.
- [ ] The PS5 intro plays with the §10 timings, holds only while loading, and Preview replays it.
- [ ] Looks right at 1080p, 1440p and 4K; works by remote alone.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-23 b | **Full UI.** Added Sports (Scoreboard + Competitions), Collections (landing + collection page), Search, Settings (all sections, 4-option Dashboard UI picker) and the startup animation with the approved slowed timings (logo 2.0 s, reveal 2.125 s). Removed the "host Classic pages" fallback. Sports uses ★ Following (no Star); OK on a card opens the game. Added `PsSegmented`, `PsToggle`, `PsBoxScore` |
| 2026-11-23 | Revised for v0.1.68: third `DashboardUi` value, switch table, header tabs + quick actions, Home on `HomeState`, PS5 Apps tab, shared Library data, `ShowPageState`, PS5 intro, `ImageCache` + bundled logos |
| 2026-11-20 | Spec created: Home A, Library A, Show A |
