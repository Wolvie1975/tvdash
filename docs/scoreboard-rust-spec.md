# Scoreboard in Rust: starting spec

A rebuild of the myNewspaper **Scoreboard** page (`http://192.168.1.100:8085/scoreboard`) as part of the separate Rust
app that already has the Events page ([events-page-rust-spec.md](events-page-rust-spec.md)). Written November 19, 2026
from the myNewspaper code at commit `da02bfc`.

**What's involved, in short:** the scraper already collects the scores and writes them to SQL Server. The Rust app
**only reads** three views and two small tables, filters and sorts in a few documented ways, and renders cards. It
never calls ESPN and never writes to the database. Live-ness comes from re-reading the database every five minutes.

Reuse from the Events spec: the connection setup (section 1), safe links, logo `http`→`https`, initials badges and
Central-time formatting.

Source of truth in the .NET app, if behaviour is unclear:

| What | File |
| --- | --- |
| Page shell | `src/myNewspaper.Web/Components/Pages/Scoreboard.razor` |
| Everything on the page (toolbar, cards, phone layout) | `src/myNewspaper.Web/Components/ScoreboardPanel.razor` |
| The read, selection rules, game model | `src/myNewspaper.Shared/Data/Scoreboard.cs` (`ScoreboardQuery.ReadAsync`, `ScoreboardSelection.Resolve`, `ScoreboardGame`, `ScoreboardFollow.Matches`) |
| Day window | `src/myNewspaper.Shared/Data/ResultsQuery.cs` (`ResultsWindow.ForDay`) |
| Follow labels, league names | `src/myNewspaper.Shared/Scores/EspnClient.cs` (`EspnLeagues`, `FollowLabel`, `FollowKinds`) |
| Phone text (matchup, score, clock) | `src/myNewspaper.Web/Components/Phone/PhoneText.cs` |
| Auto-refresh, stars, Cards/List switch | `src/myNewspaper.Web/wwwroot/scoreboard.js` |
| Refresh endpoint | `src/myNewspaper.Web/Endpoints/PublicEndpoints.cs` (`/scoreboard/fragment`) |
| Product notes | `docs/sports.md`, "Live scoreboard" and "Consolidated daily results" |

## Checklist

- [ ] Read coverage, follows and follow members
- [ ] Resolve the `show` selection
- [ ] Read the day's games (SQL), apply the follow filter, de-duplicate and sort
- [ ] Date navigation (yesterday to 7 days ahead)
- [ ] Show and Games filters
- [ ] Status groups: Live, Upcoming, Final
- [ ] Game card
- [ ] Top 25 view with "Upcoming ranked games this week"
- [ ] Warnings: unavailable, delayed coverage, stale scores
- [ ] Refresh every five minutes without losing open details or focus
- [ ] Phone layout
- [ ] Compare with the live `/scoreboard` for several selections
- [ ] Mobile checks (320, 390, 430px; light and dark)
- [ ] Later: team records, box scores, starred teams, Top 25 rail

## 1. Data

All in schema `dbo`. All times are UTC `datetime2`. The three `SportsScoreboard*` objects are **views owned by the
scraper**; read them as they are. `ResultFollows` and `ResultFollowTeams` are myNewspaper tables managed in Admin.

The read-only login from the Events spec (`db_datareader` on `WebScraper`) covers all of them.

### `SportsScoreboard` (view): one row per game

Columns the page uses:

| Column | Type | Notes |
| --- | --- | --- |
| `ID` | bigint | |
| `Provider` | varchar(16) | `espn` today |
| `Sport` | nvarchar(80) | `football`, `basketball`, `baseball`, `hockey`, `soccer`, `volleyball` |
| `League` | nvarchar(120) | ESPN league slug: `nfl`, `college-football`, `mens-college-basketball`, `usa.1`, `eng.1`, … |
| `EventId` | nvarchar(160) | ESPN game id |
| `ScoreboardDate` | date | The game's Central calendar date |
| `StartsAtUtc` | datetime2, null | |
| `TimeTbd` | bit | |
| `WasRescheduled` | bit | |
| `HomeTeamId`, `AwayTeamId` | nvarchar(160), null | ESPN team ids |
| `HomeName`, `AwayName` | nvarchar(400) | |
| `HomeLogoUrl`, `AwayLogoUrl` | nvarchar(4096), null | |
| `HomeScore`, `AwayScore` | decimal, null | **Null = no score yet. 0 is a real zero.** |
| `Status` | varchar(12) | `upcoming`, `live`, `final`, `delayed`, `suspended`, `postponed`, `cancelled`, or something else (show "Status unavailable") |
| `Completed` | bit | |
| `StatusText` | nvarchar(600), null | ESPN's own text: "Final/OT", "3rd Set", "8:00 - 4th Quarter", "Halftime" |
| `Period` | int, null | |
| `DisplayClock` | nvarchar(160), null | "8:00", "63'" |
| `NeutralSite` | bit, null | |
| `HomeShootout`, `AwayShootout` | int, null | Soccer/hockey shootouts |
| `WinnerSide` | varchar(4), null | `home`, `away` or null |
| `Venue` | nvarchar(600), null | |
| `Broadcasts` | nvarchar(2000), null | Comma-separated networks |
| `SourceUrl` | nvarchar(4096), null | ESPN game page |
| `LiveDetailsJson` | nvarchar(max), null | Baseball count/bases and per-period scores (section 6) |
| `LastObservedAtUtc` | datetime2 | When the scraper last sampled this game |
| `IsStale` | bit, null | The scraper's own staleness flag |
| `RequestAvailability` | varchar(16), null | `available`, `empty`, `partial`, `error` for the last collection |
| `HasBoxScore` | bit, null | A cached box score exists (section 10) |
| `HomeRank`, `AwayRank` | tinyint, null | AP Top 25 rank (college football and men's basketball) |

The view has more columns (`ProviderDate`, `OriginalStartsAtUtc`, `FinalFirstObservedAtUtc`, box-score URLs, request
timings) that the page doesn't use.

### `SportsScoreboardTop25Week` (view)

Same columns as `SportsScoreboard`. The scraper fills it with ranked matchups for the rest of the week, excluding
Big 12 games. Used only by the Top 25 view.

### `SportsScoreboardCoverage` (view): which leagues are collected

| Column | Type | Notes |
| --- | --- | --- |
| `ID` | int | |
| `SourceKind` | varchar(12) | `follow`, `events`, `rankings` |
| `Sport`, `League` | nvarchar | Together they're the competition key `"{Sport}\|{League}"` |
| `Label` | nvarchar(400) | |
| `LastSuccessAt` | datetime2, null | Null = never collected successfully |
| `Availability` | varchar(16) | `available`, `empty`, `partial`, `error` |

The same league can appear more than once (for example MLS from a follow and from events). De-duplicate by key.

### `ResultFollows` and `ResultFollowTeams`

| `ResultFollows` column | Type | Notes |
| --- | --- | --- |
| `ID` | int | |
| `Kind` | nvarchar(20) | `league`, `conference` or `team` |
| `Sport`, `League` | nvarchar | Same values as the scoreboard |
| `TeamId` | nvarchar(40), null | ESPN team id, for `team` follows |
| `Name` | nvarchar(400) | "Big 12", "Kansas Jayhawks", "NFL" |
| `Enabled` | bit | |

`ResultFollowTeams(FollowId, TeamId)` lists the ESPN team ids that belong to a `conference` follow.

## 2. Lookups (run on every page load)

```sql
-- Competitions with coverage (women's college basketball is always excluded from the scoreboard)
SELECT ID, SourceKind, Sport, League, Label, LastSuccessAt, Availability
FROM dbo.SportsScoreboardCoverage
WHERE League <> 'womens-college-basketball';

-- Follows that have coverage: leagues, then conferences, then teams
SELECT f.ID, f.Kind, f.Sport, f.League, f.TeamId, f.Name
FROM dbo.ResultFollows f
WHERE f.Enabled = 1 AND f.League <> 'womens-college-basketball'
  AND EXISTS (SELECT 1 FROM dbo.SportsScoreboardCoverage c
              WHERE c.Sport = f.Sport AND c.League = f.League AND c.League <> 'womens-college-basketball')
ORDER BY CASE f.Kind WHEN 'league' THEN 0 WHEN 'conference' THEN 1 ELSE 2 END, f.Name, f.League;

-- Conference members
SELECT t.FollowId, t.TeamId
FROM dbo.ResultFollowTeams t
JOIN dbo.ResultFollows f ON f.ID = t.FollowId
WHERE f.Enabled = 1 AND f.League <> 'womens-college-basketball';
```

**Follow label:** `league` follows use `Name` ("NFL"). Others use `"{Name} · {short league name}"`, for example
"Big 12 · Men's Basketball" or "Kansas Jayhawks · Football". Short names:

| Sport / League | Short |
| --- | --- |
| football / nfl | NFL |
| football / college-football | Football |
| baseball / mlb | MLB |
| basketball / nba | NBA |
| basketball / wnba | WNBA |
| basketball / mens-college-basketball | Men's Basketball |
| volleyball / womens-college-volleyball | Volleyball |
| hockey / nhl | NHL |
| soccer / usa.ncaa.w.1 | Women's Soccer |
| soccer / usa.1 | MLS |
| soccer / usa.nwsl | NWSL |
| soccer / eng.1 | Premier League |
| anything else | the league slug |

## 3. Query string and selection

| Parameter | Values | Default |
| --- | --- | --- |
| `day` | `yyyy-MM-dd`, from **yesterday** to **today + 7** (Central) | today; out-of-range or invalid → today |
| `show` | see below | empty = All sports |
| `status` | `final` or empty | empty = all statuses |

`show` values:

| Value | Meaning | Valid when |
| --- | --- | --- |
| *(empty)* | All sports | always |
| `top25` | Ranked college football and men's basketball games (any case) | always |
| `c:{Sport}\|{League}` | One competition, for example `c:hockey\|nhl` | the key is in coverage |
| `f:{FollowId}` | One follow. A `league` follow turns into its `c:` value; a `team` or `conference` follow stays `f:` and also selects its competition | the follow is in the lookup |
| `g:{Name}` | Every `conference` follow with that name (case-insensitive): "Big 12 · All sports" | at least one conference follow has that name |

Anything invalid falls back to All sports. (The live site also accepts old `competition=` and `follow=` links; you
can skip them.)

## 4. Reading the day's games

**Day window:** Central midnight to the next Central midnight, converted to UTC separately for each end (so daylight
saving days are 23 or 25 hours). In Rust, use `chrono-tz` `America/Chicago`; or in SQL:

```sql
DECLARE @day date = @P1;                                  -- the selected day
DECLARE @carryOver bit = @P2;                             -- 1 only when @day is today
DECLARE @startUtc datetime2 = CAST(CAST(@day AS datetime2) AT TIME ZONE 'Central Standard Time' AT TIME ZONE 'UTC' AS datetime2);
DECLARE @endUtc   datetime2 = CAST(CAST(DATEADD(day, 1, @day) AS datetime2) AT TIME ZONE 'Central Standard Time' AT TIME ZONE 'UTC' AS datetime2);

SELECT g.ID, g.Provider, g.Sport, g.League, g.EventId, g.ScoreboardDate, g.StartsAtUtc, g.TimeTbd, g.WasRescheduled,
       g.HomeTeamId, g.AwayTeamId, g.HomeName, g.AwayName, g.HomeLogoUrl, g.AwayLogoUrl, g.HomeScore, g.AwayScore,
       g.Status, g.Completed, g.ProviderStatus, g.StatusText, g.Period, g.DisplayClock, g.NeutralSite,
       g.HomeShootout, g.AwayShootout, g.WinnerSide, g.Venue, g.Broadcasts, g.SourceUrl, g.LiveDetailsJson,
       g.LastObservedAtUtc, g.IsStale, g.RequestAvailability, g.HasBoxScore, g.HomeRank, g.AwayRank
FROM dbo.SportsScoreboard g
WHERE (   (g.ScoreboardDate = @day AND (g.TimeTbd = 1 OR (g.StartsAtUtc >= @startUtc AND g.StartsAtUtc < @endUtc)))
       -- Today also shows older games that are still live, delayed or suspended
       OR (@carryOver = 1 AND g.ScoreboardDate < @day AND g.Status IN ('live','delayed','suspended')))
  AND g.League <> 'womens-college-basketball'
  /* + selection filter, below */
  /* + AND g.Status = 'final' AND g.Completed = 1   when status=final */
;
```

**Selection filter, added to the SQL:**

| Selection | Add |
| --- | --- |
| All sports | nothing |
| `top25` | `AND g.League IN ('college-football','mens-college-basketball') AND (g.HomeRank IS NOT NULL OR g.AwayRank IS NOT NULL)` |
| `c:` or `f:` | `AND g.Sport = @sport AND g.League = @league` (the follow's competition for `f:`) |
| `g:` | nothing in SQL; filtered in Rust below |

Pass every value as a parameter (`@P3`, …), never by string-building.

**Then in Rust, in this order:**

1. **Follow filter** for `f:` (that follow) or `g:` (keep a game if *any* of the group's follows matches). A follow
   matches a game when `Provider == "espn"`, `Sport` and `League` are equal, and:
   - `league`: always;
   - `team`: `TeamId` is not empty and equals `HomeTeamId` or `AwayTeamId`;
   - `conference`: `HomeTeamId` or `AwayTeamId` is in the follow's members.

   Match on team ids only, never names.
2. **De-duplicate** by `Provider|Sport|League|EventId`, keeping the first.
3. **Sort:** `live` first, then `delayed`/`suspended`, then `upcoming`, then everything else; then `StartsAtUtc`
   (no start time first); then `ID`.

**Top 25 extra list** (only for `show=top25`):

```sql
SELECT /* same columns */ FROM dbo.SportsScoreboardTop25Week ORDER BY ScoreboardDate, StartsAtUtc, ID;
```

**If any read fails**, don't show an error page: show "Scores are temporarily unavailable. Please retry." and keep the
rest of the page working.

> **Checked November 19, 2026** against the site's own `ScoreboardQuery.ReadAsync` on the live database, with a
> reference script that follows sections 2–4 exactly: identical games in identical order for all sports
> (yesterday 16, today 24, tomorrow 24), `c:hockey|nhl` (12), `f:65` Big 12 men's basketball (3), `f:6` Kansas men's
> basketball (1), `g:Big 12` (6), `top25` (5, plus 31 in the weekly list) and `status=final` (3). The follow labels in
> section 2 also matched.

## 5. Page layout (wide screens)

1. Heading: eyebrow "Around the leagues", title **Scoreboard**.
2. **Toolbar**
   - Date navigation: "← {Mon}" (hidden on yesterday), the selected date as "Wednesday, November 19", "{Fri} →"
     (hidden on today + 7), and a **Today** link when another day is selected. Links keep `show` and `status`.
   - **Show** select: "All sports", "Top 25", then option groups:
     - **Leagues:** each coverage key once, sorted by sport then league, named by the competition label (below).
     - **Conferences:** for each conference name used by two or more follows, a "{Name} · All sports" option
       (`g:{Name}`) first, then each conference follow (`f:{Id}`).
     - **Teams:** each team follow (`f:{Id}`).
   - **Games** select: "All statuses" / "Final scores" (`status=final`).
   - The selects submit on change. Add an **Apply** button inside `<noscript>`.
3. **Warnings** (see section 7).
4. **Empty state** when no games:
   - Top 25: "No ranked games are scheduled for this date."
   - No coverage at all: "No scoreboard coverage is currently enabled."
   - Otherwise: "No games are currently stored for this day and selection. Coverage may still be updating."
5. **Status groups**, each with a heading and "{n} game(s)":
   - **Live:** `live`, `delayed`, `suspended`. Sort by `Period` descending, then `DisplayClock` as `m:ss` ascending
     (unparseable last). Show a pulsing dot.
   - **Upcoming:** `upcoming`, by `StartsAtUtc` (no start time last).
   - **Final:** `final`, most recently observed first (`LastObservedAtUtc` descending).
   - Games with other statuses (`postponed`, `cancelled`, unknown) are not shown in any group on the wide layout.
6. For `top25`: a section "Upcoming ranked games this week" with the note "Ranked matchups across the country this
   week (excluding Big 12 games)." and its own cards, or "No remaining ranked games this week."
7. A notes popover (ⓘ): "Scores update approximately every 10 minutes. Checks every five minutes while visible." and
   "Sampled clocks stay fixed between updates. Coverage excludes F1 and unsupported leagues."

Cards sit in a grid of up to three columns.

**Competition label** (used on cards and in Leagues):

| League | Label |
| --- | --- |
| nfl | NFL |
| mlb | MLB |
| nhl | NHL |
| wnba | WNBA |
| usa.1 | MLS |
| usa.nwsl | NWSL |
| eng.1 | Premier League |
| eng.2 | EFL Championship |
| college-football | College football |
| mens-college-basketball | Men's college basketball |
| usa.ncaa.w.1 | Women's college soccer |
| womens-college-volleyball | Women's college volleyball |
| anything else | "{Sport} · {League}" |

## 6. Game card

**Header:**

| Game | Shows |
| --- | --- |
| Live, delayed or suspended | A "live" pill with the clock: if `DisplayClock` is set, `"{period} · {clock}"` where period 1–4 is 1st/2nd/3rd/4th and 5+ is OT1, OT2, …; otherwise `StatusText`, otherwise "Live". |
| Upcoming | Start time `7:00 P.M. CT` (as in Events), or "Start time TBD" when `TimeTbd` or no start; plus `Broadcasts` if set. |
| Anything else | `StatusText`, or "Final". |

**Teams:** away on the first line, home on the second. Each line:

- Logo badge (safe URL, `http`→`https`; initials if missing or broken).
- `#{rank} ` before the name when the rank is 1–25.
- The name.
- The score, only if not null. Format the decimal without trailing zeros: `3`, `2.5`. Null shows nothing on the
  card (and "—" where a score must appear, such as the phone's "13–10").
- **Winner / loser emphasis:**
  - Final and `Completed`: the `WinnerSide` team wins. If `WinnerSide` is null, the higher score wins.
  - Live: the team ahead is "leading".
  - Add hidden text ", winner" or ", leading" for screen readers.

**Game info** (a native `<details>`, shown only when any of these exist):

- Details from `LiveDetailsJson` (below).
- `Venue`, plus "· Neutral site" when `NeutralSite`.
- `Broadcasts`.

**Footer**, final games only: **Recap ↗** to `SourceUrl` (safe link, new tab).

The whole card also links to `SourceUrl` in a new tab, with an accessible label like
"Kansas 78, Duke 74, Final" or "Kansas vs Duke, 7:00 P.M. CT".

**`LiveDetailsJson`:** a JSON object. Ignore it if it isn't valid JSON or isn't an object. Build parts in order and
join them with " · ":

- Baseball only:
  - `outs`, `balls`, `strikes` (integers) → "1 outs", "2 balls", "1 strikes".
  - `onFirst`, `onSecond`, `onThird` (true) → "1st occupied", "2nd occupied", "3rd occupied".
- `awayPeriods`, then `homePeriods`: arrays of objects. Take up to 40, using each one's `displayValue` (string)
  or else `score` (number). The result reads "Away periods: 21 · 20 · 5".

Example from today's data:
`{"awayPeriods":[{"displayValue":"21","period":1,"score":21.0},…],"city":"Provo","homePeriods":[…]}`.

Shootouts: when either side has one, show "Shootout · Away {n} – Home {n}" ("—" for a missing side).

Games from an earlier day shown under today (carry-over) add "Continuing from {Wednesday, November 18, 2026}".

## 7. Freshness and warnings

- **Coverage warning:** "Some coverage is delayed or unavailable. Showing the latest stored scores." Show it when any
  relevant coverage row has `Availability` `error` or `partial`, or `LastSuccessAt` null. "Relevant" means:
  - for `g:`, the leagues of that group's follows;
  - for `c:`/`f:`, the selected competition;
  - otherwise all coverage.
- **Stale game:** `IsStale = 1`, or a live, delayed or suspended game not observed for more than 25 minutes.
  Show "Scores may be outdated — last updated {time}" instead of "Updated {time}".
- **Collection problem:** `RequestAvailability` `error` → "Latest collection is unavailable; showing the latest
  stored sample." `partial` → "…is incomplete…".
- Live clocks are a **sample** from the last collection, not a running clock. Don't tick them in the browser.

## 8. Refresh

The scraper updates scores about every 10 minutes; the page re-reads the database, never ESPN.

- Every **5 minutes**, and immediately when the tab becomes visible again, fetch a fragment of the same page with the
  same `day`, `show` and `status`, and replace the game area. The live site uses `GET /scoreboard/fragment?…` with
  `Cache-Control: no-store`.
- Skip a refresh while the tab is hidden, while a dialog is open, or while focus is inside the scoreboard. Use a
  30-second timeout.
- Keep any open "Game info" `<details>` open after the swap (match cards by `Provider|Sport|League|EventId`).
- If a refresh fails, keep the current scores and show a short retry message.
- Without JavaScript the page still works: dates and filters are links and forms, and `<details>` opens natively.

With axum, one handler can render either the full page or just the fragment (for example from a `fragment=1`
parameter).

## 9. Phone layout (below 1024px)

The live site shows a separate phone version:

- **Ribbon:** title "Scoreboard", a summary line, and a date stepper with ‹ › buttons and "Today · Wed, Nov 19"
  ("Yesterday · …", "Tomorrow · …"), plus a "Back to today" link on other days.
  - Summary line on today: "3 live now · 5 more today", or "5 games still to come today", or "24 games today".
  - On other days: "16 games · Yesterday · Tue, Nov 18".
- One **Sport** select with the same Show options.
- **Live now:** up to 8 compact score tiles, plus "{n} more live in the leagues below."
- **Up next:** the next 5 upcoming games. Each shows its time (`h:mm AM`), matchup, competition and first network
  (text before the first `,` `/` or `;`), plus a "Full schedule" link to Events.
- **By league:** one collapsible section per competition, in the order games appear. The league with the most live
  games starts open. Its summary reads, for example, "5 games · 4 live · Leeds United at Sunderland 11:30 AM",
  "all final" or "3 final". Each row shows:
  - the matchup: "#5 Away at Home", or "vs" at a neutral site;
  - the status: clock or time, plus network when upcoming;
  - the score "away–home", once live or final or any score exists.
  - The row links to `SourceUrl`.

A simpler responsive version of the card grid (one column on phones) is a fine first step. Keep the 44px touch
targets and the no-horizontal-scroll rule either way.

## 10. Left for later

- **Team records** beside names (`TeamRecords.LoadAsync`, hidden in playoffs). Shared with the Events spec's
  "later" list; build it once for both.
- **Box scores.** `HasBoxScore = 1` means a cached box score exists in `dbo.SportsBoxScores`, keyed by
  `(Sport, League, EventId)`. Payloads are versioned JSON with sport-specific layouts (`docs/sports.md`, "Cached box
  scores"). It's a sizeable piece of work on its own; until then, link to `SourceUrl`.
- **Starred teams / Your teams.** Stored only in the browser (`localStorage` key `mydailyfeed-scoreboard-teams-v1`,
  values `"{Sport}|{League}|{TeamId}"`). They pin those games into a "Your teams" section at the top. There's no
  server data.
- **Cards / List switch** (`localStorage` `mydailyfeed-scoreboard-view`).
- **Top 25 side rail** (the AP poll with today's result for each team), from the rankings tables.
- **Season results links** to MLS and EFL Championship pages.
- **Home page card** (up to six games, live first, with a sport filter).

## 11. Checking it matches

1. Open the live `/scoreboard` and the Rust page at the same moment (scores move every 10 minutes).
2. Compare counts per status group and the game order for:
   - All sports today, yesterday and tomorrow;
   - one league (`c:hockey|nhl`);
   - a conference (`f:65`);
   - a team (`f:6`);
   - `g:Big 12`, `top25` and `status=final`.
3. Check a live game's clock text, a final with `WinnerSide`, a game with no score, a TBD time, a neutral site, a
   shootout and a baseball game's Game info (when in season).
4. Turn off the network briefly and confirm the page keeps its scores and shows the retry message.
5. Mobile: 320, 390 and 430px in light and dark mode. Check no sideways scrolling, readable selects, the date stepper,
   collapsible leagues, and touch targets of at least 44px.
