# tvdash — Current Design & Architecture Snapshot (v0.1.32)

A complete snapshot of where `tvdash` stands right now (`v0.1.32`, branch `tv-tile-hub-v2`), formatted for design review and concepting new sections in Claude Design.

---

## 1. Product & Tech Stack

- **Platform:** Fedora Linux (GNOME Wayland), Rust + Slint (`1.18.1`), OpenGL in-window `libmpv` (`hwdec=auto-safe`), Brave (`--app` kiosk + `extras/back-pill` extension on `:47800/home`).
- **Modes:**
  - **TV-only (One-screen):** Complete v2 **Tile Hub** (`1920×1080` design canvas scaled proportionally via `TvUi.u` for `1080p`, `1440p`, and `4K`). All navigation works via D-pad/keyboard (`Arrows`, `OK/Enter`, `Back/Esc`) and mouse/touch.
  - **Two-screen (Laptop + TV):** Currently on the earlier two-column laptop UI; the planned v2 Blades refresh is spec'd in `docs/tvdash-two-screen-build.md`.
- **Integrations:** Plex, Jellyfin, TMDB (*Where to watch* + service logos), Open-Meteo (weather + timezone), Tautulli (Popular on server), and a local HTTP Phone Remote (`:8765`, QR code in Settings).

---

## 2. Visual System & Tokens

- **Canvas & Safe Area:** `1920×1080` design units, `96px` left/right margin, `54px` top/bottom margin, `16px` grid gutters.
- **Themes:** Three explicit themes — **Dark**, **Stone**, **Mist** (no "Follow system").
- **Poster Tint (`Dash.tint` / `ShowPageState.tint` / `CollectionPageState.tint`):** Extracts dominant hue from the active item's poster to tint cards (`card`, `poster`, `accent`, `sec`).
- **Tile Component Language:**
  - `6px` corner radius (`9px` outer focus ring).
  - Focus state: `4px` `Theme.text-primary` outline, `60px` drop-shadow, subtle expansion capped at `min(4%, 10px)` so neighbors never overlap.
  - Tile variants: `MediaTile` (art + bottom scrim + progress bar + optional top-right badge), `ServiceTile` (brand gradient + TMDB/SVG logo or wordmark + optional `RECENT` pill), `ValueTile` (label + large light value), `IconTile` (icon + label).

---

## 3. TV-Only (One-Screen) Pages

### Top Chrome (`TvShell`)
- **Left:** `TabBar` — **Home · Library · Collections · Settings** (with an `← Back` pill prepended when on pushed sub-pages: `Show`, `Collection`, `Search`).
- **Right:** Compact weather icon + temperature + clock.
- **Bottom:** Toast notifications bottom-left (bottom-right hint bar removed).

### 3.1 Home (`TvHome` — Zero Scrolling, Full 1920×1080 Grid)

```text
+------------------+----------------------------------+------------------+------------------------+
|  Clock + Date    |                                  |  Search          |  STREAMING SERVICES    |
|  ──────────────  |                                  |  (240 x 186)     |  (2 x 5 Grid, All 10)  |
|  Plex ·          |  Hero (MediaTile)                +------------------+                        |
|  CURRENTLY       |  728 x 590                       |  Library         |  [YT TV]   [YouTube]   |
|  PLAYING         |  Poster + top progress bar       |  (240 x 186)     |  [Disney+] [Hulu]      |
|                  |  (Paused play overlay when       +------------------+  [AppleTV] [Netflix]   |
|  Subtitle /      |   returning from full-screen     |  Screensaver     |  [Fandango][Prime]     |
|  Stopped at /    |   video)                         |  (240 x 186)     |  [Crunchy] [Tubi]      |
|  Time left /     |                                  |                  |                        |
|  Description     |                                  |                  |  235 x 146 each        |
+------------------+----------------+-----------------+------------------+  16px gaps             |
|  Weather         |  Last Played   |  Settings       |  Theme           |  x: 1338..1824         |
|  210 x 230       |  356 x 230     |  356 x 230      |  240 x 230       |                        |
+------------------+----------------+-----------------+------------------+------------------------+
```

