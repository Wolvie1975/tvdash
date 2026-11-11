# Moving tvdash to another machine

How to set up tvdash on another computer that drives a TV: a second laptop, a mini PC, or
a fresh install of the same machine. For day-to-day use once it's running, see
[running.md](running.md).

There are two ways to get the program there:

| | **A. Copy a bundle** (recommended) | **B. Build on the new machine** |
|---|---|---|
| Needs on the new machine | Nothing extra | Rust, a C compiler, `fontconfig-devel`, the source |
| Time | A minute | 5–10 minutes for the first build |
| Use when | It's x86-64 Linux with glibc 2.39 or newer | It's ARM (e.g. a Raspberry Pi), or an older distro |

Either way, the steps after installing (players, codecs, sign-ins, screens) are the same.

---

## 1. Check the new machine

| Need | Why | Check |
|---|---|---|
| Linux desktop, GNOME on Wayland (Fedora 44+ Workstation is what tvdash is built for) | Windows, keyring, display placement | `echo $XDG_SESSION_TYPE` → `wayland` |
| x86-64 (for a bundle) | The bundle is built for x86-64 | `uname -m` → `x86_64` |
| glibc 2.39 or newer (for a bundle) | The bundle is built against it | `ldd --version` → 2.39 or higher (Fedora 40+, Ubuntu 24.04+, Debian 13+) |
| GNOME Keyring | Keeps the Plex, Jellyfin and TMDB sign-ins | Comes with GNOME |
| HDMI to the TV | Playback | — |

---

## 2. Get the program onto it

### A. Copy a bundle

On the machine you build on (here, or in the dev container):

```bash
cd ~/Development/Environment/Rust_Projects/tvdash
cargo build --release
packaging/bundle.sh
```

That writes `dist/tvdash-0.1.0-x86_64.tar.gz`. It holds the program, the installer, the
app-menu entry and icon, and a copy of this guide. Copy it to the new machine (USB stick,
`scp`, a shared folder…), then on the new machine:

```bash
tar xzf tvdash-0.1.0-x86_64.tar.gz
tvdash-0.1.0-x86_64/install.sh              # or: install.sh --autostart to start at login
```

No root is needed: it installs to `~/.local/bin/tvdash` plus an app-menu entry, for that user
only. The installer then lists anything missing (mpv, Brave, the keyring service).

### B. Build on the new machine

```bash
sudo dnf install git gcc fontconfig-devel
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh    # then open a new terminal
git clone <your copy of the repo> tvdash      # or copy the project folder over
cd tvdash
cargo build --release
packaging/install.sh                          # or --autostart
```

---

## 3. Install the players

### mpv, with full codecs

```bash
sudo dnf install mpv
```

Fedora's own FFmpeg (`ffmpeg-free`) can't decode most H.264 films or any HEVC, which gives
you **sound but no picture**. Switch to RPM Fusion's full FFmpeg:

```bash
sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf swap ffmpeg-free ffmpeg --allowerasing
mpv --vd=help | grep -wE "h264|hevc"          # should list both
```

Optional, for smoother 4K on Intel graphics: `sudo dnf install intel-media-driver`, and add
`hwdec=auto` to `~/.config/mpv/mpv.conf`.

### Brave (streaming sites)

Use Brave's own RPM repo, not the Flatpak:

```bash
sudo dnf install dnf-plugins-core
sudo dnf config-manager addrepo --from-repofile=https://brave-browser-rpm-release.s3.brave.com/brave-browser.repo
sudo dnf install brave-browser
```

The first time a site needs DRM, Brave asks to enable **Widevine**: choose **Enable** on the TV.
Other Chromium browsers with Widevine (Chrome, Edge, Vivaldi) work too: set `binary` under
`[browser]` in the config. Fedora's `chromium` doesn't.

---

## 4. Settings: start fresh or bring yours

tvdash's files on the old machine:

