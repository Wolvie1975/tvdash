# tvdash — Sports Scoreboard build spec (2026-11-22)

Redesigns **Sports › Scoreboard** in **Blades/Glass and Classic** with the same content (box score, live status, rank, leader emphasis, controls, order) laid out for each UI.

- Concept (Design canvas): https://claude.ai/artifact/CjwfbmpFb3RTCTmjFw7Luw — `#artboard-Scoreboard` (Blades) and `#artboard-ClassicScoreboard` (Classic). Teams and scores are **sample data**. Ignore the canvas's "Box score" tweak: the decision is **always**.
- **Supersedes** `tvdash-blades-glass-build-spec.md` §5.10 and the current Classic Scoreboard layout. Tokens, focus ring and shell come from the blades-glass spec and the tab order from `tvdash-home-apps-build.md` §1.
- Related: `tvdash-sports-alerts.md` (DB columns, starred games file, `GameStatus`), `tvdash-home-apps-build.md` §5 (Next Game opens this page focused on a game, with the filter reset to All).
- Sizes are px at **1920×1080**, scaled with `TvUi.u`. Colors are token names.

---

## 0. For the coding agent: find these first

Answer each row from the repo, write the answers here, and commit the table with the first change. The **Classic** Scoreboard already shows period/clock, ranks, a "Leading" label and period scores, so most answers are "reuse what Classic does".

| # | Question | Answer |
|---|---|---|
| S1 | Where Classic gets **period + clock** (`3rd · 3:38`): column, parsed `StatusText`, or something else | _pending_ |
| S2 | Where Classic gets **ranks** (`#2`) | _pending_ |
| S3 | Where Classic gets **period scores** ("Away periods: 10 · 7 · 0"): column name and format (string list? JSON?), and whether OT/shootout periods are included | _pending_ |
| S4 | Whether a **short team name** exists (ESPN `shortDisplayName`/location, e.g. "Utah", "San Diego St"). DB F6 lists only `HomeName`/`AwayName` | _pending_ |
| S5 | What `StatusText` / period data look like **between periods** (the build shows `2nd · 0:00` for an NHL intermission) and at halftime | _pending_ |
| S6 | **Why Star doesn't work**: the click/OK handler, whether it reaches `sports-starred.json` (alerts spec §3.3), whether the UI reads the file back | _pending_ |
| S7 | Where the sport/league **dropdown** options come from, and whether "Top 25" exists | _pending_ |
| S8 | The shared data path: is there one Rust function feeding both Classic and Blades Scoreboards, or two? | _pending_ |

**Stop conditions**

- **S3:** if period scores aren't stored in a parsable form, ship without the box score columns (team rows show only totals) and report back. Don't scrape ESPN from tvdash.
- **S4:** if there's no short name, Blades uses the full name with ellipsis. Report it; a scraper change (store ESPN `shortDisplayName`) is the user's call, not part of this spec.
- **S6:** report the root cause in the table before fixing it.

---

## 1. Decisions

| Topic | Decision |
|---|---|
| Layout | Blades: **3-column grid of cards**. Classic: **2 columns of rows** |
| Live status | Period + clock (`3rd · 3:38`); between periods `End 1st` / `Halftime` / `2nd Int` (§3) |
| Period scores | **Box score, always shown** (live and final), replaces the "Away periods: …" text |
| Leader | Leader/winner **bold** in `text`; the other team in the theme's lighter `sub` color. Tie: both the same. Classic drops the "Leading" label |
| Red | **Only for live**: the live dot and live status. League labels are uppercase `sub` |
| Rank | Own column before the team; blank if unranked |
| W/L record | **Not shown** (no data; no scraper change) |
| Controls | ‹ date › · **Today** · sport/league dropdown · **Refresh** · `Updates ~10m · last 11:41 PM`. **All statuses removed** |
| Star | **Per game** (`EventId`), stored locally (alerts spec §3.3). Fix the bug (S6) |
| Order | **By start time** in every group |
| Data parity | One Rust view model feeds both UIs (§7) |

---

## 2. Page structure (both UIs)

1. Header (UI-specific, §5/§6) with **Competitions | Scoreboard**, Scoreboard selected.
2. **Controls row** (§4).
3. **Groups**, in this order, each hidden when empty:
   - **Live** — heading with the live dot
   - **Upcoming**
   - **Final**
   - **Postponed & cancelled** — compact: no box score, status text only
4. Heading: group name + count in `sub` (`6 games`). One game → `1 game`.
5. Inside every group: **ascending `StartsAtUtc`**; `TimeTbd` games last; ties by league display name, then away team name.
6. The page scrolls group by group under the standard bottom fade. Empty day: one line, `No games on Sat, Nov 21`.