- **Left Column (`96..306`, `210×590`):** Clock (`12h`/`24h` + AM/PM), Date + Year, active/next Plex item status (`Plex · CURRENTLY PLAYING` or `ON THE TV`), episode/year subtitle, stopped-at & time-left, and movie/episode description (`summary`).
- **Hero (`322..1050`, `728×590`):** Top Continue Watching item (or currently paused playback). When returning from full-screen video via `Esc`/Back, playback pauses and Hero displays the still poster with a centered play-button overlay (`"Paused · Click or press OK to resume"`).
- **Utility Column (`1066..1306`, `240×186` each):** `Search`, `Library`, `Screensaver` (launches `TvAmbient`).
- **Bottom Row (`y: 742..972`, `230px` tall):**
  - **Weather (`210×230`):** Condition icon, temp, condition, detail/note.
  - **Last Played (`356×230`):** Tracks the most recently launched movie, TV episode, or streaming app (with poster or brand logo); OK/click resumes or relaunches it.
  - **Settings (`356×230`):** Quick-jump tile to Settings.
  - **Theme (`240×230`):** Cycles **Dark → Stone → Mist** in place.
- **Right Area (`1338..1824`, `y: 176..970`):** **2×5 Streaming Services** grid showing all 10 services at once using downloaded TMDB logos (or high-contrast bundled SVG fallbacks) and `RECENT` badges.

### 3.2 Library (`TvLibrary` — Vertical Scrolling Only)
- **Top Bar:** `SubTabs` (**TV Series · Movies**) + **Browse All** toggle pill.
- **Smart Shelves (`Browse All` off):** Stacked vertical shelves (`1728px` wide, zero horizontal scroll):
  1. `WATCHING NOW` (up to 4 wide `416×240` tiles)
  2. `RECENTLY ADDED` (up to 8 `198×260` posters)
  3. `FINISH THE SEASON` (**TV Series only**, up to 4 `416×132` cards)
  4. `POPULAR` (up to 8 `198×260` ranked posters via Tautulli)
  5. `WATCH AGAIN` (up to 8 `198×260` posters)
- **Browse All (`Browse All` on):** Single-row `All`, `A`–`Z`, `#` filter bar + 8-column vertical-scrolling `MediaTile` grid (`198×256`, ignoring leading *"The"*, *"A"*, *"An"*).
- **Activation:** TV Series cards call `Library.open-show` to open `TvShow`; Movies/Episodes start playback directly.

### 3.3 Collections (`TvCollections` & `TvCollection` — Vertical Scrolling)
- **Collections Tab (`TvCollections`):** 5-column vertical-scrolling grid (`332×300` `MediaTile`s) of Plex Movie Collections with collection poster artwork, movie count, and watched progress bar.
- **Collection Detail (`TvCollection`):** Left hero info card (`520×320`) + `Play next` + `Shuffle`; right 3-column vertical-scrolling movie grid (`372×236`).

### 3.4 Show Detail (`TvShow` — Vertical Scrolling)
- Left hero info card (`520×320`) + `Resume / Play up next` + `From S1 · E1` + `Shuffle`; right season sub-tabs + 3-column vertical-scrolling episode grid (`372×236`).

### 3.5 Search (`TvSearch`)
- Left: Search input + 6×6 on-screen keyboard + `Space` / `Delete` / `Clear` (also accepts physical keyboard typing).
- Right: 4-column TMDB + Plex/Jellyfin *Where to watch* results.

### 3.6 Settings (`TvSettings` — 4 Sub-Tabs)
- **Display:** 6 `ValueTile`s (`Display layout`, `Theme`, `Temperature`, `Clock`, `Recent services: Clear list`, `Application: Quit tvdash`) + **Phone Remote QR** card + **Weather Location Lookup** (city search + 4 selectable results).
- **Accounts:** Live sign-in code banner + 2-column account cards (**Plex**, **Jellyfin** with `Server URL` editor, **TMDB**, **Tautulli**) + inline token/URL editor.
- **Server:** `Scan all` button + 5 metric `ValueTile`s (`Streams`, `Transcodes`, `Bandwidth`, `CPU`, `RAM`) + Plex Pass sparkline graphs + `ACTIVITY` list.
- **What's new:** Scrollable changelog cards (`Dash.changelog`).

### 3.7 Playback, Up Next Modal & Ambient
- **Full-Screen Player:** In-window `libmpv` (`hwdec=auto-safe`) with `TvPlayerHud` (`-10s`, `Play/Pause`, `+30s`, `Stop`, `Home`, `Subtitles`, `Audio`).
- **Up Next Modal (`Dash.next-ep-open`):** Pops up when a TV episode finishes naturally, offering **Play Next** and **Back to Dashboard**.
- **Ambient (`TvAmbient`):** Full-screen idle clock + backdrop after 5 quiet minutes on Home.

---

## 4. Planned / Future Scope (Not Yet Built)

- **Two-screen v2 Blades refresh:** `docs/tvdash-two-screen-build.md`
- **v2+:** Alerts strip, severe-weather takeover, headlines, HDMI-CEC remote (`cec-client`), skip intro.
- **v4:** "What should I watch?" (Ollama), watch stats, Home Assistant, cross-service watchlist.
