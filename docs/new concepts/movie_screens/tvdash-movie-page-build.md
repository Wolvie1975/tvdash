# tvdash — Movie page (Orbit · Classic · Blades) — Build spec (rev. 2026-11-25)

Applies to **TV-only** tvdash. Adds a **Movie page**: a page for one movie, the counterpart of the Show page. It is drawn in **Orbit**, **Classic** and **Blades**. **PS5 is deactivated** and gets no Movie page (§0, §7).

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **Movie page · all UIs**

| Board | Section |
|---|---|
| Orbit · Cast & crew | §4.1 |
| Orbit · Extras | §4.2 |
| Classic · Cast & crew | §5.1 |
| Classic · Extras | §5.2 |
| Blades · Cast & crew | §6.1 |
| Blades · Extras | §6.2 |

Each Cast & crew board has tweaks for the open tab and **in a collection on/off**. The Orbit board also has the watched state (part-watched / unwatched / watched). Titles, names and durations on the boards are placeholders. The teal glow on each board stands in for the movie's backdrop art.

All sizes are **1920 × 1080 design units**, scaled by `TvUi.u`. Each UI keeps its own safe area, tokens and fonts (Orbit: `tvdash-orbit-build.md` §1; Classic: `tvdash-library-show-v2-build.md`; Blades: `tvdash-blades-glass-build-spec.md` §1–4).

---

## 0. Decisions

