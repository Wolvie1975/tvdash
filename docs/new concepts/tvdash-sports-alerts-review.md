# Review: `tvdash` — Sports Alerts Build Spec (2026-11-21)

Review of **Section 0 (F1–F11)** and its downstream impact on the spec, verified against the live `WebScraper` SQL Server database, `My-Newsfeed`, and `Rss-Rust-Scrape` (`scoreboard.rs`).

> **Scope note:** The `tvdash` repository is not on this machine, so **F1, F9, F10**, and `tvdash`'s own Competitions/Scoreboard query layer (**F2** on the `tvdash` side) remain to be filled in from `tvdash` itself. Everything below covers the database, scraper, and schema side.

---

## 1. Section 0 Findings (`WebScraper` DB & `Rss-Rust-Scrape`)

| # | Spec Question | Findings from `WebScraper` / `Rss-Rust-Scrape` |
| --- | --- | --- |
| **F1** | Crate/module & DB driver | **Pending in `tvdash` repo.** (For reference, [`events-page-rust-spec.md`](events-page-rust-spec.md) uses `tiberius` + `bb8-tiberius` / `deadpool-tiberius` against SQL Server.) |
| **F2** | Tables/views feeding Competitions & Scoreboard | Two separate sources in `WebScraper`:<br>- **Competitions (`/events`):** `dbo.SportsEvents` (+ `SportsEventsType`, `SportsEventSources`) — schedule & links only, **no scores or final status**.<br>- **Scoreboard (`/scoreboard`):** `dbo.SportsScoreboard` (view over `SportsScoreboardGames`) — live/final status, scores, and `EventId`.<br>*(Verify in `tvdash` which of these feeds its Competitions and Scoreboard screens.)* |
| **F3** | Stable game ID column | On `dbo.SportsScoreboard`: `EventId` (`varchar`, ESPN event ID, e.g. `401921418`). Stable across reschedules and score updates.<br>On `dbo.SportsEvents`: `ID` (`int` PK), which is **not** the ESPN `EventId`. |
| **F4** | Start time & Time TBD | `StartsAtUtc` (`datetime2`, stored in **UTC**). `TimeTbd` is a `bit` (`0`/`1`; `StartsAtUtc` is `NULL` when `TimeTbd = 1`). |
| **F5** | Status column & exact values | On `dbo.SportsScoreboard`, `Status` (from `normalized_status` in `Rss-Rust-Scrape/src/scoreboard.rs`) has the exact set:<br>`upcoming`, `live`, `final`, `postponed`, `cancelled` *(double-L)*, `suspended`, `delayed`, `unknown`.<br>Also has `Completed` (`bit`), `ProviderStatus` (e.g. `STATUS_SCHEDULED`, `STATUS_IN_PROGRESS`, `STATUS_FIRST_HALF`, `STATUS_FINAL`, `STATUS_FINAL_OT`, `STATUS_FULL_TIME`), and `StatusText` (`"Final"`, `"Final/OT"`, etc.).<br>**Stop condition cleared:** `final` is reliably populated. |
| **F6** | Team IDs, names & neutral flag | `HomeTeamId`, `AwayTeamId` (`varchar`, ESPN team IDs), `HomeName`, `AwayName`, `NeutralSite` (`bit`).<br>**Important:** `HomeTeamId`/`AwayTeamId` are **not globally unique across sports** (see §2). |
| **F7** | Scores, league, venue, broadcast, links | `HomeScore`, `AwayScore`, `Sport`, `League`, `Venue`, `Broadcasts`, `SourceUrl` (ESPN game URL).<br>**Missing vs. spec:** `SportsScoreboard` has **no `city` column** (`Venue` is stadium name only, e.g. `"TD Arena"`), **no separate `recap_url`** (only `SourceUrl`), and **no `final_at` column** (only `LastObservedAtUtc`). |
| **F8** | Writer & refresh frequency | `web-scoreboard` container runs `scoreboard --watch --live-minutes 10 --lookback-days 1 --lookahead-days 7 --retention-days 1`. Updates `dbo.SportsScoreboard` every **10 minutes** (verified live: `Max(LastObservedAtUtc)` was within 14 s of current UTC).<br>**Stop condition cleared:** updates live and final during games (up to ~10 min latency). |
| **F9** | `tvdash` poll interval, `AppEvent`, tokio | **Pending in `tvdash` repo.** |
| **F10** | DB login / keyring / config | **Pending in `tvdash` repo.** |
| **F11** | Teams table vs. `SELECT DISTINCT` | **No master `Teams` table** in `WebScraper` (only `ResultFollowTeams` and `SportsRankingTeams`). Team list must come from `SELECT DISTINCT`, and `SportsScoreboard` only keeps a `-1..+7` day window. |

