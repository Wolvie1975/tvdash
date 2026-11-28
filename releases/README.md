# Installing `tvdash` on Fedora (RPM)

## 1. Install the RPM

Download `tvdash-1.1.0-1.x86_64.rpm` from this folder, then install it on Fedora (`x86_64`, GNOME Wayland):

```bash
sudo dnf install ./tvdash-1.1.0-1.x86_64.rpm
```

This installs `/usr/bin/tvdash`, the app-menu entry, and pulls in `mpv` and `mpv-libs` automatically.

> If you previously installed `tvdash` with `packaging/install.sh`, run `packaging/install.sh --uninstall` once so `~/.local/bin/tvdash` does not shadow `/usr/bin/tvdash`.

---

## 2. One-Time Codecs & Browser Setup

### Full FFmpeg codecs (H.264 / HEVC video)
Fedora's default `ffmpeg-free` lacks H.264/HEVC decoders. Switch to RPM Fusion's full `ffmpeg`:

```bash
sudo dnf install -y https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf swap -y ffmpeg-free ffmpeg --allowerasing
mpv --vd=help | grep -wE "h264|hevc"
```

*(Optional, for smoother 4K on Intel graphics:)*
```bash
sudo dnf install -y intel-media-driver
mkdir -p ~/.config/mpv && echo "hwdec=auto" >> ~/.config/mpv/mpv.conf
```

### Brave Browser (Widevine DRM for streaming tiles)
Install Brave from its RPM repo (not Flatpak):

```bash
sudo dnf install -y dnf-plugins-core
sudo dnf config-manager addrepo --from-repofile=https://brave-browser-rpm-release.s3.brave.com/brave-browser.repo
sudo dnf install -y brave-browser
```

---

## 3. Launch & Upgrades

- **Launch:** Open **tvdash** from the GNOME app menu (or run `tvdash` in a terminal).
  - If **tvdash** doesn't appear in the GNOME app menu right away, refresh the desktop/icon cache (or log out and back in):
    ```bash
    sudo update-desktop-database /usr/share/applications
    sudo gtk-update-icon-cache -f /usr/share/icons/hicolor
    ```
- **Upgrade later:** Download the newer `.rpm` and run:
  ```bash
  sudo dnf upgrade ./tvdash-1.1.0-1.x86_64.rpm
  ```
  Your settings (`~/.config/tvdash/`), browser profile (`~/.local/share/tvdash/`), and keyring sign-ins are preserved across upgrades.
- **Uninstall:**
  ```bash
  sudo dnf remove tvdash
  ```
  Your settings (`~/.config/tvdash/`) and app data (`~/.local/share/tvdash/`) are kept; delete those directories if you want a full wipe.


## 4. Add to config.toml file

Add these streaming tiles to `~/.config/tvdash/config.toml`:

```toml
[[streaming]]
name = "Crunchyroll"
url = "https://www.crunchyroll.com"
tmdb_providers = [283]

[[streaming]]
name = "Google Meet"
url = "https://meet.google.com"
hold_sleep_timer = true
```
