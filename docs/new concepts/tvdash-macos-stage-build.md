# tvdash — macOS Stage Dashboard UI — Build spec (2026-11-23, for v0.1.68)

Adds a **fourth Dashboard UI**, `macos`, inspired by macOS **Stage Manager + Spotlight**: a menu bar, a Spotlight search bar, the other sections as a stack of window previews on the left, the current section in a window on the centre stage, and a dock. It sits next to Classic, Blades and PS5. Decisions and history: `tvdash-macos-stage-skin.md`.

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **macOS-inspired (pinned)**

| Board | Section |
|---|---|
| Stage · Home | §3 Home |
| Stage · Library (Tweaks: TV Series / Movies) | §4 Library |
| Stage · Apps (A–Z) | §5 Apps |
| Stage · Show page, top / scrolled to episodes (Tweaks: seasons) | §6 Show page |
| Stage · Sports · Scoreboard / Competitions | §7 Sports |
| Stage · Collections landing / Collection page | §8 Collections |
| Stage · Search (Spotlight) | §9 Search |
| Stage · Settings · Display | §10 Settings |
| Stage · Startup animation storyboard / animated | §11 Startup animation |

All sizes are **1920 × 1080 design units** scaled by `TvUi.u`. Sample titles, teams and scores on the canvas are placeholders.

> **Depends on `tvdash-ps5-a-build.md` §2.** Build PS5 first (or at least its plumbing): this spec adds one more value to the same switch table. If macOS is built first, apply PS5 §2 with `Macos` in place of `Ps5`.

---

## 0. Decisions

- **Inspired, not copied:** no Apple logos, icons or wallpapers. Window controls are **three grey dots** (not red / yellow / green), the font is **Inter** (not SF), apps are brand-coloured rounded squares with their names or bundled logos.
- **Parity:** same globals and Rust as Classic, Blades and PS5. Nothing is computed per UI.
- **No Shuffle, no Star** (★ Following only, from MyNewsFeed follows).
- **Show page and season stacking** follow PS5 §5 (rows of 4, max 2 rows, 9+ → 7 + All N seasons).
- **Two Slint workarounds (agreed):** pre-blurred backdrop for frosted glass (§1.4) and flat stack previews instead of 3D tilt (§2.4).

---

## 1. Shared foundation

### 1.1 Tokens (`ui/macos/theme.slint`, `export global MacTheme`)

| Token | Value | Use |
|---|---|---|
| `window` | `#1C1C1E` | Window body |
| `titlebar` | `#2C2C2E` @ 95% + 1 px `#FFFFFF14` bottom | Window title bars |
| `sidebar` | `#262628` / `#232325` | Sidebars, inspector panes |
| `raised` | `#2C2C2E` + 1 px `#FFFFFF1A` | Cards in windows (Scoreboard, Settings groups) |
| `glass` | `#1C1C1E` @ 55% over the **blurred** backdrop + 1 px `#FFFFFF1A` | Menu bar, Spotlight, dock, widgets (§1.4) |
| `control` | `#FFFFFF14` (pressed / selected `#FFFFFF2E`) | Buttons, segmented tracks, keys |
| `popup` | `#48484A` | Popup menus, segmented thumbs |
| `text` / `sub` / `muted` | `#F5F5F7` / `#C7C7CC` / `#8E8E93` | Text |
| `accent` | `Theme.accent`, default `#0A84FF` | Selection, primary buttons, focus |
| `live` | `#FF453A` (text `#FF6961`) | Live dot and status only |
| `dot` | `#5A5A5E` | Window control dots |
| Radii | window 16 · widget 24 · card 18 · button 10–12 · dock icon 18–20 · dock 26 · poster 10–12 | |

Fonts: **Inter** 400 / 500 / 600 / 700 / 800, bundled. Tabular numerals for clock, scores and times.

**Focus** (TV remote): `accent` ring 3 px with a 3 px `window`-coloured gap (`0 0 0 3px window, 0 0 0 6px accent`), plus Finder-style selection where the design shows it (accent-filled name label or row).

### 1.2 Wallpaper

