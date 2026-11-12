# tvdash — TV-only Mode Plan (single screen)

Companion to `v1-plan.md` and `future-plan.md`. Planned for **v3**, after the phone remote.

Design canvas: https://claude.ai/artifact/UggZzEChLyVVffZNfdGnTf, page **Single screen** (Concept 4 — Side rail).

---

The chosen design is **Concept 4** (side rail that slides in and out, theme switch in the rail). Concepts 1–3 are alternatives kept for reference.

> **Implementation notes (2026-11-11), from the current codebase:**
>
> - **Video inside the tvdash window is required, not optional.** On GNOME Wayland an app can't raise
>   another app's window, can't raise itself over a focused full-screen window, and can minimise but
>   never un-minimise. With mpv in its own window, Home (§6), Play/Pause from Home with video behind
>   (§7) and Full screen (§7) all fight the compositor. Render mpv into the tvdash window through
>   libmpv's render API (`libmpv2`, drawn as an OpenGL underlay via Slint's rendering notifier), so
>   Home, Full screen and the overlay are views of one window. Streaming sites stay a separate browser
>   window; Home there means closing it, which tvdash already does reliably.
> - **No DevTools protocol needed** to leave a streaming site: tvdash already runs the browser in its
>   own process group and closes the whole group (SIGTERM, then SIGKILL).
> - **`tvdash --home`** needs a single-instance socket (`$XDG_RUNTIME_DIR/tvdash.sock`); that also
>   stops a second copy from starting.
> - **Shared vs. shell code:** `controller.rs` (commands in, `AppEvent`s out) is already UI-agnostic.
>   `bridge/` writes the main window's globals, which both layouts read (the TV shell is a view inside
>   that window), so no per-shell adapter is needed. It is split by concern: home, library, accounts, status.
> - **Order:** the "night mode forces Dark" check depends on v2's night mode; the phone remote (v3)
>   comes first, as below.
> - **Open questions, suggested answers:** search by typing on the phone, with a basic on-screen
>   keyboard as fallback for CEC/keyboard; ambient idle in, reusing the TV idle screen's pixel shift.

---

## Progress

**Step 1 (built):** `[display] layout` and `--layout`; one full-screen window on the TV; side
rail (closed/open, theme); Home (hero, Continue Watching, your services, sign-in codes and
prompts), Library, Services, Settings (layout, theme, units, clock, accounts, quit); focus
navigation with arrows/OK/Back (rows remember their place); ambient idle after 5 min;
`tvdash --home` over a single-instance socket (Home stops mpv or closes the browser).
The TV layout is a view inside the existing main window rather than a separate shell module,
so it reuses all the existing event handling.

**Step 2 (built):** video inside the tvdash window. libmpv is loaded at runtime
(`tvdash-player/libmpv.rs`); its render API draws each frame into an OpenGL texture inside
Slint's own context (`app/src/video.rs`, a rendering notifier), shown as an image under the
shell. Videos start full screen (OK overlay, Space, ←/→, Back = Home with the video still
playing behind it); Home's card has Pause, Full screen and Stop. Slint is asked for desktop
OpenGL, since GLES rules out mpv's VAAPI hardware decoding. No libmpv, or a render context that
fails, means mpv's own window (as in two screens), resuming where it was.

**Next:** Search on the TV; the phone remote (v3) and HDMI-CEC; night-mode Dark (needs v2).

## 1. What it is

The same app with a second layout: no laptop panel, everything on the TV. Home is a full-screen 10-foot screen with a side rail; Plex/Jellyfin video and streaming sites play full screen on top of it; **Home** always brings you back.

It's **not a separate app**. It shares every crate; only the UI shell and input handling differ.

| Part | Dual-screen | TV-only |
|---|---|---|
| `core`, `media`, `player`, `discover` | Shared | Shared |
| Sign-in flows, settings, streaming list, themes | Shared | Shared |
| Windows | Laptop dashboard + TV idle window | One full-screen window on the TV |
| Input | Touch and mouse on the laptop | Focus ring: phone remote, HDMI-CEC remote, keyboard or air mouse |
| Sizes | Desk distance (body 18–24 px) | 10-foot (body ≥ 32 px, safe area 96 / 54 px) |

---

## 2. Switching layouts

```toml
# config.toml
[display]
layout = "dual"        # "dual" | "tv"
```

Also exposed as *Settings › Displays › Layout: Two screens / TV only* (takes effect on restart).

```text
crates/app/
├─ src/main.rs         # reads display.layout, starts the matching shell
├─ src/shell_dual.rs   # laptop window + TV window
├─ src/shell_tv.rs     # one full-screen window, focus navigation, Home handling
└─ ui/
   ├─ shared/          # Theme, tiles, poster cards, playback overlay
   ├─ dual/            # laptop dashboard screens
   └─ tv/              # side rail, 10-foot screens
```

