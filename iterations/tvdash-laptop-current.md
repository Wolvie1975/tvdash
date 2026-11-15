# tvdash — Laptop Redesign, Current Version (2026-11-14)

The single, current spec for everything on the canvas page **Laptop redesign**, after the 2026-11-14 clean-up. It pulls together the chosen designs from `tvdash-laptop-redesign.md`, `tvdash-dashboard-library-update.md`, `tvdash-show-page.md` and `tvdash-collections.md`. Those docs stay as history and for extra detail. **Where they disagree, this doc wins.**

Design canvas: https://claude.ai/artifact/UggZzEChLyVVffZNfdGnTf, page **Laptop redesign**:

| Board | What it shows |
|---|---|
| **Laptop dashboard — Option B** | Header, three boxes, search panel, theme switch, Continue Watching switch; Tweaks: playing / nothing playing, description on/off |
| **Library C — Smart shelves** | Watching now, big box, shelves, Popular on Republic City, Genres / Collections; Movies / TV switch, sort menu, 7 / 30 days |
| **Library — Show page** | Seasons and episodes; Tweaks: 3 / 9 / 20 seasons |
| **Library — Collection page (Movies)** | Collection hero and ordered movie grid |
| **Screensaver 3 — Sentence clock** | Sentence clock + On the TV status; Tweaks: playing on/off |

> **Removed 2026-11-14:** Library A (streaming-app rows), Library B (browser), Screensaver 1 (gallery) and Screensaver 2 (now playing).

> **Build order:** build this doc first. The Leaf battery feature (`tvdash-leaf.md`) comes after and changes the header grid (§3).

---

## 1. Decisions

| Topic | Decision |
|---|---|
| Media server | **Plex only**, single account |
| Dashboard | **Option B**: three equal full-height boxes |
| Colour | Poster tint on the On the TV card (and detail-page heroes); everything else neutral |
| Themes | **Follow system / Dark / Stone / Mist**; no white surfaces; the TV window is always Dark |
| Input | Touch first, mouse works; nothing needs hover |
| Apps | Fixed list of **10**, logos only |
| Library | **Library C — Smart shelves**, with Watching now and Popular on Republic City |
| Screensaver | **Screensaver 3 — Sentence clock**, manual start |
| Status colour | Amber and red only for real problems; "not set up" is a neutral **Set up** button |

---

## 2. Themes and poster tint

### Base tokens

| Token | Dark | Stone | Mist |
|---|---|---|---|
| `bg` | `#0B0D10` | `#CBC5BA` | `#C3CBD3` |
| `surface` | `#14171C` | `#DAD5CC` | `#D3DAE1` |
| `surface-raised` | `#1B1F26` | `#E3DFD7` | `#DEE3E8` |
| `border` | `#2A2F37` | `#B9B2A6` | `#AEB7C1` |
| `text-primary` | `#E8EAED` | `#1F1D1A` | `#1A1E23` |
| `text-secondary` | `#A0A6AF` | `#47433D` | `#3F464F` |
| `text-muted` | `#8A9099` | `#4E4943` | `#47505A` |
| `ok` / `warn` / `alert` | `#4CC38A` / `#E5A84B` / `#F0605D` | darker, re-checked for contrast | same |

- Theme switch: segmented control in the header (Dark moon / Stone, Mist sun), 56 px tall; live; saved in `config.toml`.
- Settings › Display › Theme adds **Follow system** (desktop portal `color-scheme` via `ashpd`).

### Poster tint (Option B)

1. Find the poster's dominant colour (`image` + `kmeans_colors`).
2. Convert to Okhsl (`palette`); keep the hue, cap saturation at 0.45, **fix lightness per theme**.
3. Saturation under ~0.08 → fallback blue.

| Token | Dark lightness | Light (Stone/Mist) | Use |
|---|---|---|---|
| `card` | 0.16 | ~0.80 | Card background |
| `poster` | 0.30 | ~0.68 | Poster placeholder |
| `accent` | 0.82 | ~0.35 | Label, progress, filled button |
| `sec` | hue-tinted ~0.78 | theme `text-secondary` | Supporting text |
| `track` | 0.24 | ~0.74 | Progress track |
| `edge` | 0.30 | ~0.72 | Secondary button borders |
| `on-accent` | `#0B0D10` | `#FFFFFF` | Text on the filled button |

