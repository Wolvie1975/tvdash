# tvdash — Look & Feel Design Brief

A visual and interaction reference for **tvdash**, covering both the **Two-Screen** (laptop controller + TV) and **One-Screen** (TV-only 10-foot) layouts. Intended as a design brief for exploring UI concepts in Claude Design.

---

## 1. Visual Identity & Core Principles

- **Personality:** Calm, high-contrast living-room media hub. Feels like a quiet appliance rather than a busy web dashboard — generous breathing room, crisp hierarchy, and no chrome clutter.
- **Poster-Driven Tinting:** The active media item (Now Playing or Up Next) extracts its dominant poster hue and tints the hero card's background, border, secondary text, and primary accent button so the dashboard subtly adapts to what's on screen.
- **Typography:** `Inter` (`Light 300`, `Regular 400`, `Medium 500`, `SemiBold 600`).
  - Large, ultra-clean tabular clock (`Light 300`).
  - Uppercase tracked section labels (`letter-spacing: 0.8–1.2px`, muted color) above content cards.
  - Crisp, legible body and title scales tuned per viewing distance.

---

## 2. Color System & Themes

The **Two-Screen** layout supports three user-selectable themes (**Dark**, **Stone**, **Mist**). The **One-Screen (TV-only)** layout stays **Dark** (and forces Dark during Night Mode) to avoid glowing in a dim room.

| Token | Dark | Stone (Warm Light) | Mist (Cool Light) | Usage |
|---|---|---|---|---|
| `bg` | `#0B0D10` | `#CBC5BA` | `#C3CBD3` | App canvas background |
| `bg-night` | `#050608` | `#050608` | `#050608` | Ambient / screensaver |
| `surface` | `#14171C` | `#DAD5CC` | `#D3DAE1` | Primary cards & modals |
| `surface-raised` | `#1B1F26` | `#E3DFD7` | `#DEE3E8` | Buttons, chips, list rows |
| `border` | `#2A2F37` | `#B9B2A6` | `#AEB7C1` | 1 px hairline borders |
| `text-primary` | `#E8EAED` | `#1F1D1A` | `#1A1E23` | Headings, primary labels |
| `text-secondary` | `#A0A6AF` | `#47433D` | `#3F464F` | Subtitles, metadata |
| `text-muted` | `#8A9099` | `#4E4943` | `#47505A` | Section labels, hints |
| `accent` | `#5B9DFF` | `#194D9F` | `#194D9F` | Primary CTAs, active tabs |
| `on-accent` | `#0B0D10` | `#FFFFFF` | `#FFFFFF` | Text/icons on `accent` |
| `ok` | `#4CC38A` | `#1E6B45` | `#1E6B45` | Connected, watched |
| `warn` | `#E5A84B` | `#7A4E00` | `#7A4E00` | Stale / attention |
| `alert` | `#F0605D` | `#9E2622` | `#9E2622` | Errors / offline |
| `scrim` | `#0B0D10C0` | `#0B0D10C0` | `#0B0D10C0` | Badges & overlays on art |

---

## 3. Two-Screen Layout (Laptop Controller + TV Output)

Designed for **touch and pointer** on a laptop/tablet sitting on a coffee table while video plays on the TV.

### Ergonomics & Geometry
- **Hit targets:** Minimum `56px` height on all interactive controls (`hit-target: 56px`).
- **Corners & spacing:** `28px` card radius, `18px` control radius, `14px` button radius, `24px` grid/column gap.
- **Type scale:** Clock `132px` (Light), Titles `28px`, Body `22px`, Labels `18px`.

### Home Screen (2-Column Split)
- **Left Column (Ambient + Hero Control):**
  1. **Top bar:** Large `132px` clock + weekday/date on the left; current weather (icon, temp, condition) on the right.
  2. **TV Status / Now Playing Hero Card** (poster-tinted):
     - *Playing:* Poster art, source badge (`Plex` / `Jellyfin`), title, episode subtitle, progress bar + elapsed/remaining time, and single-row transport (`Prev`, `From start`, `Pause/Resume`, `Next`, `Stop`).
     - *Streaming (Browser):* Service monogram/logo + `Close browser` CTA.
     - *Up Next / Idle:* Top Continue Watching item with `Resume on TV` CTA (or quiet idle state).
  3. **Quick Footer / Utility:** Search ("Where to watch") trigger, Settings gear, Screensaver button.
- **Right Column (Browse & Launch):**
  1. **Continue Watching Card:** Header with `See all ›` link to Library, full-width segmented switch (`TV / Series` · `Movies`), and a 3-column poster card grid (2:3 aspect ratio + title/progress bar). Clicking an item opens a **Confirm Playback** modal (`Resume on TV` / `Play from start` / `Cancel`).
  2. **Streaming Services Card:** 5-column grid of service tiles (TMDB/custom logos or clean typographic fallback) for launching Netflix, YouTube, etc. in full-screen Brave on the TV; long-press/edit support.

