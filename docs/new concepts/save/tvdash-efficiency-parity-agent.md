# tvdash — Efficiency & parity, phase 1: implementation brief for a coding agent

Implement seven changes in the tvdash repo (Rust + Slint), **one commit each**, in the order below, plus a docs commit. Each one is small and independent. The code is given; apply it, adapting only if the surrounding code has moved.

Background and measurements: `tvdash-parity-and-efficiency-research.md` (2026-11-25). The items covered here are **B1, B2, R1, R2 (part), R4, the PS5 HUD/screensaver gap, and the Settings focus index**. Everything that needs a design decision first is listed under **Out of scope** at the end.

---

## Ground rules

- **Base:** `development` at `c52e4c2` ("Merge pull request #7 from Wolvie1975/feature/orbit-polish", v0.1.75) or later. Work on a new branch `efficiency-parity-1`.
- **Follow `GEMINI.md`.** Don't run `cargo build --release`, `cargo test --workspace` or `./packaging/rpm.sh` yourself. When a commit says **Check**, give the user the commands to run on the host (`-j 2`, one crate at a time) and wait for the result before moving on.
- **Why that matters here:** with all four UIs, compiling the `tvdash` crate needs **more than 6 GB of RAM**. Never build two crates in parallel on top of it. Commit 7 is what makes the dev loop lighter.
- **Don't run `cargo fmt` on the workspace.** Many files are already unformatted. Keep new code in the style around it.
- **Don't change anything else:** no refactors, no dependency bumps, no clippy fixes outside the lines you touch.
- Line numbers are for `c52e4c2`. If a snippet doesn't match, find the equivalent code and make the same change. **Stop and report** if the intent no longer applies.

```bash
git checkout development && git pull
git checkout -b efficiency-parity-1
```

---

## Commit 1 — Strip symbols from release builds

**Why:** the shipped binary is 100.5 MB and **not stripped**; 30 MB of it is the symbol table. Stripping takes it to about 70 MB and the RPM from 21.3 MB to about 18.8 MB. There's no runtime cost. The workspace has no `[profile.release]` at all today.

### Change

**`Cargo.toml` (workspace root):** add at the end of the file:

```toml
[profile.release]
# Ship without the symbol table: ~30 MB smaller binary, ~2.5 MB smaller RPM.
# Panic backtraces lose function names; build without this for a debug session.
strip = "symbols"
```

Don't add `lto`, `codegen-units = 1` or `panic = "abort"`. The first two raise compile memory, which is already the build's limit, and the third changes how panics in background tasks behave.

### Check (user, on the host)

```bash
cargo build --release -j 2
file target/release/tvdash          # expect "... stripped"
ls -l target/release/tvdash         # expect ~70 MB (was ~100 MB)
```

### Commit message

```
Strip symbols from release builds

The release binary shipped with its full symbol table (~30 MB of the
100 MB). strip = "symbols" cuts the binary to ~70 MB and the RPM from
21.3 MB to ~18.8 MB, with no runtime cost.
```

---

## Commit 2 — Decode the bundled league logos once

**Why:** `bundled_league_logo()` in `crates/app/src/bridge/mod.rs` (~line 846) decodes a 110–150 KB PNG **on the UI thread** every time `refresh_sports` or `refresh_scoreboard` runs. It also creates a new `Image` each time, so the GPU texture is uploaded again. Decode once per process and reuse the `Image` (which is reference-counted).

### Change (`crates/app/src/bridge/mod.rs`)

Replace the whole function:

```rust
/// Bundled league logo overrides (`(image, is_tall)`). Wins over remote ESPN CDN URLs.
fn bundled_league_logo(value: &str) -> Option<(Image, bool)> {
    let lower = value.to_ascii_lowercase();
    let (bytes, tall): (&[u8], bool) = if lower.contains("eng.2") || lower.contains("efl") {
        (include_bytes!("../../../../assets/logos/leagues/efl-championship.png"), true)
    } else if lower.contains("eng.1") || lower.contains("epl") {
        (include_bytes!("../../../../assets/logos/leagues/premier-league.png"), false)
    } else {
        return None;
    };
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, rgba.width(), rgba.height());
    Some((Image::from_rgba8(buf), tall))
}
```

