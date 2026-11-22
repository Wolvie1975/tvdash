# tvdash — Two-screen Build Guide (Blades, v2)

How to build the **dual-screen** layout (touch laptop as controller, TV for playback) in the v2 look. Design: canvas page **Two screens** on https://claude.ai/artifact/Rn5m3jsq6BYT5j4yVcd5PK. Decisions: `tvdash-look-and-feel-v2.md` §1 and §3.

**What changes from `tvdash-laptop-current.md`:** the layout and navigation only. Option B's three boxes and header become blades + one panel with a banner. **What stays:** every data source, shelf rule, Plex field, the season-selector rules, Collections, Watching now / Popular, poster tint, themes, the search panel's result groups, Screensaver 3 and the touch rules (§8 there). Where the two docs disagree on *layout*, this one wins; on *data*, `tvdash-laptop-current.md` wins.

> **Order:** shared pieces first (`tvdash-tv-only-build.md` §3 — theme, brand tiles, recent services, Back to tvdash), then the blades shell (§3 here), then pages in the order of §8.

---

## 1. Scope

| In | Out (for now) |
|---|---|
| Blades shell: library · home · apps · settings | Leaf chip (comes with `tvdash-leaf.md`; goes top-right on the banner) |
| Home, Library, Show, Collection, Apps, Settings | Server page boards (Settings › Server tab links to it; `tvdash-auto-layout-and-server.md` Part B) |
| Where to watch slide-over, Confirm playback, Up next | Live video preview |
| Recent 3 services on Home | |
| Back to tvdash (pill + F12) for streaming on the TV | |

---

## 2. Files

```text
crates/app/
├─ src/shell_dual.rs        # laptop window + TV idle window (unchanged role)
├─ src/blades.rs            # blade geometry + swipe → tab index (§3)
└─ ui/
   ├─ shared/…              # theme.slint, brand.slint (shared with TV)
   └─ dual/
      ├─ blades.slint        # Blade, BladeShell (blades + panel)
      ├─ banner.slint        # Banner (art, kicker, title, body, actions, clock)
      ├─ strip.slint         # PosterStrip, EpisodeStrip, ServiceGrid
      ├─ home.slint
      ├─ library.slint
      ├─ show.slint
      ├─ collection.slint
      ├─ apps.slint
      ├─ settings.slint
      ├─ where_to_watch.slint
      ├─ modals.slint        # ConfirmPlayback, UpNext
      └─ screensaver.slint   # Screensaver 3, unchanged content
```

---

## 3. Blades shell

### 3.1 Geometry (1920 × 1080 window)

- Order: `["library", "home", "apps", "settings"]`. Tabs `0..=active` sit on the **left**; the rest on the **right**.
- Every blade is 120 px wide, full height (40 px top/bottom margin), 24 px radius, vertical label 30 px.
- Visible strip per blade: **96 px for the one nearest the panel**, 48 px for the others. The active blade is the nearest one on the left and is tinted (`Tint.card` fill, `Tint.edge` border); others use `surface-raised` / `border`.

| Active | Left blades | Right blades | Panel x | Panel width |
|---|---|---|---|---|
| library | library | home · apps · settings | 120 | 1584 |
| **home** | library · home | apps · settings | 168 | 1584 |
| apps | library · home · apps | settings | 216 | 1584 |
| settings | all four | — | 264 | 1616 |

```rust
// src/blades.rs: same maths the canvas uses
pub struct BladePos { pub x: f32, pub left: bool, pub nearest: bool }
pub struct Layout { pub blades: [BladePos; 4], pub panel_x: f32, pub panel_w: f32 }

pub fn layout(active: usize, width: f32) -> Layout {
    let nl = active + 1;
    let nr = 4 - nl;
    let panel_l = 24.0 + 48.0 * (nl as f32 - 1.0) + 96.0;
    let panel_r = if nr > 0 { width - (24.0 + 48.0 * (nr as f32 - 1.0) + 96.0) } else { width - 40.0 };
    let mut blades = std::array::from_fn(|_| BladePos { x: 0.0, left: true, nearest: false });
    for i in 0..4 {
        blades[i] = if i < nl {
            BladePos { x: 24.0 + 48.0 * i as f32, left: true, nearest: i == nl - 1 }
        } else {
            let j = i - nl; // 0 = nearest on the right
            let x = if j == 0 { panel_r - 24.0 } else { panel_r + 96.0 + 48.0 * (j as f32 - 1.0) - 72.0 };
            BladePos { x, left: false, nearest: j == 0 }
        };
    }
    Layout { blades, panel_x: panel_l, panel_w: panel_r - panel_l }
}
```

- **Draw order:** left blades from outer to nearest; right blades from farthest to nearest; panel last.
- Labels sit at the visible edge: left blades rotate the text 180° and pad 8 px from the left (30 px for the nearest); right blades pad from the right.

