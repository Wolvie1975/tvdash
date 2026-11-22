# Scoreboard Favorite / Followed Teams Tables

How `/scoreboard` resolves followed teams, conferences, and leagues in `WebScraper`.

## Source in Code
* Page & panel: [`Scoreboard.razor`](../src/myNewspaper.Web/Components/Pages/Scoreboard.razor) → [`ScoreboardPanel.razor`](../src/myNewspaper.Web/Components/ScoreboardPanel.razor)
* Query: [`ScoreboardQuery.ReadAsync`](../src/myNewspaper.Shared/Data/Scoreboard.cs)

## Database Tables

### 1. `dbo.ResultFollows`
Stores each followed **team**, **conference**, or **league** (household-wide, managed in Admin).

| Column | Notes |
| --- | --- |
| `Id` | Primary key (`int`) |
| `Kind` | `'team'`, `'conference'`, or `'league'` |
| `Sport` | e.g. `football`, `basketball`, `volleyball`, `baseball`, `soccer` |
| `League` | e.g. `college-football`, `mens-college-basketball`, `nfl`, `mlb`, `usa.1` |
| `TeamId` | ESPN team ID (`varchar`, populated when `Kind = 'team'`; `NULL` for conference/league) |
| `Name` | Display name (e.g. `Kansas Jayhawks`, `Big 12`, `NFL`) |
| `Enabled` | `bit` — only `Enabled = 1` rows are shown |

Current `Kind = 'team'` rows in `WebScraper`:
* `Kansas Jayhawks` (`TeamId = 2305`) for `college-football`, `mens-college-basketball`, and `womens-college-volleyball`.

### 2. `dbo.ResultFollowTeams`
Child table storing member team IDs for `Kind = 'conference'` rows in `dbo.ResultFollows`.

| Column | Notes |
| --- | --- |
| `FollowId` | Foreign key → `dbo.ResultFollows.Id` |
| `TeamId` | ESPN team ID (`varchar`) of a member school in that conference |

## Matching Logic (`ScoreboardFollow.Matches`)
* **`Kind = 'league'`:** matches games with the same `Sport` and `League`.
* **`Kind = 'team'`:** matches games with the same `Sport`, `League`, and `HomeTeamId == TeamId || AwayTeamId == TeamId`.
* **`Kind = 'conference'`:** matches games with the same `Sport`, `League`, and either `HomeTeamId` or `AwayTeamId` in `ResultFollowTeams` (`Members`) for that `FollowId`.

> **Note:** Scoreboard follows are household-wide in `ResultFollows` / `ResultFollowTeams` (not per-profile `MyDailyFeedProfileFollows`).
