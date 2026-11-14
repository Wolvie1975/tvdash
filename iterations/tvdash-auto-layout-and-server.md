# tvdash — Auto Layout and Server Page (2026-11-14)

Two features in one doc:

- **Part A — Auto layout:** tvdash picks dual-screen or TV-only by itself, based on which screens are on.
- **Part B — Server page:** Plex Dash-style monitoring and library tools inside tvdash.

Companion to `tvdash-v1-plan.md` (M1 window placement, M2 Plex client), `tvdash-tv-only-plan.md` (the TV shell) and `tvdash-dashboard-library-update.md` §7a (Watching now, Popular). **Plex only.** No canvas boards exist for either feature yet.

## Current status (2026-11-14)

- **Part B:** implemented and validated in the project. This includes the Plex admin client, background polling, server summary/activity/library state, stream stop/terminate flows, metadata refresh, and poster/backdrop-picker support, plus the associated Slint UI wiring.
- **Part A:** still remaining. This is the next phase after B and is intentionally deferred until the server/admin work is complete.
- **What is left now:** primarily the auto-layout and display-switching work described in Part A: layout detection, display-change watching, re-exec switching, runtime state reattachment, and final TV-only shell integration.
- **Not blocked by B:** there are no remaining blockers for the completed B feature set. The remaining work is future-phase A work, not a fix-up for Part B.

---

## Remaining work after B

- [ ] Finish Part A: auto layout decision logic, monitor detection, and the 3 s debounce + switch logic.
- [ ] Add the runtime-reexec flow for switching without interrupting playback.
- [ ] Reconnect mpv/browser state from `$XDG_RUNTIME_DIR/tvdash/state.json` after a layout switch.
- [ ] Wire in the GNOME display-change watcher and final TV-only shell behavior.
- [ ] Validate the full automatic layout flow against real dual-screen / laptop-only / TV-only behavior.

# Part A — Auto layout

> Status: still remaining; this is the next task after the completed Part B server/dashboard work.

## A1. What it does

| Screens on at startup | Layout | Notes |
|---|---|---|
| TV **and** laptop panel | **Dual** | As today |
| TV only (lid closed, or panel turned off) | **TV-only** | GNOME turns off `eDP-1` when the lid is shut with the TV connected |
| Laptop panel only (TV off or unplugged) | **Dual, no TV** | On the TV card shows *"TV not connected"*; Play / Open on TV buttons are disabled with that reason |

While running, if the screens change (lid closed or opened, TV plugged in or unplugged), tvdash switches layout **by itself** after a 3 s debounce, and **playback keeps going**.

> **Why switch automatically instead of asking:** when the lid closes, the dashboard is on a screen that just turned off, so there's nowhere useful to show a question. A short toast after the switch ("Switched to TV only — laptop screen is off") is enough. Turn it off with `switch_while_running = false`.

## A2. Config and command line

```toml
# config.toml
[display]
layout = "auto"               # "auto" (default) | "dual" | "tv"
switch_while_running = true   # only used when layout = "auto"
tv_output = "HDMI-A-1"
laptop_output = "eDP-1"
```

- `dual` and `tv` still force one layout, for when the guess is wrong.
- Command line override, used by the re-exec in A5: `tvdash --layout dual|tv`. It wins over the config for that run only.
- *Settings › Displays › Layout:* **Automatic** / Two screens / TV only.

## A3. Deciding the layout

Keep the decision a pure function so it's easy to unit test.

```rust
// crates/core/src/layout.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout { Dual, Tv }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub layout: Layout,
    pub tv_connected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSetting { Auto, Dual, Tv }

pub fn decide(
    setting: LayoutSetting,
    active_outputs: &[String],   // connector names of screens that are ON
    tv_output: &str,
    laptop_output: &str,
) -> Decision {
    let tv = active_outputs.iter().any(|o| o == tv_output);
    let laptop = active_outputs.iter().any(|o| o == laptop_output);

    let layout = match setting {
        LayoutSetting::Dual => Layout::Dual,
        LayoutSetting::Tv => Layout::Tv,
        LayoutSetting::Auto => match (tv, laptop) {
            (true, false) => Layout::Tv,
            _ => Layout::Dual,   // both, laptop only, or (unexpected) neither
        },
    };
    Decision { layout, tv_connected: tv }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn auto_cases() {
        let d = |o| decide(LayoutSetting::Auto, &s(o), "HDMI-A-1", "eDP-1");
        assert_eq!(d(&["HDMI-A-1", "eDP-1"]).layout, Layout::Dual);
        assert_eq!(d(&["HDMI-A-1"]).layout, Layout::Tv);
        let lap = d(&["eDP-1"]);
        assert_eq!((lap.layout, lap.tv_connected), (Layout::Dual, false));
    }
}
```

