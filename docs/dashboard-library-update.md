# tvdash: Dashboard and Library update (2026-11-12)

The changes to the laptop **dashboard** and the **Library (smart shelves)** from the
2026-11-11/12 design session, and how they were built. Builds on the laptop redesign (poster
tint, themes and touch rules are unchanged).

Design canvas: https://claude.ai/artifact/UggZzEChLyVVffZNfdGnTf, page **Laptop redesign**:
boards **Laptop dashboard — Option B**, **Library C — Smart shelves** (chosen), **Library B —
Browser** (the Browse all grid and detail panel), **Screensaver 3 — Sentence clock** (chosen).

**Status: built** on branch `dashboard-library-update` (2026-11-12). Two-screen layout only;
the TV-only layout keeps its own screens but shares the data (Plex only, the ten apps and
their logos).

## Decisions (2026-11-12)

| Question | Decision |
|---|---|
| Which layouts? | The two-screen layout. The TV-only layout keeps its own screens and ambient clock |
| Jellyfin | Removed altogether: client, sign-in, setup step, config section, keyring entry. Old `[jellyfin]` sections are ignored |
| App management | None. A fixed list of ten; `[[streaming]]` can only change an app's addresses. Any other entry is dropped |
| Logos | Found and bundled (Wikimedia Commons SVGs; see `assets/logos/SOURCES.md`). Dark-theme copies have light text |
| Where to watch screen | Replaced by the search panel |
| On-screen keyboard | Test GNOME's first; if it doesn't appear, a built-in keyboard in the panel or typing from the phone remote are both fine |
| Tonight's pick "ends before night-mode time" | Skipped until night mode (v2) exists: any unwatched movie can be picked |
| Up next | The newest Continue Watching item, TV or movie |
| Genres | The six with the most titles |
| Clock | 12-hour by default (with AM/PM), 24-hour still in Settings › Display |
| Laptop resolution | 1920 × 1080; every screen was checked at that size |

---

## 1. What changed

| Area | Before | Now |
|---|---|---|
| Media server | Plex and Jellyfin | **Plex only** |
| Header | Clock + date left; theme, weather, Settings right | **Search** and **Screensaver** left · **time, date, weather** centred · **theme + Settings** right, on a `1fr 2fr 1fr` grid |
| Main area | On the TV (5 cols) + stacked Watch on TV / Continue Watching (7 cols) | **Three equal full-height boxes** (4 cols each) |
| On the TV | Poster left, title and episode beside it | **Title** full width, **details** underneath, then **poster + description** side by side |
| Apps | Any number of services, add/remove, text names | **Fixed list of 10**, **logos only**, 2 × 5 grid |
| Continue Watching | One horizontal row, mixed | **TV / Series · Movies** switch, 3-wide poster grid |
| Search | Spyglass linked to a separate Where to watch screen | **Slide-in panel** over the right two boxes, empty until you type |
| Library | Plain rows with filters and search | **Smart shelves** with a **Movies / TV / Series** switch, and **Browse all** |
| Screensaver | None | **Manual** sentence-clock screensaver; tap anywhere to wake |