### 3.2 Switching

| Input | Result |
|---|---|
| Tap a blade | That tab becomes active |
| Swipe left / right on the panel | Next / previous tab (Slint `SwipeGestureHandler`, Slint ≥ 1.9; otherwise a `TouchArea` with a 120 px threshold) |
| Keyboard / mouse | ← → with the panel focused; mouse wheel never switches tabs |

- Animate blade `x` and panel `x`/`width` over **260 ms ease-out**; the new page fades in over 180 ms.
- Detail pages (Show, Collection) open **inside the library panel** with a **‹ Library …** crumb on the banner; the library blade stays active.

```slint
// ui/dual/blades.slint
export component Blade inherits Rectangle {
    in property <string> label;
    in property <bool> active;
    in property <bool> left;
    in property <length> pad: 8px;
    callback tapped();
    width: 120px;
    border-radius: 24px;
    border-width: 1px;
    background: active ? Tint.card : Theme.surface-raised;
    border-color: active ? Tint.edge : Theme.border;
    animate x { duration: 260ms; easing: ease-out; }
    Text {
        text: label;
        font-size: 30px;
        color: active ? Theme.text-primary : Theme.text-secondary;
        rotation-angle: left ? 270deg : 90deg;
        x: left ? root.pad : root.width - self.height - root.pad;
        y: 40px;
    }
    TouchArea { clicked => { root.tapped(); } }
}
```

> Check the text rotation against the canvas: the left blades read bottom-to-top and the right ones top-to-bottom, as on the 360.

### 3.3 Panel and banner

- Panel: `surface`, 28 px radius, 1 px `Tint.edge` border, shadow `0 30 80 #0006`, `clip: true`.
- Behind everything: a radial **tint glow** from the top centre (`Tint.card` in Dark, theme `card` tint in Stone/Mist) fading to `bg`.

```slint
// ui/dual/banner.slint
export component Banner inherits Rectangle {
    in property <image> art;
    in property <string> kicker;
    in property <string> title;
    in property <string> details;
    in property <string> body;
    height: 600px;
    Image { source: art; width: parent.width; height: parent.height; image-fit: cover; }
    Rectangle { background: @linear-gradient(90deg, #0B0D10F2 0%, #0B0D10B8 45%, #0B0D100D 82%); }
    // top row: crumb or page title (left) · weather + clock (right)
    // bottom-left block (860 px wide): kicker 18 px caps · title 56–60 px · details 22 px · body 21 px · actions
    @children
}
```

- Text on the banner is always light (`#E8EAED`, `#A9C9C6`-style tint `sec`), because the art stays dark in every theme.
- Buttons on the banner: filled = `Tint.accent` with `#0B0D10` text; secondary = `#0B0D108C` with a `#E8EAED4D` border. 72–76 px tall.

---

## 4. Pages

| Page | Banner (height) | Below the banner | Data |
|---|---|---|---|
| **Home** | On the TV (600): kicker, title, episode · rating · runtime, description, progress, **−10 · Pause · +30 · Stop · Full screen**. Nothing playing → "Up next" + Resume on TV | Continue watching (most recent **6** items across both movies & TV, or optional TV / Movies filter, tap → Confirm playback) · **Recent services** (3 tiles + "2 h ago") · **All 10 apps** tile → apps blade | mpv IPC, `/hubs/continueWatching/items` (with `/library/onDeck` fallback, refreshes on start + stop), weather, `recent.rs` |
| **Library** | Finish the season item (470, **TV Series only**): segment bar, **Play S2 · E6 on TV**, Show page, ‹ › to page through; top row "Library" + TV / Movies + **Browse All** toggle | **Smart shelves** (vertical-scrolling, minimal horizontal scroll): Watching now (up to 6) · Recently added · Finish the season (TV only) · Collections (**Movies only**) · Popular (7 / 30 days) · Watch again. **Browse All mode:** single-row `All` / `A`–`Z` / `#` filter + vertical-scrolling poster grid. *Note: clicking any TV Series card must call `Library.open-show(show_key)` so `ShowPageState` loads; Movies/Episodes go to Confirm Playback / play.* | `tvdash-laptop-current.md` §4 |
| **Show page** | Series (560): details, summary, Up next line, **Resume S2 · E7 on TV** · From S1 · E1 · Shuffle | Season selector (tabs ≤ 6, pills ≤ 14, picker beyond) · **vertical-scrolling** episode grid (`280 × 158` stills with progress) | §5 there |
| **Collection** | Collection (540, **Movies only**): count · years · runtime, watched bar, Up next, **Resume / Play next on TV** · Shuffle an unwatched one | Collection order · Release date · A–Z, Hide watched, posters with #n / status | §6 there |
| **Apps** | Selected app (470) in its brand colours, "Recent · last opened 2 h ago", **Open on TV** | 5 × 2 grid of all ten, RECENT chips; tap a tile selects it (banner updates), **Open on TV** or a second tap launches | `[[streaming]]`, `recent` |
| **Settings** | none | Tabs Accounts (live sign-in code + inline token/URL edit) · Streaming · Display · Server · Diagnostics · What's new; Display: Layout, Theme (**Dark / Stone / Mist** — no Follow system), **Weather Location Lookup** (city search + pickable results), **Phone Remote QR**, Temperature, Clock; right column: Recent services (Clear), Back to tvdash key, Screensaver, **Quit tvdash** | `config.toml` |

