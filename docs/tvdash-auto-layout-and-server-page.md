# tvdash — Auto Layout and Server Page (2026-11-14)

Two features in one doc:

- **Part A — Auto layout:** tvdash picks dual-screen or TV-only by itself, based on which screens are on.
- **Part B — Server page:** Plex Dash-style monitoring and library tools inside tvdash.

Companion to `tvdash-v1-plan.md` (M1 window placement, M2 Plex client), `tvdash-tv-only-plan.md` (the TV shell) and `tvdash-dashboard-library-update.md` §7a (Watching now, Popular). **Plex only.**

## Current status (2026-11-14)

- **Part B:** implemented and validated in the project. This includes the Plex admin client, background polling, server summary/activity/library state, stream stop/terminate flows, metadata refresh, and poster/backdrop-picker support, plus the associated Slint UI wiring.
- **Part A (A1–A5 code implemented, uncompiled/untested here):**
  - **A1–A3 (Config, pure decision logic, UI):**
    - Added `crates/core/src/layout.rs` (`Layout`, `LayoutSetting`, `Decision`, `is_internal()`, `decide()` with non-matching external output fallback and unit tests).
    - Updated `crates/core/src/config.rs` (`DisplayConfig.layout: LayoutSetting` defaulting to `"auto"`, `switch_while_running: bool` defaulting to `true`).
    - Updated `crates/core/src/events.rs` (`AppEvent::TvConnected`, `AppEvent::DisplaysChanged`, `Command::SetLayoutSetting`).
    - Added `Dash.tv-connected` in `crates/app/ui/state.slint` and `home.slint` ("TV not connected" state on the TV card; Resume / Play / Open on TV actions disabled when disconnected).
    - Updated Settings › Display › Layout in `crates/app/ui/settings.slint` to **Automatic** / **Two screens** / **TV only**, wired in `callbacks.rs`, `controller.rs`, and `bridge/mod.rs`.
  - **A4 (Display detection & live watcher):**
    - Added `zbus = "5"` and `futures-util = "0.3"` to `Cargo.toml` / `crates/app/Cargo.toml`.
    - Updated `crates/app/src/displays.rs` and `main.rs` with startup `org.gnome.Mutter.DisplayConfig` `GetCurrentState` query, `MonitorsChanged` D-Bus watcher with 3 s debounce (and 5 s polling fallback off GNOME), `switch_while_running` handling, 30 s deferral while search/editors are active, and 5 s switch toasts.
  - **A5 (`$XDG_RUNTIME_DIR/tvdash/state.json` & lockfile):**
    - Updated `crates/player/src/mpv.rs` (`Mpv::reconnect`, `Mpv::detach`, non-child `quit()`) and `crates/player/src/browser.rs` (`Browser::reconnect` via `kill(pid, 0)`, `Browser::pid()`, `Browser::detach()`, non-child `wait()`/`close()`).
    - Added `RuntimeState` (`$XDG_RUNTIME_DIR/tvdash/state.json`) and `LockGuard` / `acquire_lock(2s)` (`$XDG_RUNTIME_DIR/tvdash/tvdash.lock`) in `crates/app/src/instance.rs` and `main.rs`.
    - Wired `save_runtime_state()` and `reconnect_runtime_state()` in `crates/app/src/controller.rs`.
- **Blocked on test execution here:** `run_command` failed with `connecting to sandbox server: read unix @->@: recvmsg: connection reset by peer`, so `cargo test --workspace` has not been run yet.

---

## Remaining work

- [ ] Run `cargo test --workspace --manifest-path /workspaces/Rust_Projects/tvdash/Cargo.toml` and fix any compile/test issues.
- [ ] Validate the full automatic layout flow (`A7`) against real dual-screen / laptop-only / TV-only behavior (lid open/close, TV plug/unplug, `switch_while_running = false`, mpv/browser survival across switches and restarts).

---

## Change log

| Date | Change |
|---|---|
| 2026-11-14 | Doc created: auto layout (Part A) and Plex Dash-style Server page (Part B) |
| 2026-11-14 | Status update: Part B is complete and validated; remaining work is Part A only |
| 2026-11-14 | Part A (A1–A5) implemented in `tvdash-core`, `tvdash-player`, and `tvdash` (`crates/app`); pending `cargo test --workspace` and live `A7` validation |