```rust
pub fn tint_from(dominant: Srgb<u8>, dark: bool) -> Tint {
    let base = Okhsl::from_color(dominant.into_format::<f32>());
    let s = base.saturation.min(0.45);
    let mk = |l: f32, s: f32| Srgb::from_color(Okhsl::new(base.hue, s, l)).into_format();
    if dark {
        Tint { card: mk(0.16, s), accent: mk(0.82, s.min(0.70)), /* … */ }
    } else {
        Tint { card: mk(0.80, s * 0.5), accent: mk(0.35, s.min(0.60)), /* … */ }
    }
}
```

> **Contrast test once, not per poster:** a unit test over 36 hues per theme: `accent` on `card`, `on-accent` on `accent` and `sec` on `card` all ≥ 4.5:1.

Reference values (teal sample, dark): card `#0F2A2B`, poster `#1E4A4A`, accent `#7FE0D2`, sec `#A9C9C6`, edge `#24504F`. Full table: `tvdash-laptop-redesign.md` §6.

---

## 3. Dashboard

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ [🔍][☾]            12:32            │  ☀ 51°F           [Dark|Stone|Mist] [⚙] │
│              Wednesday, November 11 │  Partly sunny · H 58 L 40               │
├──────────────────────────┬──────────────────────────┬────────────────────────┤
│ ON THE TV         Plex   │ WATCH ON TV · 10 APPS    │ CONTINUE WATCHING  All›│
│ The Long Drive Home      │ ┌─────────┐ ┌─────────┐  │ [ TV / Series | Movies ]│
│ Season 2 · Episode 7     │ │  logo   │ │  logo   │  │ [post][post][post]     │
│ 2024 · TV-14 · 52 min    │ └─────────┘ └─────────┘  │ [post][post][post]     │
│ ┌──────┐ Description     │      … 5 rows …          │                        │
│ │poster│ from Plex…      │                          │                        │
│ └──────┘                 │                          │                        │
│ ━━━━━━━━━━━━━━━━━━━━━━━  │                          │                        │
│ [−10][ ❚❚ Pause ][+30][Stop]                        │                        │
└──────────────────────────┴──────────────────────────┴────────────────────────┘
```

- Window 1920 × 1080; padding 40 px top/bottom, 64 px sides; 24 px gaps.
- Main area: 12-column grid; each box spans 4 columns and the full height.

### Header (140 px, grid `1fr 2fr 1fr`)

| Left | Centre (spread evenly) | Right |
|---|---|---|
| **Search** (64 px square; filled while the panel is open) · **Screensaver** (64 px, monitor-with-moon) | Clock 112 px light, tabular numerals, date 24 px under it · divider · weather icon, temperature 48 px light, condition + high/low 22 px | **Theme switch** · **Settings** (64 px gear) |

> The Leaf feature later changes this grid to `0.7fr 2.3fr 1fr` to add the car chip after the weather (`tvdash-leaf.md` §3).

### On the TV box

1. Label "ON THE TV" (or "UP NEXT") in the tint accent; Plex badge right.
2. Title, full width, 44 px.
3. Episode line "Season 2 · Episode 7 — Crossroads" (22 px); nothing for movies.
4. Facts: year · rating · runtime · genre (18 px).
5. Poster 160 × 240 with the description beside it; no summary → genres + "No description on the server". Long descriptions clip.
6. Progress bar and times.
7. While playing: **−10 s · Pause (filled) · +30 s · Stop**, 76 px. Nothing playing: one **Resume on TV**.

| State | Label | Content |
|---|---|---|
| Playing | ON THE TV | What's playing, tinted from its poster |
| Nothing playing | UP NEXT | "Nothing on the TV. Pick up where you left off:" + top Continue Watching item, tinted from its poster |

Plex: `title` / `grandparentTitle`, `parentIndex` + `index`, `year`, `contentRating`, `duration`, `Genre`, `summary`, `thumb`.

### Watch on TV box

- Header "WATCH ON TV · 10 APPS" · "Full screen on TV".
- **2 × 5 grid**, tiles ~250 × 130, **logo only** (78% × 68% of the tile, `image-fit: contain`), app name as the accessible label.
- Tap → app full screen on the TV.

| # | App | Start URL |
|---|---|---|
| 1 | YouTube TV | `https://tv.youtube.com` |
| 2 | YouTube | `https://www.youtube.com` |
| 3 | Disney+ | `https://www.disneyplus.com` |
| 4 | Hulu | `https://www.hulu.com` |
| 5 | Apple TV | `https://tv.apple.com` |
| 6 | Netflix | `https://www.netflix.com` |
| 7 | Fandango at Home | `https://athome.fandango.com` |
| 8 | Prime Video | `https://www.primevideo.com` |
| 9 | Crunchyroll | `https://www.crunchyroll.com` |
| 10 | Tubi | `https://tubitv.com` |

