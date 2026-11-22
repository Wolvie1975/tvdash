# tvdash — UI Redesign & Mockup Brief

Use this document as the complete reference when mocking up a new look-and-feel for **tvdash** (e.g. in Claude Design). All data, backend integrations, and video playback already exist in the Rust app — a new skin only needs to arrange these screens, components, and data fields.

---

## 1. Canvas, Viewing Context & Themes

- **Target canvas**: `1920 × 1080` (10-foot TV UI, scaled proportionally to 1440p/4K and laptop windows).
- **Safe area**: `96px` horizontal, `54px` vertical margins.
- **Input**: Works with **both** mouse/touch and 10-foot keyboard/D-pad (Up/Down/Left/Right, Enter/OK, Back/Esc, `F12` to return from streaming browser).
- **Themes** (user-selectable + optional night-mode schedule):
  - **Dark** (deep charcoal/black `#0B0D10`, raised surfaces, crisp white text)
  - **Stone** (warm dark gray/taupe)
  - **Mist** (light warm gray/stone, dark text)
- **Dynamic Poster Tinting**: Cards and focus backdrops extract a dominant hue from the active/focused poster (`tint.card`, `tint.poster`, `tint.accent`, `tint.sec`) while maintaining AA text contrast.

---

## 2. Top-Level Navigation

Top navigation bar (`TabBar`):
1. **Home**
2. **Library**
3. **Collections**
4. **Sports**
5. **Settings**

Top-right status cluster: **Weather** (icon + temp) + **Clock** (`HH:MM`).
Sub-pages (**Show**, **Collection**, **Search**) show a **← Back** pill to the left of the tab bar.

---

## 3. Screens & Available Data

### A. Home (`TvHome`)
- **Currently Playing / On the TV (Hero + Info)**:
  - Source label (`"Plex · CURRENTLY PLAYING"` or `"Browser"`), Title, Subtitle (`S2 · E11 · Episode Title` or year/genres), Status (`"Stopped at 5:41 · 37 min left"` or `"Ends at 10:42 PM"`), full Plot/Episode Summary, Progress (`0..1`), `16:9` Backdrop art (with poster fallback).
  - If video is currently playing & minimized to Home: play overlay on hero tile + compact bottom transport bar (`-10s`, `Play/Pause`, `+30s`, `Stop`, `Subtitles`, `Audio`).
- **Weather & Time**:
  - Clock (`HH:MM AM/PM`), Date (`November 20, 2026`), Weather icon, Temperature (`66°F`), Condition (`mainly clear`), High/Low (`H 69 L 50`).
- **Last Played**:
  - Either last media item (poster, title, `S2 · E11 · ...`) or last streaming app (brand logo/monogram + app name).
- **Quick Action Tiles**:
  - **Search**, **Library**, **Screensaver**, **Settings**, **Theme** (cycles Dark / Stone / Mist).
- **Streaming Services (10 tiles, 2×5 grid)**:
  - Configured sites (e.g. YouTube TV, YouTube, Disney+, Hulu, Apple TV, Netflix, Fandango at Home, Prime Video, YouTube Music, Tubi) with brand color/logo and `"RECENT"` badge on the 3 most recently opened.

---

### B. Library (`TvLibrary`)
- **Sub-tabs & pills**: `TV Series` | `Movies` | `Browse all` (A–Z filter + 8-column grid) | `Watching now` pill (live Plex/Jellyfin stream count, opens overlay).
- **Hero area (top)**:
  - Cross-fading `16:9` `FocusBackdrop` (positioned below the top `TabBar`) + `FocusInfo` (kicker, title, year/runtime/rating/genres, summary) for whichever tile is focused.
- **3 Vertically Scrollable Shelves**:
  1. **Shelf 1 (4 wide `16:9` cards)**:
     - *TV Series*: **Finish the Season** (segmented episode progress bar, remaining episodes) or **Continue Watching** fallback.
     - *Movies*: **Continue Watching** (progress bar, time left).
  2. **Shelf 2 (Top 6 numbered `1..6` with `2:3` posters)**:
     - *TV Series*: **Start Watching** (weekly seeded picks, not started yet).
     - *Movies*: **Recently Added** (newest added to server).
  3. **Shelf 3 (Top 6 numbered `1..6` with `2:3` posters)**:
     - *TV Series*: **Recently Released Episodes** (newest air dates; clicking opens the **Show** page).
     - *Movies*: **Recently Released** (newest release dates).

---

### C. Show Page (`TvShow`) & Collection Detail (`TvCollection`)
- **Show Page**:
  - Full backdrop + gradient scrims, Show Title, Year/Rating/Genres/Summary.
  - Action buttons: **Resume** / **Play**, **From S1 · E1**, **Shuffle**.
  - **Season selector** (Season 1..N, Specials last) + sticky **Season column** (poster, episode count, watched count; switches to focused episode details when scrolled).
  - **3-column Episode Grid** (vertical scroll): `16:9` thumbnail, `UP NEXT` badge, watched `✓`, progress bar, episode number, title, duration/remaining, plus `"Continue to Season N ›"` card at the end.