Both shells consume the same `AppEvent` stream and send the same commands, so fixes to playback, sign-in or search apply to both.

> **Optional:** if you ever want TV-only on a machine that never runs dual mode (e.g. a mini PC), add a second binary crate `tvdash-tv` in the same workspace that starts `shell_tv` directly. Same code, two executables.

---

## 3. Screens

| Screen | Contents |
|---|---|
| **Home** | Side rail (time, Home, Search, Library, Services, Settings, theme). Main area: **On the TV now** / **Up next** hero tinted from the art, Continue Watching row, Your services row. The On the TV now hero has **Play/Pause** (one toggle), **Full screen** and **Stop** |
| **Search** | Where to watch (v1.1), 10-foot layout, on-screen keyboard or phone typing |
| **Library** | Plex + Jellyfin library, same data as dual mode |
| **Services** | Every streaming site as large logo tiles (wrapping grid) |
| **Settings** | Same settings as dual mode, focus-navigable |
| **Playing** | mpv or the browser, full screen |
| **Overlay** | Press OK during playback: title, progress, end time, controls |
| **Ambient idle** (optional) | After ~5 min idle on Home: full-bleed art + clock (Concept 2); any key returns |

---

## 4. Side rail

| State | Width | Shows |
|---|---|---|
| Closed (default) | 104 px | Time, five icons, theme button (cycles Dark → Stone → Mist) |
| Open | 340 px | Clock + weather, labelled items, Theme segmented control |

- Left arrow from the leftmost item opens it; Right, Back or choosing an item closes it.
- Animate width over 260 ms ease-out; labels fade in over 200 ms. Main area dims to ~45% while the rail has focus.

```slint
in-out property <bool> rail-open: false;

rail := Rectangle {
    width: rail-open ? 340px : 104px;
    animate width { duration: 260ms; easing: ease-out; }
    clip: true;
}
```

---

## 5. Focus navigation

- One focus ring at a time: 4 px outline, 6 px offset, light ring in Dark and dark ring in Stone/Mist; focused tiles scale to ~1.06.
- Arrows move between items in a row and between rows; OK activates; Back closes the rail, overlay or current screen.
- Remember the last focused item per row so moving up and down doesn't lose your place.
- Slint's built-in keyboard focus handles single widgets; for rows and the rail, keep a `focus_index` per row in Rust and drive `has-focus` styling from it.

---

## 6. The "Home" problem

When mpv or Chrome is full screen, the tvdash window isn't focused and won't receive key presses. **Home must reach the app from outside its window:**

| Source | How |
|---|---|
| Phone remote (v3) | WebSocket command `home` to the app's local server |
| HDMI-CEC remote | A `cec-client` listener process forwards the Home key |
| Keyboard | GNOME custom shortcut runs `tvdash --home`, which signals the running app (D-Bus or a Unix socket) |

On `home`: quit mpv over IPC or close the browser via the DevTools protocol, then raise the tvdash window.

> **Dependency:** TV-only mode needs at least one of these before it's usable. Build the phone remote first (same v3).

---

## 7. Playing from Home

- **Play/Pause** on the On the TV now card resumes or pauses mpv over IPC without leaving Home. Video keeps playing behind Home, so you hear it; the card keeps showing the backdrop art and switches its label to **PLAYING**.
- **Full screen** brings the mpv window (or the browser) to the front, paused or playing as it was.
- **Later, if wanted:** a live picture of the video inside the card. That needs mpv to render into the tvdash window through libmpv's render API (`libmpv2` crate) instead of its own window, so it's deliberately left out of the first version. *(See the implementation note at the top: on GNOME Wayland this is needed from the start.)*

---

## 8. Windows and full screen

- tvdash's window is borderless full screen on the TV and covers GNOME's top bar.
- mpv and the browser open full screen on top; nothing on the TV is ever windowed.
- With no laptop panel, the TV is the only display, so window placement is simpler than in dual mode.

---

## 9. Themes on the TV

- Dark / Stone / Mist as in dual mode, switched from the rail.
- During night-mode hours the TV forces Dark regardless of the chosen theme (glare in a dark room, OLED burn-in).
- Continue Watching thumbnails stay dark in every theme because they show the art itself.

---

## 10. Done when

- [ ] `layout = "tv"` starts one full-screen window on the TV; `layout = "dual"` still works unchanged.
- [ ] Every Home screen item is reachable with arrows + OK only.
- [ ] Rail opens and closes as specified; theme switch works from both rail states.
- [ ] Home returns to the home screen from mpv and from a streaming site, via phone remote and via the keyboard shortcut.
- [ ] Playback overlay appears on OK and hides after 5 s.
- [ ] Night-mode hours force Dark on the TV.

---

## 11. Open questions

- [ ] Search: an on-screen keyboard, or rely on typing from the phone remote?
- [ ] Ambient idle screen (Concept 2) after ~5 min on Home: in or out?