| What | Where | Copy it? |
|---|---|---|
| Settings | `~/.config/tvdash/config.toml` | **Yes, with one change** (below) |
| Plex, Jellyfin and TMDB sign-ins | GNOME Keyring (service `tvdash`) | **No**: keyrings don't move. Sign in again (step 5) |
| Streaming sign-ins | `~/.local/share/tvdash/browser` | **No**: Brave encrypts them with a key in the old machine's keyring, so they won't open on the new one. Sign in again on the TV |
| "Opened N days ago", setup done | `~/.local/share/tvdash/state.toml` | Optional |
| Where to watch cache | `~/.local/share/tvdash/cache.db` | No, it refills itself |
| Log | `~/.local/state/tvdash/tvdash.log` | No |

**Copying `config.toml`** keeps your streaming tiles (order, search pages, TMDB provider IDs),
location, theme, display settings and server addresses. Before starting tvdash on the new
machine, **delete these two lines** from it:

```toml
client_id = "…"     # under [plex]
device_id = "…"     # under [jellyfin]
```

They identify the device to Plex and Jellyfin. tvdash makes new ones on its next start, so the
two machines show up as separate devices instead of fighting over one.

Not copying anything is fine too: the first start writes a default config and opens setup.

---

## 5. First start and sign-ins

Start **tvdash** from the app menu. With nothing signed in, first-time setup opens:

1. **Plex:** enter the code at plex.tv/link.
2. **Jellyfin:** check the server address, then Quick Connect (or username and password).
3. **Streaming:** open each site on the TV once and sign in there.

Then, for Where to watch and the service logos: **Settings › Accounts › TMDB**, paste your
TMDB API Read Access Token. (Or from a terminal: `tvdash --set-secret tmdb_api_key`.)

Check the keyring is working: `tvdash --check-keyring` should say `write/read/delete OK`.

---

## 6. Screens

**Two screens** (laptop dashboard + TV), the default:

1. GNOME Settings › Displays: make the TV the **primary** display.
2. Start tvdash. If its windows land on the wrong screens, run `tvdash --windowed` once from a
   terminal and note the `available monitor name=…` lines, then put those names in the config:

   ```toml
   [display]
   tv_output = "HDMI-1"
   laptop_output = "eDP-1"
   ```

**TV only** (no laptop panel; drive it with a keyboard for now):

```toml
[display]
layout = "tv"
tv_output = "HDMI-1"
```

- Bind a key to get back Home while a video or site has the screen: GNOME Settings › Keyboard ›
  View and Customize Shortcuts › Custom Shortcuts › **+**, command
  `/home/<you>/.local/bin/tvdash --home` (use the full path).
- If the computer is a laptop with its lid closed: keep it **plugged in**. GNOME keeps running
  with the lid closed only on mains power with an external display connected.
- Optionally show nothing on the laptop panel: GNOME Settings › Displays › **Single display**
  (the TV).

---

## 7. Check it works

| Check | Expect |
|---|---|
| Settings › Diagnostics | Green dots for Plex, Jellyfin, GNOME Keyring; Weather once a location is set; TMDB after a search |
| Play something from Continue Watching | Picture and sound on the TV (no picture → the codec step in §3) |
| Tap a streaming tile | The site opens full screen on the TV |
| Weather | Set `latitude` and `longitude` under `[general]` (restart after editing the file) |

---

## 8. Updating a machine later

Make a new bundle and run its installer again, or on a machine with the source:

```bash
cargo build --release && packaging/install.sh
```

Quit tvdash first (Settings › Quit tvdash), then start it again. Settings, sign-ins and the
browser profile stay as they are.

## 9. Removing it

```bash
<bundle or project folder>/install.sh --uninstall
```

That removes the program, the menu entry and autostart. For a clean slate also delete
`~/.config/tvdash`, `~/.local/share/tvdash` and `~/.local/state/tvdash`, and remove the
`tvdash` entries in **Passwords and Keys** (Seahorse).
