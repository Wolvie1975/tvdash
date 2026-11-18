# tvdash Changelog

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
