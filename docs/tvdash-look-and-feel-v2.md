# tvdash — Look & Feel v2 (2026-11-18)

The design decisions from the 2026-11-18 look-and-feel round. Built from the design brief plus Jason's Xbox reference photos (Series X/S rows, Xbox One tiles, Xbox 360 blades).

**Design canvas:** https://claude.ai/artifact/Rn5m3jsq6BYT5j4yVcd5PK ("tvdash — Look & feel concepts")

| Canvas page | Chosen direction | Boards |
|---|---|---|
| **TV only** | **Tile hub** (Xbox One–style flat tiles, tabs across the top) | Home · Library (Panorama groups) · Show detail · Services · Search · Settings · Player controls · Ambient idle · Back-to-tvdash pill · After closing |
| **Two screens** | **Blades, detail first** (Xbox 360–style side tabs, one panel) | Home · Where to watch · Confirm playback · Up next · Library · Show page · Collection page · Apps · Settings · Screensaver |

Build docs, one per layout:
- `tvdash-tv-only-build.md`
- `tvdash-two-screen-build.md`

> **What this replaces:** the *look* of the TV-only side-rail Home (`tvdash-tv-only-plan.md` §4, §11 rail) and the laptop's Option B three-box dashboard (`tvdash-laptop-current.md` §3). **Data and rules stay as they are:** Plex only, the ten fixed apps, poster tint (Option B), Dark / Stone / Mist, Library smart shelves (Watching now, Finish a season, Added this week, Watch again, Popular on Republic City), the season-selector rules, Collections, Screensaver 3, auto layout, the Server page and the Leaf feature.

---

## 1. Shared foundation (both layouts)

### Tokens (from the brief)

| Token | Dark | Stone | Mist | Use |
|---|---|---|---|---|
| `bg` | `#0B0D10` | `#CBC5BA` | `#C3CBD3` | Canvas |
| `bg-night` | `#050608` | `#050608` | `#050608` | Idle / screensaver |
| `surface` | `#14171C` | `#DAD5CC` | `#D3DAE1` | Panels, modals |
| `surface-raised` | `#1B1F26` | `#E3DFD7` | `#DEE3E8` | Buttons, rows, chips |
| `border` | `#2A2F37` | `#B9B2A6` | `#AEB7C1` | 1 px hairlines |
| `text-primary` | `#E8EAED` | `#1F1D1A` | `#1A1E23` | Headings |
| `text-secondary` | `#A0A6AF` | `#47433D` | `#3F464F` | Metadata |
| `text-muted` | `#8A9099` | `#4E4943` | `#47505A` | Labels, hints |
| `accent` | `#5B9DFF` | `#194D9F` | `#194D9F` | Non-tinted CTAs |
| `ok` / `warn` / `alert` | `#4CC38A` / `#E5A84B` / `#F0605D` | `#1E6B45` / `#7A4E00` / `#9E2622` | same as Stone | Status |
| `scrim` | `#0B0D10C0` | same | same | Badges on art, modal backdrop |

- **Font:** Inter 300 / 400 / 500 / 600 (800 only for the typographic service wordmarks). Tabular numerals for clocks and times.
- **Poster tint:** unchanged from `tvdash-laptop-current.md` §2 (Okhsl hue from the poster, fixed lightness per theme, contrast unit test over 36 hues). New use in v2: the TV **utility tiles** (clock tile, Search, Library, Settings tiles) take the tint `poster` colour, so the whole Home shifts with what's playing.
- **Poster and backdrop art stays dark in every theme**, so text on art is always light.

### Service tiles

- Each of the ten apps is a tile **filled with the service's own colour**, name as a bold typographic wordmark (or the bundled SVG logo when present).
- This is a deliberate departure from the brief's neutral tiles: they're the one place strong colour is allowed.

| App | Fill | Wordmark |
|---|---|---|
| YouTube TV | `#F1F1F1` | `#D50000` + "TV" `#1F1F1F` |
| YouTube | `#F1F1F1` | `#D50000` |
| Disney+ | `#2A74D6 → #0B2E6B` | `#FFFFFF` |
| Hulu | `#16191E` | `#1CE783` |
| Apple TV | `#1C1C1E` | `#FFFFFF` |
| Netflix | `#121212` | `#E50914` |
| Fandango at Home | `#1B2A5C` | `#FF8A2A` + "at home" white |
| Prime Video | `#1A8CFF` | `#FFFFFF` |
| Crunchyroll | `#F47521` | `#FFFFFF` |
| Tubi | `#7408FF` | `#FFF200` |