---

## 3. Status text

`GameStatus` from the alerts spec (§5.1). Period/clock come from S1/S5.

| Status | Text | Color |
|---|---|---|
| Live, clock running | `3rd · 3:38`, `OT · 4:12`, `2OT · 1:05`, `SO` | `live` + live dot |
| Live, between periods (clock 0:00 or a break) | Football: `End 1st`, `Halftime`, `End 3rd`. Hockey: `1st Int`, `2nd Int`. Basketball halves: `Halftime`; quarters: `End 1st`, `Halftime`, `End 3rd`. Soccer: `Halftime` | `live` + dot |
| Live, no period data | `LIVE` | `live` + dot |
| Upcoming | `Tonight · 9:30 PM` (≥ 5 PM today), `Today · 1:00 PM`, `Tomorrow · 11:00 AM`, else `Sat · 1:00 PM`; `Time TBD` | `accentInk`-light (on dark themes use a light tint of `accent`, ≥ 4.5:1 on `raised`) |
| Final | `StatusText` as-is: `Final`, `Final/OT`, `Final/SO` | `sub` |
| Postponed / Delayed / Suspended / Cancelled | `Postponed`, `Delayed`, `Suspended`, `Cancelled` | `sub` |

Times are local (America/Chicago from Settings), 12h/24h per Settings.

```rust
// crates/app/src/scoreboard_view.rs
pub fn status_text(g: &Game, now: chrono::DateTime<chrono::Local>) -> (String, StatusTone) { /* table above */ }
pub enum StatusTone { Live, Upcoming, Final, Muted }
```

---

## 4. Controls row

| Control | Behavior |
|---|---|
| ‹ / › | Previous / next day. Range = what the DB holds (1 day back, 7 ahead, alerts F8); disable at the ends |
| Date | Blades `Sat, Nov 21`; Classic `Saturday, November 21, 2026`. Not a button |
| **Today** | Jump to today. Hidden when already on today |
| Dropdown | `All sports` (default), `Top 25` (games with a ranked team, only if S7 confirms ranks), then each league present that day, A–Z by display name. Remembered while tvdash runs; reset to **All** when opened from Home's Next Game |
| **Refresh** | Re-query now; spinner on the button while loading |
| Freshness | `Updates ~10m · last 11:41 PM` in `sub`, `last` = `MAX(LastObservedAtUtc)` of the rows shown (local time). Older than 25 min → `Data is 32 min old` in `warn` |

- **Removed:** the All statuses button.
- Auto-refresh: every **2 min** while viewing today with any live game, otherwise every **10 min**. Keep scroll position and focus across refreshes (match cards by `EventId`).

---

## 5. Blades / Glass

Matches `#artboard-Scoreboard`.

| Part | Spec |
|---|---|
| Strips | Left: home, apps, library (far) · collections (near). Right: settings (near) |
| Header | `h1` "Sports" 42 / 900 + segmented Competitions \| Scoreboard + status (weather · clock) right |
| Controls | Date stepper in a `raised` track (radius 16, padding 4: ‹ 44×44, date 20 / 800, › 44×44); Today, dropdown (min 240 wide), Refresh: height 52, radius 14, `raised`, 1px `divider` border, 19 / 800; freshness right-aligned 18 / 700 `sub` |
| Group heading | 26 / 900 + count 20 / 700 `sub`; Live adds a 14px `live` dot with a 6px halo. Gap 14 to the grid; 12 extra above each later group |
| Grid | `repeat(3, 1fr)`, gap 18 → cards ≈ 421 wide |

### 5.1 Card

```
┌───────────────────────────────────────────┐
│ COLLEGE FOOTBALL               ● 3rd · 3:38│
│                       1   2   3   4     T  │
│ #2  [UTA] Utah       10   7   0        17  │  ← leader: 900 text
│     [TCU] TCU         0   0  14        14  │  ← other: 700 sub
│ ───────────────────────────────────────── │
│ Amon G. Carter Stadium · Fox  [☆ Star][Game ↗]│
└───────────────────────────────────────────┘
```

