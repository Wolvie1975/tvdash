# Cross-Platform Notes: Windows & macOS

`tvdash` is currently built for Linux (Fedora / GNOME Wayland). Most of the codebase (Slint UI, `tokio`, `reqwest`, Plex/Jellyfin/TMDB/Tautulli clients, SQLite cache, `directories`, and `keyring` v4) is already cross-platform, but a few desktop integrations are Linux/GNOME-specific.

---

## 1. Browsers & DRM (Widevine vs. FairPlay)

- **Safari does not support Widevine** — it uses Apple's **FairPlay** DRM. Streaming services work in Safari, but Safari does not support the Chromium CLI flags (`--app=`, `--user-data-dir=`, `--kiosk`, `--window-position=`) that `crates/player/src/browser.rs` uses to launch isolated, chromeless, TV-positioned windows.
- **Windows:** **Microsoft Edge** is preinstalled on Windows and supports Widevine, PlayReady, and Chromium `--app` flags. **Chrome** and **Brave** also work out of the box.
- **macOS:** **Chrome**, **Brave**, and **Edge** all include Widevine and support the same Chromium CLI flags.
- **Planned improvement:** Replace the hardcoded `"brave-browser"` default in `crates/core/src/config.rs` with OS-aware browser auto-detection (probing standard install paths on Windows/macOS/Linux when `[browser] binary` isn't explicitly set), and surface a clear picker/prompt in Settings/Setup if no compatible browser is found.

---

## 2. Linux-Only Components to Abstract (`#[cfg(target_os = ...)]`)

| Area | Current Implementation (Linux) | Windows Equivalent | macOS Equivalent |
|---|---|---|---|
| **Display detection & placement** | `crates/app/src/displays.rs` (`zbus` → `org.gnome.Mutter.DisplayConfig`) | `winit` monitor enumeration + Win32 display change events | `winit` monitor enumeration + CoreGraphics display callbacks |
| **Idle / sleep inhibition** | `crates/app/src/portal.rs` (`zbus` → `org.freedesktop.portal.Inhibit`) | `SetThreadExecutionState` (`ES_CONTINUOUS \| ES_DISPLAY_REQUIRED`) | `IOPMAssertionCreateWithName` (or `caffeinate` subprocess) |
| **System dark/light theme** | `crates/app/src/portal.rs` (`zbus` → `org.freedesktop.portal.Settings`) | Windows registry / WinRT UI settings | `NSAppearance` / `defaults read -g AppleInterfaceStyle` |
| **In-window `libmpv`** | `crates/player/src/libmpv.rs` loads `libmpv.so.2` / `libmpv.so` | Load `libmpv-2.dll` / `mpv-2.dll` (bundled next to `tvdash.exe`) | Load `libmpv.2.dylib` (from `brew install mpv` or bundled in `.app`) |
| **Browser launcher** | `crates/player/src/browser.rs` (`brave-browser`, XWayland flag) | `msedge.exe` / `chrome.exe` / `brave.exe` | `/Applications/Google Chrome.app/...`, `Brave Browser.app`, `Microsoft Edge.app` |

*(Note: `crates/core/Cargo.toml` uses `keyring = "4"`, which already supports Windows Credential Manager and macOS Keychain natively.)*

---

## 3. Bundling & Distribution Notes

- **Windows:**
  - Ship a `.zip` or installer (`.msi` / Inno Setup) containing `tvdash.exe`, `mpv.exe`, and `libmpv-2.dll` so end users don't have to install `mpv` separately.
  - Browser requirement is usually already satisfied by built-in Microsoft Edge.
- **macOS:**
  - Package as `tvdash.app` (`Contents/MacOS/tvdash`, `Info.plist`, `.icns` icon) inside a `.dmg`.
  - Bundle `libmpv.2.dylib` / `mpv` inside `Contents/Frameworks` (or document `brew install mpv`).
  - Unsigned `.app` bundles require `xattr -cr tvdash.app` on personal Macs; public distribution without Gatekeeper warnings requires Apple Developer code signing and notarization.
  - Verify Slint's OpenGL/Metal render context interoperability with `libmpv`'s render API on macOS.
