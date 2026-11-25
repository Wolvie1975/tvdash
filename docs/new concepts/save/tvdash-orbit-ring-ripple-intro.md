# tvdash — Orbit startup animation: Ring ripple (replaces Play-ring iris)

**Date:** 2026-11-24 · **Applies to:** Orbit Dashboard UI (`dashboard_ui = "orbit"`) only.
**Replaces:** `tvdash-orbit-build.md` §11 (Play-ring iris + clock blink). Where the two disagree, this doc wins.
**Unchanged:** the shared stage machine and rules in `tvdash-startup-animation-build.md` (§1 rules, §2 config, §3 stages, §7 `IntroGate`). Classic, Blades and PS5 intros are not touched.

Prototype: the in-chat "Ring ripple" concept from 2026-11-24 (replay, slow motion, slow load). The timings below match it.

---

## 1. What it looks like

1. **Logo.** On the Storm grey background, accent rings pulse out from the centre every 0.6 s, behind the centred `tvdash` / `ORBIT` wordmark.
2. **Hold.** If Home data isn't ready yet, the rings keep pulsing at the same rhythm. The pulse *is* the loading indicator, so there's no progress line.
3. **Reveal.** The rings stop spawning and fade out. One last ring swells and drifts up to the **clock badge**, then settles into it: its accent border turns into the badge's glass hairline, and the time and weather fade in. As it lands, one small echo ripple goes out from the badge. Meanwhile Home rises in behind it: nav, hero, Up next, then the apps dial.
4. **Done.** The intro layer is removed, and the real clock badge is shown in the same frame, at the same place and size.

Total about **3.7 s** on a normal load (1.8 s logo + 1.9 s reveal).

---

## 2. Look (1920 × 1080 design units, scaled by `TvUi.u`)

| Item | Value |
|---|---|
| Background | `OrbitTheme.bg` `#2B313D`, full screen; fades out during reveal so Home's backdrop shows through |
| Ripple centre | (960, 540) |
| Ripple ring | 3 px border `OrbitTheme.accent`, no fill, circle (radius = ½ size) |
| Wordmark | `tvdash` 104 / 700, −2 tracking, `text` · `ORBIT` 18 / 600, 6 tracking, `accent`, 10 px below · centred, block centre ≈ y 540 |
| Traveller ring | 4 px border `accent` → lands as the clock badge (1 px `#FFFFFF1A` border + `glass` fill) |
| Clock badge target | Home badge: **190 px**, centre **(1745, 125)** (right 80, top 30) |
| Echo ripple | 3 px border `accent`, from the badge |

No blur, no shadows, no glow. Everything is size, position and opacity.

---

## 3. Timeline (seconds)

`t` = seconds since `logo` began. `r` = seconds since `reveal` began. Easing: **out** = `1 − (1 − x)³`, **in-out** = cubic in-out.

### 3.1 Logo phase (`logo`, ends at **1.80**)

| Element | Start | Duration | Change |
|---|---|---|---|
| Ring *n* (n = 0, 1, 2 …) | 0.20 + 0.60 × n | 1.80 life | diameter 60 → 760 (out) · opacity 0.8 → 0 (linear) |
| Wordmark | 0.40 | 0.70 | opacity 0 → 1, scale 0.94 → 1 (out) |

At most 4 rings are alive at once (life 1.8 s ÷ 0.6 s spacing), so 4 pooled slots are enough.

### 3.2 Hold (`hold`, only while Home data isn't ready, capped by `startup.max_hold_secs`)

Rings keep spawning every 0.60 s. Nothing else changes.

### 3.3 Reveal (`reveal`, ends at **1.90** → `done`)

