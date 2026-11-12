# tvdash

Rust + Slint desktop app for a Fedora laptop connected to a TV. The laptop panel shows a
mouse-driven control dashboard, and the TV (HDMI, the primary display) handles playback only:
Plex plays through mpv, ten streaming apps open in a full-screen browser, and a dim idle
screen shows otherwise. See [docs/running.md](docs/running.md) for day-to-day use (start, stop, update, fixes),
[docs/deploying.md](docs/deploying.md) for setting it up on another machine,
[docs/dashboard-library-update.md](docs/dashboard-library-update.md) for the current dashboard,
Library and screensaver, [docs/v1-plan.md](docs/v1-plan.md) for the V1 plan, and
[docs/future-plan.md](docs/future-plan.md) for v1.1 onwards ([docs/tv-only-plan.md](docs/tv-only-plan.md) for the single-screen layout).

## Install and first run (Fedora 44)

1. In GNOME Settings → Displays, make the TV the **primary** display.
2. Install the players: `sudo dnf install mpv`, plus Brave from Brave's RPM repo (see
   [Streaming](#streaming)). Then switch to RPM Fusion's full FFmpeg, or most films play with
   sound but no picture (Fedora's `ffmpeg-free` can't decode most H.264 or any HEVC):

   ```bash
   sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
   sudo dnf swap ffmpeg-free ffmpeg --allowerasing
   ```
3. Build and run:

   ```bash
   sudo dnf install fontconfig-devel   # needed to build Slint; Wayland/xkbcommon load at runtime
   cargo run --release
   ```

4. The first run writes `~/.config/tvdash/config.toml` and logs an `available monitor` line
   for each output. Copy those connector names into `[display]` (`tv_output`,
   `laptop_output`). For the weather, pick your city in Settings › Display › Location.
5. With nothing connected yet, tvdash opens **first-time setup** on the laptop:
   1. **Plex**: get a code and enter it at plex.tv/link. tvdash finds your server.
   2. **Streaming**: open each app on the TV once and sign in with the laptop's mouse and
      keyboard.

   Every step can be skipped. Settings › Accounts can run setup again.

The Plex and TMDB tokens live in GNOME Keyring (service `tvdash`), never in the
config file.

## Installing on the machine that drives the TV

The release binary only needs glibc 2.39+ and fontconfig, so a build from the dev container
runs on the Fedora host too. On the host, from the project folder:

```bash
cargo build --release            # or build it inside the dev container
packaging/install.sh             # ~/.local/bin/tvdash + app-menu entry + icon
packaging/install.sh --autostart # also start at login
packaging/install.sh --uninstall # remove (keeps config, keyring entries and browser profile)
```

The installer warns about anything missing at runtime: mpv, Brave and GNOME Keyring's
Secret Service. Re-run it after each rebuild to update the installed copy.

If the `[display]` names in the config don't match your outputs, tvdash picks the
built-in panel (`eDP`/`LVDS`/`DSI`) for the laptop window and the first external output
for the TV, and logs which ones it chose.

## Using it

| Page | What's there |
|---|---|
| **Home** | Header: **Search** and **Screensaver** left; time, date and weather centred; theme switch (Dark / Stone / Mist) and Settings right. Three equal boxes: **On the TV** (title, episode, year · rating · runtime · genre, poster and description; transport buttons while playing, or **Up next** with Resume on TV), **Watch on TV** (ten app logos) and **Continue Watching** (TV / Series · Movies, tap a poster to resume on the TV) |
| **Search** | The Search button slides a panel over the right two boxes. Type a movie or show: In your library (Resume / Play on TV), On your apps (Open on TV, using the app's search page), Costs extra, Not streaming |
| **Screensaver** | "It's 9:04 PM on Thursday. Outside, 51° and partly sunny." and what's on the TV. Always dark; any tap or key wakes the dashboard (that tap does nothing else) |
| **Library** | "See all ›" on Continue Watching. Smart shelves for Movies or TV / Series: Tonight's pick (with Shuffle), Finish a season / Continue watching, Added this week, Watch again, Genres. **Browse all** (or a shelf's count, a genre, a poster) opens the full grid with a detail panel |
| **Settings › Accounts** | Plex and the TMDB token with Sign in / Sign out / Reconnect; keyring status |
| **Settings › Streaming** | The ten apps with their addresses and **Sign in on TV**; **Clear browser profile** |
| **Settings › Display** | °F/°C, 12/24-hour, seconds, theme (all live) |
| **Settings › Diagnostics** | Last success and last error per service (Plex, TMDB, weather, keyring, mpv, browser), and where the log, settings and caches are |
| *(all tabs)* | **Quit tvdash** in the Settings header with **Sign in on TV**; **Clear browser profile** (signs out of every streaming site) |

An expired Plex sign-in shows **Reconnect** on the On the TV box. While a video plays or a
streaming app is open, tvdash asks GNOME (through the desktop portal) not to blank the laptop
screen.

Command-line options:

```bash
tvdash --windowed                       # don't go full screen (single-display development)
tvdash --check-keyring                  # keyring round-trip and the list of stored secrets
tvdash --set-secret tmdb_api_key       # paste a secret (read from stdin)
tvdash --clear-secret plex_token
```

Logging: stderr and `~/.local/state/tvdash/tvdash.log` (the previous run's is kept as
`tvdash.log.1`). More detail: `RUST_LOG=tvdash=debug`.

## TV-only layout (in progress)

Everything on the TV, no laptop panel: switch with Settings › Display › **Layout** (TV only /
Two screens; it switches right away and is remembered), or try it once with `tvdash --layout tv`. One full-screen 10-foot window with a side rail: Home (On the TV / Up
next, Continue Watching, your services), Library, Services and Settings, driven by arrows,
Enter (OK) and Esc/Backspace (Back). After 5 quiet minutes on Home it shows an ambient clock.

Plex videos play **inside the tvdash window** (through libmpv, from Fedora's
`mpv-libs`, which comes with `mpv`). They start full screen:

| Key | Full-screen video |
|---|---|
| Enter (OK), ↑, ↓ | Title, progress and end time for 5 seconds |
| Space | Pause / play |
| ← / → | Back 10 s / forward 30 s |
| Esc / Backspace | Home, with the video still playing behind it |

On Home the card has **Pause**, **Full screen** (back to the video) and **Stop**. Without
libmpv, or if the window can't draw video (the log says why), videos open in mpv's own
window as in the two-screen layout. Set `MPV_LOG=v` to see mpv's own log in tvdash's.

To get Home back while a streaming site has the screen, bind a GNOME keyboard shortcut
(Settings › Keyboard › Custom Shortcuts) to `tvdash --home`. It closes the browser (or
stops mpv's own window; progress is saved) and returns to Home. Still to come (see
[docs/tv-only-plan.md](docs/tv-only-plan.md)): Search on the TV, the phone remote and HDMI-CEC.

To check in-window video on a machine without signing in to anything:

```bash
cargo run --release --example video -- ~/Videos/some-film.mkv     # or any URL mpv plays
```

## Search (TMDB)

Needs a TMDB **API Read Access Token** (themoviedb.org › Settings › API). Paste it in
Settings › Accounts › TMDB (or `tvdash --set-secret tmdb_api_key`). Searches then show
what's in Plex (matched by TMDB ID), what's included with your ten apps, and what costs
extra. Availability is cached for a day in `~/.local/share/tvdash/cache.db`; the provider
list for a week. Availability data is JustWatch's, via TMDB; the panel credits both as TMDB
requires.

## Watch on TV apps

A fixed list: YouTube TV, YouTube, Disney+, Hulu, Apple TV, Netflix, Fandango at Home,
Prime Video, Crunchyroll, Tubi. Their logos are bundled (`assets/logos/`, sources in
`SOURCES.md` there). To change an address, edit its `[[streaming]]` entry in the config
(`url`, and `search_url` with `{title}` where the title goes); other entries are dropped.

## Header widgets

Weather comes from Open-Meteo (no key) and refreshes every 10 minutes. Turn it on in
Settings › Display › Location: search for your city and pick it (that also sets the clock's
time zone). The coordinates are saved as `latitude` / `longitude` under `[general]`. Calendar and homelab were removed on
2026-11-11 (see the scope change in [docs/v1-plan.md](docs/v1-plan.md)); tvdash deletes
their old keyring entries on start, and old `[calendar]` / `[homelab]` config sections
are ignored.

## Streaming

Native Wayland windows can't pick an output, so by default the browser runs through
XWayland (`[browser] use_xwayland = true`) with `--window-position` set to the TV's
position as reported at startup. If it still opens on the laptop, set
`window_position = "X,Y"` under `[browser]` to the TV's top-left corner in desktop
coordinates.

The default browser is Brave (`binary = "brave-browser"`, from Brave's RPM repo, not the
Flatpak). Brave ships Widevine DRM switched off: the first time a streaming site needs it,
Brave asks to enable it, so choose **Enable** in the TV window. That turns it on for
tvdash's own profile (`~/.local/share/tvdash/browser`). Other Chromium browsers with
Widevine (Chrome, Edge, Vivaldi) work too; Fedora's `chromium` doesn't.

## Layout

| Crate | Package | Role |
|---|---|---|
| `crates/core` | `tvdash-core` | Config, keyring secrets, app state file, models, `AppEvent`/`Command` |
| `crates/media` | `tvdash-media` | `MediaSource` trait; the Plex client |
| `crates/player` | `tvdash-player` | mpv JSON IPC and the streaming browser launcher |
| `crates/widgets` | `tvdash-widgets` | Weather poller |
| `crates/discover` | `tvdash-discover` | TMDB client, SQLite cache, search ranking |
| `crates/app` | `tvdash` | Slint UI (`ui/*.slint`: home, library, browse, saver, settings, setup, TV), controller, Library shelves (`shelves.rs`), `main()` |

The controller (`crates/app/src/controller.rs`) owns everything with side effects: media
sources, sign-in flows, mpv and the browser. UI callbacks send it `Command`s
(`callbacks.rs`); it answers with `AppEvent`s, which `bridge/` applies on the UI thread.

To render the windows to PNG without a display (useful in the devcontainer):

```bash
cargo run --example snapshot -- out.png home-playing   # also: home, home-next, home-search,
                                                       # home-code, home-streaming, library
                                                       # (LIB_TAB=movies), browse, saver,
                                                       # saver-idle, settings, setup-0, setup-1, tv
```

## Tests

```bash
cargo test                       # unit tests, including headless UI tests of the bridge
cargo test -- --include-ignored  # also the end-to-end tests (need python3 and libmpv)
```

The end-to-end tests run the real controller against `crates/app/tests/fakes/`: a fake
plex.tv + Plex Media Server, a fake TMDB, a fake mpv that speaks the JSON IPC protocol, and
a fake browser. They cover PIN sign-in, Continue Watching, Recently Added, item details, the
whole library read, posters, play, pause/seek, stop, the progress reports the server
receives, search ranking and sign-out. The streaming test covers site
switching, Stop closing the whole process group, the first-visit hint, Chrome handing
off to an already-open window, clearing the profile and the first-run setup trigger.
The weather poller has its own end-to-end test in `crates/widgets/tests/`.

## Milestone status

- [x] **M1**: workspace, config load/create, keyring read/write, two windows full screen per output, live clock, TV idle screen
- [x] **M2**: Jellyfin Quick Connect, Continue Watching with posters, Resume on TV in mpv, Pause/±seek/Stop, progress sync
- [x] **M3**: Plex PIN sign-in and server discovery, merged rows with source badges, play/progress sync, active sessions from both servers
- [x] **M4**: Streaming tiles open each site full screen on the TV in a dedicated profile; Stop/Close closes the whole browser and the TV returns to idle
- [x] **M5**: Weather (10 min). Calendar and homelab were built, then removed from scope
- [x] **M6**: First-run setup, Settings › Accounts, expired-sign-in states, Library with search, filters, Recently Added and a detail panel
- [x] **Redesign**: two columns, On the TV tinted from the poster, touch-first, Dark / Stone / Mist
- [x] **v1.1**: manage streaming sites; Where to watch (TMDB); service logos; Settings › Display; weather stale/error states; Diagnostics and a log file; TV pixel shift; crossfades
- [x] **Dashboard & Library update** (2026-11-12): Plex only; three-box dashboard; ten fixed apps with bundled logos; search panel; smart-shelf Library and Browse all; sentence-clock screensaver; 12-hour clocks by default