> **Fallback when connector names don't match** (e.g. the TV is plugged into a different port): treat any active output that isn't `laptop_output` as the TV, and log a warning that suggests the correct `tv_output` value.

## A4. Reading which screens are on

### At startup

Use winit's monitor list. M1 already logs it to fill in the config.

```rust
let active: Vec<String> = event_loop
    .available_monitors()
    .filter_map(|m| m.name())
    .collect();
tracing::info!(?active, "active outputs");
```

> **Verify in M1:** that `MonitorHandle::name()` on GNOME Wayland returns connector names (`HDMI-A-1`, `eDP-1`) and that a turned-off panel is **missing** from the list. If it returns something else (a model name, for example), read the list from Mutter instead (below) and match on the connector.

### While running

GNOME sends a D-Bus signal whenever the screen setup changes:

| What | Value |
|---|---|
| Bus | session |
| Name | `org.gnome.Mutter.DisplayConfig` |
| Path | `/org/gnome/Mutter/DisplayConfig` |
| Signal | `MonitorsChanged` (no arguments) |

On the signal: wait 3 s (closing a lid can fire it several times), then re-read the active outputs and call `decide()` again. If the layout is different, switch (A5).

```rust
// crates/app/src/displays.rs  (zbus = "5")
use futures_util::StreamExt;

#[zbus::proxy(
    interface = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path = "/org/gnome/Mutter/DisplayConfig"
)]
trait DisplayConfig {
    #[zbus(signal)]
    fn monitors_changed(&self) -> zbus::Result<()>;
}

pub async fn watch(tx: tokio::sync::mpsc::Sender<AppEvent>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = DisplayConfigProxy::new(&conn).await?;
    let mut changes = proxy.receive_monitors_changed().await?;
    while changes.next().await.is_some() {
        // debounce: drain anything else that arrives within 3 s
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while changes.next().await.is_some() {}
        }).await;
        let _ = tx.send(AppEvent::DisplaysChanged).await;
    }
    Ok(())
}
```

The UI thread handles `AppEvent::DisplaysChanged` by re-reading `available_monitors()` and calling `decide()`.

> **If the signal isn't there** (not on GNOME): poll the monitor list every 5 s instead. The lid state is also on logind (`org.freedesktop.login1`, property `LidClosed`, system bus), but the screen list alone is enough, because GNOME already turns the panel off when the lid closes.

## A5. Switching without stopping playback

mpv and the browser are **separate processes**, so the simplest safe switch is to restart tvdash in the other layout and let it reconnect to them.

1. Save a small runtime state file, `$XDG_RUNTIME_DIR/tvdash/state.json`:
   ```json
   { "mpv_socket": "/run/user/1000/tvdash-mpv.sock", "browser_pid": 41233, "plex_rating_key": "5521" }
   ```
2. Start the new instance: `std::process::Command::new(std::env::current_exe()?).args(["--layout", "tv"]).spawn()?`, then exit the current one.
3. On startup, if `state.json` exists:
   - **mpv:** connect to the socket. If it answers, show it as playing and keep reporting `/:/timeline` progress.
   - **Browser:** if the PID is still alive, track it again so Stop / Home can close it. A non-child process can't be `wait()`ed on, so check that it's still alive with `kill(pid, 0)` (`nix` crate).
   - If neither answers, delete the file.
4. Show the toast for 5 s on the screen that's now in use.

Rules:

- Only one tvdash runs at a time. Use a lock file in `$XDG_RUNTIME_DIR/tvdash/` and let the new instance wait up to 2 s for the old one to release it.
- Never switch while the search panel has text or a settings screen has unsaved changes. Wait until they close, or 30 s, whichever comes first.
- `switch_while_running = false` → keep the startup decision until the next launch.

## A6. Where it lands in the build

| Piece | When |
|---|---|
| `decide()` + tests, startup check, **"TV not connected"** state in dual | **M1** (cheap, and useful even before TV-only exists) |
| Reconnecting to mpv and the browser from `state.json` | **M2** (mpv) / **M4** (browser): also helps after a crash |
| `MonitorsChanged` watcher + re-exec switch | **v3**, together with the TV-only shell |

Until the TV-only shell exists, `auto` with only the TV on starts **dual** and logs *"TV-only layout not built yet"*.

## A7. Done when

