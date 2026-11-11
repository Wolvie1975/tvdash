//! Open-Meteo forecast (no API key) — milestone M5. Refresh every 10 minutes and
//! map WMO `weather_code` to a condition label and icon.

use std::time::Duration;

pub const REFRESH: Duration = Duration::from_secs(10 * 60);