### Sub-Pages & Overlays
- **Library Page:** `Movies` / `TV Series` / `Browse All` (with A–Z filter ignoring leading articles "The", "A", "An"). Smart shelves: `Continue Watching`, `Recently Added`, `Finish the Season` (TV), `Collections` (Movies, horizontal scroll), `Added This Week`, `Watch Again`.
- **Show Detail Page:** Two-column layout — left poster/summary/actions (`Play` / `Play from start` / `Shuffle`); right season tabs + scrollable episode rows with progress bars.
- **Collection Detail Page:** Header with `Up next`, `Shuffle`, sort toggles (`Collection` / `Release` / `Alpha`), `Hide watched`, and movie list.
- **Where to Watch (Slide-Over Panel):** Slides in over the right column for TMDB search + streaming provider availability.
- **Settings Page:** Top tab bar (`Accounts` for Plex / Jellyfin / TMDB / Tautulli, `Streaming`, `Display` including Phone Remote QR code + URL, `Server`, `Diagnostics`, `What's new`) + `Quit tvdash` button.
- **Up Next Modal:** Appears when an episode finishes naturally (`Play Next` / `Cancel`).

---

## 4. One-Screen Layout (TV-Only 10-Foot UI)

Designed for **1920×1080** viewed from 10 feet away using a **D-pad / TV remote / arrow keys** (`OK`, `Back`, `← ↑ → ↓`).

### Ergonomics & Geometry
- **Safe area:** `96px` horizontal, `54px` vertical (`Ten.safe-x` / `Ten.safe-y`).
- **Focus indicator:** High-contrast `4px` outline (`Theme.text-primary`) offset `6px` outside the focused element (`Ring` component) — unmistakable from across the room.
- **Type scale:** Clock `160px`, Screen Titles `54px`, Body `32px`, Labels `24px`.
- **Navigation Rail:** Collapsible left side rail (`104px` collapsed icon strip → `340px` expanded with labels, dimming main content to `0.45` opacity when focused): `Home`, `Search`, `Library`, `Services`, `Settings`.

### Screens
- **Home:**
  - Top row: `160px` clock + date on the left; weather + status on the right.
  - Hero card: Now Playing / Up Next / Idle with `OK` action (`Play` / `Pause` / `Resume` / `Stop`).
  - Horizontal `Continue Watching` poster strip + `Services` tile strip.
- **Search (Where to watch):**
  - 6×6 on-screen keyboard (plus physical keyboard typing) + live TMDB results grid + detail modal with streaming provider badges.
- **Library:**
  - Top filter chips (`Movies`, `TV Series`, `Browse All`).
  - *Shelves view:* Horizontal strips (`Continue Watching`, `Recently Added`, `Finish the Season`, `Collections` on Movies, `Added This Week`, `Watch Again`).
  - *Browse All view:* A–Z letter bar + 8-column poster grid.
  - *Overlays:* Full-screen `Show Detail` (actions + season chips + episode list) and `Collection Detail` (Up next, Shuffle, Sort cycle, Hide watched, movie list).
- **Services:** 5-column grid of `190px`-tall streaming service tiles.
- **Settings:**
  - Header with version (`v0.1.11`) and 4 focusable tabs (`Display`, `Accounts`, `Server`, `What's new`):
    - **Display:** `Layout` (`Two screens` / `TV only`), `Theme`, `Temperature` (`°F`/`°C`), `Clock` (`24h`/`12h`), **Phone remote** (scannable QR code + LAN URL), `Quit tvdash`.
    - **Accounts:** Full account rows (status dot, name, status, detail/URL, action label) for Plex, Jellyfin, TMDB, and Tautulli, with live PIN / Quick Connect sign-in code banner and inline TMDB / Tautulli API key entry.
    - **Server:** Live streams, transcodes, CPU/RAM/Bandwidth sparkline graphs, activity list, `Scan all`.
    - **What's new:** Scrollable release changelog cards.

### Full-Screen Video, PIP & Ambient
- **In-Window Video:** Plays full-screen inside the app window (`#000000` canvas).
- **Floating Player HUD:** Auto-hiding (`3.5s`) bottom bar on `OK` or mouse move: title, progress bar, `Prev Ep`, `Start from beginning`, `Play/Pause`, `Next Ep`, `Audio`, `Subs`, `Vol −`, `Vol +`, `Stop`.
- **Live PIP Mini-Player:** Pressing `Back` returns to Home/Library/etc. while video continues in a `320×180px` bottom-right PIP preview with an accent border.
- **Ambient Idle Screen:** Deep `#050608` background with dimmed poster art + `90°` dark gradient, `180px` clock, date, and `Up next` hint — gently pixel-shifted every 3 minutes.
- **Modals:** `Confirm Playback` (`Resume` / `Play from start` / `Cancel`) and `Up Next` (`Play Next` / `Cancel`) centered over a `#0B0D10C0` backdrop.