- URLs live in `config.toml` `[[streaming]]` (fixable without a rebuild); no add/remove UI. Each entry keeps its `tmdb_providers` IDs for search.
- Logos bundled as SVG in `assets/logos/` (`youtube-tv.svg` … `tubi.svg`).

### Continue Watching box

- Header "CONTINUE WATCHING" · "See all ›" (opens the Library).
- **TV / Series · Movies** switch, full width, 56 px; default TV / Series.
- **3 × 2** grid of 2:3 posters; scroll for more.
- Badge top-left: "S1 · E3" (TV) or "22 min left" (movies); 6 px progress bar in the poster's accent; title underneath with ellipsis.
- Plex: `GET /hubs/continueWatching` (or `/library/onDeck`), split by `type`.

### Search panel

- Slides in (opacity + 48 px, ~250 ms) over **Watch on TV** and **Continue Watching**; On the TV stays visible.
- Empty until you type: search field (72 px, focused) + close. No recents.
- Results after ~300 ms debounce:

| Group | Action |
|---|---|
| In your library (Plex) | **Resume on TV** / **Play on TV** (filled) |
| On your apps | **Open on TV** |
| Costs extra | none |
| Not streaming | none |

- Footer: "Streaming availability in the US from JustWatch, via TMDB" (required by TMDB).
- Close (✕) or Search again: slides out, query clears. Data and ranking: `tvdash-future-plan.md` §3.

> **Test early:** whether GNOME's on-screen keyboard appears for a Slint text field on touch. If not, the panel needs its own keyboard.

---

## 4. Library — Smart shelves (Library C)

Opened from "See all ›" or Settings › Library. **Movies / TV / Series** switch next to the title; it changes every section except Watching now. The page **scrolls**.

| Row | Section | TV / Series | Movies |
|---|---|---|---|
| 1 (380 px) | **Watching now** (5 cols) | Every active stream on the server (same on both tabs) | Same |
| 1 | **Big box** (7 cols) | **Finish a season**: ≤ 3 episodes left, one bar segment per episode | **Continue watching**: started movies, bar + time left |
| 2 (352 px) | **Added this week** (6 cols, 6 posters) | New shows and episodes since Monday | New movies since Monday |
| 2 | **Watch again** (6 cols, 6 posters) | Shows watched all the way through | Movies played more than once |
| 3 (410 px) | **Popular on Republic City** (full width, 12 posters) | Most-played shows, 7 / 30 days | Most-played movies, 7 / 30 days |
| 4 (auto) | Bottom row (full width) | **Genres**: six tiles with counts | **Collections**: five cards (§6) |
| Header | Buttons | **Sort** · Browse all shows | **Sort** · Browse all movies |

### Local shelf rules (from Plex data)

| Shelf | Plex fields |
|---|---|
| Finish a season | season `leafCount − viewedLeafCount ≤ 3` and `> 0` |
| Continue watching | `viewOffset > 0` on movies |
| Added this week | `addedAt` ≥ start of the week |
| Watch again | `viewCount > 1` (movies); `viewedLeafCount == leafCount` (shows) |
| Genres | `Genre` tags per section |

### Sort

- **Sort** button left of Browse all, showing the current order; menu with 60 px rows:

