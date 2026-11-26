# Movie Page & Quick Switch Progress (`movie-page`)

Branched from `feature/feature-pack-1` (`v0.1.78`, `b95e2e4`). Version remains `0.1.78` until unit tests and live UI checks pass.

Design reference: [`docs/new concepts/movie_screens/tvdash-movie-page-build.md`](new%20concepts/movie_screens/tvdash-movie-page-build.md)

---

## Completed

### 1. Plex & Core Events (`57f16e7`)
- **`crates/core/src/events.rs`**: Added `Person`, `MovieDetail`, `CollectionLink`, `AppEvent::MovieDetail`, `AppEvent::MovieExtras`, `AppEvent::MovieCollection`, `Command::OpenMovie`, and `Command::SetMovieWatched`.
- **`crates/media/src/plex.rs`**: Added `PlexClient::movie_detail(&self, rating_key)` (genres capped at 2, crew capped at 3, cast capped at 20, resolution/HDR/audio mapping, collection tags, `primary_extra_key`) and `PlexClient::scrobble(&self, rating_key, watched)` (`/:/scrobble` / `/:/unscrobble`), plus unit test `movie_detail_maps_fields_and_caps`.

### 2. Controller & Play Label (`cab5650`)
- **`crates/app/src/movie.rs`**: Added `play_label` (part-watched / watched / unwatched states, 12h & 24h clock formatting, `< 1 min left` near-end handling + unit tests), `pick_collection`, and `build_collection_link`.
- **`crates/app/src/controller.rs`**: Wired `Command::OpenMovie`, `Command::SetMovieWatched`, movie `Command::PlayExtra` lookup, async extras + collection lookup, transcoded person photo requests (`person:<url>`), and `movie_detail` refresh after playback exits.

### 3. Shared State & Bridge (`d344135`)
- **`crates/app/ui/state.slint` & `app.slint`**: Added and exported `PersonData`, `ExtraData`, and `MoviePageState` (including `play-ring-lit`/`unlit` and `col-ring-lit`/`unlit`).
- **`crates/app/src/bridge/movie.rs`, `bridge/mod.rs`, `callbacks.rs`**: Added persistent `Rc<VecModel<…>>` models for `crew`, `cast`, and `extras`, initials + hashed-tint disc fallback when person photos are absent, a page tint from the backdrop/poster hue via `tint::card_tint` (same as the Show page), and a 60-second `Timer` keeping the `"ends at"` countdown current.

### 4. Navigation & OK-on-Movie (`9920d85`)
- **`crates/app/src/shell_tv.rs` & `tv_nav.rs`**: Added `Page::Movie` (`"movie"`), Back stack restoration, `push_search_results` (focusing results on person search), and `open_collection_from_movie` (focusing the movie's row when the Collection page loads).
- **Entry points updated to open Movie page**: Library shelves, Browse all, Collection page rows, Search library movie results, and Watching now.

### 5. UIs & Docs (`8dcbdea`, `1b2b67d`, `aad7979`, `3bf03cc`)
- **Orbit** (`8dcbdea`): `crates/app/ui/orbit/movie.slint` + `orbit/shell.slint` (Cast & crew / Extras tabs, `OrbitPlay`, collection capsule with `OrbitRing`, `···` watched toggle).
- **Classic** (`1b2b67d`): `crates/app/ui/tv/movie.slint` + `tv/shell.slint` (action row, collection tile, Cast & crew / Extras tabs, scrolled state + sticky bar).
- **Blades** (`aad7979`): `crates/app/ui/v3/movie.slint` + `v3/shell.slint` (left/right strips, panel backdrop scrims, chips, segmented collection bar, Cast & crew / Extras tabs).
- **Docs** (`3bf03cc`): Updated `CHANGELOG.md` (`## Unreleased`) and `docs/status.md`.

### 6. QuickSwitchPicker Formatting Fix (`79fa5a1`)
- **`crates/app/ui/shared/quick_switch_picker.slint` & `tv_nav.rs`**: Centered the **Automatic** card title/subtitle inside a `96 px` card so the subtitle no longer touches the bottom border, compacted the 5-column app grid (`y: 376`, `332 × 166`, `182 px` row pitch) so 3 rows clear the `y: 980` footer hint, and added `quick_switch_rects` in `tv_nav.rs`.
- Not yet in `CHANGELOG.md` — add a line under `## Unreleased`.

### 7. Follow-up Fixes
| Commit | Fix |
|---|---|
| `a85f962` | `MediaError` variants, `PlexClient::scrobble`, Slint syntax |
| `730c783` | Slint `line-height`, `Tile::activated`, `grid-col`/`grid-row` |
| `26f224a` | Shell Back pills; show-scroll length comparison |
| `5668b16`, `4b248f7` | Blades: `V3Theme` token names |
| `d6649f8` | Orbit: `OrbitPill` `selected`/`primary` and `activated` |
| `2dfac2a` | App crate compile errors: `MoviePageState` import, cached `slint::Image` used directly, tint via `tint::card_tint`, `HashSet` watched keys |

---

## Known Gaps / Out of Scope
- **Home** (What's On / Up Next) still plays directly instead of opening the Movie page.
- **PS5 UI** has no Movie page: OK on a movie plays it directly (`callbacks.rs`, `on_open_movie` checks `Display.dashboard-ui == "ps5"`).

---

## Verification Status

| Check | Status |
|---|---|
| `cargo test -j 2 -p tvdash-core` | Passed (15/15) |
| `cargo test -j 2 -p tvdash-media` | Passed (20/20) |
| `cargo test -j 2 -p tvdash -- --test-threads=1` | Passed (112 passed, 8 ignored) after `2dfac2a` |
| Live test: Orbit (`--features ui-orbit`) | Pending |
| Live test: Classic (`--features ui-classic`) | Pending |
| Live test: Blades (`--features ui-blades`) | Pending |

### Live Test Checklist (run in each UI)
- [ ] OK on a movie opens the Movie page from: Library shelves, Browse all, Collection page rows, Search results, Watching now
- [ ] Home still plays directly
- [ ] Back returns to the previous page with focus restored
- [ ] Play label correct for unwatched, part-watched and watched movies; "ends at" updates while the page stays open
- [ ] Watched toggle marks watched / unwatched and the play label updates
- [ ] Page tint matches the backdrop and suits the current theme
- [ ] Cast & crew: photos load; initials disc shows when a person has no photo
- [ ] Selecting a person opens Search with focus on the results
- [ ] Collection link opens the Collection page focused on this movie's row
- [ ] Extras tab lists extras and plays the selected one; trailer appears when present
- [ ] After playback exits, the Movie page refreshes (watched state / progress)

### Next Commands (Host)
```bash
cargo run -j 2 -p tvdash --no-default-features --features ui-orbit -- --windowed --layout tv
cargo run -j 2 -p tvdash --no-default-features --features ui-classic -- --windowed --layout tv
cargo run -j 2 -p tvdash --no-default-features --features ui-blades -- --windowed --layout tv
```
