# tvdash — Home + Apps page build spec (2026-11-21)

Rebuilds the Blades/Glass **Home** page and adds a new top-level **Apps** page. This is the **how**; the agreed decisions are summarized in `tvdash-home-redesign-2026-11-21.md`.

- Concept (Design canvas): https://claude.ai/artifact/CjwfbmpFb3RTCTmjFw7Luw — `#artboard-Main` (Home), `#artboard-Apps` (Apps). Art, logos, show titles and scores there are **placeholders / sample data**; real logos come from `assets/logos/`.
- **Supersedes** `tvdash-blades-glass-build-spec.md` §5.1 (Home) and the tab order in §3. Everything else in that spec (tokens, components, focus ring, Glass materials) still applies.
- Sizes are px at **1920×1080**; scale with `TvUi.u` like every other page.
- **Applies to Classic too (§14).** Content, links and tiles are **close to 1:1** across Classic, Blades and Glass: every dashboard UI binds the same `HomeState` / `AppsPage` globals, fed by the same Rust (`whats_on`, `up_next`, `next_game`, `apps`). Only the layout differs, to fit each UI.
- Colors are given as **token names** (`panel`, `raised`, `raised2`, `text`, `sub`, `accent`, `live`, `track` …) so Dark, Stone, Mist, Kansas Dark and Glass all work without per-theme code here.

---

## 0. For the coding agent: find these first

This spec was written without the repo in view. **Before changing code, answer each row in the repo, write the answers into this table, and commit the table with the first change.** If an answer is "doesn't exist", stop and report back (see 0.2).

### 0.1 Findings

