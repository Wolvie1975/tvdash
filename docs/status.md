# tvdash: where things stand

Last updated 2026-11-25 (`v0.1.78` bumped on `feature/feature-pack-1`, ready for RPM build). A pick-up-later note: what's done, what's waiting on you, what's
next, and the practical details that aren't obvious from the code.

## Waiting on you

- **`v0.1.78` (`feature/feature-pack-1`, Feature Pack 1: Quick switch, Credits Up Next, Weather alerts, Show extras) — tests passed, version bumped, ready for RPM build:**
  - **Part A (Quick switch):** Added `[tv] quick_switch` / `quick_switch_key` (`F9`), `GET/POST /quick`, the Back pill **Switch** button & overlay (`extras/back-pill/`), and **Settings › Display › Quick switch** (`QuickSwitchPicker` across Classic, Blades, and Orbit).
  - **Part B (Credits Up Next & shared Up Next card):** Added Plex `credits` marker reading (`includeMarkers=1`), controller `PlayNextNow` / `WatchCredits` handling, and shared `NextEpisodeCard` mounted once in `laptop.slint` (with 10 s countdown at credits, replacing the per-UI modals and fixing the invisible prompt in Orbit).
  - **Part C (Severe-weather alerts):** Added NWS alerts poller (`[weather_alerts]`), `WeatherWarning` takeover + `WeatherBanner` + `WeatherCard` + playback pause + Back pill `/weather` overlay + `--weather-test`.
  - **Part D (Show trailers & extras):** Added Plex `extras()` reader, `Command::PlayExtra`, `ShowPageState` Trailer button + Extras tab across Classic, Blades, and Orbit.
  - **Next (on host):**
    - `cargo build --release -j 2`
    - `./packaging/rpm.sh`
    - `cp dist/tvdash-0.1.78-1.x86_64.rpm releases/`
- **`v0.1.77` (`development`, efficiency & parity phase 1 + Orbit Library focus fix) — tests & live check passed, version bumped, ready for RPM build:**
  - **Done since `v0.1.75`:**
    - **Orbit Library landing focus (`state.slint`, `orbit/library.slint`, `bridge/library.rs`):** Wired the Play button, time-remaining label, progress ring, and `"Show page"` pill to `LibraryLanding.focus` so hovering other items updates them instead of staying pinned to `shelf1[0]`.
    - **Smaller release binary (`Cargo.toml`):** Added `[profile.release] strip = "symbols"` (~70 MB binary, ~18.0 MB RPM).
    - **Idle & refresh efficiency (`bridge/mod.rs`, `tv/ambient.slint`, `v3/ambient.slint`):** Cached bundled league logos in a thread-local `OnceCell` (plus `espn-championship` match), slowed screensaver wall drift to 15 fps (`66ms` / `0.8px`), and skipped unchanged empty `NowPlaying` session refreshes.
    - **Screensaver & Settings focus (`callbacks.rs`, `tv_nav.rs`):** Fixed the Screensaver quick button to start ambient and set `TvUi.ambient = true`, and focused the Dashboard UI row per UI after switching.
    - **UI Cargo features (`crates/app/Cargo.toml`, `build.rs`, `ui/laptop.slint`, `ui/stubs/`):** Added `ui-classic`, `ui-blades`, `ui-ps5`, `ui-orbit` features; `ui-ps5` is excluded from `default` builds while its design is reworked.
  - **Next (on host):**
    - Build and package `v0.1.77`:
      - `cargo build --release -j 2`
      - `./packaging/rpm.sh`
      - `cp dist/tvdash-0.1.77-1.x86_64.rpm releases/`
- **PS5 Dashboard UI (`v0.1.70`)** is complete on `feature/add-uis` with [tvdash-0.1.70-1.x86_64.rpm](releases/tvdash-0.1.70-1.x86_64.rpm).
- **macOS Stage Dashboard UI (`macos`)** has a build spec in `docs/new concepts/` and is on hold for now.
- Reference to build/upgrade an RPM on the TV machine:
  ```bash
  cargo build --release -j 2
  ./packaging/rpm.sh
  cp dist/tvdash-<version>-1.x86_64.rpm releases/
  sudo dnf upgrade ./releases/tvdash-<version>-1.x86_64.rpm
  ```