| Part | Spec |
|---|---|
| Box | `raised`, 1px `divider` border, radius 20, padding 16×20, gap 12 |
| Top row | League 14 / 900 uppercase, letter-spacing 1.5, `sub`, ellipsis · status right (§3) 16 / 900 |
| Box score grid | Columns `34 (rank) · 30 (badge) · 1fr (name) · N × 26 (periods) · 36 (T)`, column gap 8, row gap 8 |
| Header row | Period labels 13 / 800 `sub`, centered; `T` right-aligned |
| Team row | Rank 15 / 900 `sub` · badge 30×30 radius 8 (logo, else initials on team color) · **short name** 20 (S4) · period scores 16 / 700 `sub` tabular, centered · total 26 tabular, right |
| Weights | Leader: name and total 900 `text`. Other: 700 `sub`. Tie or upcoming: 800 `text` |
| Footer | 1px `divider` top, padding-top 12: `Venue · Broadcasts` 15 / 700 `sub` (flex, ellipsis) · **Star** and **Game ↗** pills (height 38, radius 10, `raised2`, 15 / 800, 16px icon) |
| Upcoming card | No period columns; header row hidden; the T column shows `Away` / `Home` (or `Team` at a neutral site) 15 / 700 `sub` |
| Postponed group | Same card without the box score columns and totals |

---

## 6. Classic

Matches `#artboard-ClassicScoreboard`. Classic tokens, Classic radii (6), Classic type (current font).

| Part | Spec |
|---|---|
| Tabs | Home · Apps · Library · Collections · **Sports** · Settings; sub-tabs Competitions \| **Scoreboard** (underlined) as today |
| Controls | Same order as Blades: ‹ · full date · › · Today · dropdown (min 300) · Refresh · freshness. Height 50, radius 6, Classic button style |
| Group heading | 30 / 600 + count 20 / 400 `sub`; Live adds a 13px `live` dot |
| Grid | **2 columns**, gap 14 × 17 → rows ≈ 900 wide |

### 6.1 Row

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ COLLEGE FOOTBALL │                          1   2   3   4     T   │ ☆ Star  │
│ ● 3rd · 3:38     │ #2  [UTA] Utah Utes      10   7   0        17  │         │
│                  │     [TCU] TCU Horned Frogs 0  0  14        14  │ Game ↗  │
│                  │ Amon G. Carter Stadium · Fox                    │         │
└──────────────────────────────────────────────────────────────────────────────┘
```

| Part | Spec |
|---|---|
| Box | Classic tile color, radius 6, padding 16×20; grid `160 · 1fr · 112`, column gap 20 |
| Left | League 14 / 700 uppercase `sub` (wraps to 2 lines) · status 18 / 900 (§3) |
| Middle | Same box score grid as Blades with Classic sizes: `40 · 34 · 1fr · N × 34 · 44`; **full team name** 22; periods 18; total 30. Venue line 15 `sub` under it |
| Right | **Star** and **Game ↗** stacked, height 42, radius 6, Classic secondary button |
| Removed | The "Leading" label and the "Away periods / Home periods" text |

---

## 7. Box score columns

| Sport / league | Columns | Labels |
|---|---|---|
| Football (NFL, college) | 4 + one `OT` column if any OT was played (all OT periods summed) | `1 2 3 4 OT` |
| Hockey (NHL) | 3 + `OT` if played + `SO` if a shootout (`1`/`0` goal, as ESPN shows it) | `1 2 3 OT SO` |
| Men's / women's college basketball | 2 halves + `OT` | `1st 2nd OT` |
| NBA, WNBA, women's college basketball (quarters) | 4 + `OT` | `1 2 3 4 OT` |
| Soccer | 2 halves (+ `ET`, `PK` if played) | `1st 2nd ET PK` |
| Baseball and anything else | **No period columns**, total only (9+ innings don't fit a Blades card; Classic matches for parity) | `T` |

- Periods not yet played: blank. Periods data missing (S3): no period columns, header hidden.
- `T` is always the DB total (`HomeScore`/`AwayScore`), not a sum of periods.
- The number of columns is decided per game, so cards in one row can differ.

---

## 8. Star and Game ↗

| Control | Behavior |
|---|---|
| **Star** | Toggles the game (`EventId`) in `~/.local/share/tvdash/sports-starred.json` (alerts spec §3.3). Off: outline star + `Star`. On: filled star in `accent` + `Starred`. Updates instantly and survives a restart |
| Followed team (from MyNewsFeed, alerts spec §3.2) | Unchanged: filled star + `Following`, not toggleable |
| **Game ↗** | Opens `SourceUrl` (ESPN game page) in the TV browser. Hidden if `SourceUrl` is empty |
| Home's Next Game | Uses starred games first (home spec §5.2 step 0) once this works |

Fix S6 first; add a regression test that starring, reloading the page and restarting all keep the star.

---

## 9. Focus (D-pad)

- Focus targets are the **pills** (Star, Game ↗), not the card. The card around the focused pill gets the card focus ring (Blades 4px `accent` offset 4; Classic 4px red inset); the pill gets its own 3px ring.
- **← →**: Star → Game ↗ → next card's Star (reading order across the row); from the last card in a row, wrap to the first card of the next row.
- **↑ ↓**: same pill in the card above/below (same column); from the first row ↑ goes to the controls row (nearest control by x); from the controls ↑ goes to Competitions | Scoreboard.
- Controls row: ← → across controls; ↓ → first card's Star.
- Default focus on open: the first card's **Star** in Live (or in the first group present). Opened from Home's Next Game: that game's Star, scrolled into view.
- Strips (Blades) and tabs (Classic) as before.

---

## 10. Data and state

One view model for both UIs (S8):

```rust
// crates/app/src/scoreboard_view.rs
pub struct ScoreboardView { pub date_label: String, pub fresh: String, pub stale: bool, pub groups: Vec<GroupView> }
pub struct GroupView { pub kind: GroupKind, pub title: String, pub count: String, pub cards: Vec<CardView> }
pub enum GroupKind { Live, Upcoming, Final, Postponed }
pub struct CardView {
    pub event_id: String, pub league: String, pub status: String, pub tone: StatusTone,
    pub period_labels: Vec<String>,          // empty → no box score columns
    pub away: SideView, pub home: SideView,
    pub meta: String,                        // "Venue · Broadcasts"
    pub starred: bool, pub following: bool, pub game_url: Option<String>,
}
pub struct SideView { pub rank: String, pub short: String, pub full: String, pub badge: Badge,
                      pub periods: Vec<String>, pub total: String, pub emphasis: Emphasis }
