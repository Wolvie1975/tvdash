# tvdash — Sports Alerts Build Spec (2026-11-21)

Toasts for the games you follow: a **Starting soon** card before a game starts and a **Final** card when it ends, shown over tvdash, over Plex video and over Brave. Uses **Concept 2 · Action card** from the design canvas.

- Design canvas (Blades concepts, "Sports alerts" row): https://claude.ai/artifact/81e81kx9uQqBERcLKja5W1
  - Chosen card: `#artboard-ToastCard` (use Tweaks: Scene = home / video / brave, Alert = soon / final, Theme)
  - Settings page: `#artboard-ToastFollowing`
  - Not chosen, for reference only: `#artboard-ToastChip`, `#artboard-ToastStrip`
- Builds on: `tvdash-blades-glass-build-spec.md` (tokens, Up Next card §5.15, Scoreboard §5.10, Settings §5.11), `tvdash-sleep-timer.md` (warning card pattern, `home_cmd.rs` socket + `127.0.0.1:47800` server, pill extension §7.3, `hold_sleep_timer`), `tvdash-google-meet.md`.
- All names, teams and times in the mockups are **sample data**.

---

## 0. For the coding agent: find these first

This spec was written without the Sports SQL code in view. **Before writing any alert code, find the answers below in the repo, write them into the table, and commit the updated table with the first change.** If any answer is "doesn't exist", stop and report back instead of guessing (see 0.3).

### 0.1 Where to look

```bash
rg -n -i "sports sql|scoreboard|competitions" crates/ --type rust
rg -n -i "tiberius|sqlx|odbc|mssql|postgres|rusqlite" Cargo.toml crates/*/Cargo.toml
rg -n -i "SELECT .* FROM" crates/ --type rust
rg -n "struct .*Game|struct .*Event|enum .*Status" crates/ --type rust
rg -n -i "sports" crates/app/ui/state.slint crates/core/src/
```

### 0.2 Findings

Database side filled in from the review of 2026-11-21 (checked against the live `WebScraper` SQL Server DB and `Rss-Rust-Scrape/src/scoreboard.rs`). **Rows marked _pending_ still need the tvdash repo.**