---

## 2. Spec Issues to Resolve Before Coding

### A. ESPN Team IDs collide across sports (blocks §3.3, §4.1, §5.2, §9)
Querying `dbo.SportsScoreboard` directly confirms that `HomeTeamId` / `AwayTeamId` are only unique within a sport/league:

| `TeamId` | Teams sharing that ID in `dbo.SportsScoreboard` |
| --- | --- |
| `12` | Kansas City Chiefs (`football`), Arizona Wildcats (`basketball`, `volleyball`), New York Islanders (`hockey`) |
| `333` | Alabama Crimson Tide (`football`, `basketball`), Bristol City (`soccer`) |
| `2` | Buffalo Bills (`football`), Buffalo Sabres (`hockey`) |
| `197` | Oklahoma State Cowboys (`basketball`), Oklahoma State Cowgirls (`basketball`) |

If `config.toml` stores bare IDs (`teams = ["12"]`) and §4.1 runs `WHERE home_id IN (@teams) OR away_id IN (@teams)`, following the **Chiefs** will fire alerts for **Arizona** and the **Islanders**.

**Recommendation:**
* Store team follows as `(sport, team_id)` — or `(league, team_id)` if men's/women's basketball (`197`) should be distinct — and include `Sport`/`League` in the SQL `WHERE` clause and `reason()`.
* This also resolves **§9 Open Question #3** (team scope).

### B. Competitions vs. Scoreboard IDs and 7-day window (§3.2)
* `dbo.SportsEvents` (Competitions) uses `ID` (`int`) and `SchoolId`, and has **no scores or final state**.
* `dbo.SportsScoreboard` (Scoreboard) uses `EventId` (`varchar`) and `HomeTeamId`, and only keeps games **`-1` to `+7` days** (`--lookback-days 1 --lookahead-days 7`).
* **Recommendation:** Verify in `tvdash` whether its Competitions page already reads `SportsScoreboard` or `SportsEvents`. If it reads `SportsEvents`, starring a game on Competitions won't match `SportsScoreboard.EventId` directly, and games starred >7 days out won't be in `SportsScoreboard` yet.

### C. Minor field & enum mismatches
* **`cancelled` spelling (§5.1 / §5.4):** `Rss-Rust-Scrape` writes `"cancelled"` (two `l`s), plus `"suspended"` and `"delayed"`. Make sure the `GameStatus` mapper handles `"cancelled"` (and maps `"suspended"` / `"delayed"` appropriately).
* **No `city` column on `SportsScoreboard` (§2 Row 4):** `Venue` has the stadium name only; omit `city` or pull it from `SportsEvents.Location` if needed.
* **`Recap ↗` URL (§1 / §2 / §4.1):** `SportsScoreboard` only has `SourceUrl` (ESPN game link), not a separate `recap_url`.
* **Startup `final_seen_at` (§5.2):** `SportsScoreboard.LastObservedAtUtc` refreshes every 10 minutes even after a game is final, so it is **not** a `final_at` timestamp. Use the spec's fallback (`now - final_grace_s - 1` on first poll) so games that finished before `tvdash` started never alert.

---

## 3. Next Steps
1. Open the `tvdash` repo and fill in **F1**, **F2** (which table/view `tvdash` uses for Competitions), **F9**, and **F10**.
2. Update §3.3 / §4.1 / §5.2 to key followed teams by `(sport, team_id)` or `(league, team_id)`.
3. Align §2 / §4.1 / §5.4 with `SportsScoreboard` columns (`SourceUrl`, `Venue` without `city`, `"cancelled"` / `"suspended"` / `"delayed"`, and `N = 10 min` for F8).
