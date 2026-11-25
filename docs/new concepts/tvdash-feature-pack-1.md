# tvdash — Feature pack 1: build spec for a coding agent

Four features, built as **four branches and four PRs**, in this order:

| Part | Feature | Branch |
|---|---|---|
| **A** | Quick switch (simplified Multiview) | `quick-switch` |
| **B** | Skip intro, Up Next at credits, shared Up Next card | `skip-markers` |
| **C** | Severe-weather alerts (NWS) | `weather-alerts` |
| **D** | Trailers & extras on the Show page | `show-extras` |

**Design reference:** the concept images are in **the same folder as this file**, one per artboard (canvas *tvdash — Feature concepts*: https://claude.ai/artifact/KBcw9Katy1WBouARtH8eFm). **Open the matching image before starting each part.** Each part names its artboards; match them to the images by name:

| Artboard | Part |
|---|---|
| Where each feature lives (`Main`) | All: overview of where each piece mounts |
| Quick switch · overlay while watching (`QuickSwitch`) | A4 |
| Quick switch · Settings picker (`QuickSwitchSettings`) | A5 |
| Skip intro · HUD closed (`SkipIntro`) | B3 |
| Skip intro · HUD open (`SkipHud`) | B3 |
| Credits start → Up Next (`CreditsUpNext`) | B4 |
| Severe weather · Warning takeover (`WeatherTakeover`) | C3, C5 |
| Severe weather · Watch / advisory card (`WeatherAdvisory`) | C4, C5 |
| Show page · Trailer & extras (`ShowTrailer`) | D4 |

The images are drawn in Classic at 1920×1080 design units. Use them for layout, sizes, colours and wording, and build every shared piece so it looks right in all four UIs. The grey boxes in brackets (e.g. `[ episode still ]`) are placeholders for real content; the "Design note" / "How it behaves" panels are notes for you, not UI to build. If an image and this spec disagree, the spec wins; say so in the PR.

**Background docs:** `tvdash-multiview.md` (v4), `tvdash-parity-and-efficiency-research.md`, `tvdash-efficiency-parity-agent.md` (already merged as PR #8).

---

## Ground rules (all parts)

- **Base:** `development` at `6bf0239` (PR #8 merged) or later. Start each part from `development` after the previous part's PR is merged.
- **Follow `GEMINI.md`.** Hand `cargo` test/build commands to the user (`-j 2`, one crate at a time) and wait for results. For the dev loop, use the single-UI features: `cargo run -j 2 -p tvdash --no-default-features --features ui-classic -- --windowed --layout tv`. **Test every UI before each PR**, then do one default-features run.
- **One commit per numbered step.** No refactors, dependency bumps or `cargo fmt` outside the lines you touch.
- **Shared first.** Anything that isn't a page (overlays, cards, pills, pickers) goes in `crates/app/ui/shared/`, mounted once in `laptop.slint` or once per shell. Per-UI edits are listed explicitly; don't add others.
- **New models:** keep a persistent `Rc<VecModel<…>>` and `set_vec` it, rather than `ModelRc::new(VecModel::from(…))` on every update (research doc R3).
- **New key handling** in `tv_nav.rs` goes **before** the per-UI dispatch (where `SportsAlertState` is handled today), with one rect table for each shared overlay at 1920×1080 design units.
- **Plex responses:** before writing a parser, check the real JSON with `curl` against the user's server (commands are given). If a field name differs from this spec, follow the server and say so in the PR.
- Line numbers are approximate. **Stop and report** if an assumption here turns out to be wrong.

**Plex helper for the curl checks** (the user runs this; the token is in the keyring):

```bash
PLEX="https://<server>:32400"          # from Settings › Accounts / config
T="$(secret-tool lookup service tvdash account plex_token)"   # or however --check-keyring reports it
px() { curl -sk -H "Accept: application/json" -H "X-Plex-Token: $T" "$PLEX$1" | jq "${2:-.}"; }
```

---

# Part A — Quick switch

**Artboards:** *Quick switch · overlay while watching*, *Quick switch · Settings picker*.

**What it is:** while you watch a streaming app, a **Switch** overlay (drawn by the Back pill extension inside Brave) shows shortcuts to your other quick-switch apps. Choosing one opens it **the way tvdash opens apps today**: it closes Brave and starts it on the new site. Which apps are in the set is a **Settings option**: *Automatic* (most recent) or *Custom* (you pick up to 4).

### A0. Decisions and defaults

| Topic | Decision (default where Jason hasn't said) |
|---|---|
| The set | Up to 4 apps. Settings › Display › Quick switch: **Automatic** (the 4 most recently opened, filled up in app order) or **Custom** (picked in order, 1–4) |
| Eligible apps | Every `[[streaming]]` app except **Google Meet** |
| Overlay contents | The set **minus the app on screen** (usually 3 tiles); all 4 if the current app isn't in the set. Plus **tvdash Home** |
| Switching | Same as opening from Apps (`open_site`), with an "Opening …" toast |
| Opening the overlay | **Switch** button next to the Back pill, and a **key** while watching: default **F9**, `[tv] quick_switch_key` |
| Inside the overlay | Arrows move, OK/Enter chooses, Back/Escape closes; every key is captured while it's open |
| Recents | Switching updates recents like any open, so Automatic follows what you use |

### A1. Config and the set resolver

#### `crates/core/src/config.rs`

Add two fields to `TvConfig` (keep `recent`):

```rust
pub struct TvConfig {
    /// Last 3 distinct streaming services opened, newest first.
    pub recent: Vec<RecentService>,
    /// Quick switch apps (service ids), in order. Empty = Automatic (most recent first).
    pub quick_switch: Vec<String>,
    /// Key that opens the Quick switch overlay while watching (a `KeyboardEvent.key` value).
    pub quick_switch_key: String,
}
```

`TvConfig` derives `Default`, which would make the key `""`. Replace the derive with a manual `Default`:

```rust
impl Default for TvConfig {
    fn default() -> Self {
        Self { recent: Vec::new(), quick_switch: Vec::new(), quick_switch_key: "F9".into() }
    }
}
```

(Remove `Default` from the `#[derive(...)]` line. Keep `#[serde(default)]`.)

> **Recent list length:** `crate::recent::push_recent` already keeps up to 12 entries (Home's `home_three` takes the first 3), so Automatic mode has enough history. No change is needed there; the `recent` doc comment ("Last 3") is out of date, so correct it to "Recently opened streaming services, newest first (up to 12)".

#### `crates/app/src/apps.rs`

```rust
/// Service ids never offered in Quick switch.
pub const QUICK_SWITCH_EXCLUDED: &[&str] = &["google-meet"];
pub const QUICK_SWITCH_MAX: usize = 4;

/// The Quick switch set, in order: the custom list if set, else the most recent apps
/// filled up in config order. Unknown, excluded and duplicate ids are dropped.
pub fn quick_switch_set(custom: &[String], recent: &[String], all: &[String]) -> Vec<String> {
    let eligible: Vec<String> =
        all.iter().filter(|id| !QUICK_SWITCH_EXCLUDED.contains(&id.as_str())).cloned().collect();
    if custom.is_empty() {
        return apps_bar(recent, &eligible, QUICK_SWITCH_MAX);
    }
    let mut out: Vec<String> = Vec::with_capacity(QUICK_SWITCH_MAX);
    for id in custom {
        if out.len() == QUICK_SWITCH_MAX {
            break;
        }
        if eligible.contains(id) && !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}
```

Tests in `apps.rs`' test module:
- Automatic with recent `[netflix, hulu]` and 6 apps → `[netflix, hulu, <first two others in config order>]`.
- Automatic never includes `google-meet`, even if it's the most recent.
- Custom `[apple-tv, youtube-tv]` → exactly that order. Unknown ids and `google-meet` are dropped, and more than 4 are cut to 4.

#### `crates/core/src/events.rs`, `pub enum Command`

```rust
/// Open a streaming app by service id (Quick switch overlay).
OpenService(String),
/// Settings › Quick switch: empty = Automatic.
SetQuickSwitch(Vec<String>),
```

**Commit:** `Quick switch: config, set resolver and commands`

---

### A2. Controller: open by id, publish the overlay state

#### `crates/app/src/home_cmd.rs`, the new state type (next to `SleepHttpState`)

```rust
/// What the Back pill extension shows in the Quick switch overlay (`GET /quick`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct QuickHttpState {
    /// `KeyboardEvent.key` that opens the overlay.
    pub key: String,
    /// Service id of the app on the TV right now, if any.
    pub current: Option<String>,
    pub tiles: Vec<QuickHttpTile>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuickHttpTile {
    pub id: String,
    pub name: String,
    /// Brand colours as `#RRGGBB`.
    pub fill: String,
    pub ink: String,
    /// The bundled SVG logo as text, if there is one (drawn inline by the extension).
    pub logo_svg: Option<String>,
}
```

#### `crates/app/src/brand.rs`

Add a text accessor for the same bundled logos used by `bundled_logo` (Disney+ uses `disney-plus.svg` here, not the PNG):

```rust
/// Bundled SVG logo as text, for the Back pill extension.
pub fn logo_svg(id: &str) -> Option<&'static str> {
    Some(match id {
        "youtube-tv" => include_str!("../../../assets/logos/youtube-tv.svg"),
        "youtube-music" => include_str!("../../../assets/logos/youtube-music.svg"),
        "youtube" => include_str!("../../../assets/logos/youtube.svg"),
        "disney-plus" => include_str!("../../../assets/logos/disney-plus.svg"),
        "hulu" => include_str!("../../../assets/logos/hulu.svg"),
        "apple-tv" => include_str!("../../../assets/logos/apple-tv.svg"),
        "netflix" => include_str!("../../../assets/logos/netflix.svg"),
        "fandango-at-home" => include_str!("../../../assets/logos/fandango-at-home.svg"),
        "prime-video" => include_str!("../../../assets/logos/prime-video.svg"),
        "crunchyroll" => include_str!("../../../assets/logos/crunchyroll.svg"),
        "tubi" => include_str!("../../../assets/logos/tubi.svg"),
        _ => return None,
    })
}

pub fn hex(c: slint::Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue())
}
```

#### `crates/app/src/controller.rs`

- Add a field `quick_tx: Option<watch::Sender<QuickHttpState>>` with a setter `set_quick_tx`, mirroring `sleep_tx` / `set_sleep_tx`.
- Add `fn publish_quick(&self)`:

  ```rust
  fn publish_quick(&self) {
      let Some(tx) = &self.quick_tx else { return };
      let all: Vec<String> = self.config.streaming.iter().map(|s| crate::brand::service_id(&s.name)).collect();
      let recent: Vec<String> = self.config.tv.recent.iter().map(|r| r.id.clone()).collect();
      let set = crate::apps::quick_switch_set(&self.config.tv.quick_switch, &recent, &all);
      let tiles = set
          .iter()
          .filter_map(|id| {
              let site = self.config.streaming.iter().find(|s| crate::brand::service_id(&s.name) == *id)?;
              let b = crate::brand::brand_for(id, &site.name);
              Some(QuickHttpTile {
                  id: id.clone(),
                  name: site.name.clone(),
                  fill: crate::brand::hex(b.fill),
                  ink: crate::brand::hex(b.ink),
                  logo_svg: crate::brand::logo_svg(id).map(str::to_owned),
              })
          })
          .collect();
      let current = self.browser.as_ref().map(|b| crate::brand::service_id(&b.site));
      tx.send_replace(QuickHttpState { key: self.config.tv.quick_switch_key.clone(), current, tiles });
  }
  ```

  If `Brand` (generated from `brand.slint`) isn't `Send` or `brand_for` needs the UI thread, it doesn't: it only builds colours and an `Image`. If the `Image` makes this awkward on the controller's thread, add a `brand::colors(id) -> (Color, Color)` helper that returns just fill and ink from the same `match`, and use that instead.

- Call `publish_quick()`:
  - once at startup, wherever `sleep_tx` first publishes;
  - at the end of `open_site` (after `push_recent`);
  - in `close_browser` (current becomes `None`);
  - after handling `SetQuickSwitch`.
- Handle the new commands in the command `match`:

  ```rust
  Command::OpenService(id) => {
      match self.config.streaming.iter().position(|s| crate::brand::service_id(&s.name) == id) {
          Some(i) => {
              let name = self.config.streaming[i].name.clone();
              self.emit(AppEvent::Toast(format!("Opening {name}…")));
              self.open_site(i).await;
          }
          None => warn!(%id, "quick switch: no such app"),
      }
  }
  Command::SetQuickSwitch(ids) => {
      self.update_config(|c| c.tv.quick_switch = ids);
      self.publish_quick();
      self.emit(AppEvent::QuickSwitch(self.config.tv.quick_switch.clone()));
  }
  ```

  Use whatever toast event already exists (`AppEvent::Toast` or the equivalent `bridge::toast` path); don't add a new one if there is one.
- Add `AppEvent::QuickSwitch(Vec<String>)` in `crates/core/src/events.rs`, emitted at startup too, so the UI can show the Settings row (commit 5).

#### `crates/app/src/main.rs`

Create the channel next to the sleep one and pass it on:

```rust
let (quick_http_tx, quick_http_rx) = tokio::sync::watch::channel(home_cmd::QuickHttpState::default());
controller.set_quick_tx(quick_http_tx);
```

Then add `quick_http_rx` to the `home_cmd::serve_pill(…)` call (commit 3).

**Check (user):** `cargo test -j 2 -p tvdash-core` and `cargo test -j 2 -p tvdash -- --test-threads=1`

**Commit:** `Quick switch: open apps by id and publish the overlay state`

---

### A3. Pill endpoint: `GET /quick`, `POST /quick`

In `crates/app/src/home_cmd.rs`:

1. Thread a `quick_rx: watch::Receiver<QuickHttpState>` through `serve_pill` → `handle_conn`, exactly like `alert_rx`.
2. Add the routes before the final 404 branch. They sit behind the existing `request_allowed` check, so only the extension and local tools can use them:

   ```rust
   } else if method == "GET" && path == "/quick" {
       let st = quick_rx.borrow().clone();
       let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
       ("200 OK", "application/json", json)
   } else if method == "POST" && path == "/quick" {
       #[derive(serde::Deserialize)]
       struct Req { id: String }
       match serde_json::from_slice::<Req>(&body) {
           Ok(r) if quick_rx.borrow().tiles.iter().any(|t| t.id == r.id) => {
               let _ = cmd_tx.send(Command::OpenService(r.id));
               ("200 OK", "text/plain", "ok\n".into())
           }
           _ => ("400 Bad Request", "application/json", r#"{"error":"not a quick switch app"}"#.into()),
       }
   ```

   (Use the same body variable and parsing style as the existing `POST /sleep` branch.)

3. Tests, next to the existing endpoint tests:
   - `GET /quick` returns the published tiles as JSON, with `key` and `current`.
   - `POST /quick {"id":"<a tile id>"}` sends `Command::OpenService`.
   - `POST /quick {"id":"google-meet"}` (not a tile) → 400, no command.
   - A web-page `Origin` → 403 (already covered by `request_allowed`; one assert for `/quick` is enough).

**Commit:** `Pill endpoint: Quick switch state and open`

---

### A4. Back pill extension: Switch button, key and overlay

#### `extras/back-pill/worker.js`

Add two message types, next to the sleep and alert ones:

```js
if (msg?.t === 'quick?') {
  fetch(BASE + '/quick').then((r) => r.json()).then(reply, () => reply(null));
  return true;
}
if (msg?.t === 'quick') {
  fetch(BASE + '/quick', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: msg.id }),
  }).catch(() => {});
}
```

#### `extras/back-pill/pill.js`

Add this inside the existing IIFE, after the sports alert card. It reuses `root` (the closed shadow root) and the `style` element.

1. **Switch button** next to the Back pill. Show it only when there's at least one tile to switch to. Same pill style; position it left of the Back pill (`right: 420px`, matching `top: 54px`). It fades with the Back pill: hook it into the existing `show()` timer, so it gets the same `opacity` and `pointerEvents` treatment.
2. **Overlay**: a full-screen `div.qs` (`position: fixed; inset: 0; z-index: 2147483647; background: rgba(0,0,0,.72)`). Centred inside it:
   - kicker `QUICK SWITCH`, and a line "Now watching · <current app name>" (omit it if `current` isn't a known app);
   - a row of tiles (3, or 4 if the current app isn't in the set): `width: 360px; height: 202px; border-radius: 24px;`, `background: tile.fill`, logo centred (max 60% width, 50% height), app name below in `tile.ink`;
   - a smaller **tvdash Home** pill below the row;
   - focus ring: `outline: 6px solid #F5A524; outline-offset: 6px`, matching the sleep/alert cards' amber.