### Recent services (last 3)

- tvdash remembers the **last 3 distinct services opened**, newest first. No full history.
- **Home shows only those 3** (plus an "All 10 apps" tile). The Services / Apps page shows all ten, with the recent three marked **RECENT**.
- Stored in `config.toml`:

```toml
[streaming]
recent = [
  { id = "netflix",     at = 1763475600 },
  { id = "youtube",     at = 1763389200 },
  { id = "crunchyroll", at = 1763302800 },
]
```

- Opening a service moves it to the front, drops any older copy and truncates to 3. Empty on a fresh install → Home shows the first three in the fixed order.
- Settings has **Clear recent services**.

### Back to tvdash (closing a full-screen service)

Two ways back, both call the same `home` command:

| Way | How | When |
|---|---|---|
| **Pill** | A small Brave extension shipped with tvdash adds **← Back to tvdash [F12]** top-right on every site; shows on mouse move, hides after 3 s | Mouse / air mouse |
| **Key** | GNOME custom shortcut runs `tvdash --home`; map **F12** and the air mouse's Home/Back key to it | Remote / keyboard |

On `home`: tvdash closes Brave (DevTools `Browser.close`), raises its own window and shows a short toast "Closed Netflix · back on Home". The phone remote's Home button (v3) will call the same command.

Services stay **full screen**; tvdash doesn't resize or move the Brave window.

---

## 2. TV only — Tile hub

Board size **1920 × 1080**, safe area 96 / 54 px. See `tvdash-tv-only-build.md` for 4K / 1440p scaling.

### Chrome

