# tvdash — Classic: Choose a dashboard (UI picker) — Build spec (rev. 2026-11-23, for v0.1.68)

This replaces the **Classic ↔ Blades toggle** in Classic's Settings › Display with a picker page that offers every Dashboard UI: **Classic, Blades, PS5 and Orbit**.

**Design canvas:** https://claude.ai/artifact/7TFUvQwFXoVHZbG36u5FvD, page **Classic · UI switcher**, board **Picker page**.

It applies to the **Classic** UI (`ui/tv/`). Blades, PS5 and Orbit already have four-card pickers in their own specs (`tvdash-ps5-a-build.md` §9.2, `tvdash-orbit-build.md` §10.1). The shared Rust change in §2 serves all four.

---

## 0. Decisions

- **Entry:** Settings › Display keeps one **Dashboard UI** value tile. OK on it opens a full page, **Choose a dashboard**.
- **Choice:** four tall tiles side by side, each with a preview, name, description and **✓ Current** on the active UI. OK switches; Back cancels.
- **Direct set, no cycling:** `next()` is no longer used to switch. The picker calls `set_dashboard_ui(value)` with the exact UI.
- **Only built UIs are listed.** Until PS5 or Orbit exists in the build, its tile is left out (§2.1), so the page shows 2, 3 or 4 tiles.
- **Hover = focus:** air-mouse hover gives the same outline, scale and shadow as remote focus, and click = OK.
- **No sideways scrolling:** four tiles always fit on one row (§3.2).

---

## 1. Look (Classic tokens, `ui/shared/theme.slint`)

| Part | Spec |
|---|---|
| Background | `Theme.bg` |
| Tabs | Unchanged Classic top tabs (54 px, **Settings** active) + clock / weather top-right |
| Breadcrumb | `SETTINGS › DISPLAY › DASHBOARD UI`, 22 px uppercase, 1.2 tracking, `text-secondary`, x 96, y 210 |
| Title | `Choose a dashboard`, 54 / 300, `text-primary`, 8 px under the breadcrumb |
| Tiles | §3.2 |
| Hint bar | Existing bottom-right pill: **OK** Switch · **‹ ›** Pick · **Back** Cancel |

Sub-tabs (Display · Accounts …) are **hidden** on this page; it is a sub-page of Display.

---

## 2. Rust

### 2.1 `DashboardUi` list and set (`crates/core/src/config.rs`)

```rust
impl DashboardUi {
    /// Every UI this build can show, in picker order.
    /// Add Ps5 / Orbit here in the same change that adds their shells.
    pub const ALL: &'static [DashboardUi] = &[
        DashboardUi::Classic,
        DashboardUi::Blades,
        // DashboardUi::Ps5,    // once ui/ps5 exists
        // DashboardUi::Orbit,  // once ui/orbit exists
    ];

    pub fn name(self) -> &'static str {
        match self { Self::Classic => "Classic", Self::Blades => "Blades", Self::Ps5 => "PS5", Self::Orbit => "Orbit" }
    }
    pub fn sub(self) -> &'static str {
        match self {
            Self::Classic => "Top tabs · tile hub",
            Self::Blades => "Side tabs",
            Self::Ps5 => "Header tabs · app row",
            Self::Orbit => "Pill nav · dials · rings",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Classic => "Flat tiles under big tabs. The original tvdash look.",
            Self::Blades => "Your tabs as blades down each side, one wide panel in the middle.",
            Self::Ps5 => "Full-screen art, a row of apps and cards along the bottom.",
            Self::Orbit => "Round everything: dials, rings and a floating pill nav.",
        }
    }
}
```

> If `Ps5` / `Orbit` aren't enum variants yet, add only the ones that exist. The `match`es above must stay exhaustive.

Tests:
- `ALL` has no duplicates and contains `Classic`.
- `parse(x.as_str()) == x` for every `x` in `ALL`.
- An unknown value in `config.toml` still loads as Classic.
- A config naming a UI that isn't in `ALL` (e.g. `"orbit"` on an older build) loads as Classic and logs a warning.

### 2.2 One setter for every UI (`src/callbacks.rs`)

Replace the toggle with a direct set. The other UIs' pickers call the same function.