## Done

| Area | State |
|---|---|
| v1 (M1–M6) | Built: Plex + Jellyfin sign-in, Continue Watching, mpv playback with progress sync, streaming sites in Brave, weather, first-run setup, Library |
| Laptop redesign | Built: two columns, poster tint, Dark / Stone / Mist, touch-first, 5-column service grid with Add service and long-press edit |
| v1.1 | Built: manage streaming sites, Where to watch (TMDB), service logos, Display settings, stale/error states, Diagnostics, pixel shift, crossfades |
| Tile logos | TMDB logos kept on disk in `~/.local/share/tvdash/logos/`; per-site `logo = "path"` override in `[[streaming]]` |
| TV-only layout, step 1 | Built: side rail, 10-foot screens, arrow/OK/Back navigation, ambient clock, `tvdash --home`, live layout switch |
| TV-only layout, step 2 | Built: Plex/Jellyfin video plays inside the tvdash window through libmpv; floating player HUD and live PIP mini-player |
| Library v0.1.3 | Built: smart shelves, full-width Browse All with A–Z filter, shelf caching |
| TV-only step 3 (v0.1.4–v0.1.5) | Built: 10-foot Search (6×6 keyboard + physical typing + detail), Library parity (smart shelves, 8-col A–Z Browse All ignoring leading articles + Show detail), scrollable Settings + version number, Audio/Subs/Vol/Prev-Next Episode controls (also Prev/Next Ep on dashboard), Night mode forcing Dark on TV, local HTTP Phone Remote (`:8765`) |
| Cleanup | `bridge.rs` split into `bridge/` (home, library, accounts, status); repeated controller code folded |
| TV-only v2 Tile Hub (v0.1.13–v0.1.24) | Complete on branch `tv-tile-hub-v2`: top TabBar, spatial focus (`focus.rs`), page stack (`shell_tv.rs`), proportional 1080p→1440p/4K scaling (`TvUi.u`), Home 2×3 Continue Watching (movies + TV via `/hubs/continueWatching/items`, capped at 6) + Weather tile + Clock/Plex Up Next left tile + Hero poster with play overlay when paused (`v0.1.24`), Services 5×2 brand tiles with scaled SVGs + last-3 recent tracking (`recent.rs`), Back-to-tvdash Brave extension (`extras/back-pill`, `:47800/home`), vertical Library shelves (Finish the Season on TV, Collections on Movies, Browse All A–Z 8-col grid), 3-col vertical Show episodes + Collection panorama, Search 6×6 keyboard, Settings sub-tabs (Display + Phone Remote QR + Weather Location lookup, Accounts, Server, What's new), Player HUD with Subtitles & Audio toggles, Up Next modal (`Play Next` / `Back to Dashboard`), Ambient idle |
| Blades, Sleep Timer & Themes (v0.1.46–v0.1.52) | Built: **Blades** (`ui/v3/`) dashboard UI with Up Next prompt, Next ep HUD control (both UIs), Accounts actions/editing, Screensaver activation, Browse-all poster fetching & `#`/`A–Z` alignment, Library Continue Watching freshness (`X-Plex-Container-Size=30`, refresh on load, 5 min shelf cache TTL), **Sleep Timer** (`sleep.rs`, picker, chip, warning card, Brave pill overlay, `tvdash --sleep` CLI, `GET/POST /sleep`, `state.json` persistence, `hold_sleep_timer` for Google Meet), and **Espresso** / **Aubergine** / **Kansas** (KU Crimson & Blue) themes |

Decisions made along the way:
- **Themes:** no "Follow system" option. Keep the three explicit themes (Dark / Stone / Mist).
- **Favicon fallback for tile logos:** skipped. Without a logo, tiles show the name.
- **Calendar and homelab:** removed from scope for good (2026-11-11).
- **Two-screen layout:** keeps using mpv's own window. In-window video is TV-only.
- **Home Hero when returning from video:** pauses playback and shows the still poster with a play-icon overlay instead of rendering minimized live video (`v0.1.24`).

## Next

In the order agreed or planned (see [tv-only-plan.md](tv-only-plan.md) and
[future-plan.md](future-plan.md)):

1. **Optional / Later:** HDMI-CEC remote (`cec-client` listener), skip intro.
2. **v2:** alerts strip, severe-weather takeover, headlines, richer "up next".
3. **v4:** "What should I watch?" (Ollama), watch stats, Home Assistant, cross-service watchlist.

## Things to check on the TV machine

- Switch to TV only (Settings › Display › Layout) and play something from Continue Watching.
  It should open full screen inside tvdash. Enter shows the overlay, Space pauses, ←/→ seek,
  Esc goes to Home with the video still playing behind it.
- If there's sound but a black or frozen picture, that's not caught by the automatic
  fallback. Run `MPV_LOG=v tvdash` from a terminal to see mpv's log (look for the
  `GL_VERSION` line and which `hwdec` it picked).