pub enum Emphasis { Lead, Trail, Even }

pub fn build_view(rows: &[Game], ranks: &Ranks, starred: &HashSet<String>,
                  follows: &[TeamKey], filter: &Filter, day: NaiveDate, now: DateTime<Local>) -> ScoreboardView;
```

- Slint: mirror these as structs in `state.slint` under the existing `Scoreboard` global (`in property <[ScoreGroup]> groups`, `date-label`, `fresh`, `stale`; callbacks `prev-day`, `next-day`, `today`, `set-filter(string)`, `refresh`, `toggle-star(string)`, `open-game(string)`).
- Blades uses `short` (falls back to `full`), Classic uses `full`.

### Unit tests

- [ ] `status_text`: running clock; `End 1st`, `Halftime`, `2nd Int` for 0:00/break; OT/2OT/SO; `LIVE` with no period data; `Tonight`/`Today`/`Tomorrow`/weekday; `Time TBD`; Final/OT as-is; postponed family.
- [ ] Period columns per sport (§7), OT column only when played, blanks for unplayed periods, none when data is missing.
- [ ] Emphasis: lead/trail by total; tie → Even; upcoming → Even.
- [ ] Grouping and order: by `StartsAtUtc`, TBD last, empty groups dropped.
- [ ] Filter: All, Top 25 (only ranked games), single league; filter reset from Next Game.
- [ ] Star toggle round-trips through `sports-starred.json`.
- [ ] Classic and Blades get identical `ScoreboardView` for the same inputs (only `short` vs `full` differs at render).

---

## 11. Milestones

| M | Goal | Done when |
|---|---|---|
| S0 | Findings | §0 table filled; Star root cause written down |
| S1 | View model | `build_view` + tests; Classic switched to it without visual change yet |
| S2 | Blades | §5 grid and cards; controls; groups; focus |
| S3 | Classic | §6 rows; "Leading" and periods text removed |
| S4 | Star | Works in both UIs; survives restart; Next Game step 0 enabled |

## 12. Done when

- [ ] A live football game reads `3rd · 3:38`; an NHL intermission reads `2nd Int`, never `2nd · 0:00`.
- [ ] Every live and final game shows a box score with the right columns for its sport; the leader is bold, the other team lighter.
- [ ] No red anywhere except live status and live dots.
- [ ] Ranks line up in their own column; names never collide with scores.
- [ ] Controls are ‹ date › · Today · dropdown · Refresh · freshness; no All statuses.
- [ ] Every group is in start-time order.
- [ ] Star toggles per game, persists, and is visible in both UIs.
- [ ] Opening a game from Home's Next Game lands on it with the filter at All.
- [ ] Blades fits 6 live cards in two rows at 1080p; Classic fits 6 live rows in three.
- [ ] Works by remote alone in Classic, Blades and Glass.

## 13. Change log

| Date | Change |
|---|---|
| 2026-11-22 | Spec written from the Scoreboard review and concepts: 3-col cards (Blades) / 2-col rows (Classic), period-aware status, box score always, leader emphasis, red only for live, rank column, no W/L records, simplified controls, per-game Star (bug to fix), start-time order |