| Element | Start | Duration | Change |
|---|---|---|---|
| Ring spawning | 0.00 | — | stops. Live rings keep growing, extra fade to 0 over 0.50 |
| Wordmark | 0.00 | 0.40 | opacity 1 → 0, scale 1 → 1.04 |
| Intro background | 0.30 | 0.50 | opacity 1 → 0 (out) |
| Traveller ring | 0.00 | 1.00 | centre (960, 540) → (1745, 125) (in-out, p) · diameter `120 + 70p + 140·sin(πp)` (swells to ≈ 295 mid-way, lands at exactly 190) |
| Traveller lands | 0.85 | 0.40 | `land` 0 → 1 (out): accent border out, `#FFFFFF1A` hairline in, `glass` fill in, time + weather in |
| Echo ripple | 1.00 | 0.70 | diameter 190 → 340 (out), opacity 0.6 → 0 |
| Pill nav | 0.50 | 0.45 | `IntroRise`, drops **from −30 px** |
| Hero (incl. play button) | 0.60 | 0.50 | `IntroRise` |
| Up next label | 0.70 | 0.50 | `IntroRise` |
| Up next poster *i* | 0.70 + 0.07 × i | 0.50 | `IntroRise` |
| Apps dial (arc, apps, label) | 0.85 | 0.55 | `IntroRise`, **rises 40 px** |
| Quick circles | 0.85 | 0.55 | `IntroRise` |
| *Reveal ends* | 1.90 | | → `done`: intro layer removed, real clock badge shown |

```rust
// src/intro.rs — replaces the iris values
Skin::Orbit => (Duration::from_millis(1800), Duration::from_millis(1900)), // logo_len, reveal_len
```

---

## 4. How it's driven

Slint `animate` only runs on property changes and has no loops, so the repeating ripple can't be plain `animate` blocks. Instead, **Rust computes each frame** with one pure function, and Slint only binds the values. This keeps the math testable (same approach as the Orbit ring paths, §1.5 of the build spec), and Preview / slow-motion testing come for free.

- While the Orbit intro runs (`stage` is `logo`, `hold` or `reveal`), a `Timer` (`TimerMode::Repeated`, 16 ms) calls `frame(t, reveal_start)` and pushes the result into the `OrbitIntro` global.
- The timer is stored in the `IntroGate` and dropped at `done`, so nothing runs after the intro.
- Home's rise-ins use the existing **`IntroRise`** (stage-driven `animate`), as in the other skins.

### 4.1 Rust: frame function

