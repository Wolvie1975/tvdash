# tvdash — Streaming Multiview Concept (Design Brief)

## Scope & Guardrails
- **Streaming Services Only:** Multiview applies strictly to web streaming services launched in Brave (e.g., YouTube TV, ESPN, Twitch, Hulu, Pluto TV, etc.).
- **Excluded:** Plex and Jellyfin (`mpv` playback) remain single-view only and are **not** part of multiview.
- **Up to 4 Concurrent Windows:** Supports 2, 3, or 4 separate streaming services side-by-side.

---

## Visual Language (v3 Blades UI)
- **Typography:** `Nunito Sans` (Regular 400, Medium 500, SemiBold 600, Bold 700)
- **Dark Theme Tokens (`V3Theme`):**
  - Canvas (`bg`): `#1B140E` | Panel: `#221A13` | Surface: `#2A2018` | Raised: `#36291E` | Border: `#4A3A2B`
  - Text: Primary `#E8EAED`, Secondary `#A0A6AF`, Muted `#7A8291`
  - Accent / Focus Ring: `#F5A524` (Amber) — or `#E8000D` (KU Crimson) in KU theme
  - Status: OK `#4CC38A`, Warn `#E5A84B`, Alert `#F0605D`
- **10-Foot TV Ergonomics:** High-contrast focus rings (`3px`), pill controls (`V3Pill`), large hit targets, readable at living-room distance.

---

## Key Screens & States to Visualize

### 1. Multiview Setup / Picker (from Streaming Blade)
A launcher/picker where the user builds a multiview session before launching:
- **Layout Selector:**
  - **2-Up (Side-by-Side):** 50% / 50% split
  - **3-Up (1 Main + 2 Stack):** Large primary left/top + 2 smaller tiles
  - **2×2 Quad Grid:** 4 equal quadrants
- **Slot Assignment (Slots 1–4):**
  - Tap/select a slot to assign a streaming service from the configured `config.streaming` list.
  - Prevent or warn on picking the exact same service twice (each slot = a distinct service).
  - Visual indicator on slot 1 as the default **Audio Active** slot.
- **Primary Action:** `Launch Multiview` button (launches each service in its own Brave `--app` window positioned to its assigned TV quadrant).

### 2. Active Multiview — Tiled State (TV Output + Control HUD)
Because each tile is a live Brave `--app` window on the TV output (not embedded inside Slint):
- **TV Output:** 2–4 borderless Brave windows tiled cleanly into the chosen geometry.
  - Smaller tiled windows naturally pull lower-resolution ABR streams (e.g., 540p–720p).
- **Audio Focus Indicator:**
  - **Only 1 window outputs audio at a time** (controlled at the OS/PipeWire layer so web players don't pause or show "unmute" banners).
  - Clear visual badge / border highlight on the slot that currently has audio (e.g., speaker icon + amber focus border).
- **Quick Actions (via Remote / Laptop Companion / Floating HUD):**
  - **Switch Audio:** Click or D-pad to move audio focus to another tile.
  - **Maximize Tile:** Click/OK on a tile to expand it to full TV screen (triggers ABR step-up to 1080p) while hiding/lowering the other tiles.
  - **Swap Service:** Replace the service in a specific slot without closing the other tiles.
  - **Close Slot / Exit Multiview:** Close one window (reflowing or leaving empty) or close all and return to `tvdash` Home.

### 3. Maximized Single-Tile State (with Quick Return)
When a user clicks/maximizes one of the 4 streams:
- The chosen Brave window resizes to full-screen TV geometry (`100%`), stepping up its stream resolution automatically.
- A minimal, auto-hiding pill/bar (or back-pill) offers:
  - **Back to Grid** (restores the 2/3/4-tile layout without reloading any streams)
  - **Cycle Next / Previous Stream** (switches which stream is maximized + moves audio to it)
  - **Exit Multiview**

---

## Technical Architecture Notes (for Design Feasibility)
- **Per-Service Profiles:** Each service uses its own `--user-data-dir` (`profiles/<service-id>`), so logins and Widevine DRM approvals persist per service regardless of which slot it's opened in.
- **Independent Process Groups:** Each Brave window runs in its own process group (`pgid`), enabling independent launch, X11/XWayland geometry resize (`--window-position` / `--window-size`), and OS-level PipeWire audio muting per PID.
- **No Reload on Maximize/Restore:** Resizing happens at the window level so streams stay live and buffered when toggling between grid and full screen.
