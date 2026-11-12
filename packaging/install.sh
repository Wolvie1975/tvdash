#!/usr/bin/env bash
# Install tvdash for the current user (no root needed).
#
#   packaging/install.sh              install / update
#   packaging/install.sh --autostart  also start tvdash at login
#   packaging/install.sh --uninstall  remove it (config, keyring and browser profile are kept)
#
# Run it on the machine that will drive the TV: from the project folder after
# `cargo build --release`, or from an unpacked bundle made by packaging/bundle.sh.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(dirname "$here")"
bin_dir="$HOME/.local/bin"
apps_dir="$HOME/.local/share/applications"
icon_dir="$HOME/.local/share/icons/hicolor/scalable/apps"
autostart_dir="$HOME/.config/autostart"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "$bin_dir/tvdash" "$apps_dir/tvdash.desktop" "$icon_dir/tvdash.svg" "$autostart_dir/tvdash.desktop"
    echo "tvdash removed. Kept: ~/.config/tvdash, ~/.local/share/tvdash, and its GNOME Keyring entries."
    exit 0
fi

# A bundle (packaging/bundle.sh) ships the binary next to this script;
# in the project folder it's the release build.
if [[ -x "$here/tvdash" ]]; then
    src="$here/tvdash"
else
    src="$repo/target/release/tvdash"
fi
if [[ ! -x "$src" ]]; then
    echo "No release build at $src. Build it first:  cargo build --release" >&2
    exit 1
fi

mkdir -p "$bin_dir" "$apps_dir" "$icon_dir"
install -m 755 "$src" "$bin_dir/tvdash"
install -m 644 "$here/tvdash.svg" "$icon_dir/tvdash.svg"
sed "s|@BIN@|$bin_dir/tvdash|" "$here/tvdash.desktop.in" > "$apps_dir/tvdash.desktop"
chmod 644 "$apps_dir/tvdash.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Installed $bin_dir/tvdash and the 'tvdash' app-menu entry."

if [[ "${1:-}" == "--autostart" ]]; then
    mkdir -p "$autostart_dir"
    cp "$apps_dir/tvdash.desktop" "$autostart_dir/tvdash.desktop"
    echo "tvdash will start at login."
fi

# What tvdash needs at runtime; warn rather than fail.
missing=0
check() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "  ! $1 not found: $2"
        missing=1
    fi
}
echo "Checking runtime dependencies:"
check mpv "Plex playback won't work. Install: sudo dnf install mpv"
if ! ldconfig -p 2>/dev/null | grep -q 'libmpv\.so\.2'; then
    echo "  ! libmpv not found: in the TV-only layout videos open in mpv's own window instead of inside tvdash. Install: sudo dnf install mpv-libs"
    missing=1
fi
check brave-browser "streaming tiles won't open. Install Brave from its RPM repo (https://brave.com/linux/), or set [browser] binary in ~/.config/tvdash/config.toml"
if ! busctl --user status org.freedesktop.secrets >/dev/null 2>&1; then
    echo "  ! no Secret Service on the session bus: sign-ins can't be saved. GNOME Keyring normally provides it."
    missing=1
fi
[[ $missing == 0 ]] && echo "  all good"
echo
echo "Run it from the app menu, or:  tvdash   (tvdash --windowed to try it without full screen)"