## 2. Dashboard layout

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ [🔍][🖵]           9:04 PM          │  ☁ 48°F            [Dark|Stone|Mist] [⚙] │
│              Wednesday, November 11 │  Light rain · H 55 L 39                  │
├──────────────────────────┬──────────────────────────┬────────────────────────┤
│ ON THE TV         Plex   │ WATCH ON TV · 10 APPS    │ CONTINUE WATCHING  See all›│
│ The Long Drive Home      │ ┌─────────┐ ┌─────────┐  │ [ TV / Series | Movies ]│
│ Season 2 · Episode 7 — … │ │  logo   │ │  logo   │  │ [post][post][post]     │
│ 2024 · TV-14 · 52 min    │ └─────────┘ └─────────┘  │ [post][post][post]     │
│ ┌──────┐ Description     │      … 5 rows …          │   (scrolls)            │
│ │poster│ from Plex…      │                          │                        │
│ └──────┘                 │                          │                        │
│ ━━━━━━━━━━━━━━━━━━━━━━━  │                          │                        │
│ [−10][ ❚❚ Pause ][+30][Stop]                        │                        │
└──────────────────────────┴──────────────────────────┴────────────────────────┘
```

- Outer padding 40 px top/bottom, 64 px sides; 24 px gaps. The three boxes are exact thirds.
- **Header**, 140 px: Search and Screensaver (64 px squares; Search fills in while the panel
  is open) · clock 112 px light with AM/PM, the date under it, a thin divider, the weather
  (52 px icon, 48 px temperature, condition and high/low) · theme switch and Settings.

## 3. On the TV box

Top to bottom: label row ("ON THE TV" / "UP NEXT" in the tint accent, Plex badge); title
(44 px); episode line ("Season 2 · Episode 7 — Crossroads"; nothing for movies); facts
(year · rating · runtime · first genre); poster (160 × 240) with the description beside it,
clipped at the bottom (no description: the genres and "No description on the server");
progress bar and times; buttons (−10 s, Pause/Resume, +30 s, Stop, 76 px) only while
playing, or **Resume on TV** for Up next. A streaming app shows its logo and **Close browser**.

The summary, rating and genres come from Plex's full item (`/library/metadata/{id}`) for
what's playing and for the Up next item; an episode's genres and rating come from its show.

## 4. Watch on TV box

| # | App | Start page | Search page (Open on TV from search) |
|---|---|---|---|
| 1 | YouTube TV | `https://tv.youtube.com` | none (start page) |
| 2 | YouTube | `https://www.youtube.com` | `/results?search_query={title}` |
| 3 | Disney+ | `https://www.disneyplus.com` | none (start page) |
| 4 | Hulu | `https://www.hulu.com` | `/search?q={title}` |
| 5 | Apple TV | `https://tv.apple.com` | `/search?term={title}` |
| 6 | Netflix | `https://www.netflix.com` | `/search?q={title}` |
| 7 | Fandango at Home | `https://athome.fandango.com` | none (start page) |
| 8 | Prime Video | `https://www.primevideo.com` | `/search/ref=atv_nb_sr?phrase={title}` |
| 9 | Crunchyroll | `https://www.crunchyroll.com` | `/search?q={title}` |
| 10 | Tubi | `https://tubitv.com` | `/search/{title}` |

> **Check the addresses once in the TV browser profile**; services change their pages. Fix
> one in `config.toml` (`[[streaming]]`, `url` / `search_url`) without a rebuild.

- 2 columns × 5 rows that fill the box; the logo fills about 78% × 68% of a tile
  (`image-fit: contain`); the app name is the tile's accessible label.
- Logos are bundled SVGs in `assets/logos/`: `<id>.svg` for Stone and Mist, `<id>-dark.svg`
  for Dark and the TV layout.
- Search counts these as "your apps" by TMDB provider ID (fixed in the code), plus TMDB
  providers whose name matches exactly (e.g. YouTube TV, which has no fixed ID).

## 5. Continue Watching box

"CONTINUE WATCHING" and "See all ›" (opens the Library); a **TV / Series · Movies** switch
(56 px; TV / Series first); 2:3 posters three to a row, scrolling; a badge with the episode
("S1 · E3") or the time left ("22 min left"); a 6 px progress bar in the poster's own colour;
the title on one line.

## 6. Search panel

The Search button slides a panel (opacity + 48 px, about 250 ms) over Watch on TV and
Continue Watching; On the TV stays visible. It opens with only the search field (72 px,
focused) and a close button. Typing searches after a 300 ms pause:

| Group | Button |
|---|---|
| In your library | **Resume on TV** / **Play on TV** (filled). A show resumes its episode in progress, else plays its next one |
| On your apps | **Open on TV** (the app's search page with the title, else its start page) |
| Costs extra | none |
| Not streaming | none |

Footer: "Streaming availability in the US from JustWatch, via TMDB". ✕ or the Search button
closes it and clears the query; playing something leaves it open.

Needs the TMDB token (Settings › Accounts); without it the panel says so.

> **Still to test on the laptop:** whether GNOME's on-screen keyboard appears for the field
> on touch.

## 7. Library: smart shelves

Opened from "See all ›" (and re-read from Plex when it opens, unless read in the last
minute). A **Movies / TV / Series** switch changes every section:

| Section | TV / Series | Movies |
|---|---|---|
| **Tonight's pick** (5 cols) | A show you haven't started; **Start S1 · E1 on TV**; **Shuffle** | A random unwatched movie; **Play on TV**; **Shuffle** |
| **Big box** (7 cols) | **Finish a season**: seasons with 1–3 episodes left, fewest first, one bar segment per episode | **Continue watching**: movies started, not finished, one bar with the time left |
| **Added this week** (6 cols, 6 posters) | Shows added, or with an episode added, since Monday 00:00 | Movies added since Monday |
| **Watch again** (6 cols, 6 posters) | Shows watched all the way through | Movies played more than once |
| **Genres** | The six biggest, with counts | Same, for movies |
| Header button | Browse all shows | Browse all movies |

| Shelf | Plex fields |
|---|---|
| Tonight's pick | shows `viewedLeafCount == 0`; movies `viewCount == 0` and no `viewOffset` |
| Finish a season | seasons (`type=3`) `leafCount − viewedLeafCount` in 1–3 |
| Continue watching | `viewOffset > 0` on movies |
| Added this week | `addedAt` ≥ Monday; newest 100 episodes per show section (`type=4`, `sort=addedAt:desc`) |
| Watch again | `viewCount > 1` (movies); `viewedLeafCount == leafCount` (shows) |
| Genres | `Genre` tags, counted per section |

**Browse all** (also a shelf's count, a genre tile, or a shelf poster): every movie or show,
or that shelf or genre, newest first, in a poster grid with **Unwatched only**. Picking one
opens a detail panel: poster, title, year · runtime · genre, watched count or episodes
watched, description, **Play / Resume on TV** and **Play from the start**.

The whole library (movies, shows, seasons, newest episodes, with TMDB IDs) is read at
start, after sign-in, every 30 minutes and when the Library opens. Posters are fetched only
for what's on screen (the first 120 in Browse), six at a time.

## 8. Screensaver: sentence clock

- **Manual only**, from the header button next to Search; it closes the search panel.
- **Wake:** any tap or key; the waking tap is swallowed (tested).
- **Always dark** (`#07090B`). The text block drifts a few pixels each minute.
- **Screen blanking:** while a video plays (not paused) or a streaming app is open, tvdash
  holds an idle inhibitor from the desktop portal (`org.freedesktop.portal.Inhibit`, idle
  flag); otherwise GNOME's normal screen-off applies. If the portal refuses, the log says
  so once.

Left-aligned at x = 180 px, vertically centred: "It's **9:04 PM** on Thursday." and
"Outside, **51°** and partly sunny." (92 px, key values bright), then the status 64 px below:

| State | Label | Title | Line | Extra |
|---|---|---|---|---|
| Playing / Paused | ON THE TV · PLAYING (PAUSED) · PLEX | Title | Episode line (movies: year · rating · runtime) | 760 px bar, "24:10 / 52:00", "Ends at 1:00 PM" |
| Streaming | ON THE TV · STREAMING | App name | "Full screen on the TV" | none |
| Nothing playing | ON THE TV · NOTHING PLAYING | "Up next: *title*" (or "Nothing playing") | Season, episode and time left | none |

The accent comes from the poster tint (dark theme). The time crossfades when it changes.

## 9. Done when

- [x] Header: search left, time/date/weather centred, theme + Settings right; nothing
  bunched or cut off at 1920 × 1080 (snapshot checked).
- [x] Three equal full-height boxes on the dashboard.
- [x] On the TV box: title, details, poster + description (or the fallback); transport
  buttons only while playing.
- [x] Ten app logos with accessible names. *Opening the right site on the TV still needs a
  check on the TV machine.*
- [x] Continue Watching switches between TV and Movies, with episode or time-left badges.
- [x] Search panel slides in empty, groups results, and closes cleanly.
- [x] Library shelves work for both Movies and TV; "Browse all" opens the full grid.
- [x] Sentence-clock screensaver starts from its header button, shows all three states,
  wakes on any tap without triggering anything else, and drifts.
- [x] No Jellyfin code paths remain.

## Not shown any more

- "Also playing" (other devices' Plex sessions) isn't on the On the TV box; sessions are
  still polled.
- The Library's own search, its source filter, and the separate Where to watch page.