- **Collection Detail**:
  - Collection poster/backdrop, title, item count, total runtime, summary, and 3-column grid of items.

---

### D. Search (`TvSearch`)
- On-screen A–Z / 0–9 keyboard (for 10-foot use) + physical keyboard input.
- Two result groups:
  - **In Your Library** (Plex/Jellyfin matches)
  - **Where to Watch** (TMDB matches showing which of your streaming services carry it + `"Open on <Service>"` / `"TMDB"` actions).

---

### E. Sports (`TvSports`)
Two sub-tabs: **Competitions** (default) and **Scoreboard**.

1. **Competitions**:
   - Reads upcoming games (today through +3 days, Central time).
   - Controls: **Schedule filter dropdown** (`All schedules`, `Big 12 · All sports`, `Big 12 Football`, `NFL`, `Premier League`, etc.), **Refresh**, total event count (`"114 events"`), `"Schedule last updated ..."`.
   - Grouped by **Day** (`Wednesday, November 19, 2026`) → **League** (A–Z).
   - Game row: Sport (+ `"Neutral site"` badge), Away team (logo or initials badge + name + `"Away"`/`"Team"`), Home team (logo/initials + name + `"Home"`/`"Team"`), Time (`7:00 P.M. CT` or `Time TBD` / `Postponed · Time TBD`), Location (City / Venue), Broadcast (`TV: ... · Radio: ...`), and clickable link pills (**Stats**, **Video**, **Audio**, **Details** — open in TV browser).
   - Paging (200 per page: `← Earlier`, `Page X of Y`, `Later →`) + empty states.

2. **Scoreboard**:
   - Auto-refreshes every 5 minutes.
   - Controls: **Date stepper** (`←` / `Day` / `→` / `Today`), **Show** dropdown (`All sports`, `Top 25`, Leagues, Conferences, Teams), **All statuses / Final scores** toggle, **Refresh**.
   - Coverage/staleness warning banner when applicable.
   - Grouped by **Live** (pulsing red dot, period + clock), **Upcoming** (start time + broadcasts), **Final** (`Final` / `Final/OT`), and **Upcoming ranked games this week** (when `Top 25` is selected).
   - Game card: League, Status/Clock, Away & Home teams (logo/initials, `#rank` if Top 25, name, score, Winner/Leading emphasis), Details (periods, baseball count/bases, shootouts, venue, broadcasts), and `Game ↗` / `Recap ↗` link.

---

### F. Settings (`TvSettings`)
6 sub-tabs:
1. **Display**: Layout (`TV only`), Theme (`Dark` / `Stone` / `Mist`), Temperature (`°F` / `°C`), Clock (`12h` / `24h`), Clear recent services, Quit tvdash, Phone Remote QR code, Weather location search.
2. **Accounts**: Plex, Jellyfin, TMDB token, Tautulli, Sports SQL Server password (`events_sql_password`).
3. **Sites**: Manage streaming sites (`+ Add site`, Edit, Hide/Show, Reorder ↑/↓).
4. **Server**: Plex server summary, libraries, active streams/transcodes, Scan all, background activities.
5. **Diagnostics**: Service health rows (Jellyfin, Plex, TMDB, Weather, Keyring, mpv, Browser, Sports SQL) + log/config file paths.
6. **What's new**: Changelog entries.

---

### G. Video Player & Ambient Screensaver
- **In-window Video (`libmpv`)**: Full-screen video with auto-hiding bottom HUD (`-10s`, `Play/Pause`, `+30s`, `Stop`, `Home`, `Subtitles`, `Audio`, progress bar, ends-at time) + **Up Next** modal at end of episode.
- **Ambient Screensaver (`TvAmbient`)**: Triggered after idle on Home (or via Screensaver tile) — drifting angled poster wall, large clock + date + weather, `Up next` preview, and library count footer with burn-in pixel shift.

---

## 4. Technical Integration Note for Slint

If mocking up a new Slint skin in this repo:
- Keep all data bindings pointing to `crates/app/ui/state.slint` (`Dash`, `Library`, `LibraryLanding`, `ShowPageState`, `CollectionPageState`, `Sports`, `Scoreboard`, `Accounts`, `Server`, `Diagnostics`, `TvUi`, `AmbientState`).
- Place the new skin under e.g. `crates/app/ui/v3/` and swap `TvShell` in `crates/app/ui/laptop.slint`.
- Update `tile_rects()` in `crates/app/src/tv_nav.rs` if D-pad focus rectangles move.
