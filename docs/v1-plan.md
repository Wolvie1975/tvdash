# tvdash — Version 1 Build Plan

A Rust + Slint desktop app for a Fedora laptop connected to a TV.

- **Laptop panel** → mouse-driven control dashboard (clock, weather, ~~calendar, homelab,~~ what's on the TV, streaming tiles, library).
- **TV (HDMI, primary display)** → playback only: Plex/Jellyfin through mpv, streaming sites in a full-screen browser, a dim idle screen otherwise.

Design canvas (screens, states, scope): https://claude.ai/artifact/UggZzEChLyVVffZNfdGnTf, page **Version 1 build**.

---

## 0. Scope change — 2026-11-11

> **Calendar and homelab are removed from tvdash entirely.** No next-event card, no Google Calendar link, no Portainer or Uptime Kuma checks. The laptop header is now: clock and date, theme switch, weather, Settings.

What that changes in this plan (each spot is also marked ~~struck through~~ or **REMOVED** below):

| Section | Change |
|---|---|
| §1 Decisions | Calendar and Homelab rows dropped |
| §2 Scope | Header is clock + weather only |
| §3 Screens | First-run setup is 3 steps (Plex → Jellyfin → streaming); no calendar sign-in state |
| §4 Workspace | `widgets` crate holds weather only (or fold weather into `core`) |
| §5 Config | Drop `[calendar]` and `[homelab]`; drop the `calendar_ics_url`, `portainer_api_key` and `uptime_kuma_api_key` keyring entries |
| §6.5, §6.6 | Removed |
| §9 M5 | Becomes "Weather" only |
| §10 Checklist | No iCal address, Portainer token or Uptime Kuma page needed |

If any of this code already exists, it can simply be deleted: nothing else depends on it.

Laptop dashboard redesign in progress on the canvas page **Laptop redesign** (touch-first, poster-tinted, Dark / Stone / Mist).

---

## 1. Decisions locked for V1

| Topic | Decision |
|---|---|
| UI toolkit | Slint, one process, two windows |
| Desktop | Fedora 44 Workstation, GNOME on Wayland |
| Displays | TV is the **primary** display; laptop panel is secondary |
| Input | Mouse/keyboard on the laptop only; TV has no input |
| Media servers | Plex and Jellyfin, **one account (yours)** on each |
| Streaming sites | Netflix, Disney+, Hulu, Prime Video, Apple TV+, YouTube TV |
| Streaming playback | Google Chrome (or Firefox) with a dedicated profile, full screen on the TV |
| Local playback | mpv, controlled over its JSON IPC socket |
| Weather | Open-Meteo (no API key) |
| ~~Calendar~~ | ~~Google Calendar via its secret iCal address~~ — **removed** |
| ~~Homelab~~ | ~~Portainer + Uptime Kuma~~ — **removed** |
| Secrets | GNOME Keyring through the Secret Service API; never in the config file |
| Defaults | °F, 24-hour clock |

> ~~**Calendar change from the design:**~~ *(No longer applies: calendar removed.)* Google's TV/device-code sign-in only allows sign-in, Drive and YouTube scopes, not Calendar. V1 therefore reads the calendar's private iCal URL instead. It's read-only, needs no Google Cloud project, and never expires. The setup screen's calendar step becomes "paste your secret address".

---

## 2. V1 scope

### In V1

| Area | Includes |
|---|---|
| **Plex** | PIN sign-in (plex.tv/link), server discovery, active sessions, continue watching, recently added, search, posters, play on TV, progress sync |
| **Jellyfin** | Quick Connect sign-in (password fallback), active sessions, continue watching, recently added, search, posters, play on TV, progress sync |
| **Streaming** | Six tiles that open each site full screen on the TV; one-time sign-in inside the dedicated browser profile |
| **Playback** | mpv full screen on the TV; pause/resume, ±seek, stop from the laptop; paused overlay on the TV |
| **Header** | Clock, weather ~~, next calendar event, homelab up/down~~ |
| **Accounts** | First-run setup, Settings › Accounts, expired-sign-in states |

### Not in V1 (later)

Night mode · headlines widget · alerts ticker and severe-weather takeover · burn-in pixel shift · stale/error states for every widget (beyond sign-in) · multiple users/profiles · phone or TV-remote control · 12 h and seconds options in settings.

---

## 3. Screens

| Window | Screen | Purpose |
|---|---|---|
| Laptop | **First-run setup** | 3 steps: Plex code → Jellyfin Quick Connect → streaming sign-ins ~~(calendar step removed)~~ |
| Laptop | **Control dashboard** (home) | Header (clock, weather ~~, next event, homelab~~); "On the TV" card with transport buttons; Watch on TV tiles; Continue Watching row; Settings link |
| Laptop | **Library** | All / Plex / Jellyfin filter, search, Continue Watching, Recently Added, detail panel with Resume on TV |
| Laptop | **Settings › Accounts** | Status of each connection; Sign out / Reconnect; "Sign in on TV" for streaming sites; clear browser profile |
| TV | **Idle** | Dim clock, "Ready — pick something on the laptop", up next |
| TV | **Playback overlay** | Shown on pause/start, hides 5 s after resume: title, progress, end time |
| TV | Streaming | The browser itself; nothing to design |

The sign-in states to build: an expired server on the "On the TV" card, Continue Watching with one source missing, ~~the calendar header chip turned into a Reconnect button,~~ and a laptop toast while you sign in to a streaming site.

---

## 4. Workspace layout

```text
tvdash/
├─ Cargo.toml              # [workspace]
├─ crates/
│  ├─ core/                # config, keyring, shared models, app events
│  ├─ media/               # Plex + Jellyfin clients behind MediaSource
│  ├─ player/              # mpv IPC, browser launcher
│  ├─ widgets/             # weather poller (calendar, homelab removed)
│  └─ app/                 # Slint UI: laptop window + TV window, main()
│     └─ ui/               # .slint files
└─ assets/fonts/           # Inter (bundled)
```

### Root `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
version = "0.1.0"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
keyring = "3"
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
image = "0.25"
slint = "1"
directories = "5"
async-trait = "0.1"
```

> **Check versions:** these are starting points. Run `cargo add` per crate to pull current releases, and check the `keyring` crate's feature flags for the Secret Service backend on Linux.

### The `MediaSource` trait (in `media`)

```rust
#[async_trait::async_trait]
pub trait MediaSource: Send + Sync {
    fn kind(&self) -> SourceKind;                       // Plex | Jellyfin
    async fn now_playing(&self) -> Result<Vec<Session>>;
    async fn continue_watching(&self) -> Result<Vec<MediaItem>>;
    async fn recently_added(&self, limit: u32) -> Result<Vec<MediaItem>>;
    async fn search(&self, query: &str) -> Result<Vec<MediaItem>>;
    async fn poster(&self, item: &MediaItem, max_h: u32) -> Result<Vec<u8>>;
    async fn stream_url(&self, item: &MediaItem) -> Result<String>;
    async fn report_progress(&self, item: &MediaItem, pos_ms: u64, state: PlayState) -> Result<()>;
}
```

The UI only sees `MediaItem` / `Session` with a `source: SourceKind` field for the badge.

---

## 5. Config and secrets

### `~/.config/tvdash/config.toml`

```toml
[display]
tv_output = "HDMI-A-1"      # connector names; list them in M1
laptop_output = "eDP-1"

[general]
units = "fahrenheit"
clock = "24h"
latitude = 0.0              # set yours
longitude = 0.0
timezone = "America/Chicago"

[plex]
enabled = true
server_url = ""             # filled after PIN sign-in + discovery
client_id = ""              # generated UUID, created on first run

[jellyfin]
enabled = true
server_url = "http://republic-city:8096"
user_id = ""                # filled after sign-in
device_id = ""              # generated UUID

# [calendar] and [homelab] removed (scope change 2026-11-11)

[browser]
binary = "google-chrome"
profile_dir = "~/.local/share/tvdash/browser"

[player]
mpv = "mpv"

[[streaming]]
name = "Netflix"
url = "https://www.netflix.com"
[[streaming]]
name = "Disney+"
url = "https://www.disneyplus.com"
[[streaming]]
name = "Hulu"
url = "https://www.hulu.com"
[[streaming]]
name = "Prime Video"
url = "https://www.primevideo.com"
[[streaming]]
name = "Apple TV+"
url = "https://tv.apple.com"
[[streaming]]
name = "YouTube TV"
url = "https://tv.youtube.com"
```

### Keyring entries (service name `tvdash`)

| Key | Holds |
|---|---|
| `plex_token` | Plex auth token from the PIN flow |
| `jellyfin_token` | Jellyfin access token |
| ~~`calendar_ics_url`~~ | ~~Google secret iCal address~~ — removed |
| ~~`portainer_api_key`~~ | ~~Portainer access token~~ — removed |
| ~~`uptime_kuma_api_key`~~ | ~~Uptime Kuma~~ — removed |

```rust
let entry = keyring::Entry::new("tvdash", "jellyfin_token")?;
entry.set_password(&token)?;
let token = entry.get_password()?;
```

---

## 6. Integration reference

> **Verify against your servers:** endpoint paths below are the commonly documented ones. Jellyfin moved some per-user routes in 10.9 (e.g. `/UserItems/Resume` alongside the older `/Users/{id}/Items/Resume`), so confirm against your server's `/api-docs/swagger`.

### 6.1 Plex

| Step | Request |
|---|---|
| Create PIN | `POST https://plex.tv/api/v2/pins` (headers `X-Plex-Product: tvdash`, `X-Plex-Client-Identifier: <uuid>`, `Accept: application/json`) → `id`, `code` (4 chars for plex.tv/link) |
| Show code | "Go to plex.tv/link and enter **CODE**" |
| Poll | `GET https://plex.tv/api/v2/pins/{id}` every 2 s until `authToken` is set; codes expire |
| Find servers | `GET https://plex.tv/api/v2/resources?includeHttps=1` with `X-Plex-Token` → pick the server, prefer a local connection |
| Now playing | `GET {server}/status/sessions` |
| Continue watching | `GET {server}/library/onDeck` (or `/hubs/continueWatching`) |
| Recently added | `GET {server}/library/recentlyAdded` |
| Search | `GET {server}/hubs/search?query=...` |
| Poster | `GET {server}{thumb}?X-Plex-Token=...` or `/photo/:/transcode?width=..&height=..&url={thumb}` |
| Stream | `{server}{Media[0].Part[0].key}?X-Plex-Token=...` |
| Progress | `GET {server}/:/timeline?ratingKey=..&key=..&state=playing|paused|stopped&time=<ms>&duration=<ms>` |

Send `Accept: application/json`, `X-Plex-Token` and `X-Plex-Client-Identifier` on every server call.

### 6.2 Jellyfin

Every request carries this header:

```text
Authorization: MediaBrowser Client="tvdash", Device="tv-laptop", DeviceId="<uuid>", Version="0.1.0", Token="<token once you have it>"
```

| Step | Request |
|---|---|
| Quick Connect start | `POST /QuickConnect/Initiate` → `Code` (show it), `Secret` |
| Poll | `GET /QuickConnect/Connect?secret=...` until `Authenticated: true` |
| Exchange | `POST /Users/AuthenticateWithQuickConnect` body `{"Secret": "..."}` → `AccessToken`, `User.Id` |
| Password fallback | `POST /Users/AuthenticateByName` body `{"Username": "...", "Pw": "..."}` |
| Now playing | `GET /Sessions?activeWithinSeconds=60` |
| Continue watching | `GET /UserItems/Resume` (10.9+) or `/Users/{userId}/Items/Resume` |
| Recently added | `GET /Items/Latest?userId=...` or `/Users/{userId}/Items/Latest` |
| Search | `GET /Items?searchTerm=...&IncludeItemTypes=Movie,Series,Episode&Recursive=true&userId=...` |
| Poster | `GET /Items/{id}/Images/Primary?maxHeight=600` |
| Stream | `/Videos/{id}/stream?static=true&api_key={token}` |
| Progress | `POST /Sessions/Playing` (start), `/Sessions/Playing/Progress`, `/Sessions/Playing/Stopped`; positions in **ticks** (1 tick = 100 ns) |

Quick Connect has to be enabled in the Jellyfin dashboard.

### 6.3 mpv (player crate)

```bash
mpv --fullscreen --input-ipc-server="$XDG_RUNTIME_DIR/tvdash-mpv.sock" \
    --fs-screen-name=HDMI-A-1 --force-window=yes --keep-open=no "<stream url>"
```

Commands over the socket, one JSON object per line:

```json
{"command": ["set_property", "pause", true]}
{"command": ["seek", 30, "relative"]}
{"command": ["seek", -10, "relative"]}
{"command": ["quit"]}
{"command": ["observe_property", 1, "time-pos"]}
{"command": ["observe_property", 2, "pause"]}
```

Feed `time-pos` and `pause` events into `report_progress` every ~10 s and on pause/stop.

> **Verify in M1:** whether `--fs-screen-name` is honoured on GNOME Wayland. With the TV as primary display, mpv should land there anyway.

### 6.4 Streaming sites (player crate)

```bash
google-chrome --user-data-dir="$HOME/.local/share/tvdash/browser" \
  --app="https://www.netflix.com" --start-fullscreen --no-first-run
```

- The app tracks the child process so "Stop" or "Close" kills it and the TV returns to idle.
- The app never reads cookies; Accounts shows "Set up · opened N days ago" rather than a login status.

> **Watch out:** Fedora's `chromium` package lacks Widevine DRM, so the streaming sites won't play in it. Use Google Chrome, or Firefox with DRM enabled (`firefox --profile <dir> --kiosk <url>`, after `firefox -CreateProfile`). Some services also limit Linux browsers to lower resolutions.

### 6.5 ~~Google Calendar~~ — REMOVED

*Kept for reference only; not part of v1.*

1. Google Calendar on the web → Settings → your calendar → **Integrate calendar** → copy **Secret address in iCal format**.
2. Paste it in setup; store it in the keyring as `calendar_ics_url`.
3. Every 5 minutes: fetch, parse (`icalendar` or `ical` crate), expand recurring events for today and tomorrow, and show the next one.

> **Tip:** treat the URL like a password. Anyone with it can read that calendar. "Reset" on the same Google settings screen revokes it.

### 6.6 ~~Homelab~~ — REMOVED

*Kept for reference only; not part of v1.*

| Source | Request | Use |
|---|---|---|
| Portainer | `GET {url}/api/endpoints/{id}/docker/containers/json?all=1` with header `X-API-Key: <token>` | Container count, running vs stopped |
| Uptime Kuma | `GET {url}/api/status-page/{slug}` and `GET {url}/api/status-page/heartbeat/{slug}` | Up/down per monitor for "6 / 6 up" |

Create a Portainer access token under *My account → Access tokens*. Create an Uptime Kuma status page that includes the monitors you want on the dashboard.

### 6.7 Weather (Open-Meteo)

```text
https://api.open-meteo.com/v1/forecast?latitude=LAT&longitude=LON
  &current=temperature_2m,weather_code
  &hourly=temperature_2m,weather_code
  &daily=temperature_2m_max,temperature_2m_min,weather_code
  &temperature_unit=fahrenheit&timezone=America%2FChicago&forecast_days=3
```

Map WMO `weather_code` to a condition label and icon. Refresh every 10 minutes.

---

## 7. Window placement (the M1 risk)

On Wayland, apps can't position windows, but they **can** ask to go full screen on a specific output.

- Create both Slint windows with the winit backend.
- Use Slint's winit access (`slint::winit_030::WinitWindowAccessor` or the current equivalent) to call `set_fullscreen(Some(Fullscreen::Borderless(Some(monitor))))`.
- Match each monitor by connector name from config (`tv_output`, `laptop_output`).
- Log `available_monitors()` names on startup so the config is easy to fill in.

If that fails on GNOME, the fallback is to open the TV window first (it lands on the primary display) and the laptop window second, then move it with the mouse once.

---

## 8. Visual tokens (from the design)

| Token | Value | Use |
|---|---|---|
| `bg` | `#0B0D10` | Window background |
| `bg-night` | `#050608` | TV idle background |
| `surface` | `#14171C` | Cards |
| `surface-raised` | `#1B1F26` | Tiles, inner panels |
| `border` | `#2A2F37` | Button and tile borders |
| `text-primary` | `#E8EAED` | Main text |
| `text-secondary` | `#A0A6AF` | Supporting text |
| `text-muted` | `#8A9099` | Labels on the laptop (passes contrast at 18 px) |
| `accent` | `#5B9DFF` | Primary buttons, progress |
| `ok` / `warn` / `alert` | `#4CC38A` / `#E5A84B` / `#F0605D` | Status |

- **Font:** Inter, bundled in `assets/fonts`; use tabular numerals for anything that updates.
- **Laptop sizes:** labels 18 px caps, body 20–24 px, clock 136 px, minimum hit target 44 px.
- **TV sizes:** body ≥ 32 px, idle clock 160 px.

```slint
export global Theme {
    out property <color> bg: #0B0D10;
    out property <color> surface: #14171C;
    out property <color> surface-raised: #1B1F26;
    out property <color> border: #2A2F37;
    out property <color> text-primary: #E8EAED;
    out property <color> text-secondary: #A0A6AF;
    out property <color> text-muted: #8A9099;
    out property <color> accent: #5B9DFF;
    out property <color> ok: #4CC38A;
    out property <color> warn: #E5A84B;
    out property <color> alert: #F0605D;
}
```

---

## 9. Build order and "done" checks

| # | Milestone | Done when |
|---|---|---|
| **M1** | Skeleton | Workspace builds; config loads/creates; keyring read/write works; laptop and TV windows open full screen on the right outputs; laptop shows a live clock, TV shows the idle screen |
| **M2** | Jellyfin + mpv | Quick Connect sign-in; Continue Watching row with posters; Resume on TV plays in mpv on the TV; Pause/±seek/Stop work from the laptop; progress shows up in Jellyfin's own resume |
| **M3** | Plex | PIN sign-in + server discovery; Plex items appear in the same rows with a Plex badge; play on TV and progress sync work; "On the TV" card shows active sessions from both servers |
| **M4** | Streaming | Six tiles open the right site full screen on the TV in the dedicated profile; Stop/Close kills the browser and the TV returns to idle |
| **M5** | Weather | Weather in the header, refreshing every 10 minutes ~~; next calendar event; homelab status~~ |
| **M6** | Setup + polish | First-run flow on an empty config; Settings › Accounts; expired-token states; library search and Recently Added |

### Event flow (all milestones)

```text
pollers (tokio tasks) ──► AppEvent channel ──► UI thread
   media, widgets,                          slint::invoke_from_event_loop
   player IPC                                updates models / properties
UI callbacks (Play, Pause, Open site) ──► command channel ──► player / media
```

Keep all network and IPC work off the UI thread; the UI only receives finished data.

---

## 10. Before you start M1

- [ ] Set the TV as the primary display in GNOME Settings → Displays.
- [ ] Note both connector names (`HDMI-A-1`, `eDP-1`, …).
- [ ] Install `mpv` and Google Chrome (or set up Firefox with DRM).
- [ ] Enable Quick Connect in Jellyfin.
- [ ] ~~Copy Google Calendar's secret iCal address.~~ (removed)
- [ ] ~~Create a Portainer access token and an Uptime Kuma status page.~~ (removed)