- [ ] `layout = "auto"`: lid open with TV on → dual; lid closed with TV on → TV-only; TV off → dual with "TV not connected".
- [ ] `dual` and `tv` force their layout; `--layout` overrides for one run.
- [ ] `decide()` unit tests pass.
- [ ] Closing the lid while an episode plays switches to TV-only within ~5 s, and the episode doesn't stop or skip.
- [ ] Opening the lid switches back to dual, with the On the TV card showing the same episode and working transport buttons.
- [ ] A streaming site open in the browser survives a switch, and Stop still closes it.
- [ ] `switch_while_running = false` keeps the layout until the next launch.

---

# Part B — Server page (Plex Dash-style)

> Status: complete in the project. Remaining work is in Part A only; no follow-up fixes are required for the B task as currently scoped.

## B1. Scope

| Feature | Where | Status |
|---|---|---|
| Live streams | Library › Watching now | Already planned (§7a). Stays **links only** |
| Most watched | Library › Popular on Republic City | Already planned (§7a) |
| **Server summary** (streams, transcodes, bandwidth, CPU, RAM) | Server page | New |
| **Live graphs** (CPU, RAM, bandwidth local vs remote) | Server page | New |
| **Server activity** (scans, analysis, refreshes) with progress | Server page | New |
| **Libraries:** last scanned, **Scan** / **Scan all** | Server page | New |
| **Stop a stream** (with confirm) | Server page only | New |
| **Refresh metadata** for one title | Show page, movie detail | New |
| **Change poster / backdrop** | Show page, movie detail, Collection page | New |
| Server settings, log viewer | — | **Out:** use Plex Web |

Plex Dash and the server's graph data need **Plex Pass**. The graphs need it too: without it, hide the Graphs section and keep everything else.

## B2. Laptop Server page

Opened from **Settings › Server** and from a **"Server ›"** link in the Watching now header. Laptop is touch-first, same rules as everywhere else (56 px targets, no hover-only actions).

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ ‹ Back    Republic City · Plex 1.4x · ● online                     [Scan all] │
├──────────┬──────────┬──────────┬──────────┬──────────────────────────────────┤
│ STREAMS  │ TRANSCODE│ BANDWIDTH│ CPU      │ RAM                              │
│ 3        │ 1        │ 18 Mb/s  │ 23%      │ 41%                              │
├──────────┴──────────┴──────────┴──────────┴──────────────────────────────────┤
│ CPU  [host ── plex ──]     RAM  [host ── plex ──]    BANDWIDTH [lan ── wan ──]│
│ ╱╲__╱‾╲__                  ‾‾‾‾‾‾‾‾‾‾‾                ╱‾‾╲___╱‾                 │
├────────────────────────────────────┬─────────────────────────────────────────┤
│ STREAMS                            │ ACTIVITY                                │
│ [post] Harbor Lights S1·E3         │ Scanning Movies          ━━━━━━░░ 72%   │
│        Phone · Plex for iOS        │ Analyzing 3 items        ━━░░░░░░ 20%   │
│        Remote · Transcode  [Stop]  │                                         │
├────────────────────────────────────┴─────────────────────────────────────────┤
│ LIBRARIES                                                                     │
│ Movies   1,204 items · scanned 12 min ago                         [Scan]      │
│ TV       318 shows · scanned 2 h ago                              [Scan]      │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Summary tiles:** five equal tiles, value 56 px tabular numerals, label 18 px caps. Streams and transcodes come from `/status/sessions` (the same poller as Watching now). Bandwidth, CPU and RAM come from the newest graph sample.
- **Graphs:** three equal panels, last **3 minutes** (30 samples at 6 s). Two lines per graph: host vs Plex process (CPU, RAM) and LAN vs WAN (bandwidth). Lines use `accent` and `text-secondary`; no poster tint here.
- **Streams:** same row design as Watching now, plus a **Stop** button (B5). tvdash's own stream shows **This TV** and its Stop goes through mpv, not Plex.
- **Activity:** one row per running server task with title, subtitle and a progress bar. **Cancel** only when Plex marks it `cancellable`. Empty state: *"Nothing running on the server."*
- **Libraries:** one row per section with item count, "scanned N ago" and **Scan**. A row shows *"Scanning…"* and a disabled button while it's in Activity.
- Themes: Dark / Stone / Mist like the rest of the laptop.

## B3. TV-only Server screen

**Read-only:** Settings › Server in the rail. Summary tiles and the three graphs at TV sizes (body ≥ 44 px), plus the Activity list. Only one action: **Scan all**. Stopping streams and poster editing stay on the laptop, where touch makes them easy.

## B4. Plex endpoints

All on `{server}` with `Accept: application/json`, `X-Plex-Token` and `X-Plex-Client-Identifier`, as in the v1 plan §6.1. The token from the PIN sign-in is the server owner's, which these calls need.