```rust
// src/orbit/intro_frame.rs
use std::f32::consts::PI;

pub const LOGO: f32 = 1.8;
pub const REVEAL: f32 = 1.9;
pub const SLOTS: usize = 4;
const FIRST: f32 = 0.2;
const EVERY: f32 = 0.6;
const LIFE: f32 = 1.8;
pub const CENTRE: (f32, f32) = (960.0, 540.0);
pub const BADGE: (f32, f32, f32) = (1745.0, 125.0, 190.0); // x, y, diameter

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Ring { pub d: f32, pub opacity: f32 }

#[derive(Clone, Copy, Default, Debug)]
pub struct Frame {
    pub rings: [Ring; SLOTS],
    pub word_opacity: f32,
    pub word_scale: f32,
    pub bg_opacity: f32,
    pub trav_x: f32, pub trav_y: f32, pub trav_d: f32, pub trav_opacity: f32,
    pub land: f32,
    pub echo_d: f32, pub echo_opacity: f32,
}

fn cl(x: f32) -> f32 { x.clamp(0.0, 1.0) }
fn out(x: f32) -> f32 { 1.0 - (1.0 - x).powi(3) }
fn in_out(x: f32) -> f32 { if x < 0.5 { 4.0 * x * x * x } else { 1.0 - (-2.0 * x + 2.0).powi(3) / 2.0 } }
fn lerp(a: f32, b: f32, p: f32) -> f32 { a + (b - a) * p }

/// `t`: seconds since `logo` began. `reveal_at`: value of `t` when `reveal` began (None before).
pub fn frame(t: f32, reveal_at: Option<f32>) -> Frame {
    let mut f = Frame { word_scale: 1.0, bg_opacity: 1.0, ..Default::default() };
    let r = reveal_at.map(|s| t - s);

    // Ripple rings: spawn every EVERY s from FIRST until reveal starts.
    let spawn_until = t.min(reveal_at.map_or(f32::INFINITY, |s| s - 1e-3));
    if spawn_until >= FIRST {
        let hi = ((spawn_until - FIRST) / EVERY).floor() as i32;
        let fade = r.map_or(1.0, |r| 1.0 - cl(r / 0.5));
        for n in (hi - 3).max(0)..=hi {
            let age = t - (FIRST + EVERY * n as f32);
            if !(0.0..=LIFE).contains(&age) { continue; }
            let q = age / LIFE;
            f.rings[n as usize % SLOTS] = Ring { d: 60.0 + 700.0 * out(q), opacity: 0.8 * (1.0 - q) * fade };
        }
    }

    match r {
        None => {
            let w = out(cl((t - 0.4) / 0.7));
            f.word_opacity = w;
            f.word_scale = 0.94 + 0.06 * w;
        }
        Some(r) => {
            f.word_opacity = 1.0 - cl(r / 0.4);
            f.word_scale = 1.0 + 0.04 * cl(r / 0.4);
            f.bg_opacity = 1.0 - out(cl((r - 0.3) / 0.5));

            let p = in_out(cl(r / 1.0));
            f.trav_x = lerp(CENTRE.0, BADGE.0, p);
            f.trav_y = lerp(CENTRE.1, BADGE.1, p);
            f.trav_d = 120.0 + 70.0 * p + 140.0 * (PI * p).sin();
            f.trav_opacity = 1.0;
            f.land = out(cl((r - 0.85) / 0.4));

            let e = cl((r - 1.0) / 0.7);
            f.echo_d = 190.0 + 150.0 * out(e);
            f.echo_opacity = if e > 0.0 && e < 1.0 { 0.6 * (1.0 - e) } else { 0.0 };
        }
    }
    f
}
```

**Tests** (`cargo test -j 2 -p tvdash -- intro_frame`):

- `frame(0.0, None)`: all rings opacity 0, wordmark opacity 0.
- `frame(1.0, None)`: exactly 2 rings with opacity > 0; wordmark opacity > 0.
- `frame(5.0, None)` (long hold): never more than 4 rings with opacity > 0; spawning still running.
- `frame(rs + 0.2, Some(rs))`: no ring is younger than 0.2 s (nothing spawned after `rs`).
- `frame(rs + 1.0, Some(rs))`: traveller at (1745, 125), diameter 190 (±0.5).
- `frame(rs + 1.25, Some(rs))`: `land` = 1, wordmark and `bg_opacity` = 0.
- `frame(rs + 1.9, Some(rs))`: all ring opacities and `echo_opacity` = 0.
- Skip during logo with data ready (`rs` = 0.9 < `LOGO`): frames stay finite, and the traveller still lands at 1.0 s.

### 4.2 Rust: driver (in `intro.rs`, Orbit only)

```rust
// Orbit: frame timer. Started together with stage = Logo, kept in IntroGate.
if skin == Skin::Orbit {
    let started = std::time::Instant::now();
    let weak = app.as_weak();
    let g = gate.clone();
    let rings = std::rc::Rc::new(slint::VecModel::from(vec![OrbitRingState::default(); SLOTS]));
    app.global::<OrbitIntro>().set_rings(rings.clone().into());
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(app) = weak.upgrade() else { return };
        let t = started.elapsed().as_secs_f32();
        let reveal_at = g.borrow().reveal_started.map(|i| i.duration_since(started).as_secs_f32());
        let f = intro_frame::frame(t, reveal_at);
        for (i, r) in f.rings.iter().enumerate() {
            rings.set_row_data(i, OrbitRingState { d: r.d, opacity: r.opacity });
        }
        push_frame(&app.global::<OrbitIntro>(), &f);   // sets the scalar properties below
    });
    gate.borrow_mut().orbit_timer = Some(timer);
}
```