Fixed per-UI wallpaper (not the focus backdrop): `radial-gradient(80% 60% at 50% 0%, #C0603A, #7A2E4A 40%, #2A1030 75%, #120814)` + a `#3A2A8A66` glow bottom-left. Rendered once to an image at window size so it can be blurred (§1.4).

### 1.3 Components

| Component | Spec |
|---|---|
| `MacMenuBar` | h44, `glass`; left `tvdash` 18 / 800 + current section 18 / 700; right `Sleep · Off` · `Kansas Dark` · `34°F Clear` · `Mon Nov 23  9:35 AM` (600) |
| `MacSpotlight` | centred, y66, 760 × 56, r16, `glass` @ 70%, shadow `0 20 50 #0008`; search icon + placeholder (`Search shows, movies and apps`, or per page). OK / typing → Search (§9) |
| `MacStack` | §2.4 |
| `MacWindow` | r16, `window`, shadow `0 0 0 1px #FFF1F, 0 50 120 #000D`; title bar h52 with dots, optional `‹ Back` button, title 17 / 700, subtitle 15 `muted`, optional centred `MacSegmented` |
| `MacDock` | centred, bottom 14, padding 10 × 16, r26, `glass`; 7 recent apps + **All apps** (72–80 px icons r18–20), divider, quick actions (Search on Home only, Screensaver, Sleep). Focused icon scales 1.15 and shows its name in a tooltip above |
| `MacWidget` | r24, `glass` |
| `MacSegmented` | track `control` r9 padding 3; options padding 5 × 16–20, 14–15; selected `#FFFFFF2E` 700 |
| `MacToggle` | 42 × 24 track (`popup` off, `accent` on), 20 px white knob |
| `MacPopup` | h28 r7 `popup`, value + `⌄` |
| `MacBoxScore` | §7.3 |

### 1.4 Frosted glass (workaround)

Slint can't blur what's behind an element. Instead:

1. When the wallpaper (or the current backdrop) is rendered, also make a **blurred copy** in Rust with the `image` crate (`imageops::blur`, σ ≈ 30 design px) at the same size. Cache both in `ImageCache`, keyed by source + size.
2. Every `glass` element draws, clipped to its own rect, the blurred copy positioned at the element's **screen** coordinates (`x: -root.absolute-position.x`, …), then a tint (`#1C1C1E` @ 55%) and the 1 px border.
3. Re-blur only when the wallpaper / backdrop or the window size changes (never per frame).

Fallback (low-end GPU, or a setting): skip step 2 and draw the tint at 80%.

```rust
// src/macos/glass.rs
pub fn blurred(src: &image::RgbaImage, sigma: f32) -> image::RgbaImage {
    image::imageops::blur(src, sigma)
}
```

---

## 2. Plumbing and shell

### 2.1 Dashboard UI value

Extend PS5 §2.1: `DashboardUi::Macos` (`"macos"`), `next()` order Classic → Blades → PS5 → macOS Stage → Classic, `label()` `macOS Stage · menu bar · dock`. Extend every row of PS5 §2.2 with a `Macos` arm: `laptop.slint` → `MacShell`, sports alert toast colours (`raised` + accent), Settings label / option cards, `tv_nav.rs` → `macos_tile_rects`, intro `Skin::Macos`.

### 2.2 Shell (`ui/macos/shell.slint`)

Layering, back to front: wallpaper → `MacStack` → centre `MacWindow` (current page) → widgets (Home only) → `MacSpotlight` → `MacMenuBar` → `MacDock` → overlays (Search panel, toasts, Sleep warning).

| Area | Rect |
|---|---|
| Stack | x 60, y 160, w250 |
| Centre window | Home: x 360, y 160, 1050 × 640 · every other page: x 360, y 146, **1500 × 790** |
| Widgets (Home) | x 1450, y 160, w400 |
| Dock | bottom 14, centred |

Pages switch on `TvUi.screen` / `TvUi.active-tab` (`shell_tv.rs` unchanged). Sub-pages (Show, Collection) stay in the same window with a `‹ Library` / `‹ Collections` button in its title bar.

### 2.3 Focus regions