| # | Question | Answer |
|---|---|---|
| F1 | Crate/module and driver | `tvdash-events` (`crates/events/src/lib.rs`, `scoreboard.rs`, `format.rs`), using `tiberius` 0.12 (`rustls`, `tds73`, `chrono`) over `tokio::net::TcpStream` + `tokio_util::compat`. Direct `connect()` per fetch (no connection pool) |
| F2 | Tables behind Competitions and Scoreboard | `WebScraper` has two sources. **Competitions:** `dbo.SportsEvents` (+ `SportsEventsType`, `SportsEventSources`), schedule and links only, **no scores or final state** (`crates/events/src/lib.rs`). **Scoreboard:** `dbo.SportsScoreboard` (+ `SportsScoreboardCoverage`, `SportsScoreboardTop25Week`, `ResultFollows`, `ResultFollowTeams`) (`crates/events/src/scoreboard.rs`). **Alerts read only `dbo.SportsScoreboard`** |
| F3 | Stable game id | `SportsScoreboard.EventId` (`varchar`, ESPN event id, e.g. `401921418`), stable across reschedules and score updates. `SportsEvents.ID` is a different `int` key and doesn't match it |
| F4 | Start time and TBD | `StartsAtUtc` (`datetime2`, **UTC**). `TimeTbd` (`bit`); `StartsAtUtc` is `NULL` when `TimeTbd = 1` |
| F5 | Status values | `Status`: `upcoming`, `live`, `final`, `postponed`, `cancelled` (two l's), `suspended`, `delayed`, `unknown`. Also `Completed` (`bit`), `ProviderStatus` (`STATUS_FINAL`, `STATUS_FINAL_OT`, `STATUS_FULL_TIME` …) and `StatusText` (`"Final"`, `"Final/OT"` …). `final` is populated reliably |
| F6 | Teams | `HomeTeamId`, `AwayTeamId` (`varchar`, ESPN ids), `HomeName`, `AwayName`, `NeutralSite` (`bit`). **Ids are only unique within a league** (§0.4) |
| F7 | Other columns | `HomeScore`, `AwayScore`, `Sport`, `League`, `Venue` (stadium name only), `Broadcasts`, `SourceUrl` (ESPN game page), `LastObservedAtUtc`. **There's no city column, no separate recap link and no "went final at" time** |
| F8 | Writer and freshness | The `web-scoreboard` container runs `scoreboard --watch --live-minutes 10 --lookback-days 1 --lookahead-days 7 --retention-days 1`, updating `dbo.SportsScoreboard` about every **10 min**, live and final status included. **N = 10 min.** Rows cover **1 day back to 7 days ahead** |
| F9 | tvdash poll / `AppEvent` / tokio | No background Sports poller yet. `LoadSportsEvents` and `LoadScoreboard` spawn on-demand `tokio::spawn` tasks in `crates/app/src/controller.rs`, sending `AppEvent::SportsEvents` / `AppEvent::Scoreboard` over `UnboundedSender<AppEvent>` to the Slint UI thread |
| F10 | DB login | `EventsConfig` (`crates/core/src/config.rs`, `[events]` in `config.toml`): `host` (`192.168.1.100`), `port` (`1433`), `database` (`WebScraper`), `user` (`mynewspaper_web`), `trust_cert` (`true`). Password in GNOME Keyring as `SecretKey::EventsSqlPassword` (`events_sql_password`) |
| F11 | Team list | **No teams table** (only `ResultFollowTeams` and `SportsRankingTeams`). Not needed: follows come from `ResultFollows` / `ResultFollowTeams` (§3.3). Team names for display can come from `SportsScoreboard` |
| F12 | `dbo.ResultFollowTeams` (your existing follows, §3.3) | Already queried in `crates/events/src/scoreboard.rs`! `dbo.ResultFollows` (`ID`, `Kind` = `league\|conference\|team`, `Sport`, `League`, `TeamId` [ESPN id when `Kind = 'team'`], `Name`, `Enabled`) stores team follows; `dbo.ResultFollowTeams` (`FollowId`, `TeamId`) stores conference member team ids joined on `f.ID = t.FollowId`. No owner/user column; `mynewspaper_web` already has `SELECT` on both |

### 0.3 Stop conditions

The original three don't apply: `final` is reliable (F5), the DB updates during games (F8) and `EventId` is stable (F3).

**New, for F12.** Stop and report back before building A4 if:

- `ResultFollowTeams` can't be matched to **(League, TeamId)** in `SportsScoreboard`, for example it stores only a name or an id that isn't the ESPN team id. Propose a mapping; don't change the table yourself.
- tvdash's login can't read it. Report the exact `GRANT SELECT` needed instead of switching logins.
- A row means something other than "this team is followed", for example it only filters a results page. Then report what it does mean before using it for alerts.

### 0.4 Team ids repeat across leagues

| `TeamId` | Teams sharing it |
|---|---|
| `12` | Kansas City Chiefs (football), Arizona Wildcats (basketball, volleyball), New York Islanders (hockey) |
| `333` | Alabama Crimson Tide (football, basketball), Bristol City (soccer) |
| `2` | Buffalo Bills (football), Buffalo Sabres (hockey) |
| `197` | Oklahoma State Cowboys and Cowgirls (men's and women's basketball) |

So a followed team is stored as **`(League, TeamId)`**, never a bare id. `League` (rather than `Sport`) is the key because it separates men's from women's basketball (`197`).

> **Agent:** exact `League` strings in `crates/events/src/scoreboard.rs`: `"nfl"`, `"mlb"`, `"nhl"`, `"wnba"`, `"nba"`, `"usa.1"`, `"usa.nwsl"`, `"eng.1"`, `"eng.2"`, `"college-football"`, `"mens-college-basketball"`, `"usa.ncaa.w.1"`, `"womens-college-volleyball"`.

---

## 1. Decisions

| Topic | Decision |
|---|---|
| What you follow | **Teams**, one entry per **(League, TeamId)** (Kansas football and Kansas men's basketball are two follows), **and** individual **starred games** (starred on Scoreboard only, §3.2) |
| Data source | **`dbo.SportsScoreboard` only.** `SportsEvents` (Competitions) has no status or scores |
| Follow list | **Your existing MyNewsFeed follows**, read from `dbo.ResultFollowTeams`. tvdash **only reads** them: no Add/Remove team in tvdash; you manage follows in MyNewsFeed (§3.3) |
| Starred games | **Local to tvdash** (`~/.local/share/tvdash/sports-starred.json`). Not shared with MyNewsFeed |
| Starring on Competitions | **Not in v1.** Maybe later (§9) |
| Triggers | **Starting soon** (default 15 min before the listed start) and **Final** (when status turns final). No live score-change alerts |
| Where it shows | **Everywhere**: tvdash pages, over in-window Plex video, over the ambient screensaver, and over Brave (via the pill extension) |
| Look | **Concept 2 · Action card**: bottom-right card that **takes focus** |
| Buttons, Starting soon | **Watch on YouTube TV** (primary, focused) · **Dismiss** |
| Buttons, Final | **Recap ↗** (primary, focused, opens `SourceUrl`, the ESPN game page with the recap and box score, in the TV browser) · **Dismiss**. Empty `SourceUrl` → only **Dismiss**, focused |
| Auto-close | **12 s** with a visible "Closes in N s". Closing on its own counts as dismissed |
| Back/Esc | Dismiss |
| One at a time | Queue the rest; show the next 3 s after the current one closes |
| Video | Keeps playing behind the card. No pause |
| Google Meet | **Held** while an app with `hold_sleep_timer = true` is open (proposed: reuse that flag; see §9 open question). Queued alerts show after the call if still relevant |
| Startup catch-up | Starting soon still fires if tvdash starts inside the window and the game hasn't started. Final only fires if the game went final **less than 30 min ago** |
| Postponed / Delayed / Time TBD | No Starting soon alert. If a time is set later (or the status goes back to `upcoming`), the game becomes eligible again |
| Cancelled / Suspended | No alerts. A suspended game that later resumes and goes `final` still gets its Final alert |

### Why YouTube TV

It's the only service you use for live TV. **Watch on YouTube TV** opens the YouTube TV `[[streaming]]` entry exactly like its Services tile (same Brave launch, same recent-services update). No deep link to a channel in v1.

---

## 2. The card

Reuse the **Up Next card** component (`blades-glass` §5.15) with sports content. Matches `#artboard-ToastCard`.

| Part | Spec (1920×1080 canvas, scale with `TvUi.u`) |
|---|---|
| Position | Bottom-right inside the safe area: `right 96`, `bottom 120`. While the player HUD is visible, sit above it (`bottom 250`, the Up Next position) |
| Box | 560 wide, radius 26, `raised` (Dark `#1C2026`), 1 px `divider` border, shadow `0 20 50 rgba(0,0,0,.5)`, padding 22×24, gap 14 |
| Row 1 | Kicker 16 / 900 uppercase, letter-spacing 2: **STARTING IN 15 MIN** (`accentInk`) or **FINAL** / **FINAL/OT** (`sub`). Right: reason chip 15 / 900, `accentSoft` / `accentInk`, radius 9: "Following Kansas" or "Starred game" |
| Rows 2–3 | Away team, then home team: badge 44 (logo or initials on team colour, radius 12), name 24, right side: **Away/Home** 17 / 800 (soon) or **score** 30 tabular (final). Final: winner 900 `text`, loser 700 `sub` |
| Row 4 | Meta 17 / 700 `sub` above a 1 px `divider`: soon = "11:00 A.M. CT · TV: FOX · David Booth Memorial Stadium" (`StartsAtUtc` in Central, `Broadcasts`, `Venue`; skip empty parts); final = "Big 12 Football · David Booth Memorial Stadium" (`League` display name, `Venue`). No city: the DB doesn't have one |
| Kicker text | Final uses `StatusText` as is ("Final", "Final/OT"), uppercased |
| Badges | Same team badge as the Scoreboard card (logo if tvdash has one, else initials on a colour) |
| Row 5 | Buttons height 56, pill radius: primary `accent` / `accentText` 20 / 900 with the 4 px focus ring; secondary `raised2` 20 / 800. Right: "Closes in 12 s" 16 / 800 `sub` |
| Motion | Slide in from the right 24 px + fade, 180 ms ease-out; out 140 ms |

- Neutral site: role reads "Team" instead of Away/Home (same as Competitions).
- The countdown **pauses while you're moving focus** (any arrow key resets it to 12 s), so it doesn't close while you're choosing.
- Classic UI: same card, Classic tokens.

### Focus

- On show, focus moves to the primary button. The previous focus target is remembered.
- Left/Right moves between the buttons; Up/Down do nothing (no accidental escape into the page).
- On close (button, Back, timeout): focus returns to where it was. If that was the video, the HUD stays hidden.
- Add the card's buttons to `tile_rects()` in `tv_nav.rs` while it's open (it traps focus, like the Sleep warning).

### Priority with other cards

| Already showing | Sports card |
|---|---|
| Sleep timer "Still watching?" | Waits until it closes. Sleep always wins |
| Up Next card | Waits until it closes |
| Picker / dialog (Sleep picker, Watching now, Search keyboard) | Waits until it closes |
| Ambient screensaver | Shows over it. Keys go to the card; the screensaver stays until you press a key **after** the card closes |
| Player HUD | Shows above the HUD |

---

## 3. Following

### 3.1 Settings › Sports (new sub-tab)

Matches `#artboard-ToastFollowing`. Sub-nav becomes: Display · Accounts · Sites · **Sports** · Server · Diagnostics · What's new.

| Card | Contents |
|---|---|
| **Followed teams** | **Read-only.** One row per follow: badge, name, meta = league display name. Header note: "Managed in MyNewsFeed · updated 10:42 AM". No Add or Remove. Empty list: "No followed teams. Follow teams in MyNewsFeed." |
| **Alerts** | **Starting soon** [Off · 5 · 10 · 15 · 30 min] (default 15) · **Final score** [On · Off] · **During video** [Show · Hold] (Hold = don't show over Plex/Brave; queue until you're back in tvdash) |
| **Starred games** | Upcoming starred games: star icon, matchup, "Sat 1:00 P.M. CT · league · TV", **Unstar** |

### 3.2 Starring a game

- **Scoreboard** card footer: a ★ toggle before "Game ↗". Starring stores the `EventId`.
- **Competitions: no star in v1.** Its rows come from `dbo.SportsEvents`, whose `ID` doesn't match `SportsScoreboard.EventId`, and it lists games beyond the scoreboard's 7-day window. Followed teams still cover those games once they reach the scoreboard window.
- Games of followed teams show a filled star with "Following" on Scoreboard (not toggleable; unfollow the team in Settings).

### 3.3 Storage

**Followed teams come from `dbo.ResultFollowTeams`**, the follows you already set in MyNewsFeed. tvdash reads them and never writes them. Its own config holds only the alert rules:

```toml
# ~/.config/tvdash/config.toml
[sports.alerts]
enabled = true
soon_minutes = 15        # 0 = off
final = true
during_video = "show"    # show | hold
```

#### Reading follows

- **Read** the follow list on startup, when Settings › Sports opens, and every **10 min**, so a follow added or removed in MyNewsFeed reaches tvdash alerts within ~10 min.
- Turn each row into a **`TeamKey { league, id, name }`** (§5.1). Matching is always on **(League, TeamId)**. If the table stores ids or league names in another form, map them here, in one function with its own tests (F12).
- Name: use the table's display name if it has one; otherwise the latest `SportsScoreboard` row for that team; otherwise "Team <id>".
- Rows that can't be mapped are skipped and listed in Diagnostics ("2 follows couldn't be matched to the scoreboard"), not silently dropped.
- Exact SQL comes after F12. Shape only:

```sql
-- read-only; columns from F12
SELECT League, TeamId, TeamName
FROM   dbo.ResultFollowTeams
/* WHERE Owner = @P1  -- if F12 finds an owner/user column */;
```

#### When the database can't be reached

- Keep the **last good follow list** in `~/.cache/tvdash/sports-follows.json` and use it while the DB is down, so a restart during an outage still knows your teams. Settings shows "Couldn't reach Sports SQL · showing the list from 10:42 AM".

#### Starred games (local)

- Starred games are tvdash-only and stay in `~/.local/share/tvdash/sports-starred.json` (above). They aren't written to the database.

Starred games are short-lived data, not config:

```text
~/.local/share/tvdash/sports-starred.json   # [{ "event_id": "401921418", "starred_at": 1795289000 }]  (SportsScoreboard.EventId)
~/.local/state/tvdash/sports-alerts.json    # fired alert keys (§5.2), pruned after 3 days
```

- Prune starred games 24 h after they go final (or 24 h after their start time if no final arrives).

---

## 4. Data flow

```text
Sports SQL ──(poll, §4.1)──► SportsWatcher (tokio) ──AppEvent::SportsAlert──► UI thread: AlertQueue ──► card
                                     │                                              │
                                     └── sports-alerts.json (fired keys)            └── GET /alerts (pill extension)
```

### 4.1 Polling

Reuse the existing Sports connection (F1/F10). Add one narrow query for **followed and starred games only**, from `now − 12 h` to `now + soon_minutes + 30 min`:

```sql
SELECT EventId, StartsAtUtc, TimeTbd, Status, StatusText,
       League, Sport, HomeTeamId, HomeName, HomeScore,
       AwayTeamId, AwayName, AwayScore, NeutralSite,
       Venue, Broadcasts, SourceUrl, LastObservedAtUtc
FROM   dbo.SportsScoreboard
WHERE  (
         -- one group per followed team, built at runtime; parameters, never string-pasted values
            (League = @P1 AND (HomeTeamId = @P2 OR AwayTeamId = @P2))
         OR (League = @P3 AND (HomeTeamId = @P4 OR AwayTeamId = @P4))
         -- starred games
         OR EventId IN (@P5, @P6)
       )
  AND  (StartsAtUtc IS NULL OR StartsAtUtc BETWEEN @From AND @To);
```

- Build the `WHERE` from the follow list with numbered `tiberius` parameters (`@P1`…). No string interpolation of ids or league names.
- Skip the query entirely when nothing is followed or starred.

**Fetch and evaluate on separate clocks.** Starting soon depends on the clock, not on new data, so evaluation runs on its own **30 s tick** against the last rows fetched. Fetching only brings in status, score and time changes.

| Situation | Fetch interval |
|---|---|
| A followed/starred game starts within `soon_minutes + 30 min`, or is live | **2 min** (the DB itself refreshes every ~10 min; 2 min keeps the Final alert within ~2 min of the DB) |
| Otherwise | **10 min** |
| Nothing followed or starred | No fetching |
| DB errors | Back off 1 → 2 → 5 min; Diagnostics' Sports SQL row shows the error; no alert spam |

> **Latency:** Final alerts arrive **up to ~12 min** after the real final whistle (10 min DB refresh + 2 min fetch). Starting soon is on time, because it's computed from `StartsAtUtc`.

> **Stale data:** if `MAX(LastObservedAtUtc)` across the fetched rows is more than **25 min** old, the `web-scoreboard` writer has probably stopped. Diagnostics shows WARNING "Scoreboard data is N min old"; Starting soon still fires from the known start time, and Final alerts simply wait.

---

## 5. Alert logic

### 5.1 Types

```rust
// crates/app/src/sports_alerts.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind { Soon, Final }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameStatus { Upcoming, Live, Final, Postponed, Cancelled, Suspended, Delayed, Unknown }

impl GameStatus {
    /// `SportsScoreboard.Status`, as written by Rss-Rust-Scrape `normalized_status` (F5).
    pub fn from_db(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "upcoming" => Self::Upcoming,
            "live" => Self::Live,
            "final" => Self::Final,
            "postponed" => Self::Postponed,
            "cancelled" | "canceled" => Self::Cancelled,   // DB writes "cancelled"; accept both
            "suspended" => Self::Suspended,
            "delayed" => Self::Delayed,
            _ => Self::Unknown,                            // includes "unknown"
        }
    }
}

/// One row of dbo.SportsScoreboard (§4.1).
#[derive(Clone, Debug)]
pub struct Game {
    pub event_id: String,         // EventId
    pub start: Option<i64>,       // StartsAtUtc as unix seconds; None when TimeTbd = 1
    pub status: GameStatus,       // Status
    pub status_text: String,      // StatusText: "Final", "Final/OT", …
    pub league: String,           // League (exact string, part of the team key)
    pub sport: String,            // Sport
    pub home: Side,
    pub away: Side,
    pub neutral: bool,            // NeutralSite
    pub venue: String,            // Venue (stadium only)
    pub broadcasts: Option<String>, // Broadcasts
    pub source_url: Option<String>, // SourceUrl → Recap ↗
}

#[derive(Clone, Debug)]
pub struct Side { pub team_id: String, pub name: String, pub score: Option<i32> }

/// A followed team: only unique as (league, team_id) (§0.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamKey { pub league: String, pub id: String, pub name: String }

#[derive(Clone, Debug, PartialEq)]
pub enum Reason { Team(String), Starred }   // Team(display name) → "Following Kansas"

#[derive(Clone, Debug)]
pub struct Alert { pub kind: AlertKind, pub game: Game, pub reason: Reason, pub key: String }

pub struct Rules { pub soon_s: i64, pub final_on: bool, pub final_grace_s: i64 }  // final_grace = 30 min
```

### 5.2 Evaluate (pure, unit-tested)

```rust
use std::collections::HashSet;

pub fn key(kind: AlertKind, g: &Game) -> String {
    match kind {
        // Include the start time so a rescheduled game alerts again at its new time.
        AlertKind::Soon => format!("soon:{}:{}", g.event_id, g.start.unwrap_or(0)),
        AlertKind::Final => format!("final:{}", g.event_id),
    }
}

pub fn reason(g: &Game, teams: &[TeamKey], starred: &HashSet<String>) -> Option<Reason> {
    // League AND id must both match: ids repeat across leagues (§0.4).
    // A followed team wins over a star for the chip text.
    for t in teams {
        if t.league == g.league && (g.home.team_id == t.id || g.away.team_id == t.id) {
            return Some(Reason::Team(t.name.clone()));
        }
    }
    starred.contains(&g.event_id).then_some(Reason::Starred)
}

/// `final_seen_at`: when this watcher first saw the game as Final (kept in memory per game id).
pub fn evaluate(
    games: &[Game],
    teams: &[TeamKey],
    starred: &HashSet<String>,
    fired: &HashSet<String>,
    final_seen_at: &mut std::collections::HashMap<String, i64>,
    rules: &Rules,
    now: i64,
) -> Vec<Alert> {
    let mut out = Vec::new();
    for g in games {
        let Some(why) = reason(g, teams, starred) else { continue };

        // Only `upcoming`: postponed / delayed / TBD start times aren't trustworthy.
        if rules.soon_s > 0 && g.status == GameStatus::Upcoming {
            if let Some(start) = g.start {
                let left = start - now;
                let k = key(AlertKind::Soon, g);
                if left > 0 && left <= rules.soon_s && !fired.contains(&k) {
                    out.push(Alert { kind: AlertKind::Soon, game: g.clone(), reason: why.clone(), key: k });
                }
            }
        }

        if rules.final_on && g.status == GameStatus::Final {
            let seen = *final_seen_at.entry(g.event_id.clone()).or_insert(now);
            let k = key(AlertKind::Final, g);
            if now - seen <= rules.final_grace_s && !fired.contains(&k) {
                out.push(Alert { kind: AlertKind::Final, game: g.clone(), reason: why, key: k });
            }
        }
    }
    out.sort_by_key(|a| a.game.start.unwrap_or(i64::MAX));
    out
}
```

- **Mark fired when the card is shown**, not when it's queued, and save `sports-alerts.json` right away. A crash mid-queue then re-shows only what you never saw.
- **First fetch after startup:** seed every game that's already `final` with `now − final_grace_s − 1`, so games that finished before tvdash started never alert. Don't use `LastObservedAtUtc` for this: the writer refreshes it every ~10 min even after a game is final, so it isn't a "went final at" time (F7).
- `final_seen_at` lives in memory only. After a restart, the seeding rule above plus the fired-key file stop repeats.
- **Drop stale queue items** before showing: a Soon alert whose game has started, or a Final older than the grace window.

### 5.3 Queue (UI thread)

```rust
pub struct AlertQueue { items: std::collections::VecDeque<Alert>, showing: Option<Alert>, held: bool }
// push(alert)        – ignore if the key is already queued or showing
// next(now) -> Option<Alert> – None while held, while another card has priority (§2), or <3 s after the last close
// close(Action)      – Primary | Dismiss | Timeout
```

### 5.4 Unit tests

- [ ] Followed team, `upcoming`, start in 14 min, soon = 15 → one Soon alert, reason Team.
- [ ] Same game, start in 16 min → none. Start already passed (status still `upcoming`) → none.
- [ ] Soon key already fired → none. Start time moved 1 h later → new key, alerts again inside the new window.
- [ ] `TimeTbd` (`start = None`), `postponed` or `delayed` → no Soon alert.
- [ ] Status turns `final` on this fetch → Final alert. Next fetch → none (fired).
- [ ] First fetch after startup with a game already `final` → none.
- [ ] `suspended` → none; later `final` → Final alert.
- [ ] `cancelled` → never alerts.
- [ ] **Id collision:** following `(NFL league, "12")` (Chiefs). A men's basketball game with Arizona (id `12`) → **no alert**. A hockey game with the Islanders (id `12`) → **no alert**. A Chiefs game → alert.
- [ ] **Men's vs women's:** following Oklahoma State men's basketball (`197`) → no alert for a women's basketball game with id `197`.
- [ ] Starred game only → reason Starred. Starred **and** followed team → reason Team.
- [ ] Game with no followed team and not starred → never alerts.
- [ ] `final = false` → no Final alerts; `soon_minutes = 0` → no Soon alerts.
- [ ] Two games starting at 11:00 and 11:05 → both queued, earlier first.
- [ ] `GameStatus::from_db`: every F5 value (`upcoming`, `live`, `final`, `postponed`, `cancelled`, `suspended`, `delayed`, `unknown`) maps to its variant; `"canceled"` → `Cancelled`; any other string → `Unknown`.
- [ ] Soon alert fires on the 30 s evaluation tick with no new fetch in between (clock-driven, §4.1).

---

## 6. Showing it everywhere

| On the TV | Card drawn by |
|---|---|
| tvdash window (any page, HUD, screensaver) | tvdash (§2) |
| **Brave** (streaming) | **Pill extension**, same pattern as the Sleep card (`tvdash-sleep-timer.md` §7.3) |
| mpv's own window (only if the Sleep timer's S1 check found mpv has focus) | mpv OSD `show-text` + temporary `keybind` for Enter/Esc (Sleep §7.4) |

### 6.1 Local server (`127.0.0.1:47800`, loopback only)

| Method | Path | Body | Reply |
|---|---|---|---|
| `GET` | `/alerts` | none | `null` or the showing alert: `{"key","kind":"soon|final","kicker","why","away":{name,init,color,right},"home":{…},"meta","primary":"Watch on YouTube TV"|"Recap ↗"|null,"closes_in":12}` |
| `POST` | `/alerts` | `{"key":"…","action":"primary|dismiss"}` | `204`. `primary` runs the same action as the button in tvdash (open YouTube TV / open the recap link) |

- The card is "showing" in exactly one place. While Brave is on top, tvdash hands the alert to `/alerts` instead of drawing it.

### 6.2 Pill extension

- Add `{t: 'alert?'}` / `{t: 'alert', key, action}` messages to `extras/back-pill/worker.js`, next to the sleep ones.
- In `pill.js`, a second card in the same closed shadow root, built like the Sleep card (`position: fixed; right: 96px; bottom: 120px; width: 560px`), with the §2 layout and Dark tokens. Poll every **3 s** (alerts are less time-critical than sleep, but 3 s keeps "Closes in" honest).
- Focus the primary button when it appears; Escape → dismiss.
- If the Sleep card is visible, the alert card stays hidden (Sleep wins, §2).
- A site's own full-screen player may cover overlays (same note as Sleep). The alert then times out unseen; tvdash still marks it shown.

### 6.3 Hold

- `during_video = "hold"` → queue while Plex video or Brave is on top; show when you're back in tvdash.
- App open with `hold_sleep_timer = true` (Google Meet) → always held, whatever `during_video` says.

---

## 7. Test and control from outside

| Command | Effect |
|---|---|
| `tvdash --alert-test soon` | Shows a sample Starting soon card (Iowa State at Kansas) through the normal queue, wherever you are, Brave included |
| `tvdash --alert-test final` | Same, Final |
| `tvdash --alerts status` | Prints followed teams, starred count, next eligible alert and last poll result |

Socket lines: `alert-test soon|final`, `alerts status` (same handler as `home` and `sleep`).

**Diagnostics:** the Sports SQL row's detail adds "Alerts: 3 teams · 2 starred · checked 2:45 PM". A failing alert query shows as WARNING with the error.

---

## 8. Files

```text
crates/app/
├─ src/sports_alerts.rs       # types, evaluate, key, reason, AlertQueue, tests (§5)
├─ src/sports_watch.rs        # tokio poller, intervals, fired-key file, AppEvent::SportsAlert (§4)
├─ src/sports_follows.rs      # read-only dbo.ResultFollowTeams → TeamKey mapping, 10 min refresh, offline cache (§3.3)
├─ src/home_cmd.rs            # + alert-test / alerts status socket lines, GET/POST /alerts (§6.1, §7)
├─ src/main.rs                # + --alert-test, --alerts client
├─ src/tv_nav.rs              # + card buttons in tile_rects() while open
├─ src/bridge/                # + sports alert → SportsAlertState
└─ ui/
   ├─ state.slint             # + global SportsAlertState { visible, kicker, kicker-final, why, away, home, meta, primary, closes-in }
   ├─ shared/sports_alert.slint   # SportsAlertCard (styled per Dashboard UI)
   ├─ v3/…                    # Blades/Glass: Settings › Sports tab; ★ pills on Competitions/Scoreboard
   └─ tv/…                    # Classic: same card + settings rows
crates/core/src/              # + [sports.alerts] config, sports-starred.json load/save
extras/back-pill/             # worker.js + pill.js alert card (§6.2)
```

---

## 9. Open questions

- [ ] **Hold flag:** reuse `hold_sleep_timer` for alerts too, or add a separate `hold_alerts` flag per app? (Spec assumes reuse.)
- [x] ~~F8 freshness~~: the DB updates every ~10 min during games, so Final alerts land within ~12 min (§4.1).
- [x] ~~Team scope~~: resolved by the id collisions. Follows are per **(League, TeamId)**; following Kansas in two sports means two entries (§3.3).
- [x] ~~Shared follows with MyNewsFeed~~: **yes, read-only.** tvdash uses your existing follows from `dbo.ResultFollowTeams` and doesn't add or remove teams (§3.3). Schema still to check (F12).
- [ ] **Star on Competitions:** not now; undecided for later. It would need a mapping from `SportsEvents.ID` to `SportsScoreboard.EventId` (e.g. a column the scraper fills).
- [x] ~~Shared starred games~~: **no.** Starred games stay local to tvdash (`sports-starred.json`).

---

## 10. Milestones

| M | Goal | Done when |
|---|---|---|
| A0 | Findings | DB side done (2026-11-21 review). Still to fill: F1, F2 (which table each screen reads), F9, F10, F12 (`ResultFollowTeams` + MyNewsFeed's use of it), plus the exact `League` strings in §0.4 |
| A1 | Logic | `sports_alerts.rs` with §5.4 tests passing, including the id-collision and status-mapping tests |
| A2 | Card in tvdash | `--alert-test soon/final` shows the card over Home, the HUD and the screensaver; focus trap, timeout, Back, priority rules |
| A3 | Real alerts | Watcher polls followed/starred games; Soon and Final fire once each; fired keys survive a restart |
| A4 | Following UI | Follows read from `dbo.ResultFollowTeams` (10 min refresh, offline cache); Settings › Sports (read-only followed teams, rules, local starred games); ★ on Scoreboard, in Blades, Glass and Classic |
| A5 | Everywhere | `/alerts` + pill card over Brave; Meet hold; `during_video = hold`; Diagnostics line |

## 11. Done when

- [ ] Following Kansas, a Kansas game shows **Starting in 15 min** once, ~15 min before start, over whatever is on the TV.
- [ ] **Watch on YouTube TV** opens YouTube TV like its Services tile; **Dismiss**, Back and the 12 s timeout all close the card and return focus.
- [ ] When the game goes final, a **Final** card shows the score with the winner in bold, once, within ~12 min; **Recap ↗** opens the ESPN game page (`SourceUrl`) in the TV browser (or the card has only Dismiss).
- [ ] Following the Chiefs never alerts for Arizona or the Islanders (shared ESPN id `12`).
- [ ] A team followed in MyNewsFeed shows in Settings › Sports and alerts within ~10 min; unfollowing it there stops its alerts within ~10 min. tvdash never writes to `ResultFollowTeams`.
- [ ] Starred games survive a restart and never appear in MyNewsFeed.
- [ ] With the scoreboard writer stopped for 25+ min, Diagnostics warns that the data is stale.
- [ ] A starred game from another team alerts the same way, with "Starred game".
- [ ] Restarting tvdash never repeats an alert, and never alerts for games that finished before it started.
- [ ] The Sleep warning and Up Next card always go first; the sports card waits.
- [ ] Nothing shows during a Google Meet call; relevant alerts show after it.
- [ ] Works by remote alone in all three Dashboard UIs.

## 12. Change log

| Date | Change |
|---|---|
| 2026-11-21 | Changed: tvdash **only reads** your existing MyNewsFeed follows (no Add/Remove team, no picker, no writes to `ResultFollowTeams`); starred games stay **local** to tvdash |
| 2026-11-21 | Decided: followed teams are **shared with MyNewsFeed** via `dbo.ResultFollowTeams` (new F12 + stop conditions; config keeps only alert rules; offline cache, no offline queue). Starring on Competitions: not in v1, undecided for later |
| 2026-11-21 | Applied the DB review: §0 filled (F2–F8, F11); alerts read `dbo.SportsScoreboard` only; follows keyed by **(League, TeamId)** because ESPN ids repeat across leagues; real status set incl. `cancelled` / `suspended` / `delayed`; Recap ↗ = `SourceUrl`; no city; Final seeding ignores `LastObservedAtUtc`; fetch 2 min / 10 min with a separate 30 s evaluation tick; stale-data warning; star on Scoreboard only |
| 2026-11-21 | Spec written. Chosen: Concept 2 (Action card); follow teams + starred games; triggers Starting soon + Final; shown everywhere. §0 findings pending from the repo |
