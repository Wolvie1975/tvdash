# tvdash Changelog

## v0.1.48 — 2026-11-21
- Replaced the Home **Settings** tile (Classic & Blades) with **Google Meet** (`https://meet.google.com`).
- Replaced the old dark theme color with **Midnight** (`#0E1526`, default) and added **Aubergine** (`#1A1024`) as a selectable theme option across both Classic and Blades.

## v0.1.47 — 2026-11-20
- Fixed the horizontal gradient band across the Library hero backdrop in Blades (`v3/library.slint`) and Classic (`tv/focus_backdrop.slint`).

## v0.1.46 — 2026-11-20
- Added the **Blades** (`ui/v3/`) dashboard UI option alongside **Classic** (`TvShell`), switchable live under **Settings → Display → Dashboard UI** (`[display] dashboard_ui = "classic" | "blades"`), with bundled `Nunito Sans` font and Dark/Stone/Mist + Night Mode support.
- Updated **Library → Browse all** (in both Classic and Blades) to return only the first 10 A–Z titles on `"All"` with a status summary, and load the full per-library set when `#` or `A–Z` is selected.
- Removed **Shuffle** from Show and Collection detail pages.
- Added team logos and a separate event-type header line on **Sports → Scoreboard** and **Competitions** in Blades.

## v0.1.45 — 2026-11-20
- Changed the TV Series **Recently Released** 3rd shelf in `TvLibrary` to query TV series (`type=2`) by `originallyAvailableAt:desc` instead of individual episodes.
- Replaced the Phone Remote QR card in **Settings → Display** with the **Weather Location** lookup card.

## v0.1.44 — 2026-11-20
- Swapped the `Settings` and `Theme` tile fills on `TvHome` so `Settings` matches the bottom row (`Dash.tint.card`) and `Theme` aligns with the right-hand column (`Dash.tint.poster`).
- Moved the clock and date into the bottom-left **Weather / Time** tile on `TvHome` and dedicated the full tall left tile to Plex content details.
- Added a **Recently Released** (Top 6) 3rd shelf to `TvLibrary` for both TV Series (`RECENTLY RELEASED EPISODES`, opening the Show page) and Movies (`RECENTLY RELEASED`), with vertical shelf scrolling.
- Defaulted to **TV only** (`LayoutSetting::Tv`) and locked two-screen mode off in the UI/startup (still available via `--layout dual`).

## v0.1.43 — 2026-11-19
- Moved `FocusBackdrop` on `TvLibrary` below the top `TabBar` (`y: 118px` with a soft top fade) so navigation tabs and clock stay crisp and legible in Dark, Stone, and Mist themes.

## v0.1.42 — 2026-11-19
- Added **YouTube Music** brand styling + bundled SVG (`assets/logos/youtube-music.svg`) and replaced Crunchyroll in `default_streaming_sites()`.

## v0.1.41 — 2026-11-19
- Updated the Home hero tile (`On the TV` / `Currently Playing`) to use the `16:9` backdrop when available (falling back to the poster until prefetched) so widescreen artwork fits without cropping.
- Fixed `TvShow` mouse-wheel scrolling jumping back to the top when background `AppEvent`s triggered `tv_nav::sync`.