- Home with **nothing playing** uses the same banner with the top Continue watching item and a single **Resume on TV**.
- Every poster and button has a pressed state (scale 0.97, darken 15%); nothing needs hover.

---

## 5. Overlays

| Overlay | Spec |
|---|---|
| **Where to watch** | Opens from the Home/Library search button. Slides in from the right over the panel (992 px wide, 250 ms, opacity + 48 px); the rest of the panel dims to `#0B0D1073`. Empty until you type (72 px field, focused). Groups: **In your library** (Resume / Play on TV, filled) · **On your apps** (Open on TV) · **Costs extra** (no action). Footer: "Streaming availability in the US from JustWatch, via TMDB". ✕ closes and clears |
| **Confirm playback** | Over `scrim`: poster 140 × 210, title, episode, progress + "14 min left", **Resume on TV** (76 px, full width) · Play from start · Cancel. Tap outside = Cancel |
| **Up next** | Driven by `Dash.next-ep-open` (`AppEvent::NextEpisodePrompt` on natural episode EOF): "Episode finished · <show>", next still 300 × 169, title, runtime + end time, **10 s countdown** bar, **Play next** (`Dash.play-next-ep()`) · **Back to Dashboard / Cancel** (`Dash.dismiss-next-ep()`). Countdown ends → plays |

---

## 6. Streaming on the TV from the laptop

- Tapping a service (Home's recent tiles, the Apps page) launches Brave **full screen on the TV** exactly as in `tvdash-tv-only-build.md` §6.2 (dedicated profile, DevTools port, pill extension, kiosk), then calls `push_recent`.
- While a service is open, the Home banner shows **"On the TV · Netflix"** with a **Close on TV** button that calls the same `home` command. The pill and F12 work too, for when you're at the TV.

---

## 7. Themes

- Dark / Stone / Mist from Settings › Display (the old header switch is gone from Home; add it back to the banner only if you miss it).
- Banner art and poster art stay dark in every theme; panel, rows, chips and blades follow the theme.
- The TV idle window is always Dark (unchanged).

---

## 8. Milestones

| M | Goal | Done when |
|---|---|---|
| D1 | Shared pieces | Same as TV T1 (theme, brand, `recent.rs`) |
| D2 | Blades shell | `blades.rs` unit tests for all four layouts (table in §3.1); tap + swipe switch with the 260 ms animation |
| D3 | Home | Banner playing / nothing-playing states, transport over mpv IPC, Continue watching → Confirm playback, Recent services launch Brave on the TV |
| D4 | Back to tvdash | Pill, F12, and the banner's Close on TV all close Brave and show the toast |
| D5 | Library, Show, Collection | Data and rules from `tvdash-laptop-current.md` §4–6; crumbs return to Library |
| D6 | Apps, Settings | Apps banner follows the selected tile; Settings tabs; Clear recent |
| D7 | Overlays + screensaver | Where to watch, Confirm playback, Up next countdown, Screensaver 3 |

## 9. Done when

- [ ] Blades match the table in §3.1 on every tab; tap and swipe both switch; detail pages keep the library blade active.
- [ ] Every control is ≥ 56 px (transport 76 px) and has a pressed state.
- [ ] Home shows the last 3 services with "… ago"; All 10 apps opens the apps blade.
- [ ] Launching any of the ten services opens Brave full screen on the TV; Close on TV, the pill and F12 all bring tvdash back.
- [ ] Continue Watching includes both movies and TV episodes (via `/hubs/continueWatching/items`), capped at the 6 most recent items, and refreshes after playback starts and stops.
- [ ] Library, Show and Collection behave as in `tvdash-laptop-current.md` §9 with vertical-scrolling Browse All (A–Z) and Show episodes, TV-only `Finish the season`, Movies-only `Collections`, and TV Series cards calling `Library.open-show`.
- [ ] Settings › Display includes Weather Location Lookup and Phone Remote QR, with explicit Dark / Stone / Mist themes (no Follow system).
- [ ] Where to watch, Confirm playback and Up next (`Dash.next-ep-open`) work by touch alone.
- [ ] Dark, Stone and Mist all pass the existing contrast test; banner text stays readable on every poster.