- **Tabs across the top** (replace the side rail): **Home · Library · Services · Settings**, 54 px; active `text-primary` weight 400, others `#5E646D` weight 300. Clock + temperature small in the top-right.
- **Sub-tabs** where a page needs them (Library: Movies · TV Series · Browse all; Settings: Display · Accounts · Server · What's new), 30 px.
- **Section labels:** 22 px uppercase, 1.2 px tracking, `text-secondary`, above each group.
- **Hint bar** bottom-right: round glyph + label, e.g. **OK** Full screen · **☰** Pause / Stop · **›** More.

### Tiles

| Rule | Value |
|---|---|
| Base unit | 240 px tall rows, 16 px gaps; small tile 178 × 240, square 240 × 240, wide 384–400 × 240 |
| Corners | **6 px** (departure from the brief's 28 px; the flat look depends on it) |
| Focus | **4 px `text-primary` outline at 0 offset**, scale 1.04, deep shadow (`0 20 60 #0009`). One focus at a time |
| Labels | Bottom-left 22–26 px; icon top-left (utility) or centred (services) |
| Utility tiles | Tint `poster` colour; the clock tile uses tint `card` |
| Progress | Thin bar along the **top** of a media tile (Home hero) or bottom (posters) |

### Screens

| Screen | Layout |
|---|---|
| **Home** | Tall clock/weather/Plex tile · big On-the-TV tile (4 × 2) · Search, Library, Screensaver tiles · **3 recent services + All 10 services** · Continue watching column of wide tiles that scrolls sideways (next column peeks) |
| **Library** | **Panorama groups** that scroll sideways: Watching now tile + big Finish the season tile → Recently added 4 × 3 → Popular on Republic City (rank chips) → further groups off-screen (Watch again, Genres / Collections) |
| **Show detail** | Big art tile (title, details, summary) · Resume tile (focused, tint) · From S1 · E1 · Shuffle · season sub-tabs · episode tiles (384 × 240) in a 3-row panorama |
| **Collection** | Not drawn yet; same pattern as Show detail (hero tile, Resume / Shuffle an unwatched one, movie tiles) |
| **Services** | 5 × 2 grid of 240-tall brand tiles, recent three marked; note on how to come back |
| **Search** | 6 × 6 flat on-screen keyboard + Space / Delete / Clear, query tile, 4-across results with "where" strip, JustWatch credit |
| **Settings** | Each setting is a tile showing its current value (Layout, Theme, Temperature, Clock, Screensaver, Back key, Recent services, Night mode, Quit in red) |
| **Player** | Full-screen video; bottom HUD with title, progress, flat square control tiles (Prev, Start, Pause, Next … Audio, Subs, Vol −/+, Stop); auto-hide 3.5 s |
| **Ambient idle** | Unchanged: `#050608`, dimmed art, 180 px clock, Up next |

---

## 3. Two screens — Blades, detail first

Laptop window **1920 × 1080**, touch first, mouse works.

### Blades

- Four tabs in a fixed order: **library · home · apps · settings**.
- Tabs **up to and including the active one stack on the left**; the rest stack on the right (Xbox 360 behaviour). On Settings, all four are on the left.
- Each blade is a 120 px-wide full-height tab with a vertical label; the one **nearest the panel shows 96 px**, others 48 px. The active one is tinted (`card` / `edge`).
- Tap a blade to switch, or **swipe sideways on the panel** to move one tab.

### Panel

- One panel between the blades: `surface`, 28 px corners, `edge` border, big shadow.
- **Top: a banner** (470–600 px) of dimmed artwork with the page's main item: kicker label, title, details, description, buttons. Clock + weather top-right on the banner.
- **Bottom:** the page's lists and grids.
- A soft radial **tint glow** behind the panel replaces the 360's green.

### Pages

| Page | Banner | Below |
|---|---|---|
| **Home** | On the TV (title, episode, description, progress, −10 · Pause · +30 · Stop · Full screen) | Continue watching (TV / Movies, 5 posters) · **Recent services** (3 tiles with "2 h ago") + **All 10 apps** |
| **Library** | Finish the season item (segment bar, Play on TV, Show page, ‹ ›); Library title + TV / Movies + Browse all | Watching now (2 rows) · Added this week · Popular on Republic City (7 / 30 days, rank chips) |
| **Show page** | Series details, Up next, Resume / From S1 · E1 / Shuffle | Season selector (tabs → pills → picker rules unchanged) + episode stills |
| **Collection** | Collection (gold tint sample), watched progress, Resume / Shuffle an unwatched one | Order (Collection / Release / A–Z) + Hide watched + movie posters |
| **Apps** | Selected app in its brand colours, "Recent · last opened 2 h ago", Open on TV | All 10 tiles, recent three marked |
| **Settings** | (no banner) Tabs: Accounts · Streaming · Display · Server · Diagnostics · What's new | Display: Layout, Theme, Follow system, Temperature, Clock · Recent services (Clear) · Back key · Screensaver · Quit |

### Overlays

| Overlay | Behaviour |
|---|---|
| **Where to watch** | Slide-over from the right (992 px), dims the rest of the panel; empty until you type; groups: In your library · On your apps · Costs extra; JustWatch credit |
| **Confirm playback** | Centred over `scrim`: poster, title, progress, **Resume on TV** (76 px) · Play from start · Cancel |
| **Up next** | When an episode ends naturally: next still + title, 10 s countdown bar, **Play next** · Cancel |
| **Screensaver** | Sentence clock, always dark (unchanged from Screensaver 3) |

---

## 4. Departures from the brief (on purpose)

| Brief | v2 | Why |
|---|---|---|
| TV side rail | Tabs across the top | Tile hub look; frees the left edge for the clock tile |
| 28 px card radius (TV) | 6 px tiles | Flat Xbox One tiles |
| Neutral service tiles | Brand-colour tiles | Recognisable at 10 ft; the only strong colour |
| Uppercase labels everywhere | Sentence-case tile labels, uppercase only for section labels | Readability on tiles |
| Laptop 2-column split | Blades + one panel | Chosen in round 3 |
| Plex / Jellyfin badges | Plex only | Project decision (2026-11-12) |

---

## 5. Open questions

- [ ] **TV size rule.** The Tile hub is a fixed grid drawn at 1080p. Proposal: scale the whole UI by `window height / 1080` (×2 at 4K, ×1.333 at 1440p). This replaces `tvdash-tv-only-plan.md` §11's "same pixel sizes, 4K shows more" rule. Confirm before building.
- [ ] **Live video in the Home tile** stays out (as decided for v1); the tile shows art + progress. Revisit with libmpv's render API later.
- [ ] **TV Collection page** board.
- [ ] **Leaf chip** placement: laptop banner top-right next to the weather; TV clock tile. Design when the Leaf feature starts.
- [ ] Brave `--load-extension` still works in current Brave? (Chrome 137 dropped it in branded builds.) Fallback is in the build docs.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-18 | v2 look chosen: TV Tile hub (+ Library Panorama), laptop Blades detail-first; Back to tvdash pill + F12; recent 3 services; both layouts on one canvas |