with:

```rust
/// Bundled league logo overrides (`(image, is_tall)`). Wins over remote ESPN CDN URLs.
///
/// Each PNG is decoded once per process (on first use) and the same `Image` is reused
/// afterwards, so Sports/Scoreboard refreshes don't re-decode or re-upload the texture.
fn bundled_league_logo(value: &str) -> Option<(Image, bool)> {
    use std::cell::OnceCell;
    use std::thread::LocalKey;

    thread_local! {
        static EFL: OnceCell<Option<Image>> = const { OnceCell::new() };
        static EPL: OnceCell<Option<Image>> = const { OnceCell::new() };
    }

    let lower = value.to_ascii_lowercase();
    let (cell, bytes, tall): (&'static LocalKey<OnceCell<Option<Image>>>, &'static [u8], bool) =
        if lower.contains("eng.2") || lower.contains("efl") {
            (&EFL, include_bytes!("../../../../assets/logos/leagues/efl-championship.png"), true)
        } else if lower.contains("eng.1") || lower.contains("epl") {
            (&EPL, include_bytes!("../../../../assets/logos/leagues/premier-league.png"), false)
        } else {
            return None;
        };
    let img = cell.with(|c| c.get_or_init(|| decode_bundled_png(bytes)).clone())?;
    Some((img, tall))
}

fn decode_bundled_png(bytes: &[u8]) -> Option<Image> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&rgba, rgba.width(), rgba.height());
    Some(Image::from_rgba8(buf))
}
```

Keep the existing `league_monogram` function below it unchanged.

### Check (user)

```bash
cargo test -j 2 -p tvdash -- --test-threads=1
```

Manual: Sports › Scoreboard and Sports › Competitions still show the EPL and EFL logos (EFL tall).

### Commit message

```
Decode the bundled league logos once

bundled_league_logo() decoded a 110-150 KB PNG on the UI thread on every
Sports/Scoreboard refresh, and built a new Image (new texture upload)
each time. Cache the decoded Image per logo in a thread-local OnceCell.
```

---

## Commit 3 — Screensaver: drift at 15 fps instead of 30

**Why:** the screensaver is what's on screen whenever the TV is idle, so it's tvdash's steady-state load. Its poster wall moves 0.4 px every 33 ms. Each tick repaints the whole window: a 2700×2300 layer rotated −10°, plus gradients. Moving 0.8 px every 66 ms gives the same drift speed at half the repaints. From the couch it looks the same.

### Changes

1. **`crates/app/ui/tv/ambient.slint`**, in `component PosterWall`, change the `Timer`:

   ```slint
   Timer {
       interval: 33ms;
       running: root.has-wall;
       triggered => {
           if root.offset >= root.repeat-height {
               root.offset -= root.repeat-height;
           }
           root.offset += 0.4px;
       }
   }
   ```

   to:

   ```slint
   // 15 fps: same drift speed as 0.4 px / 33 ms, half the full-screen repaints.
   Timer {
       interval: 66ms;
       running: root.has-wall;
       triggered => {
           if root.offset >= root.repeat-height {
               root.offset -= root.repeat-height;
           }
           root.offset += 0.8px;
       }
   }
   ```

2. **`crates/app/ui/v3/ambient.slint`**: make the same change to its `Timer` (`interval: 33ms` → `66ms`, `root.wall-offset += 0.4px` → `0.8px`), and add the same comment.

Don't change anything else in either file.

### Check (user, on the TV)

Measure before and after with the screensaver up (Home › screensaver quick action, or 5 min idle on Home):

```bash
SLINT_DEBUG_PERFORMANCE=refresh_lazy,console cargo run -j 2 -- --layout tv
# and in another terminal:
top -p "$(pgrep -x tvdash)"
```

Report the CPU% and frame-rate numbers in the PR. If the drift looks steppy on the TV, try `50ms` / `0.6px` and report which you kept.

