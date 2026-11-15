//! Pure layout decision logic (auto / dual / tv) from active display outputs.

use serde::{Deserialize, Serialize};

/// Active runtime screen layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Laptop dashboard + TV playback window.
    #[default]
    Dual,
    /// One full-screen 10-foot window on the TV, driven by remote or keyboard.
    Tv,
}

impl Layout {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "dual" => Some(Self::Dual),
            "tv" => Some(Self::Tv),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dual => "dual",
            Self::Tv => "tv",
        }
    }
}

/// Configured `[display] layout` preference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutSetting {
    /// Pick Dual or Tv based on which screens are currently on.
    #[default]
    Auto,
    /// Always use two screens (laptop dashboard + TV).
    Dual,
    /// Always use the 10-foot TV-only shell.
    Tv,
}

impl LayoutSetting {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "dual" => Some(Self::Dual),
            "tv" => Some(Self::Tv),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dual => "dual",
            Self::Tv => "tv",
        }
    }
}

/// Result of [`decide`]: which layout to run and whether a TV output is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub layout: Layout,
    pub tv_connected: bool,
}

/// Built-in laptop panels: `eDP-1`, `LVDS-1`, `DSI-1`.
pub fn is_internal(name: &str) -> bool {
    ["eDP", "LVDS", "DSI"].iter().any(|p| name.starts_with(p))
}

/// Choose the layout from `setting` and the connector names of active screens.
///
/// If `tv_output` isn't among `active_outputs`, any active non-internal output
/// other than `laptop_output` is treated as the TV (with a warning).
pub fn decide(
    setting: LayoutSetting,
    active_outputs: &[String],
    tv_output: &str,
    laptop_output: &str,
) -> Decision {
    let tv_exact = active_outputs.iter().any(|o| o == tv_output);
    let tv_fallback = if !tv_exact {
        active_outputs
            .iter()
            .find(|o| o.as_str() != laptop_output && !is_internal(o))
    } else {
        None
    };
    if let Some(detected) = tv_fallback {
        tracing::warn!(
            configured = %tv_output,
            detected = %detected,
            "configured tv_output not found; treating external output as TV (set tv_output in config.toml)"
        );
    }
    let tv = tv_exact || tv_fallback.is_some();
    let laptop = active_outputs
        .iter()
        .any(|o| o == laptop_output || is_internal(o));

    let layout = match setting {
        LayoutSetting::Dual => Layout::Dual,
        LayoutSetting::Tv => Layout::Tv,
        LayoutSetting::Auto => match (tv, laptop) {
            (true, false) => Layout::Tv,
            _ => Layout::Dual,
        },
    };
    Decision {
        layout,
        tv_connected: tv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn auto_cases() {
        let d = |o| decide(LayoutSetting::Auto, &s(o), "HDMI-A-1", "eDP-1");
        assert_eq!(d(&["HDMI-A-1", "eDP-1"]), Decision { layout: Layout::Dual, tv_connected: true });
        assert_eq!(d(&["HDMI-A-1"]), Decision { layout: Layout::Tv, tv_connected: true });
        assert_eq!(d(&["eDP-1"]), Decision { layout: Layout::Dual, tv_connected: false });
        assert_eq!(d(&[]), Decision { layout: Layout::Dual, tv_connected: false });
    }

    #[test]
    fn fallback_treats_other_external_output_as_tv() {
        let d = |o| decide(LayoutSetting::Auto, &s(o), "HDMI-A-1", "eDP-1");
        assert_eq!(d(&["DP-2", "eDP-1"]), Decision { layout: Layout::Dual, tv_connected: true });
        assert_eq!(d(&["DP-2"]), Decision { layout: Layout::Tv, tv_connected: true });
        // Another internal panel name shouldn't be mistaken for a TV.
        assert_eq!(d(&["eDP-2"]), Decision { layout: Layout::Dual, tv_connected: false });
    }

    #[test]
    fn forced_settings_override_outputs() {
        assert_eq!(
            decide(LayoutSetting::Dual, &s(&["HDMI-A-1"]), "HDMI-A-1", "eDP-1"),
            Decision { layout: Layout::Dual, tv_connected: true }
        );
        assert_eq!(
            decide(LayoutSetting::Tv, &s(&["eDP-1"]), "HDMI-A-1", "eDP-1"),
            Decision { layout: Layout::Tv, tv_connected: false }
        );
    }
}
