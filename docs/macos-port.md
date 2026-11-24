# Running tvdash on macOS

Because macOS is Unix, almost all of `tvdash`'s low-level IPC, process management, and core crates already work without major rewrites.

## What Already Works on macOS
- **Unix sockets** (`crates/app/src/instance.rs`, `crates/player/src/mpv.rs`): `UnixStream` / `UnixListener` work unchanged (falling back to `std::env::temp_dir()` when `XDG_RUNTIME_DIR` isn't set).
- **Browser process management** (`crates/player/src/browser.rs`): `libc::kill`, `SIGTERM`/`SIGKILL`, `-pgid` process groups, and `ExitStatusExt` all work on macOS.
- **Core crates**: `tvdash-core`, `tvdash-media`, `tvdash-discover`, `tvdash-events`, `tvdash-widgets`, and Slint UIs.

## Required Changes (Small Scope)

1. **Gate Linux D-Bus (`zbus`) to Linux only**
   - Move `zbus` to `[target.'cfg(target_os = "linux")'.dependencies]` in `Cargo.toml` and `crates/app/Cargo.toml`.
   - Gate the D-Bus paths in `crates/app/src/displays.rs` and `crates/app/src/portal.rs` with `#[cfg(target_os = "linux")]`.
   - `displays.rs` already falls back to `winit` monitor enumeration.
   - For `portal.rs`, idle/sleep inhibit can be stubbed or wired to `caffeinate` / IOKit (`IOPMAssertionCreateWithName`), and system theme detection can use `winit` or `defaults read -g AppleInterfaceStyle`.

2. **Add macOS `libmpv` library names** (`crates/player/src/libmpv.rs`)
   - Add `"libmpv.2.dylib"` / `"libmpv.dylib"` (and Homebrew paths such as `/opt/homebrew/lib/libmpv.2.dylib` on Apple Silicon) alongside `["libmpv.so.2", "libmpv.so"]`.

3. **Add macOS Brave binary path** (`crates/player/src/browser.rs`)
   - Check `/Applications/Brave Browser.app/Contents/MacOS/Brave Browser` when looking up the browser executable.

## Caveat: OpenGL
- `crates/app/src/main.rs` requests Slint's `winit` backend with `require_opengl()`, and `crates/app/src/video.rs` draws `libmpv` frames into a `BorrowedOpenGLTextureBuilder`.
- macOS still supports OpenGL (capped at OpenGL 4.1, deprecated by Apple), so `winit` + `libmpv` OpenGL should work. If OpenGL context creation or `libmpv` rendering hits a macOS driver quirk, `tvdash` already falls back automatically to launching `mpv` in its own window.

## Effort & Packaging
- With `brew install mpv`, getting `tvdash` compiling and running on macOS is roughly **a few hours to a day of work** (mostly gating `zbus` and verifying `libmpv.2.dylib`, OpenGL, and Brave launch paths).
- Packaging it as a `.app` bundle (e.g., via `cargo-bundle` or a build script) is an optional extra step if you want a native Dock app.
