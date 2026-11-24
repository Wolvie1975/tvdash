# tvdash — League logos + grey league bubbles

**Date:** 2026-11-24
**Scope:** Sports tab, league/type bubbles (Competitions and Scoreboard). Orbit UI (`OrbitBubble`).
**Files shipped with this spec:** `efl-championship.png`, `premier-league.png`

---

## 1. Summary

| Change | Why |
|---|---|
| Add an **EFL Championship** logo | It currently has no logo and shows a lone dot |
| Replace the **Premier League** logo with a **white** version | The current dark-purple logo can't be seen on the dark bubble |
| Make the league bubbles **dark grey** (`#3A3F4B`) | Gives the logos a visible backing instead of near-black |

---

## 2. New logo files

Copy both files into the repo:

```
assets/logos/leagues/efl-championship.png
assets/logos/leagues/premier-league.png
```

| File | Size | Background | Notes |
|---|---|---|---|
| `efl-championship.png` | 512 × 512 | Transparent | Full-colour badge with its own white fill; tall shape, centred |
| `premier-league.png` | 512 × 512 | Transparent | Solid white, single colour (like the NHL logo) |

> **Note:** If league logos currently come from a remote URL (ESPN CDN or a DB column), don't remove that path. Add a **local override** that wins when a file exists.

### 2.1 Local override map

Find where a league's logo is resolved (the code behind the bubble image) and check a local map first:

```rust
// League key -> bundled logo. A match here wins over the remote/DB logo.
const LEAGUE_LOGO_OVERRIDES: &[(&str, &str)] = &[
    ("eng.2", "assets/logos/leagues/efl-championship.png"), // EFL Championship
    ("eng.1", "assets/logos/leagues/premier-league.png"),   // Premier League
];
```

- Match on the **league key the app already uses** (ESPN slug `eng.1` / `eng.2`, or the exact `League` string from the DB). Run `SELECT DISTINCT Sport, League FROM dbo.SportsEvents` (and `dbo.SportsScoreboard`) and use those exact strings if they aren't the ESPN slugs.
- If no override and no remote logo: show the **monogram fallback** (§4) instead of the dot.

---

## 3. Bubble colours

| Token | Value | Use |
|---|---|---|
| `bubble-fill` | `#3A3F4B` | League/type bubble fill (was about `#1A1D26`) |
| `bubble-fill-focus` | `#464C59` | Focused bubble fill; keep the existing focus ring on top |

- Change this in `OrbitBubble` only. Both **Competitions** and **Scoreboard** use it, so one change covers both.
- **Ring segments:** check that the faint "upcoming" segments are still visible on `#3A3F4B`. If they aren't, raise their opacity one step (for example `#FFFFFF40` → `#FFFFFF55`).
- Don't change the hero card disc or the row time discs.

---

## 4. Logo sizing inside the bubble

All logos are fitted (`contain`, centred, never cropped or stretched) into a square inset:

| Logo shape | Inset (share of bubble diameter) |
|---|---|
| Round / square crests (NFL, NHL, MLS, NCAA, Premier League) | **66%** |
| Tall badges (EFL Championship) | **78%** |

Simplest rule: use 66% by default and 78% when the image is clearly taller than it is wide (visible height ÷ width > 1.15). The shipped PNGs are square with padding, so you can also just hard-code 78% for `eng.2`.

**Monogram fallback** (no logo at all): the league's short name (`EFL`, `B12` …) centred, 600 weight, about 30% of the bubble diameter, text colour `#E6E8EC`, on the same `bubble-fill`.

---

## 5. Optional fixes spotted in the same screen

Not required for this change. Do them if they're quick:

1. **Row time overflow:** "1:45 P.M. CT" spills out of the 64 px row disc. Show `1:45` with `PM` smaller underneath, and drop `CT` in rows (it's already in the hero).
2. **Stray separators:** `Soccer ·` with nothing after it, and `Soccer · · Paramount+`. Join only the parts that aren't empty with ` · `.
3. **Clipped first bubble:** the top bubble (Big 12 Volleyball) is cut off under the `TYPE` label. Add top padding to the `SlotList`, or keep the focused bubble fully in view.

---

## 6. Done when

- [ ] EFL Championship shows its badge in the bubble (no dot).
- [ ] The Premier League bubble shows the white logo and it's clearly readable.
- [ ] All league bubbles are `#3A3F4B`; the focused one is `#464C59` with the focus ring.
- [ ] Ring segments are still visible on the grey.
- [ ] Logos are evenly sized; nothing touches the bubble edge.
- [ ] Same look on Competitions and Scoreboard; checked at 1080p and 4K.
- [ ] Tests pass (`cargo test -j 2 -p tvdash-core`, then `cargo test -j 2 -p tvdash -- --test-threads=1`).
