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

## Milestone status

- [x] **M1**: workspace, config load/create, keyring read/write, two windows full screen per output, live clock, TV idle screen
- [ ] M2: Jellyfin + mpv
- [ ] M3: Plex
- [ ] M4: Streaming
- [ ] M5: Header widgets
- [ ] M6: Setup + polish