| # | Question | Where to look | Answer |
|---|---|---|---|
| H1 | Where the Home hero ("On the TV") state is computed and pushed to Slint: function, struct, `Dash` properties | `rg -n -i "on the tv\|currently playing\|last played" crates/` | `crates/app/src/bridge/home.rs`: `refresh_tv_card(s, laptop)` & `tv_card(s) -> (TvCard, Hue)` compute hero state and set `Dash.tv` (`TvCard`), `Dash.tint` (`Tint`), and `Dash.tv_ends_at` (`crates/app/ui/state.slint`). Last Played uses `TvUi.last-played-*` (`ui/tv/home.slint`, `ui/v3/home.slint`). |
| H2 | Where the **transport bar** lives (Slint component + Rust callbacks) and which callbacks are shared with the Player HUD | `rg -n -i "transport" crates/app/ui crates/app/src` | `crates/app/ui/v3/home.slint` (lines 530–730) and `crates/app/ui/tv/shell.slint` (`MiniTransportBtn`, lines 211–280). Both invoke `Dash.transport(string)` (`ui/state.slint:133`, wired in `callbacks.rs:248`), shared with `ui/tv/player_hud.slint` and `ui/v3/player_hud.slint`. |
| H3 | Source of Library's **Continue Watching** shelf (Plex call, sort, limit) and whether Plex **On Deck** (`/library/onDeck` or `/hubs/home/onDeck`) is already called anywhere | `rg -n -i "continue\|ondeck\|viewOffset" crates/` | `PlexClient::continue_watching` (`crates/media/src/plex.rs:690`) calls `/hubs/continueWatching/items` (`X-Plex-Container-Size=30`, fallback `/library/onDeck`). `PlexClient::on_deck` (`/library/metadata/{show_key}/onDeck`) is at `plex.rs:316`. Jellyfin uses `/UserItems/Resume` (`jellyfin.rs:188`). |
| H4 | `recent.rs` API: `push_recent`, `home_three`, storage file, how many entries it keeps | `crates/*/src/recent.rs` | `crates/app/src/recent.rs`: `push_recent(list, id, now)` dedupes, inserts at front, **caps at 3** (`list.truncate(3)`). `home_three(recent, fixed)` returns up to 3. Persisted in `AppState::recent_services` (`state.json`). |
| H5 | Current `[[streaming]]` entries in `config.toml` and `assets/logos/`. Expected 12: Apple TV, Crunchyroll, Disney+, Fandango at Home, Google Meet, Hulu, Netflix, Prime Video, Tubi, YouTube, YouTube Music, YouTube TV. List any missing logo | `config.toml`, `assets/logos/` | `default_streaming_sites()` (`crates/core/src/config.rs:609–632`) has all 12 services. `assets/logos/` has all 12 SVGs (`apple-tv.svg` … `youtube-tv.svg`). **None missing.** |
| H6 | Where the **tab order** and strip layout are defined (enum + blades layout code, `tile_rects()` in `tv_nav.rs`) | `rg -n -i "enum .*Tab\|blades" crates/app/src` | `Tab` enum in `crates/app/src/shell_tv.rs:6` (`Home=0, Library=1, Collections=2, Sports=3, Settings=4`). Strip layout in `crates/app/ui/v3/shell.slint` (`BladeStrip`). Rects in `crates/app/src/tv_nav.rs` (`blades_tile_rects` line 251, Classic `tile_rects` line 566). |
| H7 | Theme list and where **Kansas Dark** tokens live; what the Theme quick action cycles through today | `rg -n -i "kansas\|enum Mode" crates/` | `ThemeMode` (`crates/core/src/config.rs:146`): `FollowSystem, Dark, Aubergine, Ku, KuLight, Stone, Mist`. Tokens in `ui/v3/theme.slint`, `ui/theme.slint`, `tint.rs`. Quick action cycles `Dark → Aubergine → Ku → KuLight → Stone → Mist → Dark` (`tv_nav.rs:1090, 1362`). |
| H8 | `dbo.SportsScoreboard`: what `StatusText` holds **for live games** (e.g. `"3rd - 4:12"`, `"Halftime"`, or only `"In Progress"`). Run `SELECT TOP 20 Sport, StatusText FROM dbo.SportsScoreboard WHERE Status = 'live'` during a live window | Sports SQL | `crates/events/src/scoreboard.rs:90, 728–742`: queries `g.Status, g.StatusText, g.Period, g.DisplayClock`. Live header formats `format_period(g.Period) · g.DisplayClock` (e.g. `3rd · 4:12`), falling back to `g.StatusText`, else `"Live"`. |
| H9 | Where the Scoreboard gets **team ranks** (`#2 Utah`) today: `SportsRankingTeams`, a column, or part of the name | `rg -n -i "rank" crates/` + Sports SQL | `crates/events/src/scoreboard.rs:92, 357–358, 833–840`: `dbo.SportsScoreboard` has `g.HomeRank` and `g.AwayRank` columns directly (`Option<u8>`, filtered to `1..=25`). |
| H10 | Where the Scoreboard gets **period scores** today (the screenshot shows them, but `tvdash-sports-alerts.md` F7 lists no such column) | same | `crates/events/src/scoreboard.rs:701–724`: parsed from `LiveDetailsJson` (`awayPeriods` / `homePeriods` arrays with `displayValue` / `score`). |

> **Hint for H8–H10 (2026-11-21):** the **Classic** Scoreboard already shows the period and clock (`3rd · 3:38`), ranks (`#2 Utah Utes`), a "Leading" label and period scores. Find the code Classic uses for these and reuse it for Blades and the Next Game card rather than writing a new query. The data exists, even though `tvdash-sports-alerts.md` F7 doesn't list those columns.

### 0.2 Stop conditions

- **H8:** if live `StatusText` is only a generic word, the Next Game card shows `LIVE` with no period/clock. Don't scrape ESPN for it; report back.
- **H9:** if ranks aren't available, Next Game skips the "ranked" steps (§5.2 steps 1 and 3) instead of guessing.
- **H5:** a missing logo falls back to the brand-color tile with the app name (§6.3). Don't fetch logos at runtime.

---

## 1. Tab order and shell (changes blades-glass §3)

- **New fixed tab order:** home · **apps** · library · collections · sports · settings.
- Every page now has 5 strips instead of 4, so the panel is **68px narrower** everywhere (one more 60px strip + 8px gap). Check Library (8-column Browse all) and Scoreboard still fit; they should.
- Strip colors stay as §3: the 1–2 nearest each side `bladeNear`, the rest `bladeFar`.

| Page | Left strips | Right strips |
|---|---|---|
| Home | — | apps, library (near) · collections, sports, settings (far) |
| Apps | home (near) | library, collections (near) · sports, settings (far) |
| Library | home (far), apps (near) | collections, sports (near) · settings (far) |
| Collections | home, apps (far), library (near) | sports, settings (near) |
| Sports | home, apps, library (far), collections (near) | settings (near) |
| Settings | home, apps, library (far), collections, sports (near) | — |