- Add `reveal_started: Option<Instant>` (set in `try_reveal` when it sets `Reveal`) and `orbit_timer: Option<Timer>` to `IntroGate`. Drop the timer when the stage goes to `Done`.
- **Preview** (Settings › Display) starts the same timer with `data_ready = true`.
- **Intro off**: no timer, straight to `Done`, no badge animation.

### 4.3 Slint

```slint
// ui/orbit/intro.slint (replaces the iris)
import { OrbitTheme } from "theme.slint";
import { Intro, IntroStage } from "../shared/intro.slint";
import { TvUi } from "../state.slint";
import { ClockBadgeContent } from "shell.slint";   // time + weather lines, as used by the badge

export struct OrbitRingState { d: float, opacity: float }

export global OrbitIntro {
    in property <[OrbitRingState]> rings;
    in property <float> word-opacity;
    in property <float> word-scale: 1;
    in property <float> bg-opacity: 1;
    in property <float> trav-x;
    in property <float> trav-y;
    in property <float> trav-d;
    in property <float> trav-opacity;
    in property <float> land;
    in property <float> echo-d;
    in property <float> echo-opacity;
}

// Design units → screen. If TvUi.u is already a length, drop the "* 1px".
component Circle inherits Rectangle {
    in property <float> cx;
    in property <float> cy;
    in property <float> d;
    width: d * TvUi.u * 1px;
    height: self.width;
    x: cx * TvUi.u * 1px - self.width / 2;
    y: cy * TvUi.u * 1px - self.height / 2;
    border-radius: self.width / 2;
}

export component OrbitIntroLayer inherits Rectangle {
    // Placed above OrbitShell: `if Intro.stage != IntroStage.done : OrbitIntroLayer {}`
    Rectangle { background: OrbitTheme.bg; opacity: OrbitIntro.bg-opacity; }

    for r in OrbitIntro.rings : Circle {
        cx: 960; cy: 540; d: r.d;
        border-width: 3px * TvUi.u; border-color: OrbitTheme.accent;
        opacity: r.opacity;
    }

    VerticalLayout {
        alignment: center;
        opacity: OrbitIntro.word-opacity;
        // scale: wrap in a Rectangle with transform-scale if your Slint has it; otherwise
        // scale the two font sizes by OrbitIntro.word-scale.
        Text { text: "tvdash"; horizontal-alignment: center; color: OrbitTheme.text;
               font-family: "Outfit"; font-weight: 700; letter-spacing: -2px * TvUi.u;
               font-size: 104px * TvUi.u * OrbitIntro.word-scale; }
        Text { text: "ORBIT"; horizontal-alignment: center; color: OrbitTheme.accent;
               font-family: "Outfit"; font-weight: 600; letter-spacing: 6px * TvUi.u;
               font-size: 18px * TvUi.u * OrbitIntro.word-scale; }
    }

    // Traveller → clock badge. Two stacked borders instead of a colour mix.
    Circle {
        cx: OrbitIntro.trav-x; cy: OrbitIntro.trav-y; d: OrbitIntro.trav-d;
        opacity: OrbitIntro.trav-opacity;
        Rectangle { background: OrbitTheme.glass; border-radius: parent.width / 2; opacity: OrbitIntro.land; }
        Rectangle { border-radius: parent.width / 2; border-width: 4px * TvUi.u;
                    border-color: OrbitTheme.accent; opacity: 1 - OrbitIntro.land; }
        Rectangle { border-radius: parent.width / 2; border-width: 1px * TvUi.u;
                    border-color: #FFFFFF1A; opacity: OrbitIntro.land; }
        ClockBadgeContent { opacity: OrbitIntro.land; }
    }

    Circle {
        cx: 1745; cy: 125; d: OrbitIntro.echo-d;
        border-width: 3px * TvUi.u; border-color: OrbitTheme.accent;
        opacity: OrbitIntro.echo-opacity;
    }
}
```

