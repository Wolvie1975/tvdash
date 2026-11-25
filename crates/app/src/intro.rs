//! Startup intro stage machine (`idle -> logo -> (hold) -> reveal -> done`).

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use tvdash_core::config::{DashboardUi, StartupConfig};

use crate::orbit::intro_frame::{self, Frame, SLOTS};
use crate::{Intro, IntroStage, LaptopWindow, OrbitIntro, OrbitRingState, TvUi};

const ORBIT_BADGE: (f32, f32, f32) = (1768.0, 102.0, 144.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skin {
    Tiles,
    Blades,
    Ps5,
    Orbit,
}

impl From<DashboardUi> for Skin {
    fn from(ui: DashboardUi) -> Self {
        match ui {
            DashboardUi::Classic => Self::Tiles,
            DashboardUi::Blades => Self::Blades,
            DashboardUi::Ps5 => Self::Ps5,
            DashboardUi::Orbit => Self::Orbit,
        }
    }
}

pub struct IntroGate {
    pub logo_done: bool,
    pub data_ready: bool,
    pub revealed: bool,
    pub skin: Skin,
    pub generation: u64,
    pub preview_return_focus: Option<i32>,
    pub reveal_started: Option<Instant>,
    pub orbit_timer: Option<Timer>,
}

impl IntroGate {
    pub fn new(skin: Skin) -> Self {
        Self {
            logo_done: false,
            data_ready: false,
            revealed: false,
            skin,
            generation: 0,
            preview_return_focus: None,
            reveal_started: None,
            orbit_timer: None,
        }
    }

    pub fn logo_len(&self) -> Duration {
        match self.skin {
            Skin::Tiles => Duration::from_millis(2400),
            Skin::Blades => Duration::from_millis(1600),
            Skin::Ps5 => Duration::from_millis(2000),
            Skin::Orbit => Duration::from_millis(1800),
        }
    }

    pub fn reveal_len(&self) -> Duration {
        match self.skin {
            Skin::Tiles => Duration::from_millis(1300),
            Skin::Blades => Duration::from_millis(2000),
            Skin::Ps5 => Duration::from_millis(2125),
            Skin::Orbit => Duration::from_millis(1900),
        }
    }

    /// Pure stage transition on `logo_done`, `data_ready`, or `skip`.
    /// Returns the new stage if a transition occurred.
    pub fn next_stage(&mut self) -> Option<IntroStage> {
        if self.revealed || !self.logo_done {
            return None;
        }
        if !self.data_ready {
            return Some(IntroStage::Hold);
        }
        self.revealed = true;
        Some(IntroStage::Reveal)
    }
}

pub type SharedGate = Rc<RefCell<IntroGate>>;

thread_local! {
    static GATE: RefCell<Option<SharedGate>> = const { RefCell::new(None) };
}

pub fn with_gate<R>(f: impl FnOnce(&SharedGate) -> R) -> Option<R> {
    GATE.with_borrow(|g| g.as_ref().map(f))
}

/// Call before `laptop.show()`.
pub fn start(laptop: &LaptopWindow, skin: Skin, cfg: &StartupConfig) -> SharedGate {
    let intro = laptop.global::<Intro>();
    intro.set_enabled(cfg.intro_on());
    let gate = Rc::new(RefCell::new(IntroGate::new(skin)));
    GATE.with_borrow_mut(|slot| *slot = Some(gate.clone()));

    if !cfg.intro_on() {
        intro.set_instant(true);
        intro.set_stage(IntroStage::Done);
        gate.borrow_mut().revealed = true;
        return gate;
    }

    begin_run(laptop, &gate, cfg.max_hold_secs, false);
    gate
}

/// Preview the current skin's intro from Settings › Display, then return focus.
pub fn preview(laptop: &LaptopWindow, skin: Skin) {
    let Some(gate) = GATE.with_borrow(|g| g.clone()) else {
        return;
    };
    {
        let mut g = gate.borrow_mut();
        g.skin = skin;
        g.logo_done = false;
        g.data_ready = true;
        g.revealed = false;
        g.preview_return_focus = Some(laptop.global::<TvUi>().get_focused_tile());
    }
    begin_run(laptop, &gate, 6, true);
}

fn push_orbit_frame(
    orbit: &OrbitIntro<'_>,
    rings: &VecModel<OrbitRingState>,
    f: &Frame,
) {
    for (i, r) in f.rings.iter().enumerate() {
        rings.set_row_data(
            i,
            OrbitRingState {
                d: r.d,
                opacity: r.opacity,
            },
        );
    }
    orbit.set_word_opacity(f.word_opacity);
    orbit.set_word_scale(f.word_scale);
    orbit.set_bg_opacity(f.bg_opacity);
    orbit.set_trav_x(f.trav_x);
    orbit.set_trav_y(f.trav_y);
    orbit.set_trav_d(f.trav_d);
    orbit.set_trav_opacity(f.trav_opacity);
    orbit.set_land(f.land);
    orbit.set_echo_d(f.echo_d);
    orbit.set_echo_opacity(f.echo_opacity);
}

fn begin_run(laptop: &LaptopWindow, gate: &SharedGate, max_hold_secs: u64, is_preview: bool) {
    let (run_id, skin) = {
        let mut g = gate.borrow_mut();
        g.generation = g.generation.wrapping_add(1);
        g.reveal_started = None;
        g.orbit_timer = None;
        (g.generation, g.skin)
    };

    if skin == Skin::Orbit {
        let rings = Rc::new(VecModel::from(vec![OrbitRingState::default(); SLOTS]));
        let orbit = laptop.global::<OrbitIntro>();
        orbit.set_rings(rings.clone().into());
        push_orbit_frame(&orbit, &rings, &intro_frame::frame_to(0.0, None, ORBIT_BADGE));
    }

    let intro = laptop.global::<Intro>();
    intro.set_instant(true);
    intro.set_stage(IntroStage::Idle);
    intro.set_instant(false);

    // Give the window a brief moment to map its first frame before starting the logo.
    let weak = laptop.as_weak();
    let g = gate.clone();
    Timer::single_shot(Duration::from_millis(150), move || {
        let Some(app) = weak.upgrade() else { return };
        if g.borrow().generation != run_id {
            return;
        }
        app.global::<Intro>().set_stage(IntroStage::Logo);

        if skin == Skin::Orbit {
            let started = Instant::now();
            let weak_t = app.as_weak();
            let g_t = g.clone();
            let rings = Rc::new(VecModel::from(vec![OrbitRingState::default(); SLOTS]));
            app.global::<OrbitIntro>().set_rings(rings.clone().into());
            let timer = Timer::default();
            timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
                let Some(app) = weak_t.upgrade() else { return };
                if g_t.borrow().generation != run_id {
                    return;
                }
                let t = started.elapsed().as_secs_f32();
                let reveal_at = g_t
                    .borrow()
                    .reveal_started
                    .map(|i| i.saturating_duration_since(started).as_secs_f32());
                let f = intro_frame::frame_to(t, reveal_at, ORBIT_BADGE);
                push_orbit_frame(&app.global::<OrbitIntro>(), &rings, &f);
            });
            g.borrow_mut().orbit_timer = Some(timer);
        }

        let len = g.borrow().logo_len();
        let (w2, g2) = (app.as_weak(), g.clone());
        Timer::single_shot(len, move || {
            if g2.borrow().generation != run_id {
                return;
            }
            g2.borrow_mut().logo_done = true;
            if let Some(app) = w2.upgrade() {
                try_reveal(&app, &g2);
            }
        });
    });

    if !is_preview {
        // Hold cap: reveal no matter what.
        let (weak, g) = (laptop.as_weak(), gate.clone());
        Timer::single_shot(Duration::from_secs(max_hold_secs.max(1)), move || {
            if g.borrow().generation != run_id {
                return;
            }
            g.borrow_mut().data_ready = true;
            if let Some(app) = weak.upgrade() {
                try_reveal(&app, &g);
            }
        });
    }
}