Sub-pages (Show, Collection detail, Search) keep their parent tab's strips as before.

---

## 2. Home layout

```
safe area 96 / 54 → panel (radius 28, padding 32 top · 44 sides · 28 bottom) + 5 right strips
┌─────────────────────────────────────────────┬──────────────────────────┐
│ Home (h1)                                   │                          │
├─────────────────────────────────────────────┼──────────────────────────┤
│                                             │ Clock card               │
│ WHAT'S ON hero        783 × 440             ├──────────────────────────┤
│                                             │ Next Game card           │
├─────────────────────────────────────────────┤                          │
│ Up next  (label + 4 cards)                  ├──────────────────────────┤
│                                             │ Quick actions (4 rows)   │
├─────────────────────────────────────────────┴──────────────────────────┤
│ Apps  (label + 8 tiles)                                                │
└────────────────────────────────────────────────────────────────────────┘
```

| Block | Size (1920×1080) |
|---|---|
| Panel | ~1388 × 972 outside; ~1300 × 912 inside the padding |
| Header | `h1` "Home" 42 / 900, row height 56; gap 20 below. **No** status (clock/weather) on the right: the Clock card already shows it |
| Body | grid `1.6fr \| 1fr`, gap 28 → left ≈ 783, right ≈ 489. Height ≈ 682 |
| Left column | stack, gap 20: hero (440) + Up next (≈ 222) |
| Right column | stack, gap 16: Clock card (auto ≈ 196) · Next Game (auto ≈ 190) · Quick actions (fills the rest ≈ 264) |
| Apps section | gap 20 above it; label 22 / 900 + gap 10 + tiles 84 tall, full panel width |

The transport bar is **removed** (§8). There is no bottom bar any more, so the panel's bottom padding is 28.

---

## 3. What's On hero

### 3.1 Look

| Part | Spec |
|---|---|
| Box | 783 × 440 (≈ 16:9), radius 22, focused by default (focus ring 4px `accent`, offset 5) |
| Art | Backdrop `art` (poster fallback), `image-fit: cover` |
| Scrim | Bottom gradient: transparent at 0% → black 35% at **30%** → black 88% at 100%. The kicker must always sit on the scrim, never raw art |
| Play button | 84px circle top-right (28 inset), `accent` fill, white play icon. Decorative: OK on the hero does the action |
| Text block | bottom-left, 36 inset, gap 8, max width 760 |
| Kicker | 17 / 900, uppercase, letter-spacing 2: **`WHAT'S ON`** white 92% + **` · <SOURCE>`** white 62% (e.g. `WHAT'S ON · PLEX`, `WHAT'S ON · NETFLIX`) |
| Title | 52 / 900, letter-spacing −1, one line, elide |
| Subtitle | 22 / 700, white 85%. Movie: `2016 · Action, Fantasy · 1 h 43 m` (year · first two genres · runtime). Episode: `S2 · E11 · Episode title` |
| Plot | 19 / 400, line-height 1.45, white 85%, **max 2 lines**, elide |
| Progress | 6px, radius 3, track `rgba(255,255,255,.28)`, fill white. Hidden when there's no position |
| Status | 20 / 800 white (§3.3) |

### 3.2 Which item the hero shows

Pick the first that applies:

| # | Situation | Source label | Item |
|---|---|---|---|
| 1 | A Plex/Jellyfin item is loaded in mpv (playing or paused, minimized to Home) | `PLEX` / `JELLYFIN` | That item |
| 2 | A streaming app is open in Brave (Home visible over it, e.g. after F12) | App name uppercased: `NETFLIX` | App: title = app name, subtitle `Open on the TV`, no plot, no progress, art = brand-color gradient + logo |
| 3 | The last item played in tvdash is unfinished (`viewOffset > 0`, not watched) | `PLEX` | That item |
| 4 | Otherwise | `PLEX` | The **Up Next** item (§4.2, first card) |
| 5 | Nothing at all (new library, Plex down) | — | Empty hero: kicker `WHAT'S ON`, title "Nothing on right now", subtitle "Search or open the Library", OK opens Search |