`menu bar ↔ Spotlight ↔ stack ↔ window content ↔ widgets ↔ dock`. ← from the window's left edge → stack; → from its right edge → widgets (Home) ; ↓ from the window's bottom → dock; ↑ from the top → Spotlight → menu bar (Sleep, Theme are focusable there). Back: sub-page → page → Home.

### 2.4 Section stack (workaround)

- The other five sections as **flat** previews, 230 × 100 r10, `#1C1C1E` with a 16 px mini title bar (three 6 px dots), body = the section's gradient + its 38 px stroke icon; name 16 / 700 below; gap 16.
- **No 3D tilt** (Slint has no perspective). To keep the Stage Manager feel: previews are **offset 8 px right and scaled 0.96** each step down, with a soft shadow. A pre-rendered skewed image is the fallback if flat looks too plain.
- Previews are **static** (icon + gradient), not live screenshots.
- Order: `Tab` order without the current section. OK on a preview → switch tab (the window animates to the stack while the new one comes forward, 250 ms).

| Section | Gradient | Icon |
|---|---|---|
| Home | `#5D7F4B → #101A0C` | house |
| Apps | `#FF3B30 → #0B2E6B` | 2 × 2 grid |
| Library | `#6A4A8C → #1E1428` | books |
| Collections | `#B08A2E → #2E230B` | stacked box |
| Sports | `#0051BA → #E8000D` | ball |
| Settings | `#48484A → #1C1C1E` | gear |

---

## 3. Home (board "Stage · Home")

| Part | Spec |
|---|---|
| Window | "What's On — Plex" title; body = `whats-on.art` + bottom scrim; kicker `WHAT'S ON · <source>` 15 / 700, title 62 / 800, subtitle 20, plot 2 lines, progress 420 × 6 + status, **▶ Resume** (accent) + **From start** |
| Widgets | **Clock + weather** (h150: date 15, time 66 / 300, temp 40 / 300, `Clear · H:43 L:23`) · **Next Game** (h170; same content and rules as Home spec §5; falls back to **Just Added** when hidden) · **Up next** list (4 rows: 72 × 42 thumb, title 16 / 700, `43 m left`) |
| Dock | `HomeState.apps-bar` (7) + All apps · divider · Screensaver, Sleep |
| Focus | Default: Resume. OK on window → `activate-hero`; Up next row → `activate-up-next`; Next Game → `open-game` |

