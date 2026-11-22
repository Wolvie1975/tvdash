//! Startup intro stage machine (`idle -> logo -> (hold) -> reveal -> done`).

use std::{cell::RefCell, rc::Rc, time::Duration};

use slint::{ComponentHandle, Timer};
use tvdash_core::config::{DashboardUi, StartupConfig};

use crate::{Intro, IntroStage, LaptopWindow, TvUi};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skin {
    Tiles,
    Blades,
}

impl From<DashboardUi> for Skin {
    fn from(ui: DashboardUi) -> Self {
        match ui {
            DashboardUi::Classic => Self::Tiles,
            DashboardUi::Blades => Self::Blades,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntroGate {
    pub logo_done: bool,
    pub data_ready: bool,
    pub revealed: bool,
    pub skin: Skin,
    pub generation: u64,
    pub preview_return_focus: Option<i32>,
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
        }
    }

    pub fn logo_len(&self) -> Duration {
        match self.skin {
            Skin::Tiles => Duration::from_millis(2200),
            Skin::Blades => Duration::from_millis(1100),
        }
    }

    pub fn reveal_len(&self) -> Duration {
        match self.skin {
            Skin::Tiles => Duration::from_millis(1300),
            Skin::Blades => Duration::from_millis(2000),
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

fn begin_run(laptop: &LaptopWindow, gate: &SharedGate, max_hold_secs: u64, is_preview: bool) {
    let run_id = {
        let mut g = gate.borrow_mut();
        g.generation = g.generation.wrapping_add(1);
        g.generation
    };

    let intro = laptop.global::<Intro>();
    intro.set_instant(true);
    intro.set_stage(IntroStage::Idle);
    intro.set_instant(false);

    // Next event-loop turn = first frame is up: start the logo.
    let weak = laptop.as_weak();
    let g = gate.clone();
    Timer::single_shot(Duration::from_millis(16), move || {
        let Some(app) = weak.upgrade() else { return };
        if g.borrow().generation != run_id {
            return;
        }
        app.global::<Intro>().set_stage(IntroStage::Logo);
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
        let weak = laptop.as_weak();
        let g = gate.clone();
        Timer::single_shot(reveal_len, move || {
            if g.borrow().generation != run_id {
                return;
            }
            if let Some(app) = weak.upgrade() {
                app.global::<Intro>().set_stage(IntroStage::Done);
                if let Some(foc) = g.borrow_mut().preview_return_focus.take() {
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
        assert_eq!(tiles.logo_len(), Duration::from_millis(2200));
        assert_eq!(tiles.reveal_len(), Duration::from_millis(1300));

        let blades = IntroGate::new(Skin::Blades);
        assert_eq!(blades.logo_len(), Duration::from_millis(1100));
        assert_eq!(blades.reveal_len(), Duration::from_millis(2000));
    }
}
