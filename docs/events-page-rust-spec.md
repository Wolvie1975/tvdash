# Events page in Rust: starting spec

A rebuild of the myNewspaper **Competitions** page (`http://192.168.1.100:8085/events`) as a separate Rust app that
reads the same SQL Server database. Written November 19, 2026 from the myNewspaper code at commit `da02bfc`.

The page is **read-only**: it never writes to the database. The scraper fills the tables; this app only displays them.

Source of truth in the .NET app, if behaviour is unclear:

| What | File |
| --- | --- |
| Page, filters, paging | `src/myNewspaper.Web/Components/Pages/Events.razor` |
| Queries and text helpers | `src/myNewspaper.Shared/Data/EventsQuery.cs` (`Visible`, `GetUpcomingAsync`, `GetScheduleChoicesAsync`, `EventsText`) |
| League grouping | `src/myNewspaper.Shared/Data/GameGroups.cs` (`ScheduleLeague`, `Schedule`) |
| One game row | `src/myNewspaper.Web/Components/Events/GameRow.razor` |
| Time formatting | `src/myNewspaper.Shared/Data/FeedDates.cs` (`EventTime`, `LongDate`, `PublishedFull`) |

## Checklist

- [ ] Connect to SQL Server with a read-only login
- [ ] Run the upcoming-games query and map rows
- [ ] Group by day, then league; sort within each league
- [ ] Render each game row (teams, time, location, links)
- [ ] Schedule filter dropdown
- [ ] "Schedule last updated" line
- [ ] Paging (200 per page)
- [ ] Empty states
- [ ] Compare output with the live `/events` page for the same day
- [ ] Mobile layout (320, 390, 430px; light and dark)
- [ ] Later: team records beside names
- [ ] Later: Wrexham AFC toggle and soccer league links

## 1. Connecting

| Setting | Value |
| --- | --- |
| Server, app outside Docker | `192.168.1.100`, port `1433` |
| Server, app in a container on the `dev-bridge` network | `sql2025`, port `1433` |
| Database | `WebScraper` (live) or `WebScraper_Test` (a restored copy, for development) |
| TLS | Self-signed certificate: in tiberius call `config.trust_cert()` |
| Login | A dedicated read-only login (`db_datareader`). Don't use `sa` or the myNewspaper app logins. |

Suggested crates: `tokio`, `tiberius` (with `tds73` and a TLS feature), `bb8-tiberius` or `deadpool-tiberius` for
pooling, `chrono` + `chrono-tz` for dates, and `axum` with `askama` or `maud` for HTML.

Keep the password out of source and command lines: read it from an environment variable or a git-ignored `.env`.

```rust
let mut config = tiberius::Config::new();
config.host(std::env::var("SQL_HOST")?);           // 192.168.1.100 or sql2025
config.port(1433);
config.database(std::env::var("SQL_DATABASE")?);   // WebScraper or WebScraper_Test
config.authentication(tiberius::AuthMethod::sql_server(
    std::env::var("SQL_USER")?, std::env::var("SQL_PASSWORD")?));
config.trust_cert();
```

## 2. Tables and columns used

All in schema `dbo`. Times ending `Utc`/`At` are stored as UTC `datetime2`. `EventDate` is a `date`.

**`SportsEvents`**: one row per game.

| Column | Type | Notes |
| --- | --- | --- |
| `ID` | int | Primary key |
| `Url` | nvarchar, not null | Game details page ("Details" link) |
| `Title` | nvarchar, not null | |
| `Sport` | nvarchar, null | "Football", "Men's Basketball", "Soccer", … |
| `Opponent` | nvarchar, null | |
| `IsAway` | bit, null | 1 = the school is the away team; null = unknown |
| `IsNeutral` | bit, null | 1 = neutral site |
| `Location` | nvarchar, null | "City, St. / Venue Name" |
| `EventDate` | date | The game's **local calendar date**. Never convert it. |
| `StartsAtUtc` | datetime2, null | |
| `TimeTbd` | bit | |
| `Tv` | nvarchar, null | Comma-separated networks |
| `Radio` | nvarchar(200), null | May contain HTML, including broken `<img` tags |
| `StreamUrl`, `LiveStatsUrl`, `AudioUrl` | nvarchar, null | Links |
| `TeamLogoUrl`, `OpponentLogoUrl` | nvarchar, null | The school's logo and the opponent's logo |
| `ScheduleStatus` | nvarchar(100), null | ESPN status such as `STATUS_SCHEDULED`, `STATUS_POSTPONED` |
| `ProviderKey` | varchar(50), null | `big12`, `espn-nfl`, `espn-mlb`, `espn-epl`, … |
| `SchoolName` | nvarchar(100), null | The "our team" side |
| `SchoolId`, `OpponentSchoolId` | int, null | Provider team ids |
| `SportsEventsTypeId` | int, null | FK to `SportsEventsType.ID` |
| `LastSeenAt` | datetime2 | When the scraper last saw the row |