**Home wiring (`ui/orbit/home.slint`, `ui/orbit/shell.slint`):**

| Element | Wrapper |
|---|---|
| Pill nav | `IntroRise { delay: 500ms; rise: -30px; }` |
| Hero (kicker → pills, incl. `OrbitPlay`) | `IntroRise { delay: 600ms; }` |
| Up next label | `IntroRise { delay: 700ms; }` |
| Up next poster *i* | `IntroRise { delay: 700ms + i * 70ms; }` |
| Next Game / Just Added capsule | `IntroRise { delay: 980ms; }` |
| Apps dial (guide arc, apps, label) | `IntroRise { delay: 850ms; rise: 40px; }` |
| Quick circles | `IntroRise { delay: 850ms; }` |
| **Clock badge (real)** | `opacity: Intro.stage == IntroStage.done ? 1 : 0;` with **no animate**. The traveller sits exactly on it, so the swap at `done` doesn't show |

- **Shared change:** give `IntroRise` an `in property <length> rise: 24px;` (used for `dy`). The default keeps Classic and Blades exactly as they are.
- Like the Blades intro, the Orbit shell is created behind the intro layer from the first frame, with every `IntroRise` at its start value, so nothing pops at reveal.
- The Home backdrop (What's On art) can load and crossfade behind the intro background. It shows through as `bg-opacity` falls.

---

## 5. Remove (iris leftovers)

- The play-ring logo (150 px `OrbitRing` with animated `progress`), the iris clip `Rectangle`, and the clock-blink ring from `ui/orbit/intro.slint`.
- The old Orbit timings `(2250 ms, 2700 ms)` in `intro.rs`.
- Any Orbit-only "ring stops at 90 % and breathes" hold code.

---

## 6. Files

```text
crates/app/
├─ src/intro.rs                 # Skin::Orbit timings (1800 / 1900), reveal_started, orbit_timer, driver
├─ src/orbit/intro_frame.rs     # NEW: frame() + tests
├─ src/orbit/mod.rs             # pub mod intro_frame;
└─ ui/
   ├─ shared/intro.slint        # IntroRise gets `rise` (default 24px)
   └─ orbit/
      ├─ intro.slint            # rewritten: OrbitIntro global + OrbitIntroLayer
      ├─ home.slint             # IntroRise wrappers (table above)
      └─ shell.slint            # nav IntroRise; real clock badge hidden until done; export ClockBadgeContent
```

---

## 7. Done when

- [ ] Cold start in Orbit plays the ripple. `--home`, Ambient exit and tab switches never do.
- [ ] Rings pulse every 0.6 s, never more than 4 at once, and keep pulsing through a long hold (test with the server unplugged, capped at `max_hold_secs`).
- [ ] At reveal, no new rings spawn; the traveller lands exactly on the clock badge, with no jump at `done`, at 1080p, 1440p and 4K.
- [ ] Home rises in in the §3.3 order; the backdrop shows through as the intro background fades.
- [ ] A key press during the logo skips to reveal when data is ready.
- [ ] Startup animation **Off**: Home appears with no motion and the clock badge is visible at once.
- [ ] Preview in Settings › Display replays it and returns focus to Preview.
- [ ] Smooth at 4K on the laptop; the frame timer stops at `done` (check CPU drops back after the intro).
- [ ] `intro_frame` tests pass; other skins' intros are unchanged.

---

## Change log

| Date | Change |
|---|---|
| 2026-11-24 | Created. Ring ripple chosen over Dial rise, Planet and moons and Bubble bloom (all prototyped or compared in chat), because it needs no Home data before reveal, hands off once to a fixed-position badge, and its pulse doubles as the hold state. Replaces `tvdash-orbit-build.md` §11 |