- Without signing in to anything: `cargo run --release --example video -- ~/Videos/film.mkv`
  plays a file through the same drawing code.
- The log line `video drawing in the window` means the in-window path is active;
  `in-window video failed` means it fell back to mpv's window (the reason follows).

## Build, test, deploy

```bash
cargo build --release                       # target/release/tvdash
cargo test                                  # unit + headless UI tests
cargo test -- --include-ignored             # + end-to-end (python3) and libmpv tests
cargo run --example snapshot -- out.png tv  # render a screen to PNG without a display
cargo run --example video -- <file|URL> [snapshot dir]   # in-window video check
packaging/install.sh [--autostart]          # install on this machine
packaging/bundle.sh                         # dist/tvdash-<version>-x86_64.tar.gz for another machine
```

The binary needs glibc 2.39+ and fontconfig. libmpv is loaded at runtime, not linked.
More: [running.md](running.md) (day to day), [deploying.md](deploying.md) (another machine).

## Dev container notes

- The container is Debian (trixie); the TV machine is Fedora 44. Builds from the container
  run on the host.
- Installed by hand on 2026-11-12, and lost if the container is rebuilt: `libmpv2`,
  `libmpv-dev`, `mpv`, `xvfb`, Mesa GL. Without libmpv, `cargo test -- --include-ignored`
  fails the three libmpv unit tests (the controller's in-window test skips itself). To keep
  them, add to `../.devcontainer/devcontainer.json`:
  `"postCreateCommand": "sudo apt-get update && sudo apt-get install -y libmpv2 xvfb libgl1-mesa-dri"`.
- Visual check of in-window video in the container:
  ```bash
  Xvfb :99 -screen 0 1920x1080x24 &
  DISPLAY=:99 WAYLAND_DISPLAY= target/debug/examples/video "av://lavfi:testsrc2=size=1280x720:rate=25" /tmp/shots
  ```
  Mesa's *software* GLES draws YUV video black, standalone mpv included. That's why tvdash
  asks Slint for desktop OpenGL (`require_opengl()` in `main.rs`).

## Where things live

| Piece | File |
|---|---|
| Startup, CLI, libmpv load | `crates/app/src/main.rs` |
| Everything with side effects (sign-in, playback, browser, config) | `crates/app/src/controller.rs` |
| Events → UI models | `crates/app/src/bridge/` (`home`, `library`, `accounts`, `status`) |
| TV-only keys and focus | `crates/app/src/tv_nav.rs`; UI in `crates/app/ui/tv/shell.slint` |
| In-window video drawing (UI thread, OpenGL) | `crates/app/src/video.rs` |
| libmpv bindings, player, render context | `crates/player/src/libmpv.rs` |
| mpv in its own window (JSON IPC) | `crates/player/src/mpv.rs` |
| TMDB, logos, Where to watch | `crates/app/src/watch.rs`, `crates/discover/` |
| Config, keyring, state | `crates/core/src/` |

Gotchas worth remembering:
- libmpv only starts video once the render context exists, so the file is loaded from
  `RenderContext::new`. The core runs with `idle=yes` and is told to quit when the file ends.
- The render context must be created and freed on the UI thread, and before the mpv core is
  destroyed. The render context holds a reference to the core so the order is guaranteed.
- Slint's renderer leaves some GL state changed (pixel unpacking, blending). `video.rs`
  resets it before mpv draws and restores it afterwards.