| Topic | Decision |
|---|---|
| What opens it | **OK on a movie opens the Movie page** in the Library, Browse all, Collections and Search (§3.3). Home (What's On, Up next) keeps playing directly |
| Contents | Backdrop, title, details, Play / Resume, **From start**, **Trailer**, the **collection link**, then two tabs: **Cast & crew** and **Extras** |
| Backdrop | The movie's own `art`, full bleed, with that UI's usual scrims. Fallbacks as `FocusBackdrop` (blurred poster, then a poster-tint glow). It stays the same on both tabs and while an extra plays |
| OK on a person | Opens **Search** with that person's name typed in (`Watch.query`). No new person page |
| Extras | Reuse Feature pack 1 **Part D** (`plex.extras`, `Command::PlayExtra`). Part D was written for shows only; this extends it to movies (§2.3) |
| Collection link | Only when the movie is in a Plex collection. OK opens that Collection page |
| Parity | One shared state global (`MoviePageState`) and one Rust path, used by all three UIs. **Nothing is computed per UI** |
| Scrolling | Vertical only, in every UI |
| PS5 | Deactivated. No `ps5/movie.slint`. If a PS5 build runs anyway, OK on a movie keeps today's behaviour (play directly; §3.3) |
| No Shuffle, no Star | As everywhere else |

---

## 1. Plex data (check first)

The user runs these with the `px` helper from Feature pack 1 (Ground rules). **If a field differs from this spec, follow the server and say so in the PR.**

```bash
K=<a movie rating key>            # e.g. from Continue watching
px "/library/metadata/$K" '.MediaContainer.Metadata[0] | {title, year, summary, contentRating, audienceRating, duration, viewOffset, viewCount, lastViewedAt, art, thumb, primaryExtraKey, Genre, Director, Writer, Collection, Role: (.Role[:3])}'
px "/library/metadata/$K" '.MediaContainer.Metadata[0].Media[0] | {videoResolution, audioChannels, videoProfile, Part: [.Part[0].Stream[] | select(.streamType==1) | {DOVIPresent, colorTrc, displayTitle}]}'
px "/library/metadata/$K/extras" '.MediaContainer.Metadata[] | {ratingKey, title, subtype, extraType, duration, thumb}'
```

| Need | Field | Notes |
|---|---|---|
| Title, year, summary | `title`, `year`, `summary` | |
| Rating | `contentRating` | `PG-13` |
| Score | `audienceRating` | One decimal: `★ 7.8`. Hide when missing |
| Genres | `Genre[].tag` | **First two** only |
| Runtime | `duration` (ms) | `1 h 52 m` |
| Progress | `viewOffset`, `viewCount`, `lastViewedAt` | §2.2 |
| Resolution | `Media[0].videoResolution` | `4k` → `4K`, `1080` → `1080p`, `720` → `720p`, else hide |
| HDR | Video stream `DOVIPresent` → `Dolby Vision`; `colorTrc == "smpte2084"` → `HDR` | Classic and Blades join it to resolution (`4K · Dolby Vision`). Hide if unsure |
| Audio | `Media[0].audioChannels` | 8 → `7.1`, 6 → `5.1`, 2 → `Stereo`, else hide |
| Crew | `Director[]`, `Writer[]` | `tag` (name), `thumb` (may be missing). Directors first, then writers, **max 3** |
| Cast | `Role[]` | `tag` (actor), `role` (character), `thumb`. Plex's order. **Max 20** |
| Collections | `Collection[].tag` | Matched by title to the cached collections list (§2.4) |
| Trailer | `primaryExtraKey`, `/extras` | §2.3 |

**People's photos:** `thumb` is usually an external URL (`metadata-static.plex.tv/…`). Load it through the server's transcoder, `plex_image(url, w, h)` (`/photo/:/transcode?url=<encoded>`), so it goes through the same LRU image cache. No photo → initials on a disc in a colour hashed from the name (as the boards show).

---

## 2. Shared Rust

### 2.1 Media crate (`crates/media/src/plex.rs`)

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub name: String,
    pub role: String,          // character for cast, "Director" / "Writer" for crew
    pub thumb: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MovieDetail {
    pub item: MediaItem,               // playable; carries resume_ms
    pub summary: String,
    pub year: Option<u32>,
    pub content_rating: Option<String>,
    pub audience_rating: Option<f32>,
    pub genres: Vec<String>,           // first two
    pub duration_ms: u64,
    pub view_offset_ms: u64,
    pub view_count: u32,
    pub last_viewed_at: Option<i64>,
    pub resolution: Option<String>,    // "4K"
    pub hdr: Option<String>,           // "Dolby Vision" / "HDR"
    pub audio: Option<String>,         // "5.1"
    pub crew: Vec<Person>,             // max 3
    pub cast: Vec<Person>,             // max 20
    pub collections: Vec<String>,      // Collection[].tag
    pub art: Option<String>,
    pub thumb: Option<String>,
    pub primary_extra_key: Option<String>,
}

/// One movie with everything the Movie page needs (one request).
pub async fn movie_detail(&self, rating_key: &str) -> Result<MovieDetail>;
```

- Add `Role`, `Director`, `Writer`, `Collection` (`#[serde(default)]` `Vec<Tag>`), `audienceRating`, `primaryExtraKey` and `lastViewedAt` to the private `Metadata` struct if they aren't there. Reuse the existing `Genre` tag type.
- **Tests** with a fixture copied from the curl output (names anonymised): every field mapped; the caps (2 genres, 3 crew, 20 cast); `audioChannels` 6 → `5.1`; missing `Role` → empty cast; `4k` → `4K`.

**Commit:** `Plex: movie detail with cast, crew and collections`

### 2.2 Play button text (pure function, `crates/app/src/movie.rs`)

```rust
pub struct PlayLabel { pub label: String, pub meta: String, pub progress: f32, pub show_from_start: bool }

/// `now` is local time; the "ends at" time uses the remaining runtime.
pub fn play_label(d: &MovieDetail, now: chrono::DateTime<chrono::Local>, h24: bool) -> PlayLabel;
```

| State | Rule | `label` | `meta` | `progress` | From start |
|---|---|---|---|---|---|
| Part-watched | `view_offset_ms > 0` | `Resume` | `40 min left · ends at 11:12 PM` | offset ÷ duration | Shown |
| Watched | `view_count > 0`, no offset | `Play again` | `Watched · Mar 2026` (`last_viewed_at`) | 1.0 | Hidden |
| Unwatched | otherwise | `Play` | `1 h 52 m · ends at 11:27 PM` | 0 | Hidden |

- Times follow Settings › Clock (12 h / 24 h).
- While the page is open, recompute `meta` once a minute (a `slint::Timer` in the bridge), so "ends at" stays right.
- **Tests:** the three states; 12 h and 24 h; a resume point within 1 minute of the end still counts as part-watched.

### 2.3 Extras for movies (extends Feature pack 1, Part D)

**If Part D isn't merged yet, do D1 (`plex.extras`) first, as written there.** It already takes "a show or movie".

- `Command::PlayExtra(String)` (D2) looks the key up in "the last extras list for the open show". Change it to **the last extras list for the open Show page *or* Movie page**, and keep everything else in D2: no resume, no Up Next, no sleep "end of item", no recents. When it ends or stops, return to **the page it was started from**.
- `Command::PlayTrailer` isn't needed: Trailer = `PlayExtra(<first Trailer key>)`. If there's no trailer in `/extras` but `primary_extra_key` is set, use that key.

### 2.4 Collection link

From the cached collections list (`tvdash-collections.md` §1, the Collections tab already keeps it) and the watched-count cache (`orbit_counts`, Orbit spec §1.6; it's shared by every UI):

1. Keep only the collections whose title matches one of `MovieDetail.collections`.
2. If there are several, pick the first that has something unwatched, else the first.
3. Get its children (cached with the collection; fetch `/library/collections/{key}/children` only if not cached) and find this movie's position: `#4 of 8`.
4. **Next unwatched:** the first child in collection order with `view_count == 0`, other than this movie (Orbit shows it).

```rust
pub struct CollectionLink {
    pub key: String, pub title: String, pub position: usize, pub total: usize,
    pub watched: usize, pub next_unwatched: Option<String>, pub poster: Option<String>,
}
```

None found → no link (`in-collection = false`). Don't wait for this: send the page first, and the link when it's ready (§2.5).

### 2.5 Controller and events

`crates/core/src/events.rs`:

```rust
// Command
OpenMovie(String),                 // rating key
// AppEvent
MovieDetail(Box<MovieDetail>),
MovieExtras { movie_key: String, extras: Vec<ExtraItem> },
MovieCollection { movie_key: String, link: Option<CollectionLink> },
```

`crates/app/src/controller.rs`, `Command::OpenMovie(key)`:

1. `plex.movie_detail(&key)` → `AppEvent::MovieDetail`. Fetch the backdrop (`art`, 1920 × 1080 for Orbit, 1360 × 720 for Classic, 818 × 440 for Blades; request the one the current UI uses) and the people's photos through the existing image path.
2. **Separately** (the page doesn't wait): `plex.extras(&key)` → `MovieExtras`, with thumbnails through the same path as episode stills; and the collection lookup (§2.4) → `MovieCollection`.
3. Remember `open_movie: Option<String>` so late replies for another movie are dropped (`movie_key` check in the bridge).
4. **After playback** of this movie (stop or end), re-fetch `movie_detail` when returning to the page, so Resume / progress / watched are fresh. Also refresh `orbit_counts` for its collection (as the Collection page does today).

**Commit:** `Movie page: controller, events and play label`

### 2.6 Shared state (`crates/app/ui/state.slint`)

```slint
export struct PersonData { name: string, role: string, initials: string, photo: image, has-photo: bool, tint: color }
export struct ExtraData  { key: string, title: string, kind: string, duration: string, thumb: image, is-trailer: bool }

export global MoviePageState {
    in property <string> rating-key;
    in property <string> title;
    in property <string> kicker;           // "MOVIE · 2019 · 1 H 52 M · PG-13" (Orbit) — Rust builds it
    in property <string> meta-line;        // "2019 · Drama · Thriller · PG-13 · 1 h 52 m · ★ 7.8 · 4K" (Classic)
    in property <[string]> chips;          // ["2019","PG-13","Drama","Thriller","1 h 52 m","★ 7.8","4K"] (Blades)
    in property <[string]> tech-chips;     // ["Drama","Thriller","★ 7.8","4K · Dolby Vision","5.1"] (Orbit)
    in property <string> summary;
    in property <image> backdrop;
    in property <bool> has-backdrop;
    in property <color> tint;              // poster tint (Classic Resume tile, kickers)

    in property <string> play-label;       // §2.2
    in property <string> play-meta;
    in property <float> progress;
    in property <bool> show-from-start;
    in property <bool> has-trailer;

    in property <bool> in-collection;
    in property <string> collection-title;
    in property <string> collection-position;   // "#4 of 8"
    in property <int> collection-total;
    in property <int> collection-watched;
    in property <string> collection-next;       // "[Movie 5]" or ""
    in property <image> collection-poster;

    in property <[PersonData]> crew;
    in property <[PersonData]> cast;
    in property <[ExtraData]> extras;
    in-out property <string> tab: "cast";       // "cast" | "extras"
    in property <string> cast-count;            // "14" = crew + cast
    in property <bool> loading: true;

    callback play();                 // Resume / Play / Play again
    callback play-from-start();
    callback play-trailer();
    callback play-extra(string);
    callback open-collection();
    callback search-person(string);
    callback set-tab(string);
    callback toggle-watched();       // Orbit ··· only (§4.1); §9 Q1
}
```

- Keep `crew`, `cast` and `extras` as persistent `Rc<VecModel<…>>` and `set_vec` them (research doc R3).
- **Empty cases:** no extras → `extras` empty and the Extras tab hidden in every UI (Classic and Blades show the single heading `Cast & crew` without the switch). No cast and no crew → the Cast & crew tab shows one line `No cast or crew listed`. Both empty → that line, no tabs.
- Reset everything (and `tab = "cast"`) when another movie opens. `loading` is true until `MovieDetail` arrives; each UI shows its usual skeleton (blank title, summary lines in `track`).

### 2.7 Bridge (`crates/app/src/bridge/movie.rs`, new)

- `MovieDetail` → fill the global; build `kicker`, `meta-line`, `chips`, `tech-chips` from the same fields (§1), and the play label (§2.2). Start the one-minute timer; stop it when the page closes.
- `MovieExtras` → `extras` (trailers first, already sorted by D1), `has-trailer`.
- `MovieCollection` → the collection properties.
- Callbacks (`callbacks.rs`):

| Callback | Does |
|---|---|
| `play()` | `Command::Play(item)` with `resume_ms` from `view_offset` (part-watched), else from 0 |
| `play-from-start()` | `Command::Play(item)` with `resume_ms = None` |
| `play-trailer()` | `Command::PlayExtra(<first trailer or primary_extra_key>)` |
| `play-extra(key)` | `Command::PlayExtra(key)` |
| `open-collection()` | Opens the Collection page (the existing path the Collections tab uses), focused on **this movie's row** |
| `search-person(name)` | `Watch.query = name`, `Watch.edited`, `tv_nav::push(Page::Search)`, focus on the results (not the keyboard) |
| `set-tab(t)` | Sets `tab`; the UI redraws; focus goes to the first item of the new tab |
| `toggle-watched()` | §4.1 |

**Commit:** `Movie page: shared state and bridge`

---

## 3. Navigation (`crates/app/src/tv_nav.rs`)

### 3.1 Page

- `Page::Movie`, screen name `"movie"`, parent tab = the tab it was opened from (Library, Collections, or Home for Search). Hint: `OK Play · ↓ Cast & crew · Back`.
- Each enabled shell mounts it: `if TvUi.screen == "movie": <Ui>Movie { }` next to its Show page line.
- **Focus rects:** one table per UI (`orbit_tile_rects`, Classic `tile_rects`, `blades_tile_rects`), taken from §4–§6. Conditional rects (From start, Trailer, collection link, Extras tab) follow the same "only when visible" pattern as the Show page's conditional buttons, with indices shifted.
- **Back** → the page it was opened from, with focus on the movie that opened it. From Search, Back returns to the results list.

### 3.2 Default focus

| When | Focus |
|---|---|
| Page opens | The **Play / Resume** button |
| Back from playing the movie or an extra | The button or extra that started it |
| `set-tab` | First crew pill (Cast & crew) / first extra (Extras) |

### 3.3 Where OK on a movie changes

Replace the "play this movie" action with `Command::OpenMovie(key)` (via a shared `open-movie(string)` callback on `Library` / `LibraryLanding`) in these places, in every enabled UI:

| Place | Today | Now |
|---|---|---|
| Library · Movies shelves (Continue watching, Recently added, Start watching dial) | Resume / Play | Movie page |
| Browse all · Movies | Resume / Play, or the detail panel | Movie page (the movie detail panel is no longer used for Plex movies) |
| Collection page rows | `play-item(key, resume)` | Movie page (§9 Q2) |
| Search · In your library (movie results) | Detail panel | Movie page |
| Watching now rows (movie) | "movie detail" | Movie page |
| Home What's On, Up next | Play | **Unchanged** |

**PS5:** if `DashboardUi::Ps5` is the active UI, `open-movie` falls back to today's play path. One `match` in the callback, nothing in `ui/ps5/`.

**Commit:** `Movie page: navigation, and OK on a movie opens it`

---

## 4. Orbit (`ui/orbit/movie.slint`, boards "Orbit · Cast & crew", "Orbit · Extras")

Orbit rules apply: **hover = focus**, **vertical scrolling only**, one focus ring (4 px `text` + 5 px `bg` gap), Outfit font, `OrbitTheme` tokens. Quick circles hidden (as on the Show page). Pill nav: **Library** active.

### 4.1 Layout

| Part | Spec |
|---|---|
| Backdrop | Full screen `FocusBackdrop` with `scrim-left` + `scrim-bottom` |
| Back pill | `‹ Library` (or `‹ Collections` / `‹ Search`), x 110, y 130, h46 |
| Hero | x 110, y 210, w780 |
| Kicker | `MOVIE · 2019 · 1 H 52 M · PG-13` 18 / 600, 2.5 tracking, `accent` |
| Title | 84 / 700, −2 tracking, one line, elide |
| Summary | 20 / line-height 30, `sub`, **fixed 3 lines** (90) |
| Tech chips | `tech-chips`, h34 pills, radius 17, `row` fill, 16 `sub`, gap 8, 16 below the summary |
| Play row | 26 below: `OrbitPlay` 104 px (ring = `progress`) · `play-label` 23 / 600 + `play-meta` 17 `sub` · pills h44 **From start** (only `show-from-start`) · **▶ Trailer** (only `has-trailer`) · **···** |
| ··· | Small menu (Orbit Settings row style): **Mark as watched** / **Mark as unwatched** → `toggle-watched()` → Plex `/:/scrobble` or `/:/unscrobble` (`key=<ratingKey>&identifier=com.plexapp.plugins.library`), then re-fetch. §9 Q1 |
| Collection capsule | 34 below the play row, only `in-collection`: 600 × 96, radius 48, `row`: 72 px `OrbitRing` (segments = `collection-total` up to 24, lit = `collection-watched`; >24 → % arc) around the collection poster · `PART OF A COLLECTION` 14 / 600 `muted` · `[title] · #4 of 8` 22 / 600 · `3 of 8 watched · next unwatched: [Movie 5]` 16 `sub` (drop the second half when `collection-next` is empty) · `›` |
| Right panel | x 1000, y 220, w810 |
| Tab pills | h48, radius 24: **Cast & crew · 14** / **Extras · 6** (count at 70%); active = `text` fill / `bg` ink. Hint right, 14 `muted`: `↑ ↓ rows · OK on a person searches their name` / `↑ ↓ extras · OK plays in mpv · Back returns here` |
| Crew | `CREW` label 15 / 600 `sub`; one row of up to 3 pills h72, radius 36, `row`: 60 px disc (photo or initials) · name 19 / 600 · role 14 `muted` |
| Cast | `CAST` label; **4 across**, 140 px round photos, name 19 / 600, character 15 `sub`; row gap 28, column gap 20. Rows scroll ↑ ↓ (`SlotList` by rows), `fade` at the bottom of the panel |

### 4.2 Extras tab

| Part | Spec |
|---|---|
| Label | `6 EXTRAS · TRAILERS FIRST` 15 / 600 `sub` |
| Rows | `SlotList` of `OrbitRow` h88 (focused h112), radius ½ height: round thumb 72 (focused 92) with a ▶ · title 21 / 600 (focused 25) · kind 16 `sub` (focused: `accent`) · duration 18 `sub` · focused row adds **OK · Play** (primary pill) |
| Ends | Last visible row at 50% when there are more |

### 4.3 Focus

- Hero: Play ← → From start · Trailer · ···. ↓ from the play row → collection capsule (if shown). ↑ from the play row → pill nav.
- → from the right-most hero item (or the capsule) → the active tab pill. ← from anything in the right panel's left column → back to Play.
- Tabs: ← → switch tab at once. ↓ → crew row → cast grid (← → within a row, ↑ ↓ between rows). Extras: ↑ ↓ through rows.
- OK: Play / pills as labelled · capsule → `open-collection` · person → `search-person` · extra → `play-extra`.
- Back: right panel → Play → the page it came from.

---

## 5. Classic (`ui/tv/movie.slint`, boards "Classic · Cast & crew", "Classic · Extras")

Built like the Classic Show page (`tvdash-library-show-v2-build.md` §3): Inter, `Theme` tokens (`bg` #0B0D10, `surface` #14171C, `border` #2A2F37), tile radius 6, focus = 4 px `text-primary` outline (offset 4) + `0 20 60 #0009`. Safe area 96 / 54. Top tabs with **Library** active.

### 5.1 Layout

| Element | Position / size |
|---|---|
| Top chrome | Tabs (existing) · weather + clock chip (`bg @ 60%`) · `← Back` pill at y 118 |
| Backdrop | x 560, y 0, 1360 × 720, movie `art`; left scrim (`bg` → `bg @ 82%` 25% → transparent 62%) and bottom scrim from y 380 (same as the Show page) |
| Kicker | y 176: `MOVIE · PLEX`, 20 / 500, 1.2 tracking, `tint` |
| Title | 88 / 400, −1.5 tracking, one line, elide |
| Meta | `meta-line`, 24, `#C9CDD3` |
| Summary | 22 / 1.45, max 780 wide, **fixed 3 lines** (96) |
| Action row | y 452, height 120, gap 16: **Resume** 440 (`tint` fill, dark text: `▶ <play-label>` 30 / 500 + `play-meta` 19; 5 px dark progress bar along the bottom) · **From start** 260 (only `show-from-start`) · **▶ Trailer** 220 (only `has-trailer`) · **collection tile** (only `in-collection`; takes the rest of the row, 760 at full width) |
| Collection tile | `surface`: poster 60 × 90 · `PART OF A COLLECTION` 17, 1.2 tracking, `text-muted` · `[title] · #4 of 8` 26 · one 5 px segment per movie (`tint` = watched, `border` = not; >24 → one continuous bar) · right `3 of 8 watched ›` 18 `text-secondary` |
| Tabs | y 612, 26 px, like the season tabs: **Cast & crew** · **Extras** (active 400 `text-primary` + 3 px `tint` underline, other 300 `#5E646D`); 1 px `border` line under the row; right side `3 crew · 11 cast` / `6 extras · trailers first`, 20 `text-muted` |
| Left column | x 96, y 700, w260 (where the season column sits on shows) |
| Grid | x 388, y 700, w1436 |

**Cast & crew:** left column = `CREW` 20 / 1.2 tracking + up to 3 rows h84 (`surface`, radius 6): 56 px round photo · name 20 · role 16 `text-muted`. Grid = **6 across**, column gap 16, row gap 32: photo 226 × 300 (radius 6; initials 44 / 300 when no photo) · name 22 · character 17 `text-muted`.

### 5.2 Extras tab

Same as the episode grid. Grid = **3 across** 468 × 263 tiles, column gap 16, row gap 40: kind chip top-left (`TRAILER` in `tint` fill / dark text; other kinds `bg @ 80%`), duration chip bottom-right, title 24 below, kind 18 `text-muted`. Left column = the **focused extra**: still 260 × 146 · kind 18 `tint` · title 26 · `2:31 · plays in mpv` 18 · `Back returns to this page.` 18 `text-muted`.

### 5.3 Scrolling and focus

- **Scrolled state:** same as the Show page (§3.4 there). Moving focus into the second grid row fades the backdrop and top block, and shows the sticky bar at y 132 (`surface @ 95%`): title 28 · divider · compact tabs 22 · **Resume** button right (`tint`, 52 tall). The left column sticks at y 244. ↑ from the first visible row returns to the top.
- Action row: ← → across Resume · From start · Trailer · collection tile. ↓ → tabs (current tab). Tabs: ← → switch at once; ↓ → first crew row (Cast & crew) / first extra.
- Left column and grid are one area, moved geometrically (existing Classic behaviour): ← from the grid's first column → crew rows; → from the crew rows → grid.
- Back: grid → tabs → Resume → the page it came from.

---

## 6. Blades (`ui/v3/movie.slint`, boards "Blades · Cast & crew", "Blades · Extras")

Built like the Blades Show page (Blades spec §5.5): Nunito Sans, `V3Theme` tokens, focus = 4 px `accent` outline, offset 5.

### 6.1 Layout

| Element | Spec |
|---|---|
| Strips | Sub-page of Library: left **home** (far) · **apps** (near) · **library** (parent: `accentSoft` fill, `accentInk` label, 900); right **collections** · **sports** (near) · **settings** (far). From Collections / Search: that tab is the parent instead |
| Panel | 1320 wide at 1920 (between the strips), radius 28, `panel`, padding 32 / 44 |
| Backdrop | Right 62% of the panel, top 440 px; scrims: horizontal (`panel` 100% → 55% at 30% → 0 at 65%) and vertical (0 at 45% → `panel` at 100%) |
| Header row | **‹ Back** pill (h48, `raised`, 21 / 800) · breadcrumb `Library › Movies` 20 / 700 `sub` · right: weather + clock 24 / 700 |
| Title block | y 112 (panel), max 760 wide: title 72 / 900, −1.5 tracking · `chips` (chip style: 17 / 800, padding 5 × 11, radius 9, `raised`) · summary 21 / 1.45 `sub`, **fixed 2 lines** |
| Buttons | 20 below, h58, pill, gap 12: **▶ Resume · 40 min left** primary (`accent`, `accentText` 22 / 900; text = `play-label` + ` · ` + the first part of `play-meta`) · **From start** secondary (only `show-from-start`) · **▶ Trailer** secondary (only `has-trailer`) |
| Collection card | y 404 (panel), 760 × 76, radius 18, `raised`, only `in-collection`: poster 40 × 60 (radius 8) · `PART OF` 15 / 900, 2 tracking, `accentInk` + `[title] · #4 of 8` 20 / 900 · segmented progress (Blades component: equal segments, gap 4, height 7, watched `accent`, rest `rgba(255,255,255,.35)`, 360 wide) · right `3 of 8 watched ›` 18 / 700 `sub`. Without it, everything below moves up 100 px |
| Switch | y 504: segmented control **Cast & crew \| Extras** (Blades component); right side `3 crew · 11 cast` / `6 extras · trailers first`, 18 / 700 `sub`. Only `Cast & crew` (as a heading) when there are no extras |
| Left column | x 44 (panel), y 580, w260 |
| Grid | x 334 (panel), y 580, w942 |

**Cast & crew:** left column = `CREW` 17 / 900, 2 tracking + up to 3 rows h72 (`raised`, radius 16): 48 px round photo · name 20 / 800 · role 16 / 700 `sub`. Grid = `CAST` label, then **6 across** posters (2:3, radius 14, 212 tall at this width), column gap 18, row gap 24; name 18 / 800 and character 15 / 700 `sub`, both elided.

### 6.2 Extras tab

Grid = **3 across** 16:9 cards (radius 15), column gap 26, row gap 24: kind badge top-left (UP NEXT badge style: 14 / 900, `TRAILER` in `accent` / `accentText`, other kinds white on `rgba(0,0,0,.6)`), duration bottom-right (15 / 900 on `rgba(0,0,0,.6)`), title 21 / 900, kind 18 / 700 `sub`. Left column = the **focused extra**: still 260 × 146 (radius 15) · kind 17 / 900 `accentInk` · title 24 / 900 · `2:31 · plays in mpv · Back returns here` 18 / 700 `sub`.

### 6.3 Scrolling and focus

- The panel scrolls vertically when focus goes below the first grid row; the content fades under the 100 px bottom fade (Blades rule). The header row and title block scroll away with it; ↑ from the first grid row scrolls back to the top.
- Buttons: ← → between them. ↓ → collection card (if shown) → switch → first crew row / first extra. ↑ goes back the same way.
- Switch: ← → change the tab at once. Crew rows and the grid: geometric, as in Classic §5.3.
- **Strips:** ← from the left-most item (Resume, the collection card, the switch, the crew rows) → the library strip; → from the right-most item → the collections strip (Blades spec §6).
- Back: grid → switch → Resume → the page it came from.

---

## 7. Files

```text
crates/media/src/plex.rs               # MovieDetail, Person, movie_detail() + tests (§2.1)
crates/core/src/events.rs              # OpenMovie, MovieDetail, MovieExtras, MovieCollection
crates/app/src/
├─ movie.rs                            # play_label(), collection link helpers + tests (§2.2, §2.4)
├─ controller.rs                       # OpenMovie, PlayExtra for movies, refresh after playback
├─ bridge/movie.rs                     # MoviePageState fill, one-minute timer (§2.7)
├─ callbacks.rs                        # MoviePageState callbacks, open-movie (PS5 fallback)
└─ tv_nav.rs                           # Page::Movie, rects for Orbit / Classic / Blades, Back targets
crates/app/ui/
├─ state.slint                         # MoviePageState, PersonData, ExtraData
├─ orbit/movie.slint  orbit/shell.slint
├─ tv/movie.slint     tv/shell.slint
└─ v3/movie.slint     v3/shell.slint
```

No `ui/ps5/` changes.

---

## 8. Task list

Follow `GEMINI.md`: one commit per step; hand `cargo` commands to the user (`-j 2`, one crate at a time). Dev loop per UI: `cargo run -j 2 -p tvdash --no-default-features --features ui-orbit -- --windowed --layout tv` (then `ui-classic`, `ui-blades`).

1. **Plex:** curl checks (§1) → `movie_detail` + tests (§2.1).
2. **Extras for movies:** Part D1 if it isn't merged; `PlayExtra` for the Movie page (§2.3).
3. **Controller + play label + collection link** (§2.2, §2.4, §2.5) + tests.
4. **Shared state + bridge + callbacks** (§2.6, §2.7).
5. **Navigation:** `Page::Movie`, Back targets, OK on a movie opens it (§3), PS5 fallback.
6. **Orbit** page + rects (§4).
7. **Classic** page + rects + scrolled state (§5).
8. **Blades** page + rects + strips (§6).
9. **Polish & docs:** hover = focus in Orbit; 1080p / 1440p / 4K; Stone and Mist themes in Classic and Blades (the backdrop stays dark); `CHANGELOG.md` (Unreleased), `docs/status.md`. Don't bump the version.

---

## 9. Open questions (defaults in bold)

| # | Question | Default |
|---|---|---|
| 1 | Orbit's **···** (Mark as watched / unwatched): keep it in Orbit only, add it to Classic and Blades, or drop it? | **Orbit only** for now |
| 2 | Collection page: OK on a movie opens the Movie page, or keeps playing it? | **Opens the Movie page** (the focused row still shows Resume / Play as a hint; §3.3) |
| 3 | Cast photos for people Plex has no photo for: initials (as drawn), or hide those people? | **Initials** |
| 4 | Jellyfin movies (if any are shown) | **Out of scope**: Plex only, like the Show page |

---

## 10. Done when

- [ ] OK on a movie in the Library, Browse all, a Collection page, Search and Watching now opens the Movie page in Orbit, Classic and Blades. Home still plays directly.
- [ ] The page shows the movie's backdrop, title, details, the 3-line (Orbit, Classic) or 2-line (Blades) summary, and the right Play / Resume / Play again text with "ends at".
- [ ] Resume resumes, From start starts over, Trailer plays the trailer in mpv, and Back returns to the Movie page with focus on what started it.
- [ ] The collection link shows only for movies in a collection, with the right `#n of m` and watched count, and opens that Collection page on this movie.
- [ ] Cast & crew shows up to 3 crew and the cast in Plex's order, with photos or initials; OK on a person opens Search with their name.
- [ ] Extras lists trailers first; each plays in mpv with no resume point and no change to Continue watching. No extras → no Extras tab and no Trailer button.
- [ ] Opening another movie right after shows no stale cast, extras or collection.
- [ ] Nothing scrolls sideways. Orbit: hover = focus everywhere. Classic: the scrolled state and sticky bar work. Blades: strips reachable with ← / →.
- [ ] A PS5 build (if run) still plays movies directly and doesn't crash.
- [ ] Looks right at 1080p, 1440p and 4K; works by remote alone.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-25 | Spec created from the approved boards (Orbit, Classic, Blades; PS5 deactivated). OK on a movie opens the page; cast & crew, collection link, trailer + extras (extends Feature pack 1 Part D to movies) |