**`SportsEventsType`**: one row per schedule feed (a school, team or league).

| Column | Type | Notes |
| --- | --- | --- |
| `ID` | int | |
| `SchoolName` | nvarchar, null | Fallback for `SportsEvents.SchoolName` |
| `ProviderKey` | varchar(50), null | Fallback for `SportsEvents.ProviderKey` |
| `ProviderSchoolId` | int, null | Which team or league the feed covers (0 = whole league) |
| `Enabled` | bit | Turned on and off in myNewspaper Admin |

**`SportsEventSources`**: links a game to every feed that reported it (one game can come from several feeds).

| Column | Type |
| --- | --- |
| `SportsEventId` | int, null (FK to `SportsEvents.ID`) |
| `SportsEventsTypeId` | int (FK to `SportsEventsType.ID`) |

## 3. Upcoming games query

The page shows **today through three days from today**. "Today" is the date in **America/Chicago**. Compute it in
Rust with `chrono-tz` and pass it as a parameter, or use the SQL below. tiberius parameters are `@P1`, `@P2`, ….

> **Checked November 19, 2026:** this SQL and the site's own `EventsQuery.GetUpcomingAsync` returned the same games in
> the same order with the same fields, on `WebScraper_Test` (105 games) and on `WebScraper` (114). The data covered
> Big 12, NFL, NHL, NWSL, EFL Championship, Premier League and MLS, plus neutral sites, unknown home/away and TBD
> times. MLB and WNBA were off-season, so their branch (`ProviderSchoolId = 0`) wasn't exercised.

```sql
DECLARE @today date = CAST(SYSDATETIMEOFFSET() AT TIME ZONE 'Central Standard Time' AS date);

SELECT e.ID, e.Url, e.Title, e.Sport, e.Opponent,
       CASE WHEN e.ProviderKey IN ('espn-championship','espn-epl','espn-mls') THEN CAST(0 AS bit) ELSE e.IsAway END AS IsAway,
       e.Location, e.EventDate, e.StartsAtUtc, e.TimeTbd, e.Tv, e.StreamUrl, e.LiveStatsUrl,
       e.TeamLogoUrl, e.OpponentLogoUrl,
       COALESCE(e.SchoolName, t.SchoolName)   AS SchoolName,
       e.IsNeutral, e.Radio, e.AudioUrl, e.ScheduleStatus,
       COALESCE(e.ProviderKey, t.ProviderKey) AS ProviderKey,
       e.SchoolId, e.OpponentSchoolId
FROM dbo.SportsEvents e
LEFT JOIN dbo.SportsEventsType t ON t.ID = e.SportsEventsTypeId
WHERE e.EventDate BETWEEN @today AND DATEADD(day, 3, @today)
  AND (e.Sport IS NULL OR e.Sport <> N'Women''s Basketball')
  AND (e.ScheduleStatus IS NULL OR (
        e.ScheduleStatus NOT IN ('STATUS_CANCELED','STATUS_CANCELLED','STATUS_UNNECESSARY','STATUS_FULL_TIME')
        AND e.ScheduleStatus NOT LIKE 'STATUS[_]FINAL%'))
  AND (
        -- Pro leagues: only when their schedule row is enabled and is the right team or league feed
        (e.ProviderKey IN ('espn-nwsl','espn-wnba','espn-mlb','espn-championship','espn-epl','espn-mls')
          AND t.Enabled = 1 AND t.ProviderKey = e.ProviderKey
          AND (   (e.ProviderKey = 'espn-nwsl' AND t.ProviderSchoolId = 20907)          -- Kansas City Current
               OR  e.ProviderKey IN ('espn-championship','espn-epl','espn-mls')
               OR (e.ProviderKey IN ('espn-wnba','espn-mlb') AND t.ProviderSchoolId = 0)))
     OR
        -- Everything else (Big 12, NFL, NHL, …): reported by an enabled feed, or has no feed links and its own
        -- schedule is enabled or missing
        ((e.ProviderKey IS NULL OR e.ProviderKey NOT IN ('espn-nwsl','espn-wnba','espn-mlb','espn-championship','espn-epl','espn-mls'))
          AND (EXISTS (SELECT 1 FROM dbo.SportsEventSources s
                       JOIN dbo.SportsEventsType f ON f.ID = s.SportsEventsTypeId
                       WHERE s.SportsEventId = e.ID AND f.Enabled = 1)
               OR (NOT EXISTS (SELECT 1 FROM dbo.SportsEventSources s WHERE s.SportsEventId = e.ID)
                   AND (t.ID IS NULL OR t.Enabled = 1))))
      )
ORDER BY e.EventDate,
         CASE WHEN e.StartsAtUtc IS NULL THEN 1 ELSE 0 END, e.StartsAtUtc,
         e.Sport, e.ID
OFFSET @offset ROWS FETCH NEXT 200 ROWS ONLY;
```

