//! Live clock for both windows. Runs on the UI thread via a Slint timer.

use std::{cell::Cell, time::Duration};

use chrono::{Local, NaiveDateTime, Utc};
use slint::{ComponentHandle, Timer, TimerMode};
use tvdash_core::config::{ClockFormat, GeneralConfig};

use crate::{LaptopWindow, TvWindow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockText {
    pub hh: String,
    pub mm: String,
    pub ss: String,
    /// "AM" / "PM" in 12-hour mode, else "".
    pub suffix: String,
    /// "Wednesday, November 11" (TV).
    pub date: String,
    /// "Wednesday" and "November 11" (laptop header, two lines).
    pub weekday: String,
    pub month_day: String,
}

pub fn clock_text(now: NaiveDateTime, format: ClockFormat) -> ClockText {
    let hh = match format {
        ClockFormat::H24 => now.format("%H"),
        ClockFormat::H12 => now.format("%-I"),
    };
    ClockText {
        hh: hh.to_string(),
        mm: now.format("%M").to_string(),
        ss: now.format("%S").to_string(),
        suffix: match format {
            ClockFormat::H24 => String::new(),
            ClockFormat::H12 => now.format("%p").to_string(),
        },
        date: now.format("%A, %B %-d").to_string(),
        weekday: now.format("%A").to_string(),
        month_day: now.format("%B %-d").to_string(),
    }
}

thread_local! {
    /// (12/24 h, seconds), changeable from Settings › Display.
    static FORMAT: Cell<(ClockFormat, bool)> = const { Cell::new((ClockFormat::H12, false)) };
    /// Time zone (None = system local), changes with the weather location.
    static TZ: Cell<Option<chrono_tz::Tz>> = const { Cell::new(None) };
}

/// Change the time zone the clocks show.
pub fn set_tz(tz: Option<chrono_tz::Tz>) {
    TZ.set(tz);
}

/// Change the clock format; the next tick (≤ 0.5 s) shows it.
pub fn set_format(format: ClockFormat, seconds: bool) {
    FORMAT.set((format, seconds));
}

/// Start the clock. Keep the returned timer alive for as long as it should tick.
pub fn start(laptop: &LaptopWindow, tv: &TvWindow, general: &GeneralConfig) -> Timer {
    set_tz(general.tz());
    set_format(general.clock, general.seconds);
    let laptop = laptop.as_weak();
    let tv = tv.as_weak();
    let mut last: Option<ClockText> = None;

    let mut tick = move || {
        let now = match TZ.get() {
            Some(tz) => Utc::now().with_timezone(&tz).naive_local(),
            None => Local::now().naive_local(),
        };
        let (format, seconds) = FORMAT.get();
        let mut text = clock_text(now, format);
        if !seconds {
            text.ss.clear(); // only redraw once a minute
        }
        if last.as_ref() == Some(&text) {
            return;
        }
        let minute_changed = last.as_ref().is_none_or(|l| l.mm != text.mm);
        if let Some(l) = laptop.upgrade() {
            let dash = l.global::<crate::Dash>();
            dash.set_clock_ss(text.ss.as_str().into());
            dash.set_show_seconds(seconds);
            dash.set_clock_hh(text.hh.as_str().into());
            dash.set_clock_mm(text.mm.as_str().into());
            dash.set_clock_suffix(text.suffix.as_str().into());
            dash.set_weekday(text.weekday.as_str().into());
            dash.set_date_text(text.month_day.as_str().into());
        }
        if let Some(t) = tv.upgrade() {
            t.set_clock_hh(text.hh.as_str().into());
            t.set_clock_mm(text.mm.as_str().into());
            t.set_date_text(text.date.as_str().into());
            t.set_clock_ss(text.ss.as_str().into());
            t.set_show_seconds(seconds);
        }
        if minute_changed && let (Some(l), Some(t)) = (laptop.upgrade(), tv.upgrade()) {
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
        assert_eq!(t.date, "Wednesday, November 11");
        assert_eq!((t.weekday.as_str(), t.month_day.as_str()), ("Wednesday", "November 11"));
    }

    #[test]
    fn formats_12h() {
        let t = clock_text(at(19, 30), ClockFormat::H12);
        assert_eq!((t.hh.as_str(), t.mm.as_str(), t.suffix.as_str()), ("7", "30", "PM"));
        assert_eq!(clock_text(at(0, 5), ClockFormat::H12).hh, "12");
        assert_eq!(clock_text(at(7, 4), ClockFormat::H24).suffix, "");
    }
}