### Commit message

```
Screensaver: drift at 15 fps instead of 30

The poster wall repainted the whole window every 33 ms to move 0.4 px.
Move 0.8 px every 66 ms instead: same speed, half the repaints. The
screensaver is the idle state, so this is tvdash's steady-state load.
```

---

## Commit 4 — Skip "nobody is watching" session updates that haven't changed

**Why:** the controller polls Plex sessions every 10 s, around the clock. Each result replaces the Home sessions model (`set_vec`) and rebuilds the Watching now rows (`ModelRc::new(VecModel::from(…))`), even when the list was empty before and is still empty. That's the common case. Skip the update when both are empty.

(Pausing the poll itself during video or the screensaver needs a new controller command; that's in Out of scope.)

### Change (`crates/app/src/bridge/mod.rs`, in `fn apply`, ~line 574)

Replace:

```rust
AppEvent::NowPlaying(sessions) => {
    library::on_sessions(s, laptop, &sessions);
    home::on_sessions(s, sessions);
}
```

with:

```rust
AppEvent::NowPlaying(sessions) => {
    // Polled every 10 s; when nobody was watching and still isn't, leave the models alone.
    if !(sessions.is_empty() && s.raw_sessions.is_empty()) {
        library::on_sessions(s, laptop, &sessions);
        home::on_sessions(s, sessions);
    }
}
```

Only skip the both-empty case. Don't compare non-empty lists: rows also pick up posters that arrive later, and positions change during playback anyway.

### Check (user)

```bash
cargo test -j 2 -p tvdash -- --test-threads=1
```

Manual: start a stream on another Plex client. Library › Watching now shows it within ~10 s. Stop it, and it disappears within ~10 s.

### Commit message

```
Skip unchanged empty session updates

Sessions are polled every 10 s around the clock, and every result
replaced the Home sessions model and rebuilt the Watching now rows,
even when nobody was watching before or after. Skip that case.
```

---

## Commit 5 — PS5: show the player HUD and the screensaver

**Why:** this is a functional bug. In the PS5 UI, `tv_nav.rs` opens the player HUD (OK during full-screen video sets `TvUi.hud-open`), but **`ps5/shell.slint` never draws it**. Arrows and OK then act on invisible buttons: OK on the default tile is play/pause, and tile 3 is **Stop**. Likewise, `tv_nav::tick` turns the screensaver on after 5 minutes on Home (`TvUi.ambient = true`) and pre-renders the poster wall, but PS5 never draws `TvAmbient`. So the screen doesn't change, and the next key press is swallowed to dismiss it.

Orbit already solves this by reusing Classic's HUD and screensaver. Do the same for PS5. They'll use Classic's styling until the shared `Skin` work (phase 2).

### Changes (`crates/app/ui/ps5/shell.slint`)

1. **Imports:** after `import { TvDashboardPicker } from "../tv/dashboard_picker.slint";` add:

   ```slint
   import { TvPlayerHud } from "../tv/player_hud.slint";
   import { TvAmbient } from "../tv/ambient.slint";
   ```

2. **Full-screen video:** replace the block under `// 4. Full-screen in-window video`:

   ```slint
   if TvUi.video-on && TvUi.video-full: Rectangle {
       width: parent.width;
       height: parent.height;
       background: #000000;
       Image {
           source: TvUi.video;
           width: parent.width;
           height: parent.height;
           image-fit: contain;
       }
   }
   ```

   with (same as `orbit/shell.slint`):

   ```slint
   if TvUi.video-on && TvUi.video-full: Rectangle {
       width: parent.width;
       height: parent.height;
       background: #000000;
       Image {
           source: TvUi.video;
           width: parent.width;
           height: parent.height;
           image-fit: contain;
       }
       TouchArea {
           clicked => {
               TvUi.hud-open = true;
               TvUi.overlay();
           }
           moved => {
               if !TvUi.hud-open {
                   TvUi.focused-tile = 1;
                   TvUi.hud-open = true;
                   TvUi.overlay();
               }
           }
       }
       if TvUi.hud-open: TvPlayerHud { }
   }

   // 4b. Ambient screensaver overlay (Classic's, as in Orbit)
   if TvUi.ambient: TvAmbient { }
   ```

   Keep `// 5. Startup animation` (`PsIntro`) and the `keys := FocusScope` after it, so the intro still draws on top and keys still reach `TvUi.key`.

### Check (user)

```bash
cargo test -j 2 -p tvdash -- --test-threads=1
cargo run -j 2 -- --windowed --layout tv      # Settings › Display › Dashboard UI › PS5
```

Manual, in PS5:
1. Play a Plex episode full screen, press OK: the HUD appears. ←/→ moves focus, OK on ⏸ pauses, Back closes the HUD, and a second Back minimises to Home.
2. Near the end of an episode, the Up Next prompt appears in the HUD.
3. Mouse/air-mouse movement over the video opens the HUD.
4. Home › screensaver quick action (or 5 min idle on Home): the poster wall appears, and any key dismisses it.
5. Repeat 1 and 4 in Orbit and Classic to confirm nothing changed there.

### Commit message

```
PS5: show the player HUD and the screensaver

tv_nav opened the HUD and the screensaver in every UI, but the PS5 shell
never drew either one. Remote keys then acted on invisible HUD buttons
(OK = play/pause, tile 3 = Stop), and the screensaver was "on" with
nothing on screen. Mount Classic's TvPlayerHud and TvAmbient in the PS5
shell, the same way Orbit does.
```

---

## Commit 6 — After switching UI, focus the Dashboard UI row in that UI's Settings

**Why:** choosing a UI in the picker calls `tv_nav::open_settings_display_focused_on_picker` (`tv_nav.rs` ~line 274). It always sets:

```rust
s.shell.focused_tile = 13; // Dashboard UI tile in Classic Settings › Display
```

whichever UI you just switched *to*. In Blades, PS5 and Orbit, index 13 is a different control (or out of range, which `sync()` resets to 0).

### Change

1. In `tv_nav.rs`, find the Settings › Display branch for each UI's focus table: `tile_rects` (Classic), `blades_tile_rects`, `ps5_tile_rects` and `orbit_tile_rects`. Note the index of the rect for the **Dashboard UI** control in each. Its comment or position will match the "Dashboard UI" row in `ui/<ui>/settings.slint`.

2. Replace the hard-coded 13 with a lookup on the new UI:

   ```rust
   pub fn open_settings_display_focused_on_picker(laptop: &LaptopWindow) {
       laptop.global::<TvUi>().set_settings_tab("display".into());
       let picker_tile = match tvdash_core::config::DashboardUi::parse(&laptop.global::<Display>().get_dashboard_ui()) {
           tvdash_core::config::DashboardUi::Classic => 13,
           tvdash_core::config::DashboardUi::Blades => /* index from blades_tile_rects */,
           tvdash_core::config::DashboardUi::Ps5 => /* index from ps5_tile_rects */,
           tvdash_core::config::DashboardUi::Orbit => /* index from orbit_tile_rects */,
       };
       STATE.with_borrow_mut(|s| {
           s.shell.switch_tab(Tab::Settings);
           s.shell.focused_tile = picker_tile; // "Dashboard UI" row in each UI's Settings › Display
       });
       sync(laptop);
   }
   ```

   `refresh_dashboard_choices` runs before this call in `callbacks.rs` (`set_ui`), so `Display.dashboard-ui` already names the new UI. Use the full `match` (no `_ =>` arm) so a fifth UI won't compile until it gets an index.

3. If any UI's Settings has **no** Dashboard UI control on its Display tab, don't invent one: use `0` for that UI and say so in the PR.

### Check (user)

Manual: from each UI, open the picker, choose each other UI, and confirm the new UI opens on Settings › Display with focus on its Dashboard UI control. That's 12 switches; list them in the PR as a ✓ table.

### Commit message

```
Focus the Dashboard UI row in the new UI's Settings after switching

open_settings_display_focused_on_picker always used index 13, the
Dashboard UI tile in Classic's Settings > Display, whichever UI was just
selected. Look the index up per UI.
```

---

## Commit 7 — Cargo features to build only the UIs you need

**Why:** every build compiles all four shells. Generated Slint code is 35.8 MB of Rust with four UIs versus 9–13 MB with one, and the `tvdash` crate needs more than 6 GB of RAM to compile with four versus 3.4 GB with Classic only. Features let the **dev loop** build just the UI being worked on. **Release builds and tests keep all four** (default features).

### Design

- One feature per UI in `crates/app/Cargo.toml`. `default` enables all four.
- `laptop.slint` imports each shell through a Slint **library path** (`@shell-classic`, …). `build.rs` points each one at the real shell when its feature is on, or at a one-line stub component when it's off. Unused shells then aren't compiled at all. (Measured: generated code falls to 9.3–12.8 MB with one UI.)
- Rust only offers enabled UIs in the picker, and falls back to the first enabled UI if `config.toml` names a disabled one. It does **not** write that fallback back to the config.

### Changes

1. **`crates/app/Cargo.toml`**: add after the `[package]` section, before `[dependencies]`:

   ```toml
   [features]
   # Which dashboard UIs to compile. Release builds use all four (default);
   # for a lighter dev loop: --no-default-features --features ui-orbit
   default = ["ui-classic", "ui-blades", "ui-ps5", "ui-orbit"]
   ui-classic = []
   ui-blades = []
   ui-ps5 = []
   ui-orbit = []
   ```

2. **New stub files** in a new folder `crates/app/ui/stubs/`, one component each, with the same name as the shell it stands in for:

   `crates/app/ui/stubs/classic.slint`
   ```slint
   // Stand-in when the ui-classic feature is off (see build.rs).
   export component TvShell inherits Rectangle { }
   ```
   `crates/app/ui/stubs/blades.slint`
   ```slint
   // Stand-in when the ui-blades feature is off (see build.rs).
   export component V3Shell inherits Rectangle { }
   ```
   `crates/app/ui/stubs/ps5.slint`
   ```slint
   // Stand-in when the ui-ps5 feature is off (see build.rs).
   export component Ps5Shell inherits Rectangle { }
   ```
   `crates/app/ui/stubs/orbit.slint`
   ```slint
   // Stand-in when the ui-orbit feature is off (see build.rs).
   export component OrbitShell inherits Rectangle { }
   ```

3. **`crates/app/ui/laptop.slint`**: change only the four shell imports:

   ```slint
   import { TvShell } from "tv/shell.slint";
   import { V3Shell } from "v3/shell.slint";
   import { Ps5Shell } from "ps5/shell.slint";
   import { OrbitShell } from "orbit/shell.slint";
   ```

   to:

   ```slint
   // Resolved by build.rs: the real shell, or ui/stubs/* when its Cargo feature is off.
   import { TvShell } from "@shell-classic";
   import { V3Shell } from "@shell-blades";
   import { Ps5Shell } from "@shell-ps5";
   import { OrbitShell } from "@shell-orbit";
   ```

   Leave the `V3Theme` / `PsTheme` / `OrbitTheme` imports and the four `if … : …Shell { }` lines as they are. The theme globals are tiny, and the stubs make the `if` lines harmless.

4. **`crates/app/build.rs`**: replace the whole file with:

   ```rust
   use std::collections::HashMap;
   use std::path::PathBuf;

   fn main() {
       let ui = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("ui");

       // (Cargo feature, library name used in laptop.slint, real shell, stub)
       let shells = [
           ("UI_CLASSIC", "shell-classic", "tv/shell.slint", "stubs/classic.slint"),
           ("UI_BLADES", "shell-blades", "v3/shell.slint", "stubs/blades.slint"),
           ("UI_PS5", "shell-ps5", "ps5/shell.slint", "stubs/ps5.slint"),
           ("UI_ORBIT", "shell-orbit", "orbit/shell.slint", "stubs/orbit.slint"),
       ];

       let mut any = false;
       let mut library_paths = HashMap::new();
       for (feature, lib, real, stub) in shells {
           let on = std::env::var_os(format!("CARGO_FEATURE_{feature}")).is_some();
           any |= on;
           library_paths.insert(lib.to_string(), ui.join(if on { real } else { stub }));
       }
       assert!(
           any,
           "tvdash needs at least one dashboard UI feature (ui-classic, ui-blades, ui-ps5, ui-orbit)"
       );

       let config = slint_build::CompilerConfiguration::new().with_library_paths(library_paths);
       slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile Slint UI");
   }
   ```

   (`with_library_paths` and `compile_with_config` are in `slint-build` 1.18, the version in use. Cargo re-runs `build.rs` when features change.)

5. **Rust: only offer enabled UIs.** In `crates/app/src/bridge/display.rs`, add:

   ```rust
   /// Dashboard UIs compiled into this build (Cargo features `ui-*`), in picker order.
   pub fn enabled_uis() -> Vec<DashboardUi> {
       DashboardUi::ALL
           .iter()
           .copied()
           .filter(|ui| match ui {
               DashboardUi::Classic => cfg!(feature = "ui-classic"),
               DashboardUi::Blades => cfg!(feature = "ui-blades"),
               DashboardUi::Ps5 => cfg!(feature = "ui-ps5"),
               DashboardUi::Orbit => cfg!(feature = "ui-orbit"),
           })
           .collect()
   }

   /// `ui` if it's compiled in, otherwise the first UI that is.
   pub fn usable_ui(ui: DashboardUi) -> DashboardUi {
       let enabled = enabled_uis();
       if enabled.contains(&ui) { ui } else { enabled[0] }
   }
   ```

   Then replace `DashboardUi::ALL` with `enabled_uis()` in these places, and nowhere else (keep `DashboardUi::ALL` in `crates/core`):
   - `display.rs` `dashboard_choices`: `enabled_uis().into_iter().map(|ui| …)` (adjust `|&ui|` to `|ui|`).
   - `callbacks.rs` ~line 454, in `set_ui`: `if !bridge::display::enabled_uis().contains(&mode) { return; }`
   - `tv_nav.rs` `open_dashboard_picker` (~line 252) and the Settings branch around ~line 1943: compute `cur_idx` with `crate::bridge::display::enabled_uis().iter().position(|&u| u == cur).unwrap_or(0)`.

6. **Rust: fall back at startup.** In `crates/app/src/main.rs`, wherever `config.display.dashboard_ui` is passed on (~line 322 `dashboard_ui: …` and ~line 500 `intro::start(…)`), pass `bridge::display::usable_ui(config.display.dashboard_ui)` instead. Compute it once into a local just before the first use. Don't change `config` itself or save anything.

   The ~line 322 value feeds `bridge::init`, which calls `refresh_dashboard_choices`, so the picker and `Display.dashboard-ui` start on the usable UI too. Confirm nothing else reads `config.display.dashboard_ui` at startup: `grep -rn "dashboard_ui" crates/app/src`.

### Check (user, on the host; one at a time)

```bash
# Default (all four): must behave exactly as before
cargo test -j 2 -p tvdash -- --test-threads=1
cargo run -j 2 -- --windowed --layout tv          # picker lists all four

# One UI: lighter build; the picker lists only Orbit
cargo run -j 2 -p tvdash --no-default-features --features ui-orbit -- --windowed --layout tv

# config.toml has dashboard_ui = "classic", built with only ui-ps5:
# starts in PS5, config.toml still says "classic" afterwards
cargo run -j 2 -p tvdash --no-default-features --features ui-ps5 -- --windowed --layout tv

# No UI at all: build.rs fails with the "at least one dashboard UI" message
cargo build -j 2 -p tvdash --no-default-features
```

Ask the user to report peak memory for the default and the single-UI builds if convenient (`/usr/bin/time -v cargo build --release -j 2 -p tvdash`, "Maximum resident set size").

### Commit message

```
Cargo features to build only the dashboard UIs you need

Every build compiled all four shells: 35.8 MB of generated Rust and
more than 6 GB of RAM for the tvdash crate (vs 9-13 MB and ~3.4 GB with
one UI). Add ui-classic / ui-blades / ui-ps5 / ui-orbit features
(default: all four). build.rs points each @shell-* import in
laptop.slint at the real shell or a one-line stub, so disabled shells
aren't compiled. The picker only lists compiled-in UIs, and a config
naming a disabled UI falls back to the first enabled one at startup
(without rewriting config.toml).

Release builds and tests are unchanged (default features).
```

---

## Commit 8 — Document the lighter dev loop

**`GEMINI.md`**, in "Live Testing Before Release", add after the existing `cargo run` example:

```markdown
- **Working on one dashboard UI?** Build only that one; it needs about half the RAM and compiles much faster:
  - `cargo run -j 2 -p tvdash --no-default-features --features ui-orbit -- --windowed --layout tv`
  - Features: `ui-classic`, `ui-blades`, `ui-ps5`, `ui-orbit`. Combine them with commas.
  - **Always do the final live test and the release/RPM build with default features (all four UIs).**
```

Commit message: `Document building a single dashboard UI for the dev loop`

---

## Final verification (user, on the host)

```bash
cargo test -j 2 -p tvdash-core
cargo test -j 2 -p tvdash -- --test-threads=1
cargo clippy -j 2 -p tvdash --all-targets          # no new warnings in touched code
cargo build --release -j 2
./packaging/rpm.sh && ls -l dist/                  # expect ~18–19 MB (was 21.3 MB)
```

Known failing e2e tests with `--include-ignored` (out of date before this branch; don't fix here): `streaming_site_management`, `streaming_sites_open_switch_stop_and_handoff`, `where_to_watch_ranks_by_library_and_services`.

**Manual checks on the TV** (report results):
1. PS5: HUD over full-screen video, Up Next, screensaver (commit 5 list).
2. Switch between all four UIs; Settings › Display focus lands on Dashboard UI each time (commit 6 table).
3. Screensaver CPU% before and after commit 3.
4. Sports: EPL/EFL logos still show.

Don't bump the version. Push and open a PR to `development`:

```bash
git push -u origin efficiency-parity-1
gh pr create --base development --title "Efficiency & parity, phase 1" \
  --body "Implements phase 1 of tvdash-parity-and-efficiency-research.md: strip release binary, league-logo cache, 15 fps screensaver, skip empty session updates, PS5 HUD + screensaver, per-UI Settings focus, ui-* Cargo features."
```

---

## Out of scope (phase 2; each needs a decision or a spike first)

| Item (research doc ref) | Why it waits |
|---|---|
| **B3**: how many UIs ship in 1.0, and which are "legacy" | Product decision for Jason |
| **P1/P2**: `Skin` token global; shared HUD, Ambient, toast and dialogs styled per UI | Design pass first (token names, per-UI values). Commit 5 is the stopgap |
| **P3**: data-driven Settings (`SettingRow` model) | Biggest parity win, but a large change across four UIs; needs its own spec |
| **P4**: focus geometry reported from Slint instead of `tv_nav.rs` tables | Needs a spike (`absolute-position` during scroll/animation) |
| **P5**: parity script + per-UI headless tests | After P1–P3 settle what "parity" means |
| **R1 (part 2)**: bake the −10° rotation into the pre-rendered wall | The seamless vertical repeat breaks if the rotation is baked in; needs design |
| **R2 (part 2)**: pause or slow polling during video, screensaver and sleep | Needs a UI→controller "idle state" command |
| **R3**: persistent models instead of `ModelRc::new(VecModel…)` (59 sites) | Broad, mechanical; separate branch |
| **B4**: `tiberius` → `native-tls` | Already planned in the audit list (0.3–0.8) |
| **B4/B5/B6**: reqwest crypto provider, Slint default features, `rusqlite` bundled | Verify feature names and what each one costs first |
| **B7**: RPMs → GitHub Releases; optional history cleanup | Changes the release process; history rewrite needs a force-push |
