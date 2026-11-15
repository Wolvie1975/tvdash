# tvdash: where things stand

Last updated 2026-11-12. A pick-up-later note: what's done, what's waiting on you, what's
next, and the practical details that aren't obvious from the code.

## Waiting on you

1. **Review and merge the dashboard & Library update.** Branch `dashboard-library-update`
   (local only): Jellyfin removed, three-box dashboard, ten fixed apps with logos, search
   panel, smart-shelf Library, sentence-clock screensaver. See
   [dashboard-library-update.md](dashboard-library-update.md). Push from VS Code (Claude's
   shell can't reach VS Code's GitHub login).
2. **Install on the TV machine**, after a release build (`cargo build --release`):
   ```bash
   packaging/install.sh          # warns if mpv-libs (libmpv) is missing: sudo dnf install mpv-libs
   ```
   Then Settings › Quit tvdash and start it again from the app menu. The first start
   rewrites `[[streaming]]` in the config to the ten apps and drops `[jellyfin]`.
3. **Clock:** 12-hour is the new default, but an existing config already says
   `clock = "24h"`; switch it in Settings › Display.
4. **On the laptop:** check each app tile opens the right page on the TV (addresses can be
   fixed in `[[streaming]]`), whether GNOME's on-screen keyboard appears in the search
   field, and that the screen stays on while something plays.
5. **Check in-window video on the real TV** (TV-only layout). It has only been seen on a
   virtual display with Mesa's software OpenGL, not on Wayland with a real GPU. See
   [Things to check](#things-to-check-on-the-tv-machine).

## Done

| Area | State |
|---|---|
| v1 (M1–M6) | Built: Plex + Jellyfin sign-in (Jellyfin since removed), Continue Watching, mpv playback with progress sync, streaming sites in Brave, weather, first-run setup, Library |
| Laptop redesign | Built: two columns, poster tint, Dark / Stone / Mist, touch-first, 5-column service grid with Add service and long-press edit |
| v1.1 | Built: manage streaming sites, Where to watch (TMDB), service logos, Display settings, stale/error states, Diagnostics, pixel shift, crossfades |
| Tile logos (2026-11-12) | TMDB logos kept on disk in `~/.local/share/tvdash/logos/`; per-site `logo = "path"` override in `[[streaming]]` |
| TV-only layout, step 1 | Built: side rail, 10-foot screens, arrow/OK/Back navigation, ambient clock, `tvdash --home`, live layout switch |
| TV-only layout, step 2 (2026-11-12) | Built: Plex video plays inside the tvdash window through libmpv. Falls back to mpv's own window without libmpv or if drawing fails |
| Cleanup (2026-11-12) | `bridge.rs` split into `bridge/` (home, library, accounts, status); repeated controller code folded |
| Collections, sort, show page (2026-11-12) | Built on the same branch: Collections row and collection pages ([collections.md](collections.md)); Library Sort menu saved per tab; genre chips in Browse all; show page with tabs / pills / picker and episode list ([show-page.md](show-page.md)) |
| Dashboard & Library update (2026-11-12) | Built on branch `dashboard-library-update`: Plex only; header on a 1fr 2fr 1fr grid; three equal boxes; ten fixed apps with bundled logos (replaces site management and TMDB logos); search panel (replaces Where to watch); Library smart shelves + Browse all; sentence-clock screensaver with a portal idle inhibitor; 12-hour default. [Details](dashboard-library-update.md) |

Decisions made along the way:
- **Themes:** no "Follow system" option. Keep the three explicit themes (Dark / Stone / Mist).
- **Favicon fallback for tile logos:** skipped. (Superseded: the ten apps now have bundled logos.)
- **Jellyfin:** removed (2026-11-12); Plex covers it.
- **Tonight's pick ending before night mode:** skipped until night mode exists.
- **Calendar and homelab:** removed from scope for good (2026-11-11).
- **Two-screen layout:** keeps using mpv's own window. In-window video is TV-only.

## Next

In the order agreed or planned (see [tv-only-plan.md](tv-only-plan.md) and
[future-plan.md](future-plan.md)):

1. Confirm in-window video on the TV machine (above).
2. **Search on the TV.** v3 baseline input is the air mouse / wireless mouse plus keyboard/Home shortcut; on-screen keyboard remains an option if needed.
3. **v3:** air mouse / wireless mouse, keyboard/Home shortcut, optional HDMI-CEC, next episode / skip intro, audio, subtitle and volume.
4. **v2:** night mode (also forces Dark on the TV), alerts strip, severe-weather takeover,
   headlines, richer "up next".
5. **v4:** "What should I watch?" (Ollama), watch stats, Home Assistant, cross-service watchlist.

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
| Events → UI models | `crates/app/src/bridge/` (`home`, `library`, `saver`, `accounts`, `status`) |
| Library shelf rules (no UI) | `crates/app/src/shelves.rs` |
| App list, logos | `APPS` in `crates/core/src/config.rs`; `crates/app/src/logos.rs`, `assets/logos/` |
| Screen-blanking inhibitor | `crates/app/src/inhibit.rs` |
| TV-only keys and focus | `crates/app/src/tv_nav.rs`; UI in `crates/app/ui/tv/shell.slint` |
| In-window video drawing (UI thread, OpenGL) | `crates/app/src/video.rs` |
| libmpv bindings, player, render context | `crates/player/src/libmpv.rs` |
| mpv in its own window (JSON IPC) | `crates/player/src/mpv.rs` |
| TMDB search (the panel) | `crates/app/src/watch.rs`, `watch_ui.rs`, `crates/discover/` |
| Config, keyring, state | `crates/core/src/` |

Gotchas worth remembering:
- libmpv only starts video once the render context exists, so the file is loaded from
  `RenderContext::new`. The core runs with `idle=yes` and is told to quit when the file ends.
- The render context must be created and freed on the UI thread, and before the mpv core is
  destroyed. The render context holds a reference to the core so the order is guaranteed.
- Slint's renderer leaves some GL state changed (pixel unpacking, blending). `video.rs`
  resets it before mpv draws and restores it afterwards.
