# tvdash — Orbit: Storm grey background — Change spec (rev. 2026-11-24, for v0.1.68)

Agent-ready change list for the **Orbit** UI that's already built (`ui/orbit/`, spec `tvdash-orbit-build.md`). It replaces Orbit's near-black background with **Storm grey `#2B313D`**, plus the few colours that have to move with it. **Colours only: nothing changes in layout, sizes, focus behaviour or data.**

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **Orbit (less square)**. Every board now uses Storm grey.

Scope: **Orbit only.** Classic, Blades and PS5 don't change.

---

## 1. Token changes (`ui/orbit/theme.slint`, `export global OrbitTheme`)

| Token | Was | Now | Why |
|---|---|---|---|
| `bg` | `#07070A` | **`#2B313D`** | The new background |
| `muted` | `#8E95A0` | **`#9CA3AE`** | Keeps hints and captions at ≥ 4.5:1 on the lighter background (now 5.1:1; the old value would be 4.3:1) |
| `row` (fill) | `#0D1017CC` | **`#FFFFFF12`** (7% white) | A dark fill looks like a hole on grey. Rows and cards now sit *above* the background |
| `row` (border) | `#FFFFFF14` | `#FFFFFF14` | Unchanged |
| `text` / `sub` / `accent` / `live` | — | Unchanged | Contrast on `#2B313D`: `text` 11.5, `sub` 7.0, accent `#8FC1FF` 7.0 |

```slint
// ui/orbit/theme.slint (only these lines change)
export global OrbitTheme {
    out property <color> bg: #2B313D;          // was #07070A
    out property <color> muted: #9CA3AE;       // was #8E95A0
    out property <color> row: #FFFFFF12;       // was #0D1017CC
    // text, sub, accent, live, glass, row-focus, track: unchanged
}
```

### Derived from `bg` (check each one follows the token)

All of these must **read `OrbitTheme.bg`** rather than a hard-coded `#07070A`, so they change automatically:

| Use | Spec ref |
|---|---|
| `scrim-left`: `bg` at F0 → A6 (38%) → 00 (66%) | §1.1 |
| `scrim-bottom`: `bg` → CC (25%) → 00 (55%) | §1.1 |
| `fade` at the bottom of every scrolling list: `bg` → transparent over 100–140 px | §1.1 |
| **Focus ring gap** (the 5 px gap between an item and its 4 px `text` ring) | §1.1 `focus` |
| The 3–6 px `bg` outline separating a ring from its inner disc (`OrbitRing` inner `box-shadow`-style border) | §1.4 |
| Score-disc veil on Sports (`bg` at 55% over the team-colour split) | §7.2 / §7.3 |
| Quit dialog dim (`bg` at E0) | §10.2 |
| Shell / window background | §1.3 |

---

## 2. Fixed backdrop glows

Screens without art (and art fallbacks) use radial glows that end in `bg`. Their **darker middle stops** have to come up, or they leave a near-black band before the grey. Final stop = `OrbitTheme.bg` in every case.

| Screen / fallback | Stops (centre → edge) |
|---|---|
| Show / Library (purple fallback) | `#6A4A8C` 0% · `#36264A` 38% · **`#372E48` 68%** · `bg` 100% |
| Home (green fallback) | `#4F7040` 0% · `#263A20` 38% · **`#30402C` 68%** · `bg` 100% |
| Browse all (blue fallback) | `#2E4A6A` 0% · `#16243A` 38% · **`#2C3648` 68%** · `bg` 100% |
| Collections landing (gold) | `#8A6A24` 0% · `#3D2F12` 36% · **`#3A3326` 66%** · `bg` 100% |
| Collection page (teal) | `#2E5F6A` 0% · `#15292E` 36% · **`#2C3A40` 66%** · `bg` 100% |
| Search (green glow) | `#3D5A34` 0% · **`#33433A` 45%** · `bg` 80% |
| Apps (neutral) | **`#2B2F3A`** 0% · `bg` 70% (+ the focused app's `text-color` glow at 20%, unchanged) |
| Settings | `#2A3550` 0% · `bg` 70% (unchanged apart from `bg`) |
| Sports | Two transparent glows (`#0051BA40`, `#6B2C9133`) over `bg`: unchanged apart from `bg` |

> Slint's `@radial-gradient` is circular. Keep whatever approach the build already uses (large off-screen circles or pre-rendered glow images). If glows are **pre-rendered PNGs**, re-export them with the new stops and an edge colour of `#2B313D`.

---

## 3. Not changing

- **Startup animation** (`Skin::Orbit`): the logo and ring-fill stage stay on **black** (`#050608`). The iris reveals Home, which is now grey. Timings are unchanged.
- `glass` (pill nav, clock badge, quick circles: `#0B0A12B3`), `row-focus` (`#FFFFFF1F` + `#FFFFFF55`), `track` (`#FFFFFF2E`), brand app colours, team colours and the live red all stay the same.
- Dark node fills (e.g. unwatched episode nodes `#1A1624`, unselected bubbles `#12151C` / `#14111C`) stay. They now read as darker wells on grey, which is intended.

---

## 4. How to find stragglers

Hard-coded dark values anywhere under `ui/orbit/` are bugs after this change:

```bash
# should print nothing (or only the intro's #050608)
rg -n -i '#0(7070a|7060c|8070a|6080b|6070a|5070a|6080a|d1017)' crates/app/ui/orbit/
rg -n -i '#8e95a0' crates/app/ui/orbit/
```

Replace each hit with `OrbitTheme.bg`, `OrbitTheme.row` or `OrbitTheme.muted`.

---

## 5. Task list

1. Update `OrbitTheme` (`bg`, `muted`, `row`) (§1).
2. Make every derived use read `OrbitTheme.bg` (§1 table); remove hard-coded dark hexes (§4).
3. Update the fixed glows' middle stops, or re-export glow images (§2).
4. Visual check on the TV at 1080p, 1440p and 4K: every Orbit page, focus and hover rings, list fades, the Quit dialog, toasts / Sleep warning / Up Next over the new background.
5. Tests per crate, lightest first (`cargo test -j 2 -p tvdash-core`, …, `cargo test -j 2 -p tvdash -- --test-threads=1`). The user runs the release build (`GEMINI.md`).
6. Version bump and notes in `Cargo.toml`, `CHANGELOG.md` (`Orbit: Storm grey background`), `docs/fedora-install.txt`, `docs/status.md`, `releases/README.md`.

## 6. Done when

- [ ] Every Orbit screen sits on `#2B313D`; no near-black panels or bands remain (§4 search is clean).
- [ ] List rows and cards read as lighter than the background; focus rings keep a visible gap.
- [ ] Hints and captions use `#9CA3AE` and are readable from the couch.
- [ ] Backdrop glows fade into grey without a dark ring.
- [ ] The startup animation still opens on black and reveals the grey Home.
- [ ] Classic, Blades and PS5 look exactly as before.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-24 | Created: Orbit background → Storm grey `#2B313D`; `muted` → `#9CA3AE`; `row` → 7% white wash; glow middle stops lifted |