For paging, also run `SELECT COUNT(*)` with the same `FROM`/`WHERE` to get the total.

Notes:

- Rows with `Sport = 'Women''s Basketball'` are excluded, as on the current page.
- Finished and cancelled games drop off as soon as the scraper marks them. Games in progress (`STATUS_IN_PROGRESS`,
  `STATUS_END_PERIOD`, `STATUS_SECOND_HALF`, …) stay listed, and so do postponed, delayed and suspended games.
- For EFL Championship, Premier League and MLS, `IsAway` is forced to 0: those feeds are league-wide, so the stored
  "school" is just the home side.

## 4. Schedule filter (dropdown)

Query string `?schedule=<provider>|<sport>`, for example `espn-nfl|Football` or `big12|Volleyball`.

**Choices:** the same `FROM`/`WHERE` as section 3, but from today onward with no end date (checked November 19, 2026:
same 10 provider/sport pairs as `EventsQuery.GetScheduleChoicesAsync` on `WebScraper`):

```sql
DECLARE @today date = CAST(SYSDATETIMEOFFSET() AT TIME ZONE 'Central Standard Time' AS date);

SELECT DISTINCT COALESCE(e.ProviderKey, t.ProviderKey) AS Provider, e.Sport
FROM dbo.SportsEvents e
LEFT JOIN dbo.SportsEventsType t ON t.ID = e.SportsEventsTypeId
WHERE e.EventDate >= @today
  -- … then every remaining AND condition from section 3, unchanged (sport, status and the feed rules)
;
```

Value = `"{Provider or empty}|{Sport}"`. Name:

| Provider | Name |
| --- | --- |
| `espn-epl` | Premier League |
| `espn-championship` | EFL Championship |
| `espn-mls` | MLS |
| `espn-nwsl` | NWSL |
| `espn-mlb` | MLB |
| `espn-wnba` | WNBA |
| `espn-nhl` | NHL |
| `espn-nfl` | NFL |
| `big12` | Big 12 {Sport} |
| anything else | {Sport}, or "Other events" |

If any `big12` rows exist, also add **"Big 12 · All sports"** with value `big12|` and no sport.

**Order:** Big 12 entries sort together under "Big 12" with "All sports" first; everything else sorts by name, ignoring
case. The first option is "All schedules" (empty value).

**Applying a choice:** add `AND COALESCE(e.ProviderKey, t.ProviderKey, '') = @provider`, and when the choice has a sport,
`AND e.Sport = @sport`. An unknown `schedule` value is ignored (shows all).

The current page submits the form when the selection changes (`onchange`) and shows an **Apply** button inside
`<noscript>`, so it works without JavaScript. Keep that.

## 5. Page layout

Top to bottom:

1. Eyebrow "YOUR SPORTS CALENDAR", title **Competitions**, description "Games from today through the next three days,
   with start times, venues, and broadcasts."
2. Schedule dropdown (section 4).
3. Count: "{n} event" / "{n} events" (the total, not just this page).
4. "Schedule last updated {time}" from `SELECT MAX(LastSeenAt) FROM dbo.SportsEvents` (all rows, unfiltered), shown in
   Central as `Nov 19, 2026, 9:52 AM (UTC-6)`. Put the UTC value in `<time datetime="…Z">`.
5. One section per day, headed `Wednesday, November 19, 2026`.
6. Inside each day, one table per league (section 6), headed by the league name.
7. Paging (section 8).

On wide screens (from 1180px) a single header row sits above all days: Sport, Team / Away, Team / Home, Time,
Location, Links. On narrower screens each game becomes a stacked card. Each table should still have real column
headings, hidden visually, for screen readers.

**Empty states:**

- With a filter: "No events match in the next three days." plus a **Clear filters** link to `/events`.
- Without one: "No games scheduled from today through the next three days. Check back soon."

## 6. Grouping and order

1. Group by `EventDate`, earliest first.
2. Within a day, group by league name, sorted A–Z ignoring case:

   | Provider | League |
   | --- | --- |
   | `espn-championship` | EFL Championship |
   | `espn-epl` | Premier League |
   | `espn-mlb` | MLB |
   | `espn-mls` | MLS |
   | `espn-nwsl` | NWSL |
   | `espn-wnba` | WNBA |
   | `espn-nhl` | NHL |
   | `espn-nfl` | NFL |
   | `big12` + Football | College Football |
   | `big12` + Men's Basketball | Men's College Basketball |
   | `big12` + Women's Basketball | Women's College Basketball |
   | `big12` + Soccer | Women's College Soccer |
   | `big12` + Volleyball | Women's College Volleyball |
   | `big12` + other sport | {Sport}, or "Other events" |
   | anything else | {Sport}, or "Other events" |

