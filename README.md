# tvdash

Rust + Slint desktop app for a Fedora laptop connected to a TV. The laptop panel shows a
mouse-driven control dashboard, and the TV (HDMI, the primary display) handles playback only:
Plex and Jellyfin play through mpv, streaming sites open in a full-screen browser, and a dim
idle screen shows otherwise. See [docs/running.md](docs/running.md) for day-to-day use (start, stop, update, fixes) and
[docs/v1-plan.md](docs/v1-plan.md) for the V1 plan.

## Install and first run (Fedora 44)

1. In GNOME Settings → Displays, make the TV the **primary** display.
2. Install the players: `sudo dnf install mpv`, plus Brave from Brave's RPM repo (see
   [Streaming](#streaming)).
3. Build and run:

   ```bash
   sudo dnf install fontconfig-devel   # needed to build Slint; Wayland/xkbcommon load at runtime
   cargo run --release
   ```

4. The first run writes `~/.config/tvdash/config.toml` and logs an `available monitor` line
   for each output. Copy those connector names into `[display]` (`tv_output`,
   `laptop_output`), and set `latitude` / `longitude` under `[general]` for the weather.
5. With nothing connected yet, tvdash opens **first-time setup** on the laptop:
   1. **Plex**: get a code and enter it at plex.tv/link. tvdash finds your server.
   2. **Jellyfin**: check the server address, then use Quick Connect (enable it in the
      Jellyfin dashboard), or a username and password.
   3. **Calendar**: paste Google Calendar's *Secret address in iCal format*. tvdash checks
      it before saving it.
   4. **Streaming**: open each site on the TV once and sign in with the laptop's mouse and
      keyboard.

   Every step can be skipped. Settings › Accounts can run setup again.

Tokens, the calendar address and the Portainer token live in GNOME Keyring (service
`tvdash`), never in the config file.

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
| **Home** | Clock, weather, next event, homelab; **On the TV** with transport buttons and other devices' sessions; Watch on TV tiles; Continue Watching (click to resume on the TV) |
| **Library** | All / Plex / Jellyfin filter, search, Continue Watching, Recently Added; click a poster for details, **Resume on TV** or **Play from the start** |
| **Settings › Accounts** | Each connection's status with Sign in / Sign out / Reconnect; calendar address and Portainer token; streaming sites with **Sign in on TV**; **Clear browser profile** (signs out of every streaming site) |

Expired sign-ins show **Reconnect** where they matter: a Plex or Jellyfin token on the
On the TV card, and a reset calendar address on the next-event chip.

Command-line options:

```bash
tvdash --windowed                       # don't go full screen (single-display development)
tvdash --check-keyring                  # keyring round-trip and the list of stored secrets
tvdash --set-secret calendar_ics_url    # paste a secret (read from stdin)
tvdash --clear-secret portainer_api_key
```

Logging: `RUST_LOG=tvdash=debug`.

## Header widgets

| Chip | Needs | Refresh |
|---|---|---|
| Weather | `latitude` / `longitude` under `[general]` (Open-Meteo, no key) | 10 min |
| Next event | Google Calendar's secret iCal address (setup or Settings) | 5 min |
| Homelab | `[homelab]` URLs; the Portainer token (Settings); an Uptime Kuma status page whose slug is `uptime_kuma_status_page` | 1 min |

Portainer's default certificate is self-signed. If its line says unavailable, set
`portainer_verify_tls = false` under `[homelab]`. The next-event chip re-picks every
minute, so "Now" and "Tomorrow" stay current between fetches. The TV's idle screen shows
the same event as "Up next".

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
| `crates/media` | `tvdash-media` | `MediaSource` trait; Plex and Jellyfin clients |
| `crates/player` | `tvdash-player` | mpv JSON IPC and the streaming browser launcher |
| `crates/widgets` | `tvdash-widgets` | Weather, calendar and homelab pollers |
| `crates/app` | `tvdash` | Slint UI (`ui/*.slint`: home, library, settings, setup, TV), controller, `main()` |

The controller (`crates/app/src/controller.rs`) owns everything with side effects: media
sources, sign-in flows, mpv and the browser. UI callbacks send it `Command`s
(`callbacks.rs`); it answers with `AppEvent`s, which `bridge.rs` applies on the UI thread.

To render the windows to PNG without a display (useful in the devcontainer):

```bash
cargo run --example snapshot -- out.png home-playing   # also: home, home-code, home-streaming,
                                                       # library, library-search, settings,
                                                       # setup-0 … setup-3, tv
```

## Tests

```bash
cargo test                       # unit tests, including headless UI tests of the bridge
cargo test -- --include-ignored  # also the end-to-end tests (need python3)
```

The end-to-end tests run the real controller against `crates/app/tests/fakes/`: a fake
Jellyfin server, a fake plex.tv + Plex Media Server, a fake mpv that speaks the JSON IPC
protocol, and a fake browser. For each server they cover sign-in (Quick Connect, PIN,
password), Continue Watching, Recently Added, search, posters, play, pause/seek, stop, the
progress reports the server receives, and sign-out. The streaming test covers site
switching, Stop closing the whole process group, the first-visit hint, Chrome handing
off to an already-open window, clearing the profile and the first-run setup trigger.
The widget pollers have their own end-to-end test in `crates/widgets/tests/`.

## Milestone status

- [x] **M1**: workspace, config load/create, keyring read/write, two windows full screen per output, live clock, TV idle screen
- [x] **M2**: Jellyfin Quick Connect, Continue Watching with posters, Resume on TV in mpv, Pause/±seek/Stop, progress sync
- [x] **M3**: Plex PIN sign-in and server discovery, merged rows with source badges, play/progress sync, active sessions from both servers
- [x] **M4**: Streaming tiles open each site full screen on the TV in a dedicated profile; Stop/Close closes the whole browser and the TV returns to idle
- [x] **M5**: Weather (10 min), next calendar event (5 min, recurrences expanded), homelab "N / N up" (1 min); calendar Reconnect state
- [x] **M6**: First-run setup, Settings › Accounts, expired-sign-in states, Library with search, filters, Recently Added and a detail panel