3. **Logos:** parse `logo_svg` with `new DOMParser().parseFromString(svg, 'image/svg+xml')` and append `document.importNode(doc.documentElement, true)` into the tile. Don't use `<img src="data:…">`, since some sites' `img-src` CSP blocks data URLs. If there's no logo or parsing fails, show the app name in large text instead.
4. **Data:** fetch `{ t: 'quick?' }` when the page loads and again **each time the overlay opens** (so it's never stale). Tiles = `state.tiles.filter(t => t.id !== state.current)`.
5. **Keys** (`addEventListener('keydown', handler, true)`, capture phase, on `window`):
   - Overlay closed: if `e.key === state.key` (default `F9`), call `e.preventDefault(); e.stopImmediatePropagation();` and open it. Otherwise do nothing, so the player gets every other key.
   - Overlay open: handle `ArrowLeft`/`ArrowRight` (move along the tiles and the Home pill as one list), `ArrowDown`/`ArrowUp` (between the tile row and the Home pill), `Enter` (choose), and `Escape`/`Backspace`/`BrowserBack` (close). `preventDefault` and `stopImmediatePropagation` on **all** keys while open.
6. **Choose a tile:** `chrome.runtime.sendMessage({ t: 'quick', id })`, then dim the overlay and show "Opening <name>…" until the page unloads (tvdash closes Brave). **Home pill:** `chrome.runtime.sendMessage('home')`, the existing Back-to-tvdash action.
7. **Pause first (nice to have):** before sending, pause any playing video (`document.querySelectorAll('video').forEach(v => v.pause())`), so the audio stops at once instead of when Brave exits.
8. Don't open the overlay while the sleep card or the sports alert card is showing (`!card.hidden || !acard.hidden`). Those keep their own focus.

Bump `manifest.json` `"version"` to `"1.1"`. No new permissions are needed: the worker already has `host_permissions` for `127.0.0.1:47800`.

**Check (user, on the TV):** see "Manual checks" below, items 1–6.

**Commit:** `Back pill: Quick switch button, key and overlay`

---

### A5. Settings › Quick switch (all four UIs)

#### Shared state: `crates/app/ui/state.slint`

```slint
export struct QuickSwitchChoice {
    id: string,
    name: string,
    slot: int,        // 1-4 in the custom set, 0 = not in it
    brand: Brand,     // from shared/brand.slint, for the tile look
}

export global QuickSwitch {
    in property <bool> automatic: true;
    in property <string> summary;              // "Automatic · YouTube TV, Netflix, …" or "Custom · …"
    in property <[QuickSwitchChoice]> choices;
    callback set-automatic(bool);
    callback toggle(string /* id */);          // add at the end of the set, or remove (custom mode)
    callback open-picker();
}
```

(Import `Brand` from `shared/brand.slint` in `state.slint` if it isn't already.)

#### Shared picker: `crates/app/ui/shared/quick_switch_picker.slint`

`export component QuickSwitchPicker`, a full-screen page in a fixed 1920×1080 layout scaled by `TvUi.u`, so **one focus-rect table serves every UI**:
- Title "Quick switch", subtitle "Apps shown in the Switch overlay while you watch".
- Row 0: an **Automatic** toggle tile (rect 0). On means the 4 most recent apps.
- Below it, a grid of the eligible apps (5 per row), each a brand tile like the Apps page, with a slot badge `1`–`4` when selected. When Automatic is on, the grid is dimmed and shows the current automatic set's badges, read-only.
- A footer hint: `OK Add/remove · Back Done`.
- Use Classic's `Theme`/`UiTheme` tokens for now, like `TvDashboardPicker` does. It gets restyled with the shared `Skin` later.

Mount it in each shell next to the dashboard picker:

```slint
if TvUi.screen == "quick_switch_picker": QuickSwitchPicker { width: parent.width; height: parent.height; }
```

(Match each shell's existing `TvDashboardPicker` line: `tv/shell.slint` ~129, `v3/shell.slint` ~110, `ps5/shell.slint` ~136, `orbit/shell.slint` ~97.)

#### Settings row in each UI

Add one row on each UI's **Settings › Display** tab, right after "Dashboard UI": label **Quick switch**, value `QuickSwitch.summary`, OK → `QuickSwitch.open-picker()`. Copy that file's own "Dashboard UI" row markup and change only label, value and callback:

| UI | File | Copy from |
|---|---|---|
| Classic | `ui/tv/settings.slint` | the `// 13: Dashboard UI` row (~180) |
| Blades | `ui/v3/settings.slint` | the Dashboard UI row in the right column (~479) |
| PS5 | `ui/ps5/settings.slint` | `// Dashboard UI picker row` (~83) |
| Orbit | `ui/orbit/settings.slint` | the "Dashboard UI" row (~173) |

#### Navigation: `crates/app/src/tv_nav.rs`

- `Page::QuickSwitchPicker`, with screen name `"quick_switch_picker"` and hint `"OK Add/remove · Back Done"` (in the same `match`es as `Page::DashboardPicker`).
- **Focus rects:** handle `Page::QuickSwitchPicker` **once, before the per-UI dispatch** in `tile_rects` (the same place the sports-alert card rects are handled), because the picker layout is shared: rect 0 for the toggle, then one per app in the grid. Take the numbers from the picker's own layout.
- **The new Settings row:** add one rect to each UI's Settings › Display table, right after its Dashboard UI rect, and shift the later indices. Update any hard-coded indices that point past it, including the per-UI picker index from PR #8 (`open_settings_display_focused_on_picker`), if the new row comes before it.
- OK in the picker: tile 0 → `set-automatic(!automatic)`; tile *n* → `toggle(choices[n-1].id)` (ignored while Automatic is on). Back → pop to Settings.

#### Bridge and callbacks

- `bridge`: on `AppEvent::QuickSwitch(custom)` (and on `AppEvent::RecentServices`, since the automatic set changes), rebuild `QuickSwitch.choices` / `automatic` / `summary` with `apps::quick_switch_set`. Keep the choices in a persistent `Rc<VecModel>` (see research doc R3) instead of replacing the model each time.
- `callbacks.rs`:
  - `set-automatic(true)` → `Command::SetQuickSwitch(vec![])`.
  - `set-automatic(false)` → `Command::SetQuickSwitch(<current automatic set>)`, so switching to Custom starts from what you had.
  - `toggle(id)` → add the id at the end (if fewer than 4) or remove it, then `SetQuickSwitch(new)`. Removing the last one switches back to Automatic (empty list).
  - `open-picker()` → `tv_nav::push(Page::QuickSwitchPicker)`, or that module's equivalent helper.

**Check (user):** run `cargo run -j 2 -p tvdash --no-default-features --features ui-<name> -- --windowed --layout tv` for each of `ui-classic`, `ui-blades`, `ui-ps5` and `ui-orbit`: open Settings › Display › Quick switch, toggle Automatic, pick 4, remove one. Then do a default-features run.

**Commit:** `Settings › Quick switch: pick the apps for the Switch overlay`

---

### A6. Docs

- `docs/status.md` / `CHANGELOG.md`: add Quick switch under Unreleased.
- `GEMINI.md`: note that changes to `extras/back-pill/` need Brave restarted, and that tvdash loads the extension unpacked with `--load-extension`.
- `config.toml` example (if there is one in `docs/` or `packaging/`): add the two keys under `[tv]` with comments:

  ```toml
  [tv]
  # Quick switch apps, in order (service ids). Empty = Automatic (most recent).
  quick_switch = []
  # Key that opens the Switch overlay while watching (KeyboardEvent.key).
  quick_switch_key = "F9"
  ```

**Commit:** `Document Quick switch`

---

### A7. Manual checks on the TV (report in the PR)

1. Open YouTube TV from Home. Move the mouse: **Switch** shows next to Back to tvdash. Press **F9**: the overlay shows the other 3 apps, and YouTube TV isn't one of them.
2. While the overlay is open, arrow keys move focus and the video **doesn't** seek or change channel. Escape closes it and the player gets keys again.
3. Choose Apple TV: "Opening Apple TV…", audio stops at once, and Apple TV opens full screen within a few seconds. Note the time it takes in the PR.
4. Open the overlay again: YouTube TV is now one of the tiles.
5. **tvdash Home** in the overlay goes Home, like the Back pill.
6. With the sleep warning card or a sports alert showing, F9 does nothing until it's gone.
7. Settings › Quick switch: Custom with 2 apps → the overlay shows those 2 (minus the current one). Back to Automatic → the most recent 4.
8. Open an app that isn't in the custom set (from Apps): the overlay shows all the set's apps.
9. All four UIs: the Settings row is there and focusable, and the picker works with the remote.
10. `curl -s http://127.0.0.1:47800/quick | jq` shows the tiles. `curl -X POST -H 'Origin: https://example.com' http://127.0.0.1:47800/quick -d '{"id":"netflix"}'` → `forbidden`.

**Open questions for Jason (Part A):** which remote button opens the overlay (default F9; it must be a key Brave receives, not F12)? Exclude YouTube Music too?

---

# Part B — Skip intro, Up Next at credits, shared Up Next card

**Artboards:** *Skip intro · HUD closed*, *Skip intro · HUD open*, *Credits start → Up Next*.

**What it is:** Plex marks where each episode's **intro** and **credits** are (detection needs Plex Pass and has to be turned on in the server's settings). tvdash reads those markers:
- During the intro, a **Skip intro** pill appears with focus on it, so OK skips.
- When the credits start, the **Up Next** card appears with a 10 s countdown, instead of only after the episode ends.

**Also fixes a bug:** the existing Up Next modal is drawn only in `tv/shell.slint` and `v3/shell.slint`. In PS5 and Orbit, `Dash.next-ep-open` turns on with nothing on screen, and `tv_nav::key` then swallows every key until Back. B4 moves the card into a shared layer.

### B0. Decisions and defaults

| Topic | Decision |
|---|---|
| Skip intro | A pill appears while position is inside the **intro** marker. Focus starts on it, so OK skips (seeks to the marker's end). It fades 3 s after the marker ends, or as soon as you skip |
| With the HUD open | The pill sits on top of the HUD panel. ↑ from the tile row reaches it; the transport buttons keep focus |
| Up Next at credits | At the **credits** marker's start: the Up Next card, with **Play now** (focused), **Watch credits**, and a 10 s countdown that plays the next episode |
| Watch credits | Hides the card for this episode. At end of file, today's Up Next prompt appears as usual |
| No markers | Nothing changes: no pill, and Up Next only at end of file (today's behaviour) |
| Auto-skip | `[player] auto_skip_intro = false` (default). When true, intros are skipped automatically from the **second** episode played in a row |
| Scope | Plex episodes only. Movies get the credits marker too, but have no Up Next, so they show nothing |

### B1. Read markers from Plex (`crates/media`)

**Check the shape first (user):**

```bash
# an episode's rating key from Continue Watching, e.g. 12345
px "/library/metadata/12345?includeMarkers=1" '.MediaContainer.Metadata[0].Marker'
# expected: [{"type":"intro","startTimeOffset":…,"endTimeOffset":…}, {"type":"credits",…}]
```

In `crates/media/src/plex.rs`:

```rust
/// A Plex intro/credits marker on an episode or movie (milliseconds).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    #[serde(rename = "type")]
    pub kind: String,            // "intro" | "credits" | (others ignored)
    pub start_time_offset: u64,
    pub end_time_offset: u64,
}
```

- Add `#[serde(default, rename = "Marker")] marker: Vec<Marker>` to the private `Metadata` struct (~line 1223).
- Add a client method:

  ```rust
  /// Intro/credits markers for one item. Empty when the server hasn't analysed it.
  pub async fn markers(&self, rating_key: &str) -> Result<Vec<Marker>> {
      let meta = self
          .metadata(&format!("/library/metadata/{rating_key}"), &[("includeMarkers", "1".into())], "markers")
          .await?;
      Ok(meta
          .into_iter()
          .next()
          .map(|m| m.marker.into_iter().filter(|k| k.kind == "intro" || k.kind == "credits").collect())
          .unwrap_or_default())
  }
  ```

- Add tests with a fixture JSON (copy the real `curl` output above, with titles anonymised): two markers parsed; unknown types dropped; no `Marker` key → empty.

**Commit:** `Plex: read intro and credits markers`

### B2. Controller: track markers during playback

In `crates/app/src/controller.rs`:

- Add to `struct Playback`:

  ```rust
  /// Intro/credits markers for this item (Plex), loaded after playback starts.
  markers: Vec<tvdash_media::plex::Marker>,
  /// Which prompts were already handled for this item.
  intro_prompted: bool,
  credits_prompted: bool,
  credits_dismissed: bool,
  ```

  Initialise them (empty/false) wherever `Playback { … }` is built.
- **Load markers:** when a Plex episode starts playing (where `Playback` is created), spawn `plex.markers(&item.id)` and send the result back on the internal channel as `Internal::Markers { item_id, markers }`. On receipt, store them only if `self.playback` is still the same item.
- **Watch position** in `on_mpv` (the branch that updates `p.status.position_ms`, ~line 2667), once per second:
  - **Intro:** if `pos` is inside an `intro` marker and `!intro_prompted`:
    - if `auto_skip_intro` is on and this isn't the first episode in a row (`self.episodes_in_a_row >= 1`), `seek_to(end / 1000)` and toast "Skipped intro";
    - otherwise emit `AppEvent::SkipPrompt(Some(SkipPrompt { kind: SkipKind::Intro, start_ms, end_ms }))`.

    Set `intro_prompted = true`. When `pos` leaves the marker, emit `AppEvent::SkipPrompt(None)`.
  - **Credits:** if `pos >= credits.start` and `!credits_prompted`, set `credits_prompted = true`. Then, for a Plex episode, spawn `plex.adjacent_episode(&id, true)` (as the end-of-file path does at ~line 2753). If there is one, `fetch_posters` it and send `AppEvent::NextEpisodeAtCredits(next_ep)`.
  - Add `episodes_in_a_row: u32` to `Controller`: +1 when the next episode starts from Up Next (either path), reset to 0 on any other `Play`.
- **New commands** (`crates/core/src/events.rs`, `Command`):

  ```rust
  /// Skip intro: seek to the end of the current intro marker.
  SkipIntro,
  /// Up Next at credits: stop this episode now and play the next one.
  PlayNextNow,
  /// Up Next at credits: keep watching the credits; ask again at end of file.
  WatchCredits,
  ```

  - `SkipIntro`: find the intro marker containing `pos` and `seek_to(end / 1000)`, then emit `SkipPrompt(None)`.
  - `PlayNextNow`: take the pending next episode (keep it on the controller when you emit `NextEpisodeAtCredits`), call `stop_playback()` (this reports Stopped; Plex marks the episode watched past ~90%), then play it as `Command::Play` does.
  - `WatchCredits`: `credits_dismissed = true`. The existing end-of-file path (~line 2744) then runs as today. If not dismissed and the episode reaches end of file anyway (countdown ignored), the end-of-file path also runs as today.
- **New events** (`AppEvent`): `SkipPrompt(Option<SkipPrompt>)` and `NextEpisodeAtCredits(MediaItem)`. Define `SkipPrompt` / `SkipKind` in `tvdash-core` `events.rs` (kind, start_ms, end_ms).
- **Config:** `PlayerConfig` gains `pub auto_skip_intro: bool` (serde default `false`). Add a Settings row only in **B5** (optional).
- **Tests:** unit-test the marker decision as a pure function, e.g. `fn marker_action(pos, &markers, flags) -> Option<MarkerAction>`: inside intro → prompt once; leaving → clear; credits start → once; auto-skip only from the second episode.

**Commit:** `Skip intro and Up Next at credits: controller`

### B3. Skip pill (shared, mounted in each shell's video block)

**State** (`ui/state.slint`, in `Dash`):

```slint
in property <bool> skip-visible: false;
in property <string> skip-label: "Skip intro";
in property <float> skip-progress: 0.0;   // 0 → 1 across the intro, for the bar
callback skip();
```

The bridge sets `skip-visible` / `skip-label` from `SkipPrompt`, and updates `skip-progress` from `AppEvent::Player` positions while visible. It hides 3 s after `SkipPrompt(None)` (use a `slint::Timer`), or at once after `skip()`. `skip()` → `Command::SkipIntro`.

**Component** `ui/shared/skip_pill.slint`, `export component SkipPill`:
- HUD closed (*Skip intro · HUD closed*): pill 84 px tall, light fill `#E8EAED`, dark text, play-to-bar icon (inline SVG `Path`), an `OK` key chip, and a 6 px accent bar along the bottom at `skip-progress`. Above it, the line "Intro · 0:38 left" (from `skip-progress` and the marker length; pass the text in from Rust as `skip-sub`).
- HUD open (*Skip intro · HUD open*): the dark pill style (like the Back pill), 68 px, with the `↑` hint.
- `in property <bool> hud-open` and `in property <length> hud-top` set the position: right edge at 96 px × `TvUi.u`; bottom at 120 px × `TvUi.u` when the HUD is closed, or 24 px above `hud-top` when it's open.

**Mount it** in each shell's full-screen video block, right after the HUD line (4 one-line edits):

| Shell | After | `hud-top` (design px) |
|---|---|---|
| `tv/shell.slint` | `if TvUi.hud-open: TvPlayerHud { }` (~102) | 760 |
| `v3/shell.slint` | `if TvUi.hud-open: V3PlayerHud { }` (~99) | 870 |
| `ps5/shell.slint` | `if TvUi.hud-open: TvPlayerHud { }` (~310) | 760 |
| `orbit/shell.slint` | `if TvUi.hud-open: TvPlayerHud { }` (~283) | 760 |

```slint
if Dash.skip-visible: SkipPill { hud-open: TvUi.hud-open; hud-top: 760px * TvUi.u; }
```

**Keys** (`tv_nav.rs`, full-screen video branch ~line 2297):
- HUD closed and `skip-visible`: OK → `dash.invoke_skip()`. All other keys behave as today; the first OK no longer opens the HUD while the pill shows. Down opens the HUD.
- HUD open and `skip-visible`: Up from the tile row puts focus on the pill (`focused_tile = 10`; add that index to the HUD's clamp). OK there → skip; Down → back to tile 1.

**Scrubber markers (nice to have):** in `tv/player_hud.slint` and `v3/player_hud.slint`, draw the intro and credits ranges as lighter segments on the progress track, from `Dash.marker-ranges: [{start: float, end: float}]` (fractions), with labels "INTRO" and "CREDITS" under them.

**Commit:** `Skip intro pill in every UI`

### B4. Shared Up Next card (replaces the per-UI modals)

- **New** `ui/shared/up_next_card.slint`, `export component UpNextCard` (*Credits start → Up Next*): 600 px card, bottom-right (96 px from the right, 96 px from the bottom, × `TvUi.u`). It uses the **sports alert card** surface (`#1C2026`, border `#2A303A`, radius 26), with the theme accent for the kicker, focus and countdown. Layout: kicker `UP NEXT · S2 · E4` + right label, a 16:9 still (`Dash.next-ep-poster`), title, meta, a countdown bar and buttons.
- **Two modes**, from a new `Dash.next-ep-mode: string`:
  - `"credits"`: buttons **Play now** / **Watch credits**, the countdown bar, and "Playing in N s". A `Timer` counts `Dash.next-ep-countdown` down from 10, then calls `play-next-ep()`.
  - `"end"` (today's end-of-file prompt): buttons **Play** / **Not now**, with no countdown. Keep today's labels if they differ; read them from `tv/shell.slint` ~290.
- **Mount it once** in `laptop.slint`, next to `SportsAlertCard`: `if Dash.next-ep-open: UpNextCard { }`.
- **Delete** the `if Dash.next-ep-open: Rectangle { … }` blocks in `tv/shell.slint` (~290) and `v3/shell.slint` (~285).
- **Bridge:** `NextEpisodeAtCredits(item)` sets the same `next-ep-*` properties as `NextEpisodePrompt` (bridge/mod.rs ~723), plus `next-ep-mode = "credits"`, `next-ep-countdown = 10` and `next-ep-open = true`. `NextEpisodePrompt` sets `next-ep-mode = "end"`.
- **Callbacks:** in credits mode, `play-next-ep` → `Command::PlayNextNow`; `dismiss-next-ep` → `Command::WatchCredits`. In end mode, they do what they do today.
- **Keys:** the existing `next_ep_open` branch in `tv_nav::key` (~2098) already handles ←/→/OK/Back. Leave it, but make sure it runs only while the card is actually drawn, which is now always.
- **Sports alert clash:** while the Up Next card is open, hold the sports alert queue, or place the sports card above it. Pick whichever is simpler in `bridge/alerts.rs` and say which in the PR.

**Commit:** `Shared Up Next card; Up Next at credits (fixes invisible prompt in PS5/Orbit)`

### B5. (Optional) Settings row: Skip intros automatically

One toggle row on each UI's Settings › Playback-or-Display tab (wherever "Sleep" settings live in that UI), bound to `config.player.auto_skip_intro` via `Command::SetAutoSkipIntro(bool)`. Skip this commit if the four rows plus focus rects are more than ~1 hour of work, and say so; the config key still works.

### B6. Manual checks (report in the PR)

1. A Plex episode with markers (check with `curl` first): the pill appears at the intro with focus; OK jumps to the end of the intro; the pill fades.
2. Let the intro play without pressing anything: the pill disappears about 3 s after the intro ends.
3. HUD open during the intro: the pill sits above the panel; ↑ reaches it; OK skips.
4. Credits start: the Up Next card appears with a countdown; at 0 the next episode plays and Continue Watching marks the previous one watched.
5. **Watch credits**: the card closes, and at the end of the file today's prompt appears.
6. An episode with no markers: no pill, and Up Next only at end of file.
7. **PS5 and Orbit:** the Up Next card is visible at end of file and at credits, and keys work (this was broken before).
8. `auto_skip_intro = true`: first episode shows the pill; the next episode skips by itself with a toast.

---

# Part C — Severe-weather alerts (NWS)

**Artboards:** *Severe weather · Warning takeover*, *Severe weather · Watch / advisory card*.

**What it is:** tvdash polls the National Weather Service's active alerts for the saved location (`[general] latitude/longitude`).

| Level | When | What happens |
|---|---|---|
| **Warning** (takeover) | `event` is in `[weather_alerts] takeover` | Full-screen takeover, playback paused (mpv), or the same takeover inside Brave via the Back pill when a streaming app is open. **Got it** → a red banner until it expires. **Resume playback** → unpause |
| **Watch** (card) | Any other event whose name ends in "Warning" or "Watch" | The card in the sports-alert slot, 12 s, once per alert. Video keeps playing |
| **Advisory** | Everything else ("Advisory", "Statement", …) | No card; one line on Home's weather tile (C6, optional) |

US only (NWS). Nothing is shown when latitude/longitude aren't set.

### C1. NWS client (`crates/widgets/src/nws.rs`)

**Check the shape first (user):**

```bash
LAT=…; LON=…   # from ~/.config/tvdash/config.toml [general]
curl -s -H "User-Agent: tvdash (you@example.com)" -H "Accept: application/geo+json" \
  "https://api.weather.gov/alerts/active?point=$LAT,$LON" | jq '.features[0].properties | {id,event,headline,severity,urgency,areaDesc,effective,expires,ends,instruction,messageType}'
# If there are no active alerts, try a state with one, e.g. ?area=OK, to capture a fixture.
```

```rust
//! National Weather Service active alerts (api.weather.gov), US only.

pub const DEFAULT_BASE: &str = "https://api.weather.gov";
pub const REFRESH: Duration = Duration::from_secs(120);
pub const REFRESH_ACTIVE: Duration = Duration::from_secs(60); // while a warning is active

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NwsAlert {
    pub id: String,
    pub event: String,              // "Tornado Warning"
    pub headline: Option<String>,
    pub area: String,               // areaDesc
    pub severity: String,           // Extreme | Severe | Moderate | Minor | Unknown
    pub effective: Option<DateTime<Utc>>,
    pub expires: Option<DateTime<Utc>>,
    pub ends: Option<DateTime<Utc>>,
    pub instruction: Option<String>,
    pub description: Option<String>,
    pub message_type: String,       // Alert | Update | Cancel
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertLevel { Takeover, Card, Advisory }

pub fn level(event: &str, takeover: &[String]) -> AlertLevel {
    if takeover.iter().any(|t| t.eq_ignore_ascii_case(event)) {
        AlertLevel::Takeover
    } else if event.ends_with("Warning") || event.ends_with("Watch") {
        AlertLevel::Card
    } else {
        AlertLevel::Advisory
    }
}

/// Active alerts at a point. `contact` goes in the User-Agent, as NWS asks.
pub async fn fetch(http: &reqwest::Client, base: &str, (lat, lon): (f64, f64), contact: &str) -> Result<Vec<NwsAlert>>;
```

- Request `GET {base}/alerts/active?point={lat:.4},{lon:.4}` with headers `User-Agent: tvdash/<CARGO_PKG_VERSION> (<contact or "no contact set">)` and `Accept: application/geo+json`. Parse `features[].properties`. Skip `messageType == "Cancel"` and anything whose `expires` is in the past.
- "Until" time shown in the UI: `ends` if present, else `expires`.
- **Tests** with the fixture: parsing, `level()` for Tornado Warning (Takeover with the default list), Winter Storm Watch (Card), Wind Advisory (Advisory), expired dropped, Cancel dropped.

**Config** (`crates/core/src/config.rs`): a new `[weather_alerts]` section, `WeatherAlertsConfig`:

```rust
pub struct WeatherAlertsConfig {
    pub enabled: bool,              // default true
    /// Email or URL for the NWS User-Agent (they ask for one). Empty is allowed.
    pub contact: String,
    /// Events that take over the screen and pause playback.
    pub takeover: Vec<String>,      // default below
}
// default takeover:
// ["Tornado Warning", "Severe Thunderstorm Warning", "Flash Flood Warning",
//  "Extreme Wind Warning", "Hurricane Warning", "Tsunami Warning"]
```

**Poller:** in `crates/widgets/src/lib.rs` `spawn`, add a task next to the weather one, with the same location logic and the same `general` watch for location changes. Poll every `REFRESH`, or `REFRESH_ACTIVE` while any takeover-level alert is active. On error, retry after 5 min and report `Service::Weather` health with an "alerts:" prefix. Send `AppEvent::WeatherAlerts(Vec<NwsAlert>)` on every successful poll, including empty lists. Add `nws` to `Endpoints`, so tests can point it at a fake server.

**Commit:** `NWS active alerts: client, config and poller`

### C2. Controller and bridge: decide what to show

- **Controller** (`AppEvent::WeatherAlerts` is UI-bound, but the pause has to happen in the controller): add `Command::WeatherTakeover(bool)`. The bridge sends `true` when it shows a takeover and `false` when it's dismissed.
  - `true`: if mpv is playing, `set_pause(true)` and remember `paused_for_weather = true`.
  - `false` with "resume": unpause only if `paused_for_weather`.
- **Bridge** (`bridge/weather_alerts.rs`, new):
  - Keep `seen: HashMap<id, Level>` and `dismissed: HashSet<id>`. Persist `dismissed` (with each alert's `expires`) in the same runtime-state file the sports alerts use, so a restart doesn't take over again. Drop entries once they expire.
  - A **new takeover** (a takeover-level id not dismissed): fill `WeatherAlertState` and set `visible = true`, then send `Command::WeatherTakeover(true)`. If it's an **Update** to a dismissed id (same event, later `ends`), show it again.
  - A **new card**: queue it in the sports-alert slot (C4).
  - **Advisories:** `Dash.weather-alert-line = "Wind Advisory until 6 PM"` (the most severe one), or empty.
  - **Banner:** `WeatherAlertState.banner` is the text of the most severe takeover-level alert that's active and dismissed, else empty.
  - Publish `WeatherHttpState` for the pill (C5).

**State** (`ui/state.slint`):

```slint
export global WeatherAlertState {
    in-out property <bool> visible: false;      // takeover on screen
    in property <string> alert-id;
    in property <string> event;                 // "Tornado Warning"
    in property <string> area-until;            // "[county] · until 9:45 PM"
    in property <string> instruction;
    in property <string> issued-line;           // "Issued 9:12 PM · Source: weather.gov · Alerts for your saved location"
    in property <string> expires-in;            // "33"
    in property <string> banner;                // "" = no banner
    in-out property <int> focus: 0;             // 0 Got it, 1 Resume playback
    in property <bool> can-resume;              // playback was paused for this
    callback got-it();
    callback resume();
}
```

Update `expires-in` once a minute with a `slint::Timer` in the bridge.

**Commit:** `Weather alerts: bridge state, dismissals and pause`

### C3. Takeover and banner (shared, in `laptop.slint`)

- `ui/shared/weather_warning.slint`, `export component WeatherWarning` (*Severe weather · Warning takeover*): full window, background `#140A0A`, a 14 px `#F0605D` top strip, and a warning icon (inline SVG `Path`) with the kicker `NATIONAL WEATHER SERVICE · WARNING`. A "Playback paused" chip shows only when `can-resume`. Below: the 120 px event title, the area-until line, a 34 px instruction (max 3 lines, elided), and the issued line. On the right, an "Expires in N min" box. At the bottom: **Got it** (light, focused) and **Resume playback** (only when `can-resume`), plus the hint text from the artboard.
- `ui/shared/weather_banner.slint`, `export component WeatherBanner`: a 56 px `#F0605D` strip across the top with dark text: `⚠ Tornado Warning · until 9:45 PM`. It's shown when `banner != ""` and the takeover isn't visible.
- **Mount both** in `laptop.slint`, after the shells and before `SleepPicker`, so toasts and the sleep warning stay on top:

  ```slint
  if WeatherAlertState.banner != "" && !WeatherAlertState.visible: WeatherBanner { width: root.width; }
  if WeatherAlertState.visible: WeatherWarning { width: root.width; height: root.height; }
  ```

- **Keys** (`tv_nav::key`, before the screensaver and Up Next checks): while `visible`, ←/→ move `focus`, OK → `got-it()` / `resume()`, and Back → `got-it()`. Consume every key.
- The takeover also closes the screensaver (`TvUi.ambient = false`) and the HUD.
- **Testing hook:** add `tvdash --weather-test takeover|card|advisory|clear`, using the same socket path as `--alert-test`. It injects a fake `NwsAlert` list into the bridge, so the user can see each state without waiting for weather.

**Commit:** `Severe-weather takeover and banner (all UIs)`

### C4. Watch card (shared slot with sports alerts)

- `ui/shared/weather_card.slint`, `export component WeatherCard` (*Severe weather · Watch / advisory card*): the sports card's size and surface, with a warn-coloured kicker (`WEATHER · WATCH`, `#E5A84B`), a "Your location" chip, a 30 px title, a one-line summary (until + headline), and **Dismiss** (focused) / **Details**, plus "Closes in N s".
- Mount it in `laptop.slint` at the **same position** as `SportsAlertCard`, with its own `WeatherCardState` global (title, sub, visible, focus, closes-in).
- **Queue:** in `bridge/alerts.rs`, a weather card takes priority. While it's visible, hold the sports queue's countdown, the same way the sleep warning holds it, if that's how it works today; otherwise add a `held: bool` to `SportsAlertsUi`. **Details** opens the takeover component in a "details" mode: no pause, and **Close** instead of the two buttons.
- **Keys:** handle it next to the sports card in `tv_nav::key`, with the same ←/→/OK/Back pattern.

**Commit:** `Weather watch card in the alert slot`

### C5. Back pill: weather over streaming apps

When a streaming app is in front, tvdash's window is behind Brave, so the pill extension draws the alert inside the page.

- `home_cmd.rs`: `GET /weather` → `WeatherHttpState { level: "takeover"|"card"|"none", id, event, area_until, instruction, issued_line, closes_in }`, published by the bridge through a `watch` channel, like `/alerts`. `POST /weather {"id","action":"got-it"|"resume"|"dismiss"|"details"}` → the bridge handles it as if pressed on the TV. Both routes sit behind `request_allowed`.
- `worker.js`: `weather?` / `weather` messages, as for alerts.
- `pill.js`: poll `weather?` every 3 s, alongside `alert?`.
  - **takeover:** pause every `<video>`, then show a full-viewport shadow-DOM overlay matching the artboard: Got it (calls `got-it`; the video stays paused) / Resume (calls `resume` and plays the videos it paused). While it's open, capture keys (capture phase), as the Quick switch overlay does.
  - **card:** show it in the sports card's place, with the same countdown and Dismiss / Details.
  - The weather card takes the sports card's slot; don't show both.

**Commit:** `Back pill: weather takeover and card over streaming apps`

### C6. (Optional) Advisory line on Home

Each UI's Home weather tile shows `Dash.weather-alert-line` under the conditions in the warn colour, when it isn't empty. That's 4 small edits: find each UI's weather text on Home (`tv/home.slint`, `v3/home.slint`, `ps5/home.slint`, `orbit/home.slint`). Skip this if a UI's weather tile has no room, and list which ones you skipped.

**Commit:** `Weather advisory line on Home`

### C7. Manual checks (report in the PR)

1. `tvdash --weather-test takeover` while a Plex episode plays: the takeover appears, mpv pauses, Got it → red banner, Resume → plays.
2. The same while YouTube TV is open: the takeover appears inside Brave and the video pauses; Got it / Resume work.
3. `--weather-test card`: the card appears in the sports slot, the video keeps playing, and it closes after 12 s. With a sports alert queued, the weather card shows first.
4. `--weather-test clear`: the banner goes away.
5. Restart tvdash with the fake takeover still injected and dismissed: no second takeover.
6. All four UIs: takeover, banner and card render and take keys.
7. Live: with `RUST_LOG=info`, the log shows an NWS poll every 2 min and no errors; Settings › Diagnostics lists weather health.

---

# Part D — Trailers & extras on the Show page

**Artboard:** *Show page · Trailer & extras*.

**What it is:** Plex lists trailers and other extras (behind the scenes, interviews, featurettes, deleted scenes) for an item. The Show page gets:
- a **Trailer** button in its action row, when there's a trailer;
- an **Extras** tab after the seasons, reusing the episode grid.

Both play in mpv, and Back returns to the Show page. Movies have no detail page in tvdash, so movie extras are out of scope.

### D1. Read extras from Plex (`crates/media`)

**Check the shape first (user):**

```bash
px "/library/metadata/<show rating key>/extras" '.MediaContainer.Metadata[] | {ratingKey,title,type,subtype,extraType,duration,thumb}'
# Also check the show itself for a primary trailer:
px "/library/metadata/<show rating key>" '.MediaContainer.Metadata[0].primaryExtraKey'
```

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ExtraItem {
    pub item: MediaItem,          // playable (kind Other, resume_ms None)
    pub kind: ExtraKind,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraKind { Trailer, BehindTheScenes, Interview, Featurette, DeletedScene, Short, Other }

impl ExtraKind {
    pub fn label(self) -> &'static str { /* "Trailer", "Behind the scenes", … */ }
}

/// Extras for a show or movie, trailers first, then in Plex's order.
pub async fn extras(&self, rating_key: &str) -> Result<Vec<ExtraItem>>;
```

- Map `subtype` (`"trailer"`, `"behindTheScenes"`, `"interview"`, `"featurette"`, `"deletedScene"`, `"short"`). If `subtype` is missing, fall back to the numeric `extraType` (Plex: 1 trailer, 5 behind the scenes, 6 interview, 7 scene/featurette…; confirm against the curl output).
- `into_media_item` already maps unknown types (`"clip"`) to `MediaKind::Other`. Make sure the item keeps its `Media/Part` key, so `stream_url` works for it, and set `resume_ms = None`.
- **Tests** with the fixture: kinds mapped; trailers sorted first; empty response → empty list.

**Commit:** `Plex: read trailers and extras`

### D2. Controller: load extras with the Show page

- In `open_show` (~line 1494), after the show detail is sent, spawn `plex.extras(&show_key)` **separately**, so the page doesn't wait for it. Send `AppEvent::ShowExtras { show_key, extras }`. Fetch the extras' thumbnails through the same path the episode stills use.
- `Command::PlayExtra(String /* rating key */)`: look the item up in the last extras list for the open show and play it like `Command::Play`, but with no resume, no Up Next, no sleep "end of item" handling, and no recent-list changes. When it ends or stops, return to the Show page (as Back from Plex playback does today).

**Commit:** `Load and play show extras`

### D3. Bridge and shared state

`ShowPageState` (`ui/state.slint`) gains:

```slint
in property <bool> has-trailer: false;
in property <string> trailer-meta;      // "2:14 · plays here"
in property <int> extras-count: 0;
callback play-trailer();
callback play-extra(string);
```

- On `ShowExtras` for the open show: set `has-trailer` / `trailer-meta` from the first `Trailer`. If there are extras, **append a tab** to the `seasons` model: `SeasonTabData { rating-key: "extras", index: -1, title: "Extras", progress: "<count>", fraction: 0 }`.
- When `select-season("extras")` comes in: fill the `episodes` model from the extras, each as `EpisodeRowData { rating-key, num: "", title, meta: "Trailer · 2:14", summary: "", progress: 0, watched: false, can-resume: false, thumb }`. Set `season` (the season column) to a column built from the show itself: name "Extras", meta "<n> videos", poster = show poster.
- `play-episode(key, _)` while `selected-season == "extras"` → `Command::PlayExtra(key)`. `play-trailer()` → `Command::PlayExtra(<first trailer key>)`.
- Reset `has-trailer`, `extras-count` and any extras tab when another show opens.

**Commit:** `Show page state: trailer and Extras tab`

### D4. Per-UI: Trailer button, and checking the Extras tab

The Extras tab comes through the shared `seasons` model, but each UI draws seasons its own way. For each UI:

| UI | File | Trailer button | Extras tab check |
|---|---|---|---|
| Classic | `tv/show.slint` | In the action row after "From S1 · E1" (y ≈ 452; same tile style, 240 wide), visible only when `has-trailer` | Season tabs at y ≈ 620 read `title`, so "Extras" shows. The season column shows the show poster |
| Blades | `v3/show.slint` | Same place in its action row | Check it doesn't format tabs as "Season {index}" |
| PS5 | `ps5/show.slint` | Same place in its action row | PS5 reads seasons in 12 places: check each copes with `index: -1` / `rating-key: "extras"` |
| Orbit | `orbit/show.slint` | Same place in its action row | The episode ring should draw 0 progress segments for extras; if the ring looks wrong with them, show a plain list for the Extras tab |

- **`tv_nav.rs`:** in each UI's Show page rects, add the Trailer button after the From-start button (only when `has-trailer`, the same way other conditional rects are handled), and shift the later indices. The Extras tab needs no new rects if tabs and the grid are generated from the model counts.
- If any UI's Show page can't take the button without a layout change, put the trailer in that UI's Extras tab only, and say which in the PR.

**Commit:** `Trailer button and Extras tab in all four UIs`

### D5. Manual checks (report in the PR)

1. A show with a trailer: the Trailer button appears; OK plays it full screen; Back / Stop returns to the Show page.
2. A show without extras: no button and no Extras tab, and the layout is unchanged.
3. The Extras tab: tiles have stills, titles and "Kind · duration"; OK plays; nothing gets a resume point or progress bar; Continue Watching doesn't change.
4. All four UIs: the button is focusable with the remote, and the Extras tab and its grid work.
5. Open another show right after: no stale trailer or tab.

---

# Finishing each part

```bash
cargo test -j 2 -p tvdash-core
cargo test -j 2 -p tvdash-media
cargo test -j 2 -p tvdash-widgets
cargo test -j 2 -p tvdash -- --test-threads=1
cargo clippy -j 2 -p tvdash --all-targets          # no new warnings in touched code
cargo build --release -j 2
```

Then, for each part:

```bash
git push -u origin <branch>
gh pr create --base development --title "<Part title>" \
  --body "Implements Part <X> of tvdash-feature-pack-1.md. Manual check results: …"
```

Known failing e2e tests with `--include-ignored` (out of date before this work; don't fix them here): `streaming_site_management`, `streaming_sites_open_switch_stop_and_handoff`, `where_to_watch_ranks_by_library_and_services`.

Docs per part: `CHANGELOG.md` (Unreleased), `docs/status.md`, and the config example for new keys (`[tv] quick_switch*`, `[player] auto_skip_intro`, `[weather_alerts]`). Don't bump the version.

## Open questions for Jason

1. **Quick switch key:** which remote button? (Default F9.)
2. **Quick switch:** exclude YouTube Music too?
3. **Weather contact:** the email to put in the NWS User-Agent (`[weather_alerts] contact`)?
4. **Weather takeover list:** keep the default six warning types, or add others (e.g. Winter Storm Warning, Blizzard Warning)?
5. **Auto-skip intros:** add the Settings row (B5), or is the config key enough?