3. Within a league: games with a known time first, by `StartsAtUtc`; then games that are `TimeTbd`, have no start
   time, or have a status label (section 7). Ties go by `ID`.

## 7. One game row

```text
school   = SchoolName trimmed, or "Our team"
opponent = Opponent trimmed, or "Opponent to be announced"
neutral  = IsNeutral == 1
away     = !neutral && IsAway == 1          // school is the away team
known    = !neutral && IsAway is not null
```

| Column | Content |
| --- | --- |
| Sport | `Sport`. If neutral, add a "Neutral site" tag. |
| Team / Away | If `away`: school + `TeamLogoUrl`; otherwise opponent + `OpponentLogoUrl`. Label "Away" when `known`, "Team" when neutral, none otherwise. |
| Team / Home | The other team. Label "Home" when `known`, "Team" when neutral, none otherwise. |
| Time | See below. Add a muted "TBD" style when the text contains "Time TBD". |
| Location | Split `Location` on the first `" / "`: city, then venue on a second line. No `" / "` → the whole string is the city. |
| Links | See below. Leave empty when there's nothing. |

**Time:**

- `ScheduleStatus` = `STATUS_POSTPONED` / `STATUS_DELAYED` / `STATUS_SUSPENDED` → "Postponed · Time TBD" /
  "Delayed · Time TBD" / "Suspended · Time TBD".
- Otherwise, `TimeTbd` = 1 or no `StartsAtUtc` → "Time TBD".
- Otherwise `StartsAtUtc` in America/Chicago as `{h}:{mm} {A.M.|P.M.} CT`, for example `7:00 P.M. CT` and
  `12:05 P.M. CT` (12-hour clock, no leading zero).

**Links**, in this order:

1. "TV / Streaming" label with `Tv`, if set.
2. "Radio" label with the cleaned `Radio` text, if any remains. To clean it: HTML-decode, remove every tag (a tag starts
   at `<` followed by a letter, `/`, `!` or `?` and ends at the next `>` outside quotes; a tag that is never closed
   runs to the end), replace each tag with a space, then collapse whitespace.
3. Buttons: **Stats** (`LiveStatsUrl`), **Video** (`StreamUrl`), **Audio** (`AudioUrl`), **Details** (`Url`), each with
   `target="_blank" rel="noopener noreferrer"`.

**Safe links:** only show a URL if it parses as absolute with scheme `http` or `https`. Drop anything else
(`javascript:`, relative paths, junk).

**Logos:** same check, then change `http://` to `https://`, because browsers block http images on an https page. If
there's no logo, or it fails to load, show an initials badge:

- Split the name on spaces and keep only letters and digits in each word.
- One word → its first 3 characters, uppercased ("Kansas" → "KAN").
- Several words → the first letter of up to 3 words ("Grand Canyon" → "GC").
- Nothing left → "?".

Escape every database value when rendering HTML. askama and maud both do this by default.

## 8. Paging

- 200 games per page, query string `?page=N` (0-based, clamp to 0–10000, then to the last page).
- Only shown when the total is more than one page: "← Earlier", "Page X of Y", "Later →".
- Page links keep the current `schedule`.

## 9. Checking it matches

1. Open `http://192.168.1.100:8085/events` and the Rust page on the same day.
2. Compare the event count, the days, the league headings and their order, and a handful of rows (teams, Away/Home
   sides, time, location, links).
3. Pick each dropdown choice and compare again.
4. Check a day with a postponed or TBD game, a neutral-site game, and a game whose radio text contains HTML.

The two may differ by a few minutes of scraper updates; the counts should otherwise match exactly.

**Mobile:** check at 320, 390 and 430px wide in light and dark mode: no sideways scrolling, team names wrap, the
dropdown is readable, and links and buttons are at least 44px tall.

## 10. Left for later

- **Team records** beside each name, such as `12-4 (5-2)` for overall and conference. In myNewspaper this is
  `TeamRecords.LoadAsync` plus `LeaguePhases`, and records are hidden league-wide during playoffs. It's a separate
  piece of work.
- **Wrexham AFC only** checkbox, shown only on the EFL Championship schedule: `?team=352` adds
  `AND e.ProviderKey = 'espn-championship' AND (e.SchoolId = 352 OR e.OpponentSchoolId = 352)`.
- **Soccer league links** under the dropdown (Premier League / Championship / MLS shortcuts).
- **Favourite teams highlight.** Each row carries a `data-sports-teams` attribute that the site's JavaScript uses
  with profile preferences; this has no meaning outside myNewspaper.
- **Older query-string filters** `?sport=` and `?school=<SportsEventsType.ID>` still work on the current page, but the
  dropdown doesn't produce them.
