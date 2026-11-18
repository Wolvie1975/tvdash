# tvdash Changelog

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