App open in Brave (What's On `is-app`): the window shows the brand colour + logo and **Return to app**.

---

## 4. Library (board "Stage · Library")

Window 1500 × 790, title `Library`, centred `MacSegmented` **TV Series · Movies · Browse all**, right **Watching now · 1** pill (green dot).

| Part | Spec |
|---|---|
| Backdrop | focused item's art in the window's top-right (960 × 330) with left / bottom scrims |
| Text block | kicker 14 / 700 accent · title 50 / 800 · summary **3 lines** (81 px) · **▶ Play E6** / **▶ Resume** + **Show page** / **From start** |
| On this page | 270-wide frosted card top-right: TV: Finish the season · Start watching · Added this week · Popular on Republic City; Movies: Continue watching · Recently added · Popular on Republic City |
| Shelf 1 | TV **Finish the season** / Movies **Continue watching**: 4 cards 16:9 h140 r12 (art + scrim, title 18 / 800, meta 13, segment bar or progress) |
| Shelf 2 | TV **Start watching** / Movies **Recently added**: 6 posters 116 × 174 r10 with a numbered accent badge, title 16 / 700 + meta 13 beside |
| Further shelves | Below the fold (window scrolls vertically), PS5 §4 rules |

Data: same shelf data as PS5 §4 (shared in `bridge/library.rs`).

---

## 5. Apps (board "Stage · Apps")

Window title `Apps` + `12 apps · A–Z`. **Launchpad-style** grid: 6 columns, icons 156 × 156 r36 (brand colour + logo, or name), name 17 / 600 below, gap 56 × 30. Focused icon scales 1.06 + accent ring. Footer `Opens full screen on the TV · F12 or the Back to tvdash pill brings you back`. `AppsPage.apps`, A–Z, no sort / filter / recency. OK → launch + `push_recent`.

---

## 6. Show page (boards "Stage · Show page")

PS5 §5 structure inside the window (title `[Show]`, subtitle `TV Series`, `‹ Library`).

- **Top:** backdrop fills the window body with left / bottom scrims; kicker `UP NEXT · S2 · E6 — Title · 18 MIN LEFT`; title 66 / 800; meta; summary **3 lines**; **▶ Resume S2 · E6** (accent) + **From S1 · E1**; right: a 440-wide **Quick Look card** (still 16:9 h236 with UP NEXT chip and progress, `E6 · Title`, `Season 2 · 18 min left of 52`); `Seasons` row of cards 340 × 160 (max 4; 5+ → 3 + `+N more ↓`); cue `↓ Season 2 episodes`.
- **Episodes:** sticky toolbar h64 (title 26 / 800, up next, compact buttons) · season strip 4-col h96 (selected = light card `#F5F5F7` with dark text) · label · 3-col episode grid (still h250 r14, UP NEXT chip, watched ✓ circle, progress, 2-line summary, **Continue to Season 3 ›** dashed).
- Stacking and picker: PS5 §5.1 (`src/ps5/seasons.rs`, shared — move to `src/seasons.rs` when both UIs use it).

---

## 7. Sports (boards "Stage · Sports")

Window title `Sports`, centred `MacSegmented` **Competitions | Scoreboard**. Spotlight placeholder `Search teams and games`.

### 7.1 Scoreboard

| Part | Spec |
|---|---|
| Toolbar (h60, `sidebar` bg) | date stepper (`control`: ‹ · `Mon, Nov 23` 15 / 700 · ›) · **Today** (hidden on today) · `All sports ⌄` · `↻ Refresh` · right `Updates ~10m · last 9:31 AM` 14 `muted` |
| Groups | Live (10 px live dot + halo) · Upcoming · Final · Postponed & cancelled; heading 19 / 800 + count 15 `muted`; start-time order |
| Grid | 3 columns, gap 16; window body scrolls |
| Card | `raised` r18, padding 14 × 18: league 12 / 700 `muted` + status (§ PS5 6.3 wording; live `live`, upcoming accent tint) · box score (§7.3) · footer `Venue · TV` 13 `muted` · **★ Following** (accent @ 20% chip) · **Game ↗** (`Recap ↗` when final) |
| Focus | One target per card (accent ring); **OK opens the game page** |

### 7.2 Competitions

Finder **list view**: toolbar (`All schedules ⌄`, `Schedule last updated 9:12 AM · 14 events`, `‹ Earlier` · `Page 1 of 2` · `Later ›`) · column header (`Time ⌃` · Event · Sport · TV · Location · Links, 13 / 600 `muted`) · day bars h40 `sidebar` 15 / 800 · game rows h58 striped (`#FFFFFF08`), selected row filled `accent` (text white, sub 85%): time 700 tabular, opponent badge 26 + title 700, sport, TV, `Home · City, St. / Venue`, link buttons **Stream · Stats · Audio · Details** (h28 r7). Bound to `Sports` (`rows`, `choices`, paging, `open-url`). Focus: rows ↑ ↓, links ← →.

### 7.3 `MacBoxScore`

PS5 §6.4 grid with macOS sizes: `30 · 28 · 1fr · N × 24 · 40`, gaps 8 × 7; header 12 / 700; rank 13 / 700; badge 28 r8; name 17; periods 14 / 500 `muted`; total 22. Leader 800 `text`, other 500 `muted`; upcoming shows `Away` / `Home`.

---

## 8. Collections (boards "Stage · Collections")

- **Landing:** window title `Collections` + `12 collections`, right `Icons | List` toggle (List = Finder list view, same as §7.2 styling: Name · Movies · Watched · Smart). Note line `Something left to watch first, then most recently added`. **Icon view:** 6 columns, posters 166 × 249 r12 (Smart chip, 4 px progress), name 16 / 700 (selected: accent-filled label, Finder style) + `8 movies · 3 watched` 13. Selected tile: `#FFFFFF14` rounded background + accent ring. OK → collection page.
- **Collection page** (`CollectionPageState`): title bar `‹ Collections` + name + `Smart collection`. Left 480 `sidebar` pane: art 250 high fading into the pane, kicker chip, title 40 / 800, meta, progress + `3 of 8 watched`, summary **3 lines**, `Up next · Title · 1 h 12 min left`, **▶ Resume <title> on TV** (accent) + **From start**. Right: toolbar with `MacSegmented` **Collection order · Release date · A–Z** + **Hide watched** `MacToggle`; 4-column posters 156 × 234 r10 (number chip, watched ✓, resume bar, watched dimmed 60%), title 15 / 700 + status 12 (resume line in accent). **No Shuffle.**

---

## 9. Search (board "Stage · Search (Spotlight)")

Opened by OK / typing on Spotlight, the dock Search icon, or Home's empty hero. Bound to `Watch`.

| Part | Spec |
|---|---|
| Overlay | desktop dimmed (`#0000008C` below the menu bar); Spotlight grows into a **panel** x 240, y 62, 1440 × 880, r22, `glass` @ 95%, shadow `0 50 120 #000D` (250 ms grow from the bar) |
| Query row (h76) | search icon + query 28 + accent caret; right hint `Library · your apps · where to watch` |
| Left (w380) | 6 × 6 keyboard (keys h50 r10 `control`; focused key accent) · Space / Delete / Clear · **Only what I can watch now** `MacToggle` |
| Middle (w540) | Spotlight-style **result list**: section headers 13 / 700 `muted` — **Top hit** · **In your library** · **Included with your apps** · **Costs extra** · **Not streaming**; rows h58 r10 (poster 34 × 50, title 16 / 700, meta 13, chip: Plex `#E5A00D`, app green `#30D158`, Rent / Buy `popup`, none dark); selected row filled accent |
| Right | **Preview** of the selected row: art 270 high fading into the panel, title 32 / 800, meta, overview 3 lines, `On your apps` / `Elsewhere` (from `detail-yours` / `detail-elsewhere`), action buttons from `Watch.detail-actions` (e.g. **▶ Resume on Plex**, **From start**, **Open in Netflix**) |
| Close | Back (or clearing the query and Back) shrinks the panel into Spotlight |

Selecting a row updates the preview (`Watch.select`); OK on an action → `Watch.action`.

---

## 10. Settings (board "Stage · Settings · Display")

A **System Settings-style** window (stack shows the other five sections).

| Part | Spec |
|---|---|
| Sidebar (w340) | dots · search field (`Search`) · profile row (avatar `J` in accent, `Jason`, `Plex · Republic City`) · sections with 26 px coloured rounded icons: **Display** (blue) · **Accounts** (indigo) · **Sites** (pink) · **Sports** (orange) · **Server** (green) · **Diagnostics** (grey) · **What's new** (purple); selected = accent fill |
| Pane header | `‹ ›` + section name 18 / 700 |
| Groups | `raised` r12 boxes; rows h54: name 15 / 600 + hint 12 `muted`, control on the right |

**Display:**

| Row | Control |
|---|---|
| **Dashboard UI** | group with 4 thumbnails (16:9 r9, name below; selected = accent ring + accent-filled name): Classic · Blades · PS5 · **macOS Stage** |
| Theme | `MacPopup` (`Kansas Dark ⌄`), hint `Accent and window tint` |
| Time format | `MacSegmented` 12h / 24h |
| Weather location | text field 220 (`City or ZIP`) |
| Weather units | `MacSegmented` °F / °C |
| Startup animation | **Preview** button + `MacToggle` |
| Recent services | **Clear** button, hint `Resets the dock order` |

Other sections: same content as PS5 §9.3 in macOS group / row styling (Sports › **Following** grouped `Teams & conferences — alerts on` / `Leagues — scores only`, alert toggles).

---

## 11. Startup animation (boards "Stage · Startup animation")

`Skin::Macos` in `IntroGate`. **Approved (slowed, ×1.25) timings.**

| Stage | Time | What moves |
|---|---|---|
| Boot | 0 → **2.0 s** (`logo_len`) | Black. Wordmark (130 px, `dash` in accent) + 300 × 6 progress bar fade in over 0.375 s; the bar fills to 88% by 2.0 s |
| Hold | only while Home loads (max 6 s) | the bar's last part pulses (opacity 1 ↔ 0.45, 0.5 s period); when data is ready it fills to 100% (0.25 s) and the boot screen fades out with a 1.04 scale (0.375 s) |
| Reveal | **1.9 s** (`reveal_len`) from reveal start | 0 → 0.5 s: wallpaper (and its blurred copy) fades in · 0.25 → 0.625 s: menu bar slides down from −50 px · 0.375 → 0.75 s: dock rises from +160 px, overshoots −14 px, settles by 0.94 s · 0.875 s + 75 ms × i: stack preview *i* (0–4) slides in from −120 px over 0.375 s · 1.0 → 1.44 s: centre window fades in and scales 0.94 → 1 · 1.31 → 1.69 s: Spotlight drops in from −30 px · 1.5 → 1.875 s: widgets fade in |

- All opacity / translate / scale (`animate`, ease-out; the dock uses an overshoot curve). No live blur during the animation: the blurred wallpaper is prepared during the boot stage.
- **Preview** in Settings replays it; **Off** goes straight to Home.

```rust
// src/intro.rs
Skin::Macos => (Duration::from_millis(2000), Duration::from_millis(1900)), // logo_len, reveal_len
```

---

## 12. Files

```text
crates/core/src/config.rs        # DashboardUi::Macos (+ PS5 values)
crates/app/
├─ src/intro.rs                  # Skin::Macos + timings
├─ src/tv_nav.rs                 # macos_tile_rects() for every page and region (§2.3)
├─ src/macos/{mod.rs, glass.rs}  # blurred wallpaper / backdrop (§1.4)
├─ src/seasons.rs                # shared stacking (moved from src/ps5/)
└─ ui/macos/
   ├─ theme.slint  shell.slint  intro.slint
   ├─ menu_bar.slint  spotlight.slint  stack.slint  dock.slint  window.slint  widgets.slint
   ├─ controls.slint            # MacSegmented, MacToggle, MacPopup, buttons
   ├─ home.slint  library.slint  apps.slint  show.slint  season_picker.slint
   ├─ sports.slint  box_score.slint  competitions.slint
   ├─ collections.slint  collection.slint
   ├─ search.slint  settings.slint
```

## 13. Task list

1. **Plumbing:** `DashboardUi::Macos`; four-way switch table; `MacTheme`, fonts.
2. **Glass:** blurred wallpaper / backdrop in `ImageCache`; `glass` element; fallback.
3. **Shell:** menu bar, Spotlight bar, flat stack (§2.4), centre window, dock, focus regions.
4. **Home** (§3) · **Apps** (§5) · **Library** (§4) · **Show** (§6).
5. **Sports** (§7) · **Collections** (§8).
6. **Search panel** (§9) · **Settings** (§10).
7. **Startup animation** + Preview (§11).
8. **Polish & release:** 1080p / 1440p / 4K (re-blur on resize); hold-arrow test; toasts / Sleep warning / Up Next modal over glass; mouse; tests per crate (lightest first, `-j 2`); version bump per `GEMINI.md`; the user runs the release build and RPM.

## 14. Done when

- [ ] Settings › Display offers Classic, Blades, PS5 and macOS Stage in every UI; the choice survives a restart.
- [ ] Every tab and sub-page opens in the macOS Stage look; the stack always shows the other five sections and switches tabs.
- [ ] Menu bar, Spotlight, dock and widgets look frosted (blurred wallpaper), or tinted when the fallback is on; no per-frame blur.
- [ ] Home, Library, Show, Sports, Collections and Search show the same content as the other UIs at the same moment.
- [ ] Spotlight opens the Search panel; results group correctly; the preview's actions play / open the right thing.
- [ ] The intro plays with the §11 timings, holds only while loading, and Preview replays it.
- [ ] No Apple branding anywhere; works by remote alone; looks right at 1080p, 1440p and 4K.

## Change log

| Date | Change |
|---|---|
| 2026-11-23 | Spec created from the macOS-inspired canvas: shell (menu bar, Spotlight, flat stack, centre window, dock), Home, Library, Apps, Show, Sports, Collections, Search panel, System Settings-style Settings, startup animation (boot 2.0 s, reveal 1.9 s); frosted glass via pre-blurred backdrop |
