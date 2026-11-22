//! Sleep timer state machine, formatting, and unit tests.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SleepTimer {
    #[default]
    Off,
    /// Unix seconds. Wall clock, so time spent suspended and layout re-execs both count.
    At { deadline: i64 },
    /// Plex in mpv only: sleep when this item plays to the end.
    EndOfItem { rating_key: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Idle,
    Counting(i64),
    Warn(i64),
    Fire,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sleep {
    pub timer: SleepTimer,
    pub warned: bool,
    /// Last-used preset in minutes (when `remember_last` is enabled).
    pub last_minutes: Option<i64>,
}

impl Sleep {
    pub fn set_minutes(&mut self, minutes: i64, now: i64) {
        self.timer = SleepTimer::At { deadline: now + minutes * 60 };
        self.warned = false;
        self.last_minutes = Some(minutes);
    }

    pub fn set_end_of_item(&mut self, rating_key: &str) {
        self.timer = SleepTimer::EndOfItem { rating_key: rating_key.to_owned() };
        self.warned = false;
    }

    pub fn off(&mut self) {
        let last = self.last_minutes;
        *self = Sleep { last_minutes: last, ..Sleep::default() };
    }

    /// Call before `tick` while an app with `hold_sleep_timer` is open.
    /// Parks the deadline just outside the warning window, so nothing shows during the call.
    pub fn hold(&mut self, now: i64, warn: i64) -> bool {
        if let SleepTimer::At { deadline } = self.timer
            && deadline - now <= warn
        {
            self.timer = SleepTimer::At { deadline: now + warn + 1 };
            self.warned = false;
            return true;
        }
        false
    }

    /// Call once a second.
    pub fn tick(&mut self, now: i64, warn: i64) -> Tick {
        let SleepTimer::At { deadline } = self.timer else { return Tick::Idle };
        let left = deadline - now;
        if left > warn {
            Tick::Counting(left)
        } else if !self.warned {
            // Always give the full warning, even when waking from suspend past the deadline.
            self.warned = true;
            self.timer = SleepTimer::At { deadline: now + warn };
            Tick::Warn(warn)
        } else if left > 0 {
            Tick::Warn(left)
        } else {
            Tick::Fire
        }
    }

    /// mpv `end-file` with reason `eof`. Returns true if Up Next should be replaced by the warning.
    pub fn item_finished(&mut self, rating_key: &str, now: i64, warn: i64) -> bool {
        match &self.timer {
            SleepTimer::EndOfItem { rating_key: k } if k == rating_key => {
                self.timer = SleepTimer::At { deadline: now + warn };
                self.warned = true;
                true
            }
            _ => false,
        }
    }

    /// mpv `end-file` with reason `stop` / `quit` (you pressed Stop): an end-of-item timer has nothing left to wait for.
    pub fn item_stopped(&mut self) -> bool {
        let was = matches!(self.timer, SleepTimer::EndOfItem { .. });
        if was {
            self.off();
        }
        was
    }

    pub fn is_active(&self) -> bool {
        !matches!(self.timer, SleepTimer::Off)
    }

    pub fn phase(&self) -> &'static str {
        match self.timer {
            SleepTimer::Off => "off",
            SleepTimer::EndOfItem { .. } => "end_of_item",
            SleepTimer::At { .. } if self.warned => "warn",
            SleepTimer::At { .. } => "counting",
        }
    }

    pub fn seconds_left(&self, now: i64) -> i64 {
        match self.timer {
            SleepTimer::At { deadline } => (deadline - now).max(0),
            _ => 0,
        }
    }
}

pub fn label(left: i64) -> String {
    match left {
        3600.. => format!("{} h {:02} min", left / 3600, (left % 3600) / 60),
        60.. => format!("{} min", (left + 59) / 60),
        _ => format!("0:{:02}", left.max(0)),
    }
}

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn format_ends_at(deadline: i64, use_24h: bool) -> String {
    use chrono::TimeZone;
    let Some(dt) = chrono::Local.timestamp_opt(deadline, 0).single() else {
        return String::new();
    };
    if use_24h {
        dt.format("%H:%M").to_string()
    } else {
        dt.format("%-I:%M %p").to_string()
    }
}

pub fn format_ends_at_24h(deadline: i64) -> String {
    format_ends_at(deadline, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_warn_and_fire() {
        let mut s = Sleep::default();
        s.set_minutes(30, 0);
        assert_eq!(s.tick(1739, 60), Tick::Counting(61));
        assert_eq!(s.tick(1740, 60), Tick::Warn(60));
        assert_eq!(s.tick(1741, 60), Tick::Warn(59));
        assert_eq!(s.tick(1800, 60), Tick::Fire);
    }

    #[test]
    fn waking_past_deadline_gives_full_warning() {
        let mut s = Sleep::default();
        s.set_minutes(10, 0); // deadline = 600
        assert_eq!(s.tick(1200, 60), Tick::Warn(60));
        assert_eq!(s.tick(1259, 60), Tick::Warn(1));
        assert_eq!(s.tick(1260, 60), Tick::Fire);
    }

    #[test]
    fn item_finished_matches_rating_key() {
        let mut s = Sleep::default();
        s.set_end_of_item("123");
        assert!(!s.item_finished("999", 100, 60));
        assert_eq!(s.timer, SleepTimer::EndOfItem { rating_key: "123".into() });

        assert!(s.item_finished("123", 100, 60));
        assert_eq!(s.tick(101, 60), Tick::Warn(59));
        assert_eq!(s.tick(160, 60), Tick::Fire);
    }

    #[test]
    fn item_stopped_only_cancels_end_of_item() {
        let mut s = Sleep::default();
        s.set_minutes(30, 0);
        assert!(!s.item_stopped());
        assert_eq!(s.timer, SleepTimer::At { deadline: 1800 });

        s.set_end_of_item("123");
        assert!(s.item_stopped());
        assert_eq!(s.timer, SleepTimer::Off);
    }

    #[test]
    fn label_formats_correctly() {
        assert_eq!(label(3600), "1 h 00 min");
        assert_eq!(label(2521), "43 min");
        assert_eq!(label(59), "0:59");
        assert_eq!(label(0), "0:00");
    }

    #[test]
    fn serde_round_trip() {
        let mut s = Sleep::default();
        s.set_minutes(45, 1000);
        let json = serde_json::to_string(&s).unwrap();
        let back: Sleep = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);

        s.set_end_of_item("456");
        let json = serde_json::to_string(&s).unwrap();
        let back: Sleep = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn hold_parks_deadline_outside_warn_window() {
        let mut s = Sleep::default();
        s.set_minutes(10, 0); // deadline = 600
        assert!(s.hold(580, 60)); // 20s left
        assert_eq!(s.timer, SleepTimer::At { deadline: 641 });
        assert!(!s.warned);
        assert_eq!(s.tick(581, 60), Tick::Warn(60));
    }

    #[test]
    fn hold_with_plenty_of_time_does_not_change() {
        let mut s = Sleep::default();
        s.set_minutes(20, 0); // deadline = 1200
        assert!(!s.hold(600, 60)); // 10 min left
        assert_eq!(s.timer, SleepTimer::At { deadline: 1200 });
    }
}
