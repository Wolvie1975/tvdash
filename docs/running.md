# Running tvdash

A quick reference for starting, stopping, updating and fixing tvdash on the laptop that
drives the TV.

## Start and stop

| To… | Do this |
|---|---|
| Start it | Open **tvdash** from the app menu, or run `tvdash` in a terminal |
| Try it without full screen | `tvdash --windowed` (two normal windows you can move around) |
| See what it's doing | **Settings › Diagnostics** shows each service's last success and error. Full log: `~/.local/state/tvdash/tvdash.log` (or start it from a terminal). More detail: `RUST_LOG=tvdash=debug tvdash` |
| Use it on the TV only | `layout = "tv"` under `[display]` (or Settings › Layout on the TV), then restart. Try it once with `tvdash --layout tv` |
| Get back Home from a video or site | Run `tvdash --home`. Bind it to a key in GNOME Settings › Keyboard › Custom Shortcuts |
| Stop it | **Settings** (gear, top right) › **Quit tvdash**, or press Alt+F4 on either window |

It also starts by itself when you log in. To change that:

```bash
rm ~/.config/autostart/tvdash.desktop              # stop starting at login
~/Development/Environment/Rust_Projects/tvdash/packaging/install.sh --autostart   # start at login again
```

Only one copy should run at a time. If a window seems stuck, quit it before starting
another.

## Screens

- The **TV** window goes full screen on the external display, and the **dashboard** on the
  laptop panel.
- If they land on the wrong screens, run `tvdash --windowed` once. The terminal prints an
  `available monitor name=…` line for each screen. Put those names in
  `~/.config/tvdash/config.toml`:

  ```toml
  [display]
  tv_output = "HDMI-1"
  laptop_output = "eDP-1"
  ```

## Accounts

Everything is under **Settings › Accounts** on the dashboard:

- **Plex / Jellyfin:** Sign in, Sign out, Reconnect.
- **TMDB (Where to watch):** paste your TMDB API Read Access Token (themoviedb.org ›
  Settings › API). It turns on Where to watch and the service logos on the tiles.
- **Streaming sites** (Settings › Streaming): **Sign in on TV** opens the site on the TV. Sign in with the
  laptop's mouse and keyboard, then press **Stop**.
- **Run first-time setup again** walks through all of these in order.

Sign-ins are kept in the system keyring, so they survive restarts. To check what's
stored:

```bash
tvdash --check-keyring
```

## Settings file

`~/.config/tvdash/config.toml`. Restart tvdash after editing it.

| Setting | Where | What it does |
|---|---|---|
| `latitude`, `longitude` | `[general]` | Location for the weather (off until set) |
| `units`, `clock`, `seconds` | `[general]` | Easier from Settings › Display |
| `timezone` | `[general]` | e.g. `"America/Chicago"` |
| `theme` | `[general]` | `"dark"` (default), `"stone"` or `"mist"`; the header switch changes it for you |
| `tv_output`, `laptop_output` | `[display]` | Which screen is which (see above) |
| `server_url` | `[jellyfin]` | Your Jellyfin server address |
| `binary` | `[browser]` | Browser for streaming sites (default `brave-browser`) |
| `[[streaming]]` entries | end of file | The Watch on TV tiles, in order: `name`, `url`, optional `search_url` (with `{title}`), `tmdb_providers`, `hidden`. Easier from Settings › Streaming or by holding a tile |

## Updating after code changes

```bash
cd ~/Development/Environment/Rust_Projects/tvdash
cargo build --release      # or build in the dev container
packaging/install.sh       # copies the new build into place
```

Quit tvdash and start it again to run the new version.

## When something goes wrong

| Problem | Fix |
|---|---|
| Plex or Jellyfin plays **sound but no picture** (black screen), or tvdash says "mpv can't decode the video" | Fedora's own FFmpeg (`ffmpeg-free`) can't decode most H.264 films or any HEVC. Switch to RPM Fusion's full FFmpeg: `sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm`, then `sudo dnf swap ffmpeg-free ffmpeg --allowerasing`. Check with `mpv --vd=help \| grep -wE "h264\|hevc"` (both should be listed), then play the title again; no restart needed |
| Plex sign-in says no address answered | Your router may block `*.plex.direct` names. tvdash falls back to the server's local address; if that fails too, check the server is on and the laptop is on the same network |
| Streaming site says DRM or won't play | In the Brave window on the TV, accept the prompt to enable **Widevine**. If you missed it: Brave → Settings → Extensions → turn on Widevine, then reopen the site |
| A streaming site opens on the laptop instead of the TV | Set `window_position = "X,Y"` under `[browser]` to the TV's top-left corner (e.g. `"1920,0"` if the TV is to the right of a 1920-wide panel) |
| "Keyring unavailable" in the footer | The keyring service isn't running, so sign-ins won't be saved. Log out and back in, or check GNOME Keyring / KWallet is enabled |
| Streaming sites keep their old sign-ins | **Settings › Clear browser profile** signs out of all of them |

## Uninstall

```bash
~/Development/Environment/Rust_Projects/tvdash/packaging/install.sh --uninstall
```

This removes the app, the menu entry and autostart. Your settings
(`~/.config/tvdash`), the browser profile (`~/.local/share/tvdash`) and keyring entries
stay. Delete those folders too for a clean slate.