> **Verify in M2:** the `/statistics/*` endpoints and some admin routes aren't in Plex's public docs. Check each one against your server (`curl` with your token) before building the UI on it, and adjust field names to what comes back.

### Monitoring

| Need | Request | Fields |
|---|---|---|
| Server name, version | `GET /` | `friendlyName`, `version`, `myPlexSubscription` (Plex Pass) |
| Streams | `GET /status/sessions` | Same as §7a, plus `Session.id`, `Session.bandwidth` |
| CPU / RAM | `GET /statistics/resources?timespan=6` | `StatisticsResources[]`: `at`, `hostCpuUtilization`, `processCpuUtilization`, `hostMemoryUtilization`, `processMemoryUtilization` |
| Bandwidth | `GET /statistics/bandwidth?timespan=6` | `StatisticsBandwidth[]`: `at`, `lan` (bool), `bytes`, `deviceID`; plus `Device[]` for names |
| Activity | `GET /activities` | `Activity[]`: `uuid`, `type`, `title`, `subtitle`, `progress`, `cancellable` |
| Cancel activity | `DELETE /activities/{uuid}` | — |

- **Bandwidth per sample:** sum `bytes` per `at` separately for `lan == true` and `false`, then `bits/s = bytes × 8 / 6`.
- Keep the last 30 samples per graph in a ring buffer (`VecDeque`), and drop any with an `at` you've already seen.

### Libraries

| Need | Request |
|---|---|
| Sections | `GET /library/sections` → `key`, `title`, `type`, `scannedAt`, `refreshing` |
| Item count | `GET /library/sections/{key}/all?X-Plex-Container-Start=0&X-Plex-Container-Size=0` → `totalSize` |
| Scan one | `GET /library/sections/{key}/refresh` |
| Scan all | `GET /library/sections/all/refresh` |

### Per title

| Need | Request |
|---|---|
| Refresh metadata | `PUT /library/metadata/{ratingKey}/refresh` |
| Poster choices | `GET /library/metadata/{ratingKey}/posters` → `Metadata[]` (or `Photo[]`): `key`, `thumb`, `selected`, `provider` |
| Set poster | `PUT /library/metadata/{ratingKey}/poster?url={url-encoded key}` |
| Backdrop choices / set | `GET …/arts`, `PUT …/art?url=…` |
| Upload your own (later) | `POST /library/metadata/{ratingKey}/posters` with the image bytes as the body |

### Stop a stream

| Need | Request |
|---|---|
| Stop | `GET /status/sessions/terminate?sessionId={Session.id}&reason={url-encoded text}` |

## B5. Stopping streams: rules

- **Server page only.** Library › Watching now stays links only, so tapping a row there never stops anything by accident.
- Tap **Stop** → confirm sheet: *"Stop Harbor Lights on Phone?"*, an optional message (prefilled *"Stopped from tvdash"*, shown on that device), and **Stop stream** (filled, `alert` colour) / Cancel.
- **This TV:** Stop sends `quit` to mpv and reports `state=stopped`, like the On the TV card's Stop button. No Plex terminate call.
- Requires Plex Pass. If the call fails, show *"Plex didn't stop the stream"* with the HTTP status, and leave the row there.

## B6. Change poster and refresh

- **Show page, movie detail, Collection page:** a **⋯** button (56 px) in the hero, next to Shuffle, opens a menu: **Refresh metadata** · **Change poster** · **Change backdrop**.
- **Refresh metadata:** fire the call, show *"Refreshing…"* on the button, and reload the item when it disappears from `/activities` (or after 30 s).
- **Poster picker:** a full-height sheet with a grid of 2:3 choices, 4 across, the current one ticked, the provider (`tmdb`, `local`, …) under each. Tap → set → close → re-derive the poster tint (Option B) from the new poster.
- **Backdrop picker:** the same sheet with 16:9 tiles, 3 across.
- Requests pass the poster `key` through `/photo/:/transcode?width=240&height=360&url=…` for the thumbnails, so a 4K poster isn't downloaded for every cell.

## B7. Polling and performance

| Data | Interval | Only while… |
|---|---|---|
| `/status/sessions` | 5 s (Library uses 10 s) | Server page open |
| `/statistics/resources`, `/statistics/bandwidth` | 6 s | Server page open |
| `/activities` | 3 s, 1 s while anything is running | Server page open, or a refresh is pending |
| Library sections | On open and after a scan finishes | Server page open |

