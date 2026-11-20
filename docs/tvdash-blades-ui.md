# tvdash — "Blades" UI option (decisions & to-dos)

Second UI option for tvdash, alongside the current one ("Classic"). Same pages and data as the redesign brief; different look and navigation.

Mockups: https://claude.ai/artifact/81e81kx9uQqBERcLKja5W1 (Design canvas "tvdash — Blades concepts")

## Look & feel (decided)
- Flat, modern take on side "blades" navigation. Glossy version was tried and dropped.
- The current tab is a solid rounded panel. Other top-level tabs (home, library, collections, sports, settings) are vertical strips with lowercase vertical labels: tabs before the current one on the left, tabs after it on the right. Strips nearer the panel are one step brighter.
- On sub-pages (Show, Collection, Search), the parent tab's strip is tinted with the accent, and a ← Back pill sits at the top of the panel.
- Focus: a single 4px accent ring with offset. Accent is amber (#F5A524 Dark / #F0A33A Stone / #B45309 Mist).
- Themes follow the app's Dark / Stone / Mist.
- Font in mockups: Nunito Sans.

## Screens mocked
Home (Dark/Stone/Mist), Library (TV Series, Browse all), Watching now overlay, Show, Collections, Collection detail, Search, Sports Competitions + Scoreboard, Settings (Display, Sites, Server, Diagnostics), Video player HUD, Ambient screensaver.

## Library → Browse all (decided)
- Never mix the two libraries: Browse all has a Movies / TV Series switch and shows one library at a time.
- To limit load, the **All** letter returns only the **first 10 titles A–Z** of the chosen library, with a "First 10 of N · pick a letter to see the rest" line. Picking a letter (or #) loads every title under it.
- Applies to both Classic and Blades.

## App changes to make (to-do)
- [ ] **Dashboard UI selector in BOTH UIs.** Settings → Display gets a "Dashboard UI" choice: *Classic · top tabs* / *Blades · side tabs*. It must exist in the current (Classic) settings page too, so users can switch to Blades and back.
- [ ] Persist the choice in config; swap the shell (`TvShell` in `laptop.slint`) by the setting.
- [ ] **Remove Shuffle** from the app entirely (Show page and Collection detail); only Resume / From S1·E1 and Play all remain.
- [ ] **Browse all:** per-library (Movies / TV Series) only; "All" = first 10 A–Z (limit the query, not just the view); letters load the full set for that letter.
- [ ] Blades skin under e.g. `crates/app/ui/v3/`, bound to the existing `state.slint` globals.
- [ ] Update `tile_rects()` in `crates/app/src/tv_nav.rs` for Blades focus rectangles (side strips are focusable tab targets).
- Phone remote QR was removed from the Blades Settings mockup (decide separately whether the app keeps it).
