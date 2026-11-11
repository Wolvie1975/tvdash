//! Live clock for both windows. Runs on the UI thread via a Slint timer.

use std::time::Duration;

use chrono::{Local, NaiveDateTime, Utc};
use slint::{ComponentHandle, Timer, TimerMode};
use tvdash_core::config::{ClockFormat, GeneralConfig};

use crate::{LaptopWindow, TvWindow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockText {
    pub hh: String,
    pub mm: String,
    pub date: String,
}

pub fn clock_text(now: NaiveDateTime, format: ClockFormat) -> ClockText {
    let hh = match format {
        ClockFormat::H24 => now.format("%H"),
        ClockFormat::H12 => now.format("%-I"),
    };
    ClockText { hh: hh.to_string(), mm: now.format("%M").to_string(), date: now.format("%A, %-d %B").to_string() }
}

/// Start the clock. Keep the returned timer alive for as long as it should tick.
pub fn start(laptop: &LaptopWindow, tv: &TvWindow, general: &GeneralConfig) -> Timer {
    let tz = general.tz();
    let format = general.clock;
    let laptop = laptop.as_weak();
    let tv = tv.as_weak();
    let mut last: Option<ClockText> = None;

    let mut tick = move || {
        let now = match tz {
            Some(tz) => Utc::now().with_timezone(&tz).naive_local(),
            None => Local::now().naive_local(),
        };
        let text = clock_text(now, format);
        if last.as_ref() == Some(&text) {
            return;
        }
        if let Some(l) = laptop.upgrade() {
            let dash = l.global::<crate::Dash>();
            dash.set_clock_hh(text.hh.as_str().into());
            dash.set_clock_mm(text.mm.as_str().into());
            dash.set_date_text(text.date.as_str().into());
        }
        if let Some(t) = tv.upgrade() {
            t.set_clock_hh(text.hh.as_str().into());
            t.set_clock_mm(text.mm.as_str().into());
            t.set_date_text(text.date.as_str().into());
        }
        if let (Some(l), Some(t)) = (laptop.upgrade(), tv.upgrade()) {
            crate::bridge::on_minute(&l, &t);
        }
        last = Some(text);
    };

    tick();
    let timer = Timer::default();
    // Poll twice a second so the minute flips within ~0.5 s of the real change.
    timer.start(TimerMode::Repeated, Duration::from_millis(500), tick);
    timer
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap().and_hms_opt(h, m, 5).unwrap()
    }

    #[test]
    fn formats_24h() {
        let t = clock_text(at(7, 4), ClockFormat::H24);
        assert_eq!((t.hh.as_str(), t.mm.as_str()), ("07", "04"));
        assert_eq!(t.date, "Wednesday, 11 November");
    }

    #[test]
    fn formats_12h() {
        let t = clock_text(at(19, 30), ClockFormat::H12);
        assert_eq!((t.hh.as_str(), t.mm.as_str()), ("7", "30"));
    }
}