## v0.1.40 — 2026-11-19
- Added the **Sports** tab (`tvdash-events` crate + `tv/events.slint`) reading read-only from the `WebScraper` SQL Server database (`tiberius`), with GNOME Keyring password storage (`events_sql_password` / **Settings → Accounts → Sports · SQL Server**).
- Implemented **Sports → Competitions** (today through +3 days in Central time, schedule filter dropdown, day/league grouping, team logos/initials, TV/Radio, and `Stats`/`Video`/`Audio`/`Details` links opening in `tvdash`'s TV browser, 200/page).
- Implemented **Sports → Scoreboard** (yesterday through +7 days, `Show` filter for All sports / Top 25 / Leagues / Conferences / Teams, Final scores toggle, Live/Upcoming/Final groups, `LiveDetailsJson` period/baseball details, coverage/staleness warnings, and 5-minute auto-refresh).

## v0.1.39 — 2026-11-19
- Unified the dashboard UI around `TvShell` for both one-screen (`TV only`) and two-screen (`Two screens`) modes, removing the old dual-only Slint pages.
- Added **Sites** (`Accounts.sites` + `SiteEditorDialog`) and **Diagnostics** (`Diagnostics.rows` / `Diagnostics.paths`) tabs to `TvSettings`.
- Enabled embedded `libmpv` playback inside `TvWindow` (`tv.slint`) for two-screen mode, and added a bottom now-playing transport bar to `TvShell` with `-10s`, `Play/Pause`, `+30s`, `Stop`, `Subtitles`, and `Audio` controls.
- Fixed `TvShow` mouse-wheel scrolling (`TouchArea { scroll-event }` + smooth inner container, `ShowPageState.scrolled` updates, and exact episode-grid scroll clamping).

## v0.1.38 — 2026-11-19
- Fixed a re-entrant `RefCell` panic when opening the TV `Library` page by moving `tv_nav::sync` outside `bridge::STATE.with_borrow_mut` and guarding `ensure_library_loaded` with `try_borrow_mut`.

## v0.1.37 — 2026-11-19
- Triggered `LoadLibraryShelves` automatically on first entry to the TV `Library` tab so `Start watching` (top 6) and `Finish the season` load without needing to switch to `Movies` first.

## v0.1.36 — 2026-11-19
- Added a top-edge scrim to `FocusBackdrop` so the top `TabBar` stays legible over bright backdrops.
- Fetched posters for all 6 weekly `Start watching` picks on Library load so tiles beyond the first 48 alphabetical items populate immediately.

## v0.1.35 — 2026-11-19
- Fixed `TvAmbient` screensaver clock/date/weather `VerticalLayout` sizing so `AM/PM`, current date, and weather display cleanly at the bottom left.
- Added explicit preferred height to the `TvShow` season/episode metadata `VerticalLayout`s.

## v0.1.34 — 2026-11-19
- Fixed `TvLibrary` / `TvShow` backdrops painting over the top `TabBar` and clock/weather chrome in `TvShell`.
- Fixed `TvLibrary` `FocusInfo` layout stretching below the hero area into the numbered row.
- Added a `CONTINUE WATCHING` fallback in `TvLibrary` TV Series Shelf 1 when no active show is within 3 episodes of a season finale.

## v0.1.33 — 2026-11-19
- Redesigned TV Library landing (`TvLibrary` Option C): cross-fading `FocusBackdrop` + `FocusInfo`, live `Watching now` pill & overlay, 16:9 `Finish the season` / `Continue watching` Shelf 1, and weekly seeded-shuffle `Start watching` / `Recently added` Shelf 2 with bundled outlined rank numerals.
- Redesigned TV Show page (`TvShow`) with vertical scrolling: show backdrop + scrims, `Resume` / `From S1 · E1` / `Shuffle` action row, season selector, sticky `260×390` season column (with `SEASON N` / `SPECIALS` fallback and focused-episode details when scrolled), `468×263` 3-column episode grid with `UP NEXT` chip, watched tick, progress bar, dashed `Continue to ... ›` last slot, and `1728×76` sticky bar.
- Redesigned TV Screensaver (`TvAmbient`) with a `tiny-skia` pre-rendered 96-poster wall (`-10°` rotation, 0.42 opacity, seamless upward drift), vignettes, 260 px clock + date/weather, 3-state `Up next` block, library count footer, and ±8 px burn-in walk.

## v0.1.32 — 2026-11-19
- Switched the Collection detail page (`TvCollection`) to a 3-column vertical-scrolling grid scaled with `TvUi.u`.

## v0.1.31 — 2026-11-19
- Added Plex metadata poster artwork to Movie Collection tiles on the Collections tab.

## v0.1.30 — 2026-11-19
- Cleared `TvUi.video` before deleting OpenGL textures on playback switch/resize and always opened new playback full-screen (`video_full = true`).
- Capped eager `browse_items` poster fetching to 48 items and bounded `fetch_posters` to 4 concurrent downloads to prevent memory spikes.
- Enabled `hwdec = auto-safe` in `libmpv` for hardware-accelerated video decoding.

## v0.1.29 — 2026-11-18
- Added Jellyfin `Server URL` editing button in TV Settings › Accounts.
- Used downloaded TMDB service logos on Streaming Service tiles and fixed low-contrast fallback SVGs (Crunchyroll, Prime Video, Fandango at Home, Tubi, YouTube).
- Populated movie and episode descriptions (`summary`) in the left Home info tile and wrapped `Plex · CURRENTLY PLAYING` cleanly.

## v0.1.28 — 2026-11-18
- Removed the bottom-right `HintBar` overlay from the TV-only shell.

## v0.1.27 — 2026-11-18
- Reorganized the Home bottom row: moved Weather underneath the left Clock column, compacted `Last Played` beside `Settings`, and added a `Theme` quick-cycle tile under Screensaver.
- Relabeled the Home `UP NEXT` card badge/header to `CURRENTLY PLAYING`.

## v0.1.26 — 2026-11-18
- Replaced the Home Continue Watching grid with a 2×5 grid of all 10 Streaming Services (no scrolling).
- Added a wide `Last Played` tile (tracks last movie, TV series/episode, or streaming service) and a `Settings` quick-jump tile on Home.
- Replaced the `Services` top tab with `Collections` (5-column vertical grid of Movie Collections) and removed Collections from `Library › Movies`.

## v0.1.25 — 2026-11-18
- Fixed Plex Movie Collections deserialization (`smart` / `childCount` flexible parsing), aggregated collections across all movie library sections, logged fetch errors, and bumped the shelves cache key.

## v0.1.24 — 2026-11-18
- Replaced minimized in-tile video on Home with the static poster + play button overlay (pausing playback on Home and skipping minimized FBO renders).
- Added the Up Next modal popup (`Play Next` / `Back to Dashboard`) when a TV episode finishes in TV-only mode.

## v0.1.23 — 2026-11-18
- Paused in-window video when returning to the Home dashboard and overlaid a play icon over the paused frame in the Hero tile (resumes when returning to full screen).

## v0.1.22 — 2026-11-18
- Fixed Home & Library Continue Watching to include both movies and TV shows via `/hubs/continueWatching/items`, refresh shortly after playback starts, and cap to the 6 most recent items.
- Switched TV Show season episodes to a 3-column vertical-scrolling grid.

## v0.1.21 — 2026-11-18
- Fixed TV Library tile clicks so TV shows load `ShowPageState` (seasons & episodes) and movies/episodes play immediately instead of opening the hidden two-screen detail modal.
- Scaled TV Show detail page (`TvShow`) proportionally with `TvUi.u`.

## v0.1.20 — 2026-11-18
- Rebuilt TV-only Library with vertical-scrolling shelves (zero horizontal scroll), `Finish the Season` (TV Series only), `Collections` (Movies only), `Browse All` with an A–Z filter bar and 8-column vertical poster grid, and `TvUi.u` proportional scaling.

## v0.1.19 — 2026-11-18
- Added Weather Location Lookup (city search + selectable results) to TV-only Settings › Display.

## v0.1.18 — 2026-11-18
- Restored TV-only Settings sub-tabs (**Display**, **Accounts**, **Server**, **What's new**) in the v2 Tile Hub look and feel.
- Brought back the Phone Remote QR card in Settings › Display, account sign-in/token editing in Accounts, live server metrics/graphs/activity in Server, and scrollable changelog cards in What's new.
- Scaled Settings `ValueTile` and sub-tab elements proportionally with `TvUi.u`.

## v0.1.17 — 2026-11-18
- Smoothed minimized in-tile video playback by capping the FBO to 960×540 and disabling focus drop-shadow blur while minimized video is playing.
- Added Subtitles and Audio track toggles back to the TV-only Player HUD.
- Added AM/PM (12-hour mode) and current year to the Home Clock tile.

## v0.1.16 — 2026-11-18
- Capped focus tile expansion so the Hero tile no longer overlaps adjacent columns/rows when focused.
- Tightened spacing in the left Clock + Plex info tile, and renamed "All 10 services" to "View all services".
- Added live in-tile video playback in the Home Hero tile when starting from Continue Watching (click Hero for full screen).

## v0.1.15 — 2026-11-18
- Scaled TV-only hub proportionally to fill 1080p/1440p/4K screens edge-to-edge with aligned TabBar and HintBar.
- Changed TV-only Home Continue Watching to a 2×3 grid (6 items, no scrolling), added a dedicated Weather tile below Screensaver, and added Plex Up Next details to the left Clock tile.
- Scaled ServiceTile SVG logos proportionally (with white Disney+ SVG contrast fix and bold wordmark fallback).

## v0.1.14 — 2026-11-18
- Expanded TV-only Home and Services tiles to fill the full 1080p/1440p vertical canvas and added monitor fallback for 1440p/4K scaling.
- Made TV-only navigation mouse-friendly: pointer cursors, clickable Back pill on detail/Search pages, mouse wheel scrolling on panoramas, and mouse-move wake for Player HUD.

## v0.1.13 — 2026-11-18
- Overhauled TV-only layout to the v2 Tile Hub (top TabBar, spatial focus across mixed-size tiles, 1080p design scaling for 1440p/4K).
- Added brand-colored ServiceTiles with bundled SVG logos and last-3 recent service tracking on Home and Services.
- Added the Back-to-tvdash Brave extension (`extras/back-pill`) and `POST http://127.0.0.1:47800/home` endpoint.
- Rebuilt TV-only Library, Show, Collection, Search (6×6 keyboard), Settings, Player HUD, and Ambient screens.

## v0.1.12 — 2026-11-17
- Added Jellyfin Server URL editing in Settings › Accounts (both two-screen and TV-only) and unmasked URL inputs for Jellyfin and Tautulli.

## v0.1.11 — 2026-11-17
- Added a scannable Phone Remote QR code and LAN URL to Settings › Display in both TV-only and two-screen layouts.

## v0.1.10 — 2026-11-17
- Reactivated Jellyfin integration (Quick Connect, username/password, Continue Watching, Recently Added, Search, Playback, and Setup step).
- Fixed TV-only Settings › Accounts to show the live Plex/Jellyfin sign-in code and instructions, plus inline TMDB / Tautulli API key entry.

## v0.1.9 — 2026-11-17
- Added an Up Next ("Play Next" / "Cancel") popup when a TV episode finishes playing in both two-screen and TV-only layouts.
- Fixed end-of-episode completion reporting so Plex marks finished episodes as watched and keeps the show in Continue Watching / Up Next.

## v0.1.8 — 2026-11-17
- Added Movie Collections to TV-only Library (shelf + 10-foot detail view with sort, hide watched, up next, and shuffle) and made two-screen Collections horizontally scrollable.
- Removed unused Genre filters from Library.
- Split TV-only Settings into Display, Accounts, Server, and What's new tabs.
- Added this What's new / version log in Settings.

## v0.1.7 — 2026-11-17
- Replaced +/- 10s playback buttons with Start from beginning / From start in both two-screen and TV-only layouts.
- Added Play from start to the two-screen Confirm Playback popup.
- Fixed Library TV series episode lists carrying over from the previous show; now loads the initial season automatically.

## v0.1.6 — 2026-11-17
- Added Confirm Playback popup for Continue Watching.
- Fixed Settings › Server layout.
- Separated TV / Series and Movies in Library.

## v0.1.5 — 2026-11-16
- Alphabetical Browse All in Library ignoring leading articles ("The", "A", "An").

## v0.1.4 — 2026-11-16
- TV-only 10-foot Search, Library parity, HUD controls, Night mode, and local HTTP Phone Remote (:8765).
