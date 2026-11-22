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

### 0.2 Fill in

| # | Question | Answer (fill in) |
|---|---|---|
| F1 | Which crate/module talks to the Sports SQL database, and which DB engine + Rust driver does it use? | |
| F2 | Which tables/views (and query functions) feed **Competitions** and **Scoreboard**? | |
| F3 | Column for a **stable game id** (doesn't change when the time or score changes) | |
| F4 | Columns for **start time**: is it stored in UTC or Central? Is "Time TBD" a null, a flag or a text value? | |
| F5 | Column(s) for **status** and the **exact set of values** (e.g. scheduled / live / final / final-OT / postponed / canceled). Run a `SELECT DISTINCT` on it | |
| F6 | Columns for **home/away team**: is there a stable **team id**, or only names? Neutral-site flag? | |
| F7 | Columns for **scores**, **league/sport/conference**, **venue/city**, **broadcast** (TV), and the **link columns** (Stats / Video / Audio / Details / Recap / Game) | |
| F8 | **What writes this database, and how often?** (Scraper/job name, schedule.) Does it update **live status and scores during games**, or only the schedule? | |
| F9 | How tvdash reads it today: poll interval (Scoreboard says 5 min), the `AppEvent` that carries games to the UI, and whether the queries run on the tokio runtime | |
| F10 | Where the DB login lives (keyring key name, config section) | |
| F11 | Is there a teams table (to build the **Add team** picker), or must the team list come from `SELECT DISTINCT` over games? | |

### 0.3 Stop and report back if

- **F5 has no "final" state** and there's no other reliable signal (e.g. final scores only appear the next day). The Final alert depends on it.
- **F8: the database only updates once or twice a day.** Then Final alerts would arrive hours late; we need to decide on a different source before building A3.
- **F3 has no stable id.** Then the alert keys (§5.2) need a composite key; propose one.

---

## 1. Decisions

| Topic | Decision |
|---|---|
| What you follow | **Teams** (every game they play) **and** individual **starred games** |
| Triggers | **Starting soon** (default 15 min before the listed start) and **Final** (when status turns final). No live score-change alerts |
| Where it shows | **Everywhere**: tvdash pages, over in-window Plex video, over the ambient screensaver, and over Brave (via the pill extension) |
| Look | **Concept 2 · Action card**: bottom-right card that **takes focus** |
| Buttons, Starting soon | **Watch on YouTube TV** (primary, focused) · **Dismiss** |
| Buttons, Final | **Recap ↗** (primary, focused, opens the game's recap/details link in the TV browser) · **Dismiss**. No link in the DB → only **Dismiss**, focused |
| Auto-close | **12 s** with a visible "Closes in N s". Closing on its own counts as dismissed |
| Back/Esc | Dismiss |
| One at a time | Queue the rest; show the next 3 s after the current one closes |
| Video | Keeps playing behind the card. No pause |
| Google Meet | **Held** while an app with `hold_sleep_timer = true` is open (proposed: reuse that flag; see §9 open question). Queued alerts show after the call if still relevant |
| Startup catch-up | Starting soon still fires if tvdash starts inside the window and the game hasn't started. Final only fires if the game went final **less than 30 min ago** |
| Postponed / Time TBD | No Starting soon alert. If a time is set later, the game becomes eligible again |

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
| Row 4 | Meta 17 / 700 `sub` above a 1 px `divider`: soon = "11:00 A.M. CT · TV: FOX · Lawrence, KS"; final = "Big 12 Football · David Booth Memorial Stadium" |
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
| **Followed teams** | Rows: badge, name, meta (league/sport scope), **Remove**. **+ Add team** opens a picker: team list from F11, grouped by league, with the Search screen's A–Z key grid to filter |
| **Alerts** | **Starting soon** [Off · 5 · 10 · 15 · 30 min] (default 15) · **Final score** [On · Off] · **During video** [Show · Hold] (Hold = don't show over Plex/Brave; queue until you're back in tvdash) |
| **Starred games** | Upcoming starred games: star icon, matchup, "Sat 1:00 P.M. CT · league · TV", **Unstar** |

### 3.2 Starring a game

- **Competitions** row: add a **★ Star** pill as the first of the right-aligned link pills (Stats · Video · …). Starred = accent star + "Starred".
- **Scoreboard** card footer: a ★ toggle before "Game ↗".
- Games of followed teams show a filled star with "Following" (not toggleable; unfollow the team in Settings).

### 3.3 Storage

```toml
# ~/.config/tvdash/config.toml
[sports.alerts]
enabled = true
soon_minutes = 15        # 0 = off
final = true
during_video = "show"    # show | hold
teams = ["<team id from F6>", "<team id>"]   # names if F6 has no ids
```

Starred games are short-lived data, not config:

```text
~/.local/share/tvdash/sports-starred.json   # [{ "game_id": "...", "starred_at": 1795289000 }]
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

Reuse the existing Sports query layer (F1/F2). Add one narrow query for **followed and starred games only**, from `now − 6 h` to `now + soon_minutes + 5 min`:

```sql
-- shape only; real table/column names from §0
SELECT game_id, start_utc, status, home_id, home_name, away_id, away_name,
       home_score, away_score, league, venue, city, broadcast_tv, recap_url, details_url, neutral
FROM   <games>
WHERE  (home_id IN (@teams) OR away_id IN (@teams) OR game_id IN (@starred))
  AND  start_utc BETWEEN @from AND @to;
```

| Situation | Interval |
|---|---|
| A followed/starred game is inside its Starting soon window or live | **60 s** |
| Otherwise | **5 min** (same as Scoreboard) |
| Nothing followed or starred | No polling |
| DB errors | Back off 1 → 2 → 5 min; Diagnostics' Sports SQL row shows the error; no alert spam |

> **Depends on F8.** If the database itself only refreshes every N minutes, Final alerts can be up to N minutes late no matter how often tvdash polls. Note N in Diagnostics.

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
pub enum GameStatus { Scheduled, Live, Final, Postponed, Canceled, Unknown }  // map F5 values here

#[derive(Clone, Debug)]
pub struct Game {
    pub id: String,
    pub start: Option<i64>,      // unix seconds; None = Time TBD
    pub status: GameStatus,
    pub final_label: String,     // "Final" / "Final/OT" as the DB writes it
    pub home: Side,
    pub away: Side,
    pub neutral: bool,
    pub league: String,
    pub venue: String,
    pub city: String,
    pub tv: Option<String>,
    pub recap_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Side { pub id: String, pub name: String, pub score: Option<i32> }

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
        AlertKind::Soon => format!("soon:{}:{}", g.id, g.start.unwrap_or(0)),
        AlertKind::Final => format!("final:{}", g.id),
    }
}

pub fn reason(g: &Game, teams: &[(String, String)], starred: &HashSet<String>) -> Option<Reason> {
    // teams: (id, display name). A followed team wins over a star for the chip text.
    for (id, name) in teams {
        if g.home.id == *id || g.away.id == *id { return Some(Reason::Team(name.clone())); }
    }
    starred.contains(&g.id).then_some(Reason::Starred)
}

/// `final_seen_at`: when this watcher first saw the game as Final (kept in memory per game id).
pub fn evaluate(
    games: &[Game],
    teams: &[(String, String)],
    starred: &HashSet<String>,
    fired: &HashSet<String>,
    final_seen_at: &mut std::collections::HashMap<String, i64>,
    rules: &Rules,
    now: i64,
) -> Vec<Alert> {
    let mut out = Vec::new();
    for g in games {
        let Some(why) = reason(g, teams, starred) else { continue };

        if rules.soon_s > 0 && g.status == GameStatus::Scheduled {
            if let Some(start) = g.start {
                let left = start - now;
                let k = key(AlertKind::Soon, g);
                if left > 0 && left <= rules.soon_s && !fired.contains(&k) {
                    out.push(Alert { kind: AlertKind::Soon, game: g.clone(), reason: why.clone(), key: k });
                }
            }
        }

        if rules.final_on && g.status == GameStatus::Final {
            let seen = *final_seen_at.entry(g.id.clone()).or_insert(now);
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
- **First poll after startup:** seed `final_seen_at` from the DB's own "final at"/"updated at" column if F7 has one. Otherwise seed every already-final game with `now − final_grace_s − 1`, so games that finished before tvdash started never alert.
- **Drop stale queue items** before showing: a Soon alert whose game has started, or a Final older than the grace window.

### 5.3 Queue (UI thread)

```rust
pub struct AlertQueue { items: std::collections::VecDeque<Alert>, showing: Option<Alert>, held: bool }
// push(alert)        – ignore if the key is already queued or showing
// next(now) -> Option<Alert> – None while held, while another card has priority (§2), or <3 s after the last close
// close(Action)      – Primary | Dismiss | Timeout
```

### 5.4 Unit tests

- [ ] Followed team, scheduled, start in 14 min, soon = 15 → one Soon alert, reason Team.
- [ ] Same game, start in 16 min → none. Start already passed (status still Scheduled) → none.
- [ ] Soon key already fired → none. Start time moved 1 h later → new key, alerts again inside the new window.
- [ ] Time TBD (`start = None`) or Postponed → no Soon alert.
- [ ] Status turns Final on this poll → Final alert. Next poll → none (fired).
- [ ] First poll after startup with a game already Final (no DB timestamp) → none.
- [ ] Starred game only → reason Starred. Starred **and** followed team → reason Team.
- [ ] Game with no followed team and not starred → never alerts.
- [ ] `final = false` → no Final alerts; `soon_minutes = 0` → no Soon alerts.
- [ ] Two games starting at 11:00 and 11:05 → both queued, earlier first.
- [ ] Status mapping: every value from F5 maps to a `GameStatus` (table test).

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
- [ ] **F8 freshness:** if the DB isn't updated during games, is a ~15–30 min late Final alert acceptable, or do we need another source for status?
- [ ] **Team scope:** does following "Kansas" mean every Kansas sport, or should a followed team carry a sport filter (e.g. Kansas · Football + Men's Basketball)? Spec v1: every sport.

---

## 10. Milestones

| M | Goal | Done when |
|---|---|---|
| A0 | Findings | §0 table filled and committed; stop conditions checked |
| A1 | Logic | `sports_alerts.rs` with §5.4 tests passing; status mapping covers every F5 value |
| A2 | Card in tvdash | `--alert-test soon/final` shows the card over Home, the HUD and the screensaver; focus trap, timeout, Back, priority rules |
| A3 | Real alerts | Watcher polls followed/starred games; Soon and Final fire once each; fired keys survive a restart |
| A4 | Following UI | Settings › Sports (teams, rules, starred), ★ on Competitions and Scoreboard, in Blades, Glass and Classic |
| A5 | Everywhere | `/alerts` + pill card over Brave; Meet hold; `during_video = hold`; Diagnostics line |

## 11. Done when

- [ ] Following Kansas, a Kansas game shows **Starting in 15 min** once, ~15 min before start, over whatever is on the TV.
- [ ] **Watch on YouTube TV** opens YouTube TV like its Services tile; **Dismiss**, Back and the 12 s timeout all close the card and return focus.
- [ ] When the game goes final, a **Final** card shows the score with the winner in bold, once; **Recap ↗** opens the link in the TV browser (or the card has only Dismiss).
- [ ] A starred game from another team alerts the same way, with "Starred game".
- [ ] Restarting tvdash never repeats an alert, and never alerts for games that finished before it started.
- [ ] The Sleep warning and Up Next card always go first; the sports card waits.
- [ ] Nothing shows during a Google Meet call; relevant alerts show after it.
- [ ] Works by remote alone in all three Dashboard UIs.

## 12. Change log

| Date | Change |
|---|---|
| 2026-11-21 | Spec written. Chosen: Concept 2 (Action card); follow teams + starred games; triggers Starting soon + Final; shown everywhere. §0 findings pending from the repo |
