# Multi-UI Footprint in `tvdash`

An assessment of whether keeping multiple dashboard UIs (**Classic**, **Blades**, **PS5**, and **Orbit**) in `tvdash` adds negative weight to the application across runtime performance, binary size, compile time, and maintenance.

## Summary

| Dimension | Impact | Notes |
|---|---|---|
| **Runtime CPU / GPU / RAM** | Negligible | Inactive UIs are not instantiated in Slint; backend state is shared. |
| **Binary / RPM Size** | Small (~5 MB) | Total growth from `v0.1.7` (16 MB) to `v0.1.75` (21 MB) across all added UIs, assets, and features. |
| **Compile Time / Build RAM** | Moderate | `slint-build` compiles all 4 shells into `crates/app` on every UI/app build. |
| **Maintenance & Testing** | Primary cost | Navigation, focus, intro/overlay styling, and feature parity span 4 separate UI trees. |

---

## 1. Runtime CPU, GPU, and Memory

- **Conditional instantiation in Slint:** In `crates/app/ui/laptop.slint`, all four shells are mounted behind mutually exclusive `if` conditions on `Display.dashboard-ui`:
  - `TvShell` (`ui/tv/shell.slint`) when `Display.dashboard-ui` is `"classic"` (or anything other than the three alternatives)
  - `V3Shell` (`ui/v3/shell.slint`) when `Display.dashboard-ui == "blades"`
  - `Ps5Shell` (`ui/ps5/shell.slint`) when `Display.dashboard-ui == "ps5"`
  - `OrbitShell` (`ui/orbit/shell.slint`) when `Display.dashboard-ui == "orbit"`
- **No inactive subtree overhead:** In Slint, components inside an `if` block that evaluates to `false` are not instantiated in the active item tree. Inactive UIs do not allocate UI items, run layout passes, tick `Timer`s, or issue render commands.
- **Shared data & backend:** All four UIs read from the same `state.slint` globals and `crates/app/src/bridge/` models. Background polling (Plex, Jellyfin, sports, weather), image caches, and playback state are not duplicated per UI.
- **Static binary mapping:** Compiled component metadata and bundled assets (icons, fonts, league logos) for unused UIs remain in the executable, which has negligible impact on modern desktop/TV hardware.

## 2. Binary & Package Size

Source sizes of the `.slint` UI directories in `crates/app/ui/`:
- `tv/` (Classic): ~288 KB
- `v3/` (Blades): ~276 KB
- `orbit/` (Orbit): ~204 KB
- `ps5/` (PS5): ~180 KB
- `shared/` (shared components/overlays): ~32 KB

Observed RPM package sizes in `dist/`:
- `tvdash-0.1.7-1.x86_64.rpm` (before Blades, PS5, and Orbit): **16 MB**
- `tvdash-0.1.75-1.x86_64.rpm` (with all 4 UIs, plus features/assets added since `0.1.7`): **21 MB**

## 3. Compile Time & Build Memory

- `crates/app/build.rs` runs `slint_build::compile("ui/app.slint")`.
- Because `ui/app.slint` exports `LaptopWindow` (which imports all 4 shells), `slint-build` generates Rust code for all four UIs together, and `rustc` compiles and optimizes all of it when building `crates/app`.
- This is the main technical weight: each additional UI increases `slint-build` codegen time and `rustc` memory/CPU usage during `crates/app` builds (especially release builds, where `-j 2` is already used to keep memory/CPU bounded).
- *Potential future lever (if needed):* Cargo feature flags could conditionally exclude unused UIs at compile time, though that would add conditional compilation complexity if you still want runtime switching via the `Choose a dashboard` picker.

## 4. Maintenance & Testing (Main Cost)

While `state.slint`, `bridge/`, and `controller.rs` keep the backend unified, each UI has its own layout, theme, and component tree:
- **UI parity:** New features or fixes (e.g., Settings rows, Browse all, Collection detail, Accounts editing) may need to be implemented and styled in up to 4 `.slint` trees.
- **Shared overlays & wiring:** Shared components like `crates/app/ui/shared/sports_alert.slint` and `crates/app/src/intro.rs` branch on `Display.dashboard-ui` / `DashboardUi` to match each UI's theme and intro style.
- **Navigation & focus:** `crates/app/src/tv_nav.rs` and per-UI focus/navigation logic must account for how each shell structures its pages and controls.

---

## Key Files Referenced

- `crates/app/build.rs` — `slint_build::compile("ui/app.slint")`
- `crates/app/ui/app.slint` — Slint compilation entry point
- `crates/app/ui/laptop.slint` — Conditional mounting of `TvShell`, `V3Shell`, `Ps5Shell`, and `OrbitShell`
- `crates/app/ui/state.slint` — Shared Slint globals and `Display.dashboard-ui`
- `crates/app/ui/shared/sports_alert.slint` — Example of per-UI theme branching in a shared overlay
- `crates/app/src/intro.rs`, `crates/app/src/tv_nav.rs`, `crates/app/src/callbacks.rs` — Rust-side `DashboardUi` handling