/// Call once the first Home data has arrived (or failed).
pub fn mark_data_ready(laptop: &LaptopWindow) {
    with_gate(|gate| {
        gate.borrow_mut().data_ready = true;
        try_reveal(laptop, gate);
    });
}

/// Any key or click during `logo` / `hold`.
pub fn skip(laptop: &LaptopWindow) {
    with_gate(|gate| {
        gate.borrow_mut().logo_done = true;
        try_reveal(laptop, gate);
    });
}

pub fn is_blocking_input(laptop: &LaptopWindow) -> bool {
    matches!(
        laptop.global::<Intro>().get_stage(),
        IntroStage::Idle | IntroStage::Logo | IntroStage::Hold
    )
}

fn try_reveal(laptop: &LaptopWindow, gate: &SharedGate) {
    let (next, reveal_len, run_id) = {
        let mut g = gate.borrow_mut();
        (g.next_stage(), g.reveal_len(), g.generation)
    };
    let Some(stage) = next else { return };
    laptop.global::<Intro>().set_stage(stage);
    if stage == IntroStage::Reveal {
        gate.borrow_mut().reveal_started = Some(Instant::now());
        let weak = laptop.as_weak();
        let g = gate.clone();
        Timer::single_shot(reveal_len, move || {
            if g.borrow().generation != run_id {
                return;
            }
            if let Some(app) = weak.upgrade() {
                app.global::<Intro>().set_stage(IntroStage::Done);
                let mut gb = g.borrow_mut();
                gb.orbit_timer = None;
                if let Some(foc) = gb.preview_return_focus.take() {
                    app.global::<TvUi>().set_focused_tile(foc);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_waits_for_both_logo_and_data() {
        let mut g = IntroGate::new(Skin::Tiles);
        assert_eq!(g.next_stage(), None);

        g.data_ready = true;
        assert_eq!(g.next_stage(), None, "must not reveal before logo_done");

        g.logo_done = true;
        assert_eq!(g.next_stage(), Some(IntroStage::Reveal));
        assert!(g.revealed);
        assert_eq!(g.next_stage(), None, "already revealed");
    }

    #[test]
    fn logo_done_before_data_enters_hold_then_reveals_on_data_or_cap() {
        let mut g = IntroGate::new(Skin::Blades);
        g.logo_done = true;
        assert_eq!(g.next_stage(), Some(IntroStage::Hold));
        assert!(!g.revealed);

        g.data_ready = true;
        assert_eq!(g.next_stage(), Some(IntroStage::Reveal));
        assert!(g.revealed);
    }

    #[test]
    fn skip_before_data_goes_to_hold() {
        let mut g = IntroGate::new(Skin::Tiles);
        // Simulate skip: logo_done = true while data_ready = false
        g.logo_done = true;
        assert_eq!(g.next_stage(), Some(IntroStage::Hold));
    }

    #[test]
    fn lengths_match_spec() {
        let tiles = IntroGate::new(Skin::Tiles);
        assert_eq!(tiles.logo_len(), Duration::from_millis(2400));
        assert_eq!(tiles.reveal_len(), Duration::from_millis(1300));

        let blades = IntroGate::new(Skin::Blades);
        assert_eq!(blades.logo_len(), Duration::from_millis(1600));
        assert_eq!(blades.reveal_len(), Duration::from_millis(2000));
    }
}