| Option | Plex `sort` |
|---|---|
| A–Z | `titleSort` |
| Z–A | `titleSort:desc` |
| **Recently added** (default) | `addedAt:desc` |

- Applies to Added this week, Watch again and Browse all only. Use `titleSort` so "The …" sorts like Plex. Remembered per Movies / TV in `config.toml`.

### Watching now

- Header: green live dot + **WATCHING NOW**; right "3 streams · all sections".
- 84 px rows (three fit; scroll for more): poster 44 × 66, title + "S2 · E7" or "Movie"; device line ("Living room TV · tvdash (mpv)") with chips **This TV** · **Remote** · **Direct play** / **Transcode 720p**; right side **Playing** / **Paused** + time left; 5 px progress bar.
- Tap → Show page or movie detail. No remote control of other devices.
- Empty: dashed box, "Nothing is streaming from Republic City"; live dot grey.
- Data: `GET /status/sessions`, every **10 s while the Library is open**.

| Shown | Plex field |
|---|---|
| Device · app | `Player.title` · `Player.product` |
| Playing / Paused | `Player.state` (`buffering` → Playing) |
| Remote | `Player.local == false` |
| Transcode | `TranscodeSession.videoDecision == "transcode"` |
| This TV | `Player.machineIdentifier` == tvdash's `X-Plex-Client-Identifier` |

> **M2 check:** tvdash shows up in sessions only if it reports playback (`/:/timeline`) with its client identifier and a device name.

### Popular on Republic City

- Header + **Last 7 days / Last 30 days** switch (default 30, saved).
- Twelve 2:3 posters with rank chips 1–12, title and "23 plays". Ranked only; Sort doesn't apply.
- Data: Tautulli `get_home_stats`, `stat_id=top_tv` / `top_movies`, `stats_type=plays`, `time_range=7|30`, `stats_count=12`. Use `top_*`, not `popular_*` (single account).
- Cache 1 h per (section, range) in `cache.db`. Tautulli down or not set up → hide the row.
- Config: `[tautulli] url`; keyring `tautulli_api_key`; Settings › Accounts gets a Tautulli row.

**Browse all:** full grid with the detail panel; movie genres become a filter here.

---

## 5. Show page

| Area | Contents |
|---|---|
| Header | "‹ Library" + breadcrumb ("TV / Series › Harbor Lights") |
| Left (4 cols, poster tint) | Backdrop · "TV SERIES · PLEX" · title 48 px · year, rating, seasons, genre · summary · "Up next: S1 · E3 — The Breakwater · 14 min left" · **Resume S1 · E3 on TV** (filled, 72 px) · **Play from S1 · E1** · **Shuffle episodes** |
| Right (8 cols) | Season selector, then that season's episodes |

### Season selector

| Seasons | Selector |
|---|---|
| 1–6 | Full tabs: name + progress |
| 7–14 | Compact pills "S1", "S2"… with progress bars; selected season's name and progress on the right |
| 15+ | "Season 12 ▾" button opening a six-across grid of every season |

- Opens on the up-next season. **Specials** last, labelled "Specials" / "Sp", never S0.

### Episode rows (~120 px)

16:9 still (176 × 99) with watched tick / progress · "03  The Breakwater" · runtime, "N min left" or "watched" · one-line summary · **Play**, or filled **Resume** (row highlighted).

| Need | Plex |
|---|---|
| Seasons | `/library/metadata/{show}/children` |
| Episodes | `/library/metadata/{season}/children` |
| Up next | `/library/metadata/{show}/onDeck` |

---

## 6. Collections (Movies)

### Collections row (Library bottom row, Movies side)

- "COLLECTIONS" · "23 collections"; **five cards** across, 128 px: poster 69 × 104, name, "8 movies · 3 watched" ("all watched", "· smart"), progress bar; card tinted from the collection poster.
- Order: something left to watch first, then most recently added movie. More than five: scroll sideways.

### Collection page