```rust
/// Switch the dashboard UI. Saves config, pushes the new value to Slint, and
/// opens the new UI on its Settings › Display with focus on its own picker.
pub fn set_dashboard_ui(laptop: &LaptopWindow, cfg: &mut Config, ui: DashboardUi) {
    if !DashboardUi::ALL.contains(&ui) { return; }
    if cfg.display.dashboard_ui == ui { pop_picker(laptop); return; }   // same UI: just close the picker
    cfg.display.dashboard_ui = ui;
    if let Err(e) = cfg.save() { tracing::warn!(?e, "couldn't save dashboard_ui"); }
    let d = laptop.global::<Display>();
    d.set_dashboard_ui(ui.as_str().into());
    d.set_dashboard_ui_label(format!("{} · {}", ui.name(), ui.sub()).into());
    shell_tv::open_settings_display_focused_on_picker(laptop);         // land where you can switch back
    toast(laptop, &format!("Switched to {}", ui.name()));
}
```

| File | Today | Change |
|---|---|---|
| `src/tv_nav.rs` ~1596 | OK on the Dashboard UI tile toggles Blades ↔ Classic | OK opens `Page::DashboardPicker` |
| `src/callbacks.rs` ~444 | parses two values | builds the label from `name()` + `sub()`; wires `Display.choose-dashboard(string)` → `set_dashboard_ui(DashboardUi::parse(..))` |
| `ui/state.slint` `Display` | `dashboard-ui`, setter | add `in property <[DashboardChoice]> dashboard-choices` and `callback choose-dashboard(string)` |
| `ui/tv/settings.slint` ~188 | Blades / Classic text on the tile | value = `Display.dashboard-ui-label` (e.g. `Classic · Top tabs · tile hub`) |
| `src/shell_tv.rs` | pages | add `Page::DashboardPicker` (child of Settings › Display; Back pops to it) |
| Blades / PS5 / Orbit pickers | their own handlers | call `set_dashboard_ui` too |

`rg -n 'next\(\)' crates/app/src | rg -i dashboard` should come back empty afterwards (`next()` can stay in core if anything else uses it).

### 2.3 Choices pushed to Slint

```slint
// ui/state.slint
export struct DashboardChoice { id: string, name: string, sub: string, description: string, preview: image, current: bool }
```

```rust
// src/bridge/display.rs
pub fn dashboard_choices(current: DashboardUi) -> Vec<DashboardChoice> {
    DashboardUi::ALL.iter().map(|&ui| DashboardChoice {
        id: ui.as_str().into(), name: ui.name().into(), sub: ui.sub().into(),
        description: ui.description().into(),
        preview: preview_image(ui),          // §3.3
        current: ui == current,
    }).collect()
}
```

Push it once at start-up and again after every switch.

---

## 3. Page (`ui/tv/dashboard_picker.slint`)

### 3.1 Layout

| Element | Position |
|---|---|
| Breadcrumb + title | x 96, y 210 |
| Tile row | x 96, y 330, gap 16, centred when there are fewer than 4 tiles |
| Hint bar | Bottom-right, existing |

### 3.2 Tile (`Tile` from `ui/tv/tile.slint`)

| Part | Spec |
|---|---|
| Size | **420 × 600**, radius 6, fill `Theme.surface`. 4 × 420 + 3 × 16 = 1728 = the 1080p safe width |
| Preview | Top **420 × 300**, top corners 6, `image-fit: cover` (§3.3) |
| Name | x 24, y 324: 40 / 400 `text-primary` |
| Description | Under the name, 20 px, line-height 30, `text-secondary`, **max 3 lines** |
| Current | Bottom-left (24, bottom 24): **✓ Current** chip h40, `Theme.accent` fill, `Theme.on-accent` 18 / 600 |
| Focused (not current) | Same spot: **OK · Switch to <name>** chip, `text-primary` fill, `bg` 18 / 600 |
| Focus / hover | Classic tile focus: 4 px `text-primary` outline at 0 offset (radius 9), **scale 1.04**, shadow `0 20 60 #0009`, 120 ms ease-out |

### 3.3 Previews

**Bundled images**, one per UI: `assets/ui-previews/{classic,blades,ps5,orbit}.png`, **840 × 600** (2× so they stay sharp at 4K). Take them as screenshots of each UI's Home with placeholder data, or export them from the canvas boards.

