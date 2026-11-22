# tvdash — TV-only Build Guide (Tile hub, v2)

How to build the **TV only** layout in the v2 look. Design: canvas page **TV only** on https://claude.ai/artifact/Rn5m3jsq6BYT5j4yVcd5PK. Decisions: `tvdash-look-and-feel-v2.md` §1–2.

Still valid from `tvdash-tv-only-plan.md`: one full-screen window (§8), the "Home problem" (§6), night mode forcing Dark (§9), mouse as a first-class input (§11 Input). **Replaced:** the side rail (§4) and the rail layout in §11.

> **Order:** build the shared pieces in §3 first (they're used by the two-screen layout too), then the TV shell (§4), then screens in the order of §7.

---

## 1. Scope

| In | Out (for now) |
|---|---|
| Tabs Home · Library · Services · Settings, Search page | Live video inside the Home tile |
| Spatial focus across mixed-size tiles | Collection page board (follow the Show detail pattern) |
| Back to tvdash: pill extension + F12 / remote key | Phone remote (v3; same `home` command) |
| Recent 3 services | Full launch history |
| Player HUD, Ambient idle | HDR tuning |

---

## 2. Files

```text
crates/app/
├─ src/shell_tv.rs          # window, tabs, page stack, key handling
├─ src/focus.rs             # spatial navigation over tile rects (§5)
├─ src/home_cmd.rs          # `home` command: socket + HTTP + CLI (§6)
├─ src/recent.rs            # last-3 services (§3.3)
└─ ui/
   ├─ shared/theme.slint     # tokens + tint (shared with dual)
   ├─ shared/brand.slint     # ten service fills/wordmarks (shared)
   └─ tv/
      ├─ tabs.slint          # TabBar, SubTabs, HintBar
      ├─ tile.slint          # Tile, MediaTile, ServiceTile, ValueTile
      ├─ home.slint
      ├─ library.slint       # panorama groups
      ├─ show.slint
      ├─ services.slint
      ├─ search.slint        # 6×6 keyboard
      ├─ settings.slint
      ├─ player_hud.slint
      └─ ambient.slint
extras/back-pill/            # Brave extension (§6.2)
```

---

## 3. Shared pieces

### 3.1 Theme tokens

```slint
// ui/shared/theme.slint
export enum ThemeName { dark, stone, mist }

export global Theme {
    in-out property <ThemeName> name: ThemeName.dark;
    out property <color> bg:             name == ThemeName.dark ? #0B0D10 : name == ThemeName.stone ? #CBC5BA : #C3CBD3;
    out property <color> surface:        name == ThemeName.dark ? #14171C : name == ThemeName.stone ? #DAD5CC : #D3DAE1;
    out property <color> surface-raised: name == ThemeName.dark ? #1B1F26 : name == ThemeName.stone ? #E3DFD7 : #DEE3E8;
    out property <color> border:         name == ThemeName.dark ? #2A2F37 : name == ThemeName.stone ? #B9B2A6 : #AEB7C1;
    out property <color> text-primary:   name == ThemeName.dark ? #E8EAED : name == ThemeName.stone ? #1F1D1A : #1A1E23;
    out property <color> text-secondary: name == ThemeName.dark ? #A0A6AF : name == ThemeName.stone ? #47433D : #3F464F;
    out property <color> text-muted:     name == ThemeName.dark ? #8A9099 : name == ThemeName.stone ? #4E4943 : #47505A;
    out property <color> scrim: #0B0D10C0;
    out property <color> bg-night: #050608;
}

// Poster tint, computed in Rust (tint_from, tvdash-laptop-current.md §2)
export global Tint {
    in-out property <color> card:   #0F2A2B;
    in-out property <color> poster: #1E4A4A;   // TV utility tiles
    in-out property <color> accent: #7FE0D2;
    in-out property <color> sec:    #A9C9C6;
    in-out property <color> track:  #1B3A3A;
}
```

> **TV is Dark only** in practice (night mode forces Dark, art stays dark). Keep the theme switch working anyway; the tokens are shared with the laptop.

### 3.2 Brand tiles

```slint
// ui/shared/brand.slint
export struct Brand { id: string, name: string, fill: brush, ink: color, wordmark: string, sub: string, sub-ink: color }
```

Fill the ten entries from the table in `tvdash-look-and-feel-v2.md` §1 (Rust side, so `config.toml` stays the source for URLs). Use the bundled SVG logo when `assets/logos/<id>.svg` exists; otherwise the wordmark text.

### 3.3 Recent services

```rust
// src/recent.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Recent { pub id: String, pub at: i64 }

pub fn push_recent(list: &mut Vec<Recent>, id: &str, now: i64) {
    list.retain(|r| r.id != id);
    list.insert(0, Recent { id: id.to_owned(), at: now });
    list.truncate(3);
}

/// Home's three tiles: recent first, padded from the fixed order.
pub fn home_three<'a>(recent: &[Recent], fixed: &'a [String]) -> Vec<&'a str> {
    let mut out: Vec<&str> = recent.iter().filter_map(|r| fixed.iter().find(|f| **f == r.id).map(|s| s.as_str())).collect();
    for f in fixed { if out.len() == 3 { break; } if !out.contains(&f.as_str()) { out.push(f); } }
    out
}

pub fn ago(at: i64, now: i64) -> String {
    let m = (now - at) / 60;
    match m { 0..=59 => format!("{m} min ago"), 60..=1439 => format!("{} h ago", m / 60), 1440..=2879 => "Yesterday".into(), _ => "Earlier".into() }
}
```

- Call `push_recent` **when the launch succeeds**, then save `config.toml` (`[streaming] recent`).
- Settings › Recent services › **Clear** empties the list.

---

## 4. TV shell

### 4.1 Window and scale

- Borderless full screen on the TV, covers GNOME's top bar (unchanged).
- **Design units are 1080p.** Proposal (open question in the v2 doc): render at 1920 × 1080 logical px and scale by `height / 1080`:

| Screen | Scale |
|---|---|
| 1080p | 1.0 |
| 1440p | 1.333 |
| 4K | 2.0 |

```rust
// shell_tv.rs: before creating the window
let scale = monitor_height_px as f32 / 1080.0;
std::env::set_var("SLINT_SCALE_FACTOR", format!("{scale:.3}"));
```

> If you keep the old §11 "same pixel sizes" rule instead, the grid needs extra columns at 4K. The fixed tile grid is simpler, so scaling is recommended.

- Request art at the real pixel size (`scale ×` tile size) so 4K stays sharp.

### 4.2 Tabs and page stack

```slint
// ui/tv/tabs.slint
export component TabBar inherits HorizontalLayout {
    in property <[string]> tabs: ["Home", "Library", "Services", "Settings"];
    in property <int> active;
    in property <bool> bar-focused;      // tabs get focus when you press Up from the top row
    spacing: 56px;
    for t[i] in tabs: Text {
        text: t;
        font-size: 54px;
        font-weight: i == active ? 400 : 300;
        color: i == active ? Theme.text-primary : #5E646D;
        // underline only while the bar itself has focus
        Rectangle { y: parent.height + 6px; height: 4px; width: parent.width; visible: bar-focused && i == active; background: Theme.text-primary; }
    }
}
```

| Key | Action |
|---|---|
| ← → on the tab bar | Switch tab (instant, no confirm) |
| ↓ from the tab bar | Focus the page's default tile |
| ↑ from the top tile row | Focus the tab bar |
| Back | Close overlay → leave detail page → go to Home tab → (on Home) nothing |
| OK | Activate focused tile |

- Pages: `Home`, `Library`, `Show(id)`, `Collection(id)`, `Services`, `Search`, `Settings`. Detail pages push onto a stack; **Back** pops.
- Remember the **last focused tile per page** so Back returns you to it.

### 4.3 Tiles

```slint
// ui/tv/tile.slint
export component Tile inherits Rectangle {
    in property <bool> focused;
    in property <brush> fill: Tint.poster;
    callback activated();
    border-radius: 6px;
    background: fill;
    drop-shadow-blur: focused ? 60px : 0px;
    drop-shadow-offset-y: focused ? 20px : 0px;
    drop-shadow-color: #00000099;
    animate drop-shadow-blur, drop-shadow-offset-y { duration: 120ms; easing: ease-out; }
    @children
    // 4 px focus outline, drawn just outside the tile
    Rectangle {
        x: -4px; y: -4px; width: parent.width + 8px; height: parent.height + 8px;
        border-radius: 9px; border-width: focused ? 4px : 0px; border-color: Theme.text-primary;
    }
    TouchArea { clicked => { root.activated(); } }   // mouse
}
```

- **Scale 1.04 on focus:** use Slint's element transforms if your Slint version supports scaling a `Rectangle`; otherwise grow `width`/`height` by 4% and shift `x`/`y` by half the difference (animate 120 ms).
- Variants: `MediaTile` (art + bottom gradient + title/sub + progress), `ServiceTile` (brand fill + wordmark + name bottom-left + optional RECENT chip), `ValueTile` (Settings: label top-left, value 40 px light bottom-left), `IconTile` (Search, Library, Screensaver: 48 px icon top-left, label bottom-left).

### 4.4 Hint bar

- Bottom-right, 30–54 px from the edges; pill `#0B0D10C7` with a 1 px `#2A2F37E6` border.
- Each page sets its own hints, e.g. Home on the hero: `OK Full screen · ☰ Pause / Stop · › More`.

---

## 5. Spatial focus

Tiles have mixed sizes (4 × 2 hero, 2-wide Resume, wide episode tiles), so "next item in the row" isn't enough. Every page gives `focus.rs` its tile rectangles; arrows pick the nearest tile in that direction.

```rust
// src/focus.rs
#[derive(Clone, Copy)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
#[derive(Clone, Copy)]
pub enum Dir { Up, Down, Left, Right }

pub fn next(tiles: &[Rect], from: usize, dir: Dir) -> Option<usize> {
    let a = tiles[from];
    let (acx, acy) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
    tiles.iter().enumerate()
        .filter(|(i, _)| *i != from)
        .filter_map(|(i, b)| {
            let (bcx, bcy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            // must lie beyond the edge we're leaving
            let (primary, ortho) = match dir {
                Dir::Right if b.x >= a.x + a.w - 1.0 => (b.x - (a.x + a.w), (bcy - acy).abs()),
                Dir::Left  if b.x + b.w <= a.x + 1.0 => (a.x - (b.x + b.w), (bcy - acy).abs()),
                Dir::Down  if b.y >= a.y + a.h - 1.0 => (b.y - (a.y + a.h), (bcx - acx).abs()),
                Dir::Up    if b.y + b.h <= a.y + 1.0 => (a.y - (b.y + b.h), (bcx - acx).abs()),
                _ => return None,
            };
            // prefer tiles that overlap on the other axis
            let overlap = match dir {
                Dir::Left | Dir::Right => b.y < a.y + a.h && b.y + b.h > a.y,
                Dir::Up | Dir::Down    => b.x < a.x + a.w && b.x + b.w > a.x,
            };
            let score = primary + ortho * if overlap { 0.5 } else { 2.0 };
            Some((i, score))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
}
```

- **Panoramas** (Home's Continue watching, Library groups, Show episodes): when focus moves past the visible edge, scroll the page so the focused tile sits 96 px from the edge (animate 200 ms).
- **Mouse:** hover sets focus (no ring jump while the pointer is still); the wheel scrolls panoramas.

---

## 6. Back to tvdash

### 6.1 One command, three entry points

```rust
// src/home_cmd.rs
pub enum HomeSource { Pill, Hotkey, Phone }

pub async fn go_home(app: &AppHandle, src: HomeSource) {
    if let Some(b) = app.browser.take() { b.close().await; }   // CDP Browser.close, then wait for exit
    if app.player.is_fullscreen() { app.player.leave_fullscreen(); }
    app.raise_window();
    app.toast(format!("Closed {} · back on Home", app.last_service_name()));
    tracing::info!(?src, "home");
}
```

| Entry | Transport |
|---|---|
| Pill (Brave extension) | `POST http://127.0.0.1:47800/home` (local axum server, loopback only) |
| F12 / remote key | GNOME shortcut runs `tvdash --home` → Unix socket `$XDG_RUNTIME_DIR/tvdash.sock` → `home` |
| Phone remote (v3) | WebSocket `home` on the same local server |

```rust
// main.rs: `tvdash --home` is a tiny client, not a second app
use std::io::Write;
if std::env::args().any(|a| a == "--home") {
    let path = std::env::var("XDG_RUNTIME_DIR")? + "/tvdash.sock";
    std::os::unix::net::UnixStream::connect(path)?.write_all(b"home\n")?;
    return Ok(());
}
```

### 6.2 Brave launch and the pill extension

Launch Brave with a dedicated profile, DevTools on loopback, the extension loaded, kiosk:

```bash
brave-browser \
  --user-data-dir="$HOME/.local/share/tvdash/brave" \
  --remote-debugging-port=9222 \
  --load-extension=/usr/share/tvdash/back-pill \
  --kiosk "https://www.netflix.com"
```

> **Check early:** Chrome 137 stopped honouring `--load-extension` in branded builds. If Brave does the same, install the extension once into the dedicated profile: open `brave://extensions` in that profile, turn on Developer mode, **Load unpacked** → `/usr/share/tvdash/back-pill`. It stays installed for that profile.

```json
// extras/back-pill/manifest.json
{
  "manifest_version": 3,
  "name": "tvdash — Back to tvdash",
  "version": "1.0",
  "background": { "service_worker": "worker.js" },
  "host_permissions": ["http://127.0.0.1:47800/*"],
  "content_scripts": [{ "matches": ["<all_urls>"], "js": ["pill.js"], "run_at": "document_idle" }]
}
```

```js
// extras/back-pill/pill.js
(() => {
  const host = document.createElement('div');
  const root = host.attachShadow({ mode: 'closed' });   // site CSS can't restyle it
  root.innerHTML = `
    <style>
      button { position: fixed; top: 54px; right: 96px; z-index: 2147483647; height: 76px;
        padding: 0 16px 0 24px; border-radius: 38px; border: 2px solid rgba(232,234,237,.35);
        background: rgba(11,13,16,.86); color: #E8EAED; font: 600 28px Inter, system-ui, sans-serif;
        display: flex; align-items: center; gap: 16px; transition: opacity .2s; cursor: pointer; }
      button:hover { background: #E8EAED; color: #0B0D10; }
      kbd { border: 2px solid currentColor; border-radius: 10px; padding: 4px 12px; font: 700 18px inherit; }
    </style>
    <button aria-label="Back to tvdash">← Back to tvdash <kbd>F12</kbd></button>`;
  document.documentElement.appendChild(host);
  const btn = root.querySelector('button');
  let t;
  const show = () => { btn.style.opacity = '1'; btn.style.pointerEvents = 'auto'; clearTimeout(t);
                       t = setTimeout(() => { btn.style.opacity = '0'; btn.style.pointerEvents = 'none'; }, 3000); };
  addEventListener('mousemove', show, { passive: true });
  btn.addEventListener('click', () => chrome.runtime.sendMessage('home'));
  show();
})();
```

```js
// extras/back-pill/worker.js — the fetch happens here, not in the page, so no CORS / mixed-content issues
chrome.runtime.onMessage.addListener((msg) => {
  if (msg === 'home') fetch('http://127.0.0.1:47800/home', { method: 'POST' }).catch(() => {});
});
```

- Full-screen video players can hide page overlays. If a site's player covers the pill, the F12 key still works; note any such site.

### 6.3 F12 shortcut (copy-paste)

```bash
# Adds a custom shortcut without wiping existing ones
KEY="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/tvdash-home/"
LIST=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
if [[ "$LIST" != *"$KEY"* ]]; then
  if [[ "$LIST" == "@as []" ]]; then NEW="['$KEY']"; else NEW="${LIST%]}, '$KEY']"; fi
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW"
fi
S="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY"
gsettings set "$S" name 'tvdash home'
gsettings set "$S" command 'tvdash --home'
gsettings set "$S" binding 'F12'
```

- Map the air mouse's Home / Back key to F12 if it sends something else (check with `wev` or `evtest`).
- **Test** that the GNOME shortcut wins over Brave's own F12 (DevTools) in kiosk mode.

---

## 7. Screens

| # | Screen | Components | Data |
|---|---|---|---|
| 1 | **Home** | Clock tile · hero `MediaTile` (4 × 2, top progress) · Search / Library / Screensaver `IconTile` · 3 `ServiceTile` + All 10 services · Continue watching column panorama | mpv state, `/hubs/continueWatching`, weather, `recent.rs` |
| 2 | **Services** | 5 × 2 `ServiceTile` (240 tall), RECENT chips, "Coming back" note | `[[streaming]]`, `recent` |
| 3 | **Library** | Sub-tabs; panorama groups: Watching now tile, Finish the season (4 × 2), Recently added (4 × 3), Popular (rank chips), Watch again, Genres / Collections | Same as `tvdash-laptop-current.md` §4 (sessions 10 s, Tautulli `top_*`) |
| 4 | **Show detail** | Hero tile · Resume (2-wide, tint) · From S1 · E1 · Shuffle · season sub-tabs · episode `MediaTile` 384 × 240 in a 3-row panorama | `/library/metadata/{show}/children`, `/onDeck` |
| 5 | **Collection** | Same pattern as Show; movie tiles with #n / status | `/library/collections/{id}/children` |
| 6 | **Search** | Query tile · 6 × 6 key tiles + Space / Delete / Clear · 4-across results with "where" strip · JustWatch credit; physical keyboard typing also works | TMDB + watch providers (`tvdash-future-plan.md` §3) |
| 7 | **Settings** | Sub-tabs; `ValueTile` grid (OK cycles the value; Quit asks to confirm) | `config.toml` |
| 8 | **Player HUD** | Title block, progress, square control tiles; OK shows, auto-hide 3.5 s | mpv IPC |
| 9 | **Ambient idle** | Unchanged | — |

> **Search keyboard:** the v2 design keeps the on-screen 6 × 6 keyboard, which settles the old "on-screen keyboard vs phone typing" question for TV-only. Phone typing can still be added in v3.

---

## 8. Milestones

| M | Goal | Done when |
|---|---|---|
| T1 | Shared pieces | `theme.slint`, `brand.slint`, `recent.rs` with unit tests (`push_recent` dedupes and caps at 3; `home_three` pads) |
| T2 | Shell | Full-screen window, scale factor, TabBar, page stack, Back behaviour |
| T3 | Focus | `focus.rs` with tests on the Home layout (Right from the hero goes to Search; Down from the hero goes to the first service tile) |
| T4 | Home + Services | Both screens with live data; launching a service updates Recent |
| T5 | Back to tvdash | Pill extension, local `/home`, socket + `--home`, F12 shortcut, toast |
| T6 | Library + Show (+ Collection) | Panoramas scroll with focus; Resume plays from `viewOffset` |
| T7 | Search, Settings, Player HUD, Ambient | All reachable with arrows + OK + Back, and with the mouse |

## 9. Done when

- [ ] Every tile on every screen is reachable with arrows + OK, and Back always goes up one level.
- [ ] Focus outline is visible from across the room on every tile type (art, brand colours, tint).
- [ ] Home shows the last 3 services; opening a 4th drops the oldest; Clear empties it.
- [ ] From any of the ten services, the pill and F12 both return to Home within ~1 s and show the toast.
- [ ] UI looks the same at 1080p, 1440p and 4K (scale factor), with sharp art at 4K.
- [ ] Night-mode hours force Dark.