| Area | Contents |
|---|---|
| Header | "‹ Library" + "Movies › Collections › Studio Ember" |
| Left (4 cols, tint) | Backdrop · "COLLECTION · PLEX" · name 48 px · "8 movies · 1998–2024 · 15 h 40 m in total" · progress "3 of 8 watched" · summary · Up next · **Resume / Play next on TV** (72 px) · **Shuffle an unwatched one** |
| Right (8 cols) | Order buttons (**Collection order** default · Release date · A–Z) · **Hide watched** switch · 4-across movie grid (poster 120 × 180, "#4" or year, title, status) |

**Up next:** a started movie → Resume; else first unwatched in collection order → Play; all watched → Play from the start.

| Need | Plex |
|---|---|
| Collections | `/library/sections/{movies}/collections` (`childCount`, `smart`, `collectionSort`) |
| Movies in one | `/library/collections/{ratingKey}/children` |

---

## 7. Screensaver 3 — Sentence clock

- **Manual only:** the header's screensaver button. Starting it closes the search panel.
- **Wake:** any tap or key; the waking tap is swallowed.
- **Always dark** (`#07090B`), whatever the theme. Text block drifts a few px each minute (burn-in).
- While the TV plays, tvdash inhibits GNOME's screen blanking (portal idle inhibit).

Layout, left-aligned at x = 180 px, vertically centred:

1. Sentences, 92 px weight 300, connective words `#8A9099`, values `#E8EAED` weight 400:
   - "It's **12:32** on Thursday."
   - "Outside, **51°** and partly sunny."
2. On the TV block, 64 px below:

| State | Label (22 px caps, accent dot) | Title (52 px) | Line (26 px) | Extra |
|---|---|---|---|---|
| Playing | ON THE TV · PLAYING · PLEX | Title | Episode line | 760 px progress bar, "24:10 / 52:00", "Ends at 13:00" |
| Paused | ON THE TV · PAUSED · PLEX | Same | Same | Same bar, still |
| Nothing playing | ON THE TV · NOTHING PLAYING | "Up next: *title*" | Episode + time left | None |

- Accent from the poster tint. Time sentence crossfades each minute; progress updates every 10 s.

> The Leaf feature later adds a third sentence and a milestone track (`tvdash-leaf.md` §6).

---

## 8. Touch rules

| Rule | Value |
|---|---|
| Minimum target | 56 px (transport 76 px) |
| Hover | Never required |
| Scrolling | Touch momentum on Continue Watching, the Library page, shelves, episode lists |
| Feedback | Pressed state on every button |
| Accessibility | Every icon-only control and logo tile has an accessible name |

---

## 9. Done when

- [ ] Header, three equal boxes and every control fit at the laptop's real resolution and scaling, in Dark, Stone and Mist.
- [ ] On the TV box tints from the poster; transport buttons only while playing; Up next state otherwise.
- [ ] Ten logo tiles open the right sites full screen on the TV.
- [ ] Continue Watching switches TV / Movies with correct badges.
- [ ] Search panel slides in empty, groups results, credits JustWatch, closes cleanly.
- [ ] Library: Watching now lists live streams within 10 s and marks **This TV**; shelves work for Movies and TV; Sort works; Popular switches 7 / 30 days and hides without Tautulli; Genres (TV) / Collections (Movies) at the bottom.
- [ ] Show page: right season selector for 3, 9 and 20 seasons; Resume plays from `viewOffset`.
- [ ] Collection page: up-next rule, three orders, Hide watched.
- [ ] Screensaver 3 starts manually, shows all three On the TV states, wakes without side effects, drifts.
- [ ] Contrast unit test passes for every hue in every theme.
- [ ] No Jellyfin code paths remain.

---

## 10. Change log

| Date | Change |
|---|---|
| 2026-11-11 | Option B poster tint, Dark / Stone / Mist themes, touch rules |
| 2026-11-12 | Plex only; three equal boxes; fixed 10 apps as logos; Continue Watching switch; search panel; Library C smart shelves + sort; Show page; Collections; Screensaver 3 chosen |
| 2026-11-13 | Watching now replaces Tonight's pick; Popular on Republic City full width |
| 2026-11-14 | Library A/B and Screensavers 1/2 removed from the canvas; this consolidated doc created; Leaf battery to follow (`tvdash-leaf.md`) |
