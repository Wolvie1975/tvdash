//! Google Calendar via its secret iCal address (keyring `calendar_ics_url`) —
//! milestone M5. Fetch every 5 minutes, expand recurrences for today and
//! tomorrow, and surface the next event.

use std::time::Duration;

pub const REFRESH: Duration = Duration::from_secs(5 * 60);