- Stop all of these when the page closes. During playback the poll cost is tiny, but it's still the same laptop that runs mpv.
- **Later, optional:** replace the `/activities` poll with Plex's notification WebSocket, `ws://{server}/:/websockets/notifications?X-Plex-Token=…` (`tokio-tungstenite`). Messages are `NotificationContainer` objects; `type == "activity"` carries the same activity data.

## B8. Code layout

```text
crates/
├─ media/src/plex/
│  ├─ client.rs        # existing: sign-in, browse, play, timeline
│  └─ admin.rs         # NEW: PlexAdmin (statistics, activities, sections, posters, terminate)
└─ app/ui/
   ├─ shared/graph.slint     # NEW: two-line graph from a Path
   ├─ dual/server.slint      # NEW: laptop Server page
   ├─ dual/poster-picker.slint
   └─ tv/server.slint        # NEW: read-only TV version
```

`PlexAdmin` is separate from the `MediaSource` trait: it's Plex-only admin work, not something the UI browses.

```rust
// crates/media/src/plex/admin.rs
pub struct PlexAdmin { http: reqwest::Client, server: String, token: String, client_id: String }

impl PlexAdmin {
    pub async fn resources(&self) -> Result<Vec<ResourceSample>>;   // /statistics/resources?timespan=6
    pub async fn bandwidth(&self) -> Result<Vec<BandwidthSample>>;  // /statistics/bandwidth?timespan=6
    pub async fn activities(&self) -> Result<Vec<Activity>>;
    pub async fn cancel_activity(&self, uuid: &str) -> Result<()>;
    pub async fn sections(&self) -> Result<Vec<Section>>;
    pub async fn scan(&self, section_key: Option<&str>) -> Result<()>; // None = all
    pub async fn refresh_item(&self, rating_key: &str) -> Result<()>;
    pub async fn posters(&self, rating_key: &str) -> Result<Vec<ArtChoice>>;
    pub async fn set_poster(&self, rating_key: &str, key: &str) -> Result<()>;
    pub async fn terminate(&self, session_id: &str, reason: &str) -> Result<()>;
}
```

### Graph in Slint

Build the SVG path string in Rust (0–100 % mapped to the panel height) and hand it to a `Path`:

```slint
export component Graph inherits Rectangle {
    in property <string> line-a;   // e.g. "M 0 80 L 10 72 L 20 75 …" in a 300×100 box
    in property <string> line-b;
    in property <color> color-a;
    in property <color> color-b;
    Path { width: 100%; height: 100%; viewbox-width: 300; viewbox-height: 100;
           commands: root.line-a; stroke: root.color-a; stroke-width: 3px; }
    Path { width: 100%; height: 100%; viewbox-width: 300; viewbox-height: 100;
           commands: root.line-b; stroke: root.color-b; stroke-width: 3px; }
}
```

```rust
fn path_for(samples: &[f32], max: f32) -> String {
    let n = samples.len().max(2) - 1;
    samples.iter().enumerate().map(|(i, v)| {
        let x = i as f32 * 300.0 / n as f32;
        let y = 100.0 - (v / max).clamp(0.0, 1.0) * 100.0;
        format!("{} {x:.1} {y:.1}", if i == 0 { "M" } else { "L" })
    }).collect::<Vec<_>>().join(" ")
}
```

For bandwidth, scale `max` to the largest sample in the window, rounded up to a clean number (10, 25, 50, 100 Mb/s), and write the scale in the corner.

## B9. Where it lands in the build

**v1.1**, after M2 (it needs the Plex client and token). The **M3** free slot works too if you want it sooner. Order: summary tiles and streams → Activity and Libraries → graphs → Stop → poster picker and refresh → TV read-only screen.

## B10. Done when

- [ ] Server page opens from Settings › Server and from Watching now's "Server ›".
- [ ] Summary tiles show streams, transcodes, bandwidth, CPU and RAM, updating every 5–6 s.
- [ ] CPU, RAM and bandwidth graphs show the last 3 minutes with two lines each; the Graphs section hides without Plex Pass.
- [ ] Starting a library scan from the page shows it in Activity with progress, and the row's button is disabled until it finishes.
- [ ] Stop asks first, stops a phone stream within ~10 s, and shows the message on the phone; Stop on **This TV** goes through mpv.
- [ ] Library › Watching now still has no Stop.
- [ ] Change poster lists choices, sets the new one, and the poster tint updates.
- [ ] Refresh metadata reloads the item when the server finishes.
- [ ] TV-only Server screen shows tiles, graphs and Activity, and has only Scan all.
- [ ] All polling stops when the page closes.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-14 | Doc created: auto layout (Part A) and Plex Dash-style Server page (Part B) |
| 2026-11-14 | Status update: Part B is complete and validated; remaining work is Part A only |