```rust
fn preview_image(ui: DashboardUi) -> slint::Image {
    let bytes: &[u8] = match ui {
        DashboardUi::Classic => include_bytes!("../../assets/ui-previews/classic.png"),
        DashboardUi::Blades  => include_bytes!("../../assets/ui-previews/blades.png"),
        DashboardUi::Ps5     => include_bytes!("../../assets/ui-previews/ps5.png"),
        DashboardUi::Orbit   => include_bytes!("../../assets/ui-previews/orbit.png"),
    };
    image_from_png(bytes)   // existing helper used for bundled logos
}
```

> **Fallback:** if an image is missing, draw the canvas's simple shape preview (a few rectangles in that UI's colours) instead of leaving the tile blank.

### 3.4 Focus

| Key | Action |
|---|---|
| Open | Focus the **current** UI's tile |
| ← → | Previous / next tile (no wrap) |
| ↑ | Settings tab in the tab bar |
| OK | `Display.choose-dashboard(id)`: on the current UI it just closes the page |
| Back | Close the page, return to Display with focus on the Dashboard UI tile. Nothing changes |

`tv_nav.rs`: give the picker its own tile rects (four 420 × 600 rects at y 330), the same way the other Classic pages do.

---

## 4. Switching behaviour

1. Save `[display] dashboard_ui` to `config.toml`.
2. Push `Display.dashboard-ui`. `laptop.slint` swaps the shell straight away; mpv and Brave are untouched, so playback carries on.
3. Open the new UI on **Settings › Display** with focus on **its own** Dashboard UI picker, so switching back is one OK away.
4. Toast: `Switched to PS5`.
5. **No startup animation** on a switch (intros are cold start only, `tvdash-startup-animation-build.md` §1).

The choice survives a restart.

---

## 5. Open questions

| # | Question | Default |
|---|---|---|
| 1 | A **"Keep PS5? Switching back in 10 s"** prompt after each switch (Keep / Switch back, Back = switch back)? | **Not included.** Add it later in `set_dashboard_ui` if a UI ever looks broken on the TV |
| 2 | Show a **Preview startup animation** button on each tile? | No: Settings › Display already has Preview for the current UI |
| 3 | List UIs that aren't built yet as "Coming soon"? | No: only `DashboardUi::ALL` |

---

## 6. Files

```text
crates/core/src/config.rs          # DashboardUi::ALL, name(), sub(), description() + tests
crates/app/
├─ src/callbacks.rs                # set_dashboard_ui(); choose-dashboard wiring; label
├─ src/bridge/display.rs           # dashboard_choices(), preview_image()
├─ src/shell_tv.rs                 # Page::DashboardPicker, open_settings_display_focused_on_picker()
├─ src/tv_nav.rs                   # OK on the tile opens the picker; picker tile rects
├─ assets/ui-previews/{classic,blades,ps5,orbit}.png
└─ ui/
   ├─ state.slint                  # DashboardChoice, dashboard-choices, choose-dashboard
   └─ tv/
      ├─ settings.slint            # tile value = dashboard-ui-label
      └─ dashboard_picker.slint    # new page
```

## 7. Task list

1. `DashboardUi::ALL` + text helpers + tests (§2.1).
2. `set_dashboard_ui` and the switch table (§2.2), including the other UIs' pickers.
3. `DashboardChoice` + `dashboard_choices` + previews (§2.3, §3.3).
4. `Page::DashboardPicker`, the Slint page and its focus rects (§3).
5. Switch behaviour: land on the new UI's Display picker, toast (§4).
6. Check: hover = focus; 1080p / 1440p / 4K; tests per crate, lightest first (`cargo test -j 2 -p tvdash-core`, …, `cargo test -j 2 -p tvdash -- --test-threads=1`); version bump in `Cargo.toml`, `CHANGELOG.md`, `docs/fedora-install.txt`, `docs/status.md`, `releases/README.md`; the user runs the release build (`GEMINI.md`).

## 8. Done when

- [ ] OK on Settings › Display › **Dashboard UI** opens **Choose a dashboard** with one tile per built UI, focus on the current one.
- [ ] OK on another tile switches immediately, saves the choice, lands on the new UI's Display picker, and shows the toast; playback keeps going.
- [ ] OK on the current tile or Back closes the page with no change.
- [ ] Every UI's picker uses `set_dashboard_ui`; nothing toggles with `next()` anymore.
- [ ] A config naming an unbuilt UI falls back to Classic.
- [ ] Hover gives the focus effect; works by remote alone; looks right at 1080p, 1440p and 4K.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-23 | Spec created: picker page (option B) replaces the Classic ↔ Blades toggle; shared `set_dashboard_ui` for all four UIs |