> Rule 4 means the Up Next row then starts from its **second** candidate, so nothing appears twice (§4.2).

### 3.3 Status line

| State | Text |
|---|---|
| Playing in mpv | `Ends at 12:44 AM` (now + remaining, clock format from Settings) |
| Paused in mpv | `Paused at 3:01 · 1 h 40 min left` |
| Stopped / unfinished | `Stopped at 3:01 · 1 h 40 min left` |
| Not started (rule 4 with no position) | `Not started · 1 h 43 m` |
| App open (rule 2) | `Open on the TV` |

Times: `m:ss` under an hour, `h:mm:ss` from an hour. "Left": `37 min left`, `1 h 40 min left`.

### 3.4 OK on the hero

| Situation | OK does |
|---|---|
| mpv loaded | Bring the player full screen (HUD visible) |
| App open | Return to the app in Brave (same as its Apps tile) |
| Unfinished / Up Next item | Resume / play it in mpv |
| Empty | Open Search |

---

## 4. Up next row (replaces the Streaming grid)

### 4.1 Look

| Part | Spec |
|---|---|
| Label | `Up next` 22 / 900, gap 12 below |
| Row | grid `1.5fr 1fr 1fr 1fr`, gap 14 → ≈ 247 · 165 · 165 · 165 wide |
| Art | Fixed height **124**, radius 14, thumbnail with `image-fit: cover` (16:9 thumbs crop slightly on the smaller cards) |
| Progress | 5px, radius 3, inset 10 from the art's left/right/bottom, track `rgba(255,255,255,.3)`, white fill. Hidden at 0 |
| UP NEXT badge | First card only: top-left 10 inset, 13 / 900, letter-spacing 1.5, padding 4×9, radius 8, `accent` / `accentText` |
| Title | 19 / 900, gap 8 under art, one line, elide (episode cards show the **show** name) |
| Meta | 15 / 700 `sub`: episode `S2 · E4 · 21 min left` or `S2 · E5` when unstarted; movie `1 h 46 m left` or `2019 · 1 h 58 m` |
| Empty slots | Fewer than 4 items → fewer cards, left-aligned. 0 items → hide the whole row (label too) and let the hero grow to fill (still 16:9 max: center it) |

### 4.2 What goes in it

Build one ordered candidate list, drop the hero item, take the first 4:

1. **Next episode of the hero item** if the hero is an episode: the next leaf after it (same season, else first episode of the next season). Skip if already watched.
2. **Plex On Deck** (next unwatched episode of shows you're in the middle of), newest `lastViewedAt` first.
3. **Continue Watching** (partly watched movies and episodes, `viewOffset > 0`), `lastViewedAt` desc. Same source as Library's shelf (H3).
4. If still short: **Recently added** (newest `addedAt`).

Rules:
- De-duplicate by `ratingKey`, and keep **one card per show** (the first one wins).
- Never include the hero item.
- Refresh when Home becomes visible, when mpv stops, and every 5 min while Home is visible. Cache the last list so Home paints instantly.
- OK on a card: resume from `viewOffset` (or play from start), in mpv, full screen.

---

## 5. Next Game card (replaces Last Played)

### 5.1 Look

| Part | Spec |
|---|---|
| Box | `raised`, 1px `divider` border, radius 22, padding 18×22, gap 12. Focusable (ring 4px `accent`) |
| Row 1 | Left: `NEXT GAME · <LEAGUE>` 15 / 900, letter-spacing 2, `sub` (league display name uppercased: `COLLEGE FOOTBALL`). Right: state (below) 16 / 900 |
| State: live | 10px `live` dot with 4px halo (`live` 25%) + `LIVE · 3rd · 4:12` in `live` (from `StatusText`, H8; just `LIVE` if it's generic) |
| State: upcoming | `TONIGHT · 7:00 PM` / `TODAY · 1:00 PM` / `TOMORROW · 11:00 AM` in `accentInk` (`StartsAtUtc` in local time; `Time TBD` if `TimeTbd`) |
| State: final | `FINAL` or `StatusText` uppercased (`FINAL/OT`), `sub` |
| Team rows (away, then home) | gap 8. Rank 34 wide 15 / 900 `sub` (`#2`, blank if unranked); team badge 32×32 radius 9 (same badge as Scoreboard: logo, else initials on team color); name 22; score 30 tabular, right-aligned. **Leader/winner: 900 `text`; other: 700 `sub`.** Tied or upcoming: both 800 `text`. Upcoming shows no score |
| Footer | 1px `divider` top, padding-top 10, 16 / 700 `sub`: `Venue · Broadcasts` (skip empty parts) |
| OK (or click/tap) | Open Sports › Scoreboard on that game's date, scrolled to and focused on that game. Always reset the Scoreboard's sport/league filter to **All** first, so the game is never filtered out |

### 5.2 Which game

Query `dbo.SportsScoreboard` (same connection as the Scoreboard, F1/F10 in the alerts spec) for rows from `now − 2 h` to end of tomorrow (local). Pick the first match:

| Step | Rule |
|---|---|
| 0 | *(Once follows/stars work)* a game matching `sports_alerts::reason()` (followed team or starred game), using steps 1–5 within those games only |
| 1 | `live` with a ranked team; lowest rank number wins (`#2` beats `#17`). Skip if no ranks (H9) |
| 2 | Any `live`; earliest start first |
| 3 | `upcoming` **today** with a ranked team; earliest start, then best rank. Skip if no ranks |
| 4 | Next `upcoming` (today or tomorrow); earliest start |
| 5 | `final` that started within the last **5 h** (no "went final at" column exists, so use start time as a proxy); latest start first |
| 6 | None → **hide the card**; Quick actions grow into the space |

- Ignore `postponed`, `cancelled`, `suspended`, `delayed`, `unknown`.
- Refresh every **2 min** while Home is visible and a live game is shown, else every **10 min**. If the Scoreboard already fetched these rows within that interval, reuse them.
- DB unreachable → keep the last card for up to 30 min, then hide it.
- Pure function `pick_next_game(rows, ranks, follows, starred, now) -> Option<Game>` in `crates/app/src/next_game.rs`, unit-tested (§10).

---

## 6. Right column: clock and quick actions

### 6.1 Clock card

| Part | Spec |
|---|---|
| Box | `raised`, 1px `divider` border, radius 22, padding 22×26, gap 6 |
| Time | 84 / 900 tabular + AM/PM 30 / 800 `sub` (none in 24h mode) |
| Date | 22 / 700 `sub`: `Saturday, November 21, 2026` |
| Weather row | 21 / 800: condition icon + `61°F · drizzle`; right-aligned **`H 68 L 53`** in `sub`. **Remove the second condition word** on the right |

Not focusable.

### 6.2 Quick actions

- Card: `raised`, 1px `divider` border, radius 22, padding 10×14; **4 equal rows**, each focusable (row ring 3px `accent`, offset 2).
- Row: 40px icon box (`raised2`, radius 12, 22px stroke icon) · label 22 / 800 · right hint 19 / 700 `sub`.

| Row | Hint | OK |
|---|---|---|
| Search | — | Search sub-page |
| Screensaver | — | Ambient screensaver |
| Sleep timer | `Off` / `30 min` / `Ends 12:30 AM` (existing) | Sleep picker (existing) |
| Theme | current theme name (`Kansas Dark`) | cycle to the next theme (H7 list) |

**Removed:** Library (it's the next-but-one strip) and Settings (already gone).

---

## 7. Apps bar (bottom of Home)

| Part | Spec |
|---|---|
| Label | `Apps` 22 / 900, gap 10 below |
| Row | grid `repeat(8, 1fr)`, gap 12, full panel width → tiles ≈ 152 × 84. **Never wraps, never scrolls** |
| Tile | radius 16, brand color fill, logo from `assets/logos/` centered with `image-fit: contain` inside 70% × 60% of the tile. `accessible-label` = app name. Missing logo → app name 21 / 900 in the brand's text color |
| Tiles 1–7 | Apps by **most recently used** (`recent.rs`), then the remaining `[[streaming]]` apps in config order to fill 7 |
| Tile 8 | **All apps ›**: `raised2` fill, 2px `divider` border, label 21 / 900 `text`. Opens the Apps page |
| RECENT badges | **Removed** (the bar is already in recency order) |
| OK on an app | Launch it exactly as today (Brave full screen on the TV) and call `push_recent` |

```rust
// crates/app/src/apps.rs
/// First `n` app ids for the Home bar: recent first, then config order, no duplicates.
pub fn apps_bar(recent: &[String], all: &[String], n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(n);
    for id in recent.iter().chain(all.iter()) {
        if out.len() == n { break; }
        if all.contains(id) && !out.contains(id) { out.push(id.clone()); }
    }
    out
}
```

- `recent.rs` must keep at least 7 entries (H4). If it keeps fewer, raise its cap; the old `home_three` can be removed once nothing uses it.
- Recent apps that are no longer in `[[streaming]]` are skipped (the `all.contains` check).

---

## 8. Removals and moves

| Item | Change |
|---|---|
| Transport bar | **Delete** from Home (Slint component and its Home-only bindings, H2). Keep any mpv command callbacks the Player HUD also uses |
| Last Played card | Replaced by Next Game |
| Streaming 5×2 grid | Replaced by Up next row + Apps bar |
| Library quick action | Removed |
| Home header status (weather + clock at top right) | Removed (Clock card shows it) |
| `RECENT` badge component | Unused on Home; delete if nothing else uses it |

---

## 9. Apps page (new tab)

Matches `#artboard-Apps`.

| Part | Spec |
|---|---|
| Strips | Left: home (near). Right: library, collections (near) · sports, settings (far) |
| Header | `h1` "Apps" 42 / 900 + count 22 / 700 `sub` (`12 apps`) + status on the right (weather + clock, as on other tabs). **No sort, no filter** |
| Grid | 4 columns, gap 24 rows × 22 columns, ≈ 308 wide each |
| Tile | 16:9 (≈ 308 × 173), radius 20, brand color + logo (`contain`, 70% × 55%), focus ring 4px `accent` offset 5 |
| Name | 20 / 800 `text` under the tile, gap 10 |
| Order | **Always A–Z by display name** (case-insensitive). No recency labels |
| More than 12 apps | Grid scrolls vertically row by row under the standard bottom fade |
| Default focus | First tile |
| OK | Launch the app (same as the Apps bar) + `push_recent` |
| Back | Home |

State: `global AppsPage { in property <[AppTile]> apps; }` with `AppTile { id, name, logo: image, has-logo: bool, color: color, text-color: color }`. Reuse `AppTile` for the Home bar.

---

## 10. State (Slint) and Rust

Names are proposals; fit them to what H1 finds.

```slint
// crates/app/ui/state.slint
export struct WhatsOn {
    has-item: bool, source: string,           // "PLEX", "NETFLIX"
    title: string, subtitle: string, plot: string,
    art: image, progress: float,              // 0..1, <0 = hide
    status: string, is-app: bool,
}
export struct UpNextCard { key: string, title: string, meta: string, thumb: image, progress: float, up-next: bool }
export struct GameSide { rank: string, init: string, logo: image, has-logo: bool, color: color, name: string, score: string, lead: bool }
export enum GameState { live, upcoming, final }
export struct NextGame { visible: bool, event-id: string, date: string, league: string, state: GameState, state-text: string, away: GameSide, home: GameSide, meta: string }
export struct AppTile { id: string, name: string, logo: image, has-logo: bool, color: color, text-color: color }

export global HomeState {
    in property <WhatsOn> whats-on;
    in property <[UpNextCard]> up-next;
    in property <NextGame> next-game;
    in property <[AppTile]> apps-bar;
    callback activate-hero();
    callback activate-up-next(string);        // key
    callback open-game(string, string);       // event-id, date
    callback launch-app(string);              // id
    callback open-apps-page();
}
export global AppsPage { in property <[AppTile]> apps; }
```

```text
crates/app/
├─ src/next_game.rs     # pick_next_game() + tests (§5.2)
├─ src/up_next.rs       # build_up_next() + tests (§4.2)
├─ src/whats_on.rs      # hero item + status line (§3.2–3.3) + tests
├─ src/apps.rs          # apps_bar(), A–Z list for the Apps page (§7, §9)
├─ src/tv_nav.rs        # Home + Apps focus maps (§11), new tab in the strip order (§1)
├─ src/bridge/          # push HomeState / AppsPage
└─ ui/v3/               # Blades/Glass Home, new Apps page; tab order
```

### Unit tests

- [ ] `whats_on`: mpv loaded beats everything; app open beats last-played; finished last item falls through to Up Next; empty → empty hero.
- [ ] Status text: playing → `Ends at …`; paused / stopped wording; `m:ss` vs `h:mm:ss`; `1 h 40 min left`.
- [ ] `build_up_next`: never contains the hero item; one card per show; next episode crosses into the next season; fills from Recently added when short; ≤ 4.
- [ ] `pick_next_game`: live ranked beats live unranked; `#2` beats `#17`; with no ranks, steps 1 and 3 are skipped; today's upcoming ranked beats an earlier unranked one only within step 3; final older than 5 h ignored; postponed/cancelled ignored; nothing → `None`.
- [ ] `apps_bar`: recent first, then config order, no duplicates, unknown recent ids skipped, exactly 7.
- [ ] Apps page order is A–Z case-insensitive (`YouTube` < `YouTube Music` < `YouTube TV`).

---

## 11. Focus (D-pad)

Home (default focus: **hero**):

| From | ← | → | ↑ | ↓ |
|---|---|---|---|---|
| Hero | — | Next Game (or Search if hidden) | — | First Up next card |
| Up next card | previous card | next card; from the last → Quick actions row at the same height (nearest) | Hero | Apps tile below (by x) |
| Next Game | Hero | apps strip | — | Search |
| Quick action row | Hero or nearest Up next card (by y) | apps strip | row above / Next Game | row below; from Theme → All apps tile |
| Apps tile | previous tile | next tile; from All apps → apps strip | Up next card above (by x), or Quick actions for tiles under the right column | — |

Apps page (default focus: **first tile**): arrows move in the grid; ← from column 1 → home strip; → from column 4 → library strip; ↓ past the last row does nothing; Back → Home.

Strips follow blades-glass §6 unchanged.

---

## 12. Milestones

| M | Goal | Done when |
|---|---|---|
| H0 | Findings | §0.1 table filled and committed |
| H1 | Tab order + Apps page | New tab between home and library on every page; Apps grid A–Z with logos; OK launches; focus map |
| H2 | Home layout | New grid; hero restyled (white kicker, scrim, subtitle, 2-line plot, progress, status); transport bar, Last Played, Streaming grid, Library row removed; weather fix |
| H3 | Hero + Up next logic | §3.2–3.4 and §4.2 with tests; never a duplicate title on Home |
| H4 | Apps bar | 7 recent-first tiles + All apps; `recent.rs` cap ≥ 7 |
| H5 | Next Game | §5 with tests; OK lands on the game in Scoreboard; hides cleanly with no games |
| H6 | Classic | §14: Apps top tab + page, Home rearranged onto the same `HomeState`; parity check against Blades |

## 13. Done when

- [ ] The same title never appears twice on Home.
- [ ] Stopping a movie at 3:01 shows `WHAT'S ON · PLEX` with `Stopped at 3:01 · 1 h 40 min left` and a white progress bar; OK resumes at 3:01.
- [ ] The kicker is readable on bright, busy art (scrim reaches it) in every theme.
- [ ] Up next shows the next episode first when an episode is in the hero.
- [ ] Next Game shows a live ranked game with period/clock (or `LIVE` alone if H8 says so), the leader in bold; OK opens the Scoreboard focused on it; with no games the card hides and Quick actions fill the space.
- [ ] The Apps bar shows the 7 most recently used apps, then All apps; opening an app moves it to the front next time.
- [ ] The Apps page lists every app A–Z with its logo; YouTube TV and YouTube are clearly different tiles.
- [ ] Every page shows the apps strip in the right place (§1) and nothing is clipped at 1080p.
- [ ] Works by remote alone in Classic, Blades and Glass.
- [ ] Classic Home and Apps show the same items, in the same order, as Blades at the same moment (same hero, same Up next cards, same Next Game, same 7 apps).

## 14. Classic dashboard UI

Classic keeps its own look (large top tabs, tile grid, Classic tokens and tile sizes). It gets the **same content and behavior** as §3–§9, laid out in its existing grid. No concept artboard yet; this section maps Classic's current Home (as of 2026-11-21) onto the new content.

### 14.1 Top tabs

- **Home · Apps · Library · Collections · Sports · Settings** (Apps added between Home and Library, same style as the others). Same order as the Blades strips (§1).

### 14.2 Home: what moves where

Classic Home today: left info column · center hero image · quick-tile column (Search, Library, Screensaver, Sleep) · right "Streaming services" 2×6 grid · bottom row of Clock, Last played, Theme.

```
┌──────────┬──────────────────────────┬──────────┬──────────────────┐
│ What's On│                          │ Search   │ APPS             │
│ info     │  Hero art                │ Screen-  │ 2 × 4 tiles      │
│ column   │                          │ saver    │ (7 recent +      │
│          │                          │ Sleep    │  All apps ›)     │
│          │                          │ Theme    ├──────────────────┤
├──────────┼──────────────────────────┴──────────┤ NEXT GAME card   │
│ Clock    │ Up next (4 cards)                   │                  │
└──────────┴─────────────────────────────────────┴──────────────────┘
```

| Current Classic | New | Spec |
|---|---|---|
| Left info column: `Plex · CURRENTLY PLAYING` (red), title, year, `Stopped at 3:01`, `1 h 40 min left`, plot | **What's On info column**: kicker `WHAT'S ON · PLEX` (white, Classic kicker size), title, **subtitle** (§3.1), **progress bar**, status line (§3.3, one line), plot (Classic's current length; elide to fit) | §3 |
| Hero image with a `CURRENTLY PLAYING` badge and the title over the art | Hero art **only**: remove the badge and the title overlay (the info column already shows them). Focusable; OK = §3.4 | §3.4 |
| Quick tiles: Search, Library, Screensaver, Sleep | **Search, Screensaver, Sleep, Theme** (Library removed; Theme moves up from the bottom row). Same tile size/style; Sleep and Theme show their hint (`Off`, `Kansas Dark`) | §6.2 |
| Bottom row: Clock card | Clock card stays. Weather: **`59°F · rain`** on one line, **`H 68 L 53`** alone on the next (drop the second "Rain") | §6.1 |
| Bottom row: Last played + Theme tile | **Up next row**: same 4 cards (first one large with UP NEXT badge) across the width under the hero and quick tiles | §4 |
| Right: "STREAMING SERVICES" 2×6 | **APPS** 2×4: 7 apps (recent first) + **All apps ›**. Classic tile style kept: brand color, logo, **name label** at the bottom-left. No RECENT badges | §7 |
| (right column, freed bottom space) | **Next Game card** under the Apps grid, Classic card style, same content and rules | §5 |

- Hero item, status text, Up next list, Next Game pick and app order all come from the **same Rust functions** as Blades. Nothing is computed per UI.
- Focus: hero is the default. Arrow keys move geometrically between blocks (existing Classic `tile_rects()` behavior); Up from the top row goes to the tabs.

### 14.3 Apps tab (Classic)

- Same list as §9: every app, **A–Z**, no sort/filter, OK launches + `push_recent`.
- **Classic tile style** (the same tiles as Home's Apps grid: brand color, logo, name label), laid out in Classic's full width: **5 columns** at 1920, same gap as the Home tiles. Scrolls if needed.
- Header: Classic tab title only (no extra heading); count `12 apps` in `sub` above the grid.
- Default focus: first tile. Back → Home tab.

### 14.4 Done when (Classic)

- [ ] Apps tab sits between Home and Library and opens the A–Z grid.
- [ ] Home shows no duplicate titles; no `CURRENTLY PLAYING` text anywhere.
- [ ] Classic and Blades show the same hero, Up next cards, Next Game and 7 apps at the same moment.

## 15. Change log

| Date | Change |
|---|---|
| 2026-11-21 | Extended to **Classic** (§14): Apps top tab and page, Home rearranged onto the same content (What's On info column, Up next row, Next Game under the Apps grid). Classic scoreboard hint added to §0 |
| 2026-11-21 | Spec written from the Home review: What's On hero, Up next row, Next Game card (ranked fallback while Star is broken), Apps bar + new Apps page (A–Z, no sort), transport bar and Library quick action removed, tab order home · apps · library · collections · sports · settings |
