# tvdash Handoff — v0.1.59 (2026-11-22)

## Current State
- **Branch:** `tv-tile-hub-v2`
- **Latest commit:** `0883fbf` (`v0.1.59: Blades 6x2 streaming, Classic Theme/Sleep swap, yellow Google Meet`)
- **Latest RPM:** `releases/tvdash-0.1.59-1.x86_64.rpm`
- **Tests:** `cargo test -j 2 -p tvdash -- --test-threads=1` passed on the host prior to the `0.1.59` release commit.

---

## Completed Work & Key Decisions

### 1. Themes (`ku`, `ku-light`, `dark`, `aubergine`, `stone`, `mist`)
- **Kansas Dark (`ku`)**: KU Crimson (`#E8000D`) focus/highlights, KU Blue accents, Signature Gray (`#85898A`) muted text, Jayhawk Yellow (`#FFC82D`) warnings.
- **Kansas Light (`ku-light`)**: Jayhawk Yellow (`#FFC82D`) background, Signature Gray panels/cards, KU Blue Blades navigation strips, and Crimson (`#E8000D`) focus/highlights across both **Classic** and **Blades**.
- **Espresso (`dark`)**, **Aubergine (`aubergine`)**, **Stone (`stone`)**, **Mist (`mist`)**: Full cycle order in UI: `Espresso → Aubergine → Kansas Dark → Kansas Light → Stone → Mist`.

### 2. Startup Animation
- Configurable via `[startup]` (`intro = "on" | "off"`, `max_hold_secs = 6`) and **Settings › Display**:
  - **Tiles assemble** for Classic/Tile hub (`crates/app/ui/tv/intro_tiles.slint`).
  - **Blade swoosh** for Blades (`crates/app/ui/v3/intro_blades.slint`).
- Centered animations with hold progress line and key/click skip; timer starts after `laptop.show()`.

### 3. Home Layouts (Classic & Blades)
- **Streaming Services (`default_streaming_sites()` in `crates/core/src/config.rs`)**: 12 default entries, including **Crunchyroll** and **Google Meet** (`https://meet.google.com`, `hold_sleep_timer = true`).
  - Note: Existing user `config.toml` files with explicit `[[streaming]]` arrays won't auto-pick up new defaults unless those entries are added manually to `config.toml`.
- **Google Meet tile (`crates/app/src/brand.rs`, `assets/logos/google-meet.svg`)**: Yellow background (`#FBBC04` → `#F9AB00`) with dark ink (`#1F1F1F`) logo/wordmark for high contrast.
- **Classic Home (`crates/app/ui/tv/home.slint`, `crates/app/src/tv_nav.rs`)**:
  - **2×6 Streaming Services** grid (12 tiles).
  - **Screensaver** uses its own `"screensaver"` icon (separate from Sleep's `"moon"`).
  - **Theme** (tile `8`, `356×230` at `x: 694px`) and **Sleep** (tile `7`, `240×230` at `x: 1066px`) swapped in the bottom row so longer theme names (`"Kansas Dark"`, `"Kansas Light"`) fit without truncation.
- **Blades Home (`crates/app/ui/v3/home.slint`, `crates/app/src/tv_nav.rs`)**:
  - **Google Meet** removed from the right-hand Quick Actions box (now 5 rows: `Search`, `Library`, `Screensaver`, `Sleep timer`, `Theme`, `60px` tall with `63px` spacing; tile ID `7` marked offscreen/unused in `blades_tile_rects`).
  - **Streaming** grid expanded from 5×2 to **6×2** (`i < 12`, `138×88px` tiles at `152px` horizontal spacing) so Crunchyroll and Google Meet appear in the Streaming area.

### 4. Sports Alerts & Scoreboard
- **Sports Alerts** (Starting soon & Final action cards) across tvdash (Home, video HUD, ambient screensaver) and Brave (`extras/back-pill` via `GET/POST /alerts`).
- Read-only followed teams sync from `dbo.ResultFollows` (with offline cache), local `sports-starred.json` with `★` toggles on Scoreboard, **Settings › Sports** sub-tab, `during_video` / Google Meet hold, and CLI test flags (`--alert-test soon|final`, `--alerts status`).

---

## Workflow Constraints (`GEMINI.md`)
- **Do not run `cargo test`, `cargo check`, `cargo build --release`, or `./packaging/rpm.sh` in the agent sandbox.** Prompt the user to run them on the host (`-j 2` per crate, `--test-threads=1` for `tvdash`).
- **Release checklist:** Bump version in `Cargo.toml`, `CHANGELOG.md`, `docs/fedora-install.txt`, `docs/status.md`, and `releases/README.md` (both install and upgrade commands) together before the host RPM build.
- **Git:** `git commit` requires `BypassSandbox: true` (`.git/index.lock` fails in sandbox). User runs `git push` on the host.

---

## Suggested Next Steps in a New Chat
1. Run `git push` on the host if `0883fbf` hasn't been pushed yet.
2. Verify `v0.1.59` visually on the TV machine (`sudo dnf upgrade ./releases/tvdash-0.1.59-1.x86_64.rpm`), checking:
   - Classic Home bottom row (`Theme` showing `"Kansas Dark"` / `"Kansas Light"` cleanly, `Sleep` below `Screensaver`).
   - Blades Home 6×2 `Streaming` grid and 5-row Quick Actions box.
   - Yellow **Google Meet** tile contrast in both Classic and Blades.
3. Continue with any remaining items in `docs/status.md` (`HDMI-CEC`, `v2`/`v4` features, or `docs/new concepts/`).
