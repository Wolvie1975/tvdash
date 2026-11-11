# tvdash

Rust + Slint desktop app for a Fedora laptop connected to a TV. The laptop panel shows a
mouse-driven control dashboard, and the TV (HDMI, the primary display) handles playback only.
See [docs/v1-plan.md](docs/v1-plan.md) for the full V1 plan.

## Layout

| Crate | Package | Role |
|---|---|---|
| `crates/core` | `tvdash-core` | Config (`~/.config/tvdash/config.toml`), keyring secrets, models, `AppEvent`/`Command` |
| `crates/media` | `tvdash-media` | `MediaSource` trait; Plex (M3) and Jellyfin (M2) clients |
| `crates/player` | `tvdash-player` | mpv JSON IPC (M2) and the streaming browser launcher (M4) |
| `crates/widgets` | `tvdash-widgets` | Weather, calendar and homelab pollers (M5) |
| `crates/app` | `tvdash` | Slint UI (`ui/*.slint`), the laptop and TV windows, and `main()` |

## Build and run (Fedora 44)

```bash
sudo dnf install fontconfig-devel   # needed to build Slint; Wayland/xkbcommon load at runtime
cargo run --release                 # full screen on the outputs in [display]
cargo run -- --windowed             # development on a single display
cargo run -- --check-keyring        # GNOME Keyring round-trip and the list of stored secrets
```

The first run writes a default config and logs an `available monitor` line for each output.
Copy those connector names into `[display]` (`tv_output`, `laptop_output`).

To render both windows to PNG without a display (useful in the devcontainer):

```bash
cargo run --example snapshot -- laptop.png laptop
cargo run --example snapshot -- tv.png tv
```

## Streaming browser placement

Native Wayland windows can't pick an output, so by default Chrome runs through
XWayland (`[browser] use_xwayland = true`) with `--window-position` set to the TV's
position as reported at startup. If it still opens on the laptop, set
`window_position = "X,Y"` under `[browser]` to the TV's top-left corner in desktop
coordinates. Use Google Chrome rather than Fedora's `chromium`, which lacks Widevine.

## Tests

```bash
cargo test                      # unit tests
cargo test -- --include-ignored # also the end-to-end test (needs python3)
```

The end-to-end test in `crates/app/src/controller.rs` runs the real controller
against `crates/app/tests/fakes/`: fake Jellyfin, fake plex.tv + Plex Media Server,
a fake mpv that speaks the JSON IPC protocol, and a fake browser. For each server it covers sign-in →
Continue Watching → posters → play → pause/seek → stop, plus the progress reports the
server receives (and, for Plex, discovery and session filtering). The streaming test
covers site switching, Stop closing the whole process group, the first-visit hint, and
Chrome handing off to an already-open window.

## Milestone status

- [x] **M1**: workspace, config load/create, keyring read/write, two windows full screen per output, live clock, TV idle screen
- [x] **M2**: Jellyfin Quick Connect, Continue Watching with posters, Resume on TV in mpv, Pause/±seek/Stop, progress sync
- [x] **M3**: Plex PIN sign-in and server discovery, merged rows with source badges, play/progress sync, active sessions from both servers
- [x] **M4**: Streaming tiles open each site full screen on the TV in a dedicated profile; Stop/Close closes the whole browser and the TV returns to idle
- [ ] M5: Header widgets
- [ ] M6: Setup + polish
