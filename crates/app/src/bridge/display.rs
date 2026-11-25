//! Display settings & Dashboard UI picker model builders.

use slint::{ComponentHandle, ModelRc, VecModel};
use tvdash_core::config::DashboardUi;

use crate::{DashboardChoice, Display, LaptopWindow};

/// Dashboard UIs compiled into this build (Cargo features `ui-*`), in picker order.
pub fn enabled_uis() -> Vec<DashboardUi> {
    DashboardUi::ALL
        .iter()
        .copied()
        .filter(|ui| match ui {
            DashboardUi::Classic => cfg!(feature = "ui-classic"),
            DashboardUi::Blades => cfg!(feature = "ui-blades"),
            DashboardUi::Ps5 => cfg!(feature = "ui-ps5"),
            DashboardUi::Orbit => cfg!(feature = "ui-orbit"),
        })
        .collect()
}

/// `ui` if it's compiled in, otherwise the first UI that is.
pub fn usable_ui(ui: DashboardUi) -> DashboardUi {
    let enabled = enabled_uis();
    if enabled.contains(&ui) { ui } else { enabled[0] }
}

pub fn dashboard_ui_label(ui: DashboardUi) -> String {
    format!("{} · {}", ui.name(), ui.sub())
}

pub fn dashboard_choices(current: DashboardUi) -> Vec<DashboardChoice> {
    enabled_uis()
        .into_iter()
        .map(|ui| DashboardChoice {
            id: ui.as_str().into(),
            name: ui.name().into(),
            sub: ui.sub().into(),
            description: ui.description().into(),
            current: ui == current,
        })
        .collect()
}

pub fn refresh_dashboard_choices(laptop: &LaptopWindow, current: DashboardUi) {
    let d = laptop.global::<Display>();
    d.set_dashboard_ui(current.as_str().into());
    d.set_dashboard_ui_label(dashboard_ui_label(current).into());
    d.set_dashboard_choices(ModelRc::new(VecModel::from(dashboard_choices(current))));
}

pub(super) fn refresh_quick_switch(s: &super::UiState, laptop: &LaptopWindow) {
    let all: Vec<String> = s
        .sites
        .iter()
        .map(|site| crate::brand::service_id(&site.name))
        .collect();
    let recent: Vec<String> = s.recent.iter().map(|r| r.id.clone()).collect();
    let set = crate::apps::quick_switch_set(&s.quick_switch_custom, &recent, &all);
    let automatic = s.quick_switch_custom.is_empty();

    let mut choices = Vec::new();
    for site in &s.sites {
        let id = crate::brand::service_id(&site.name);
        if crate::apps::QUICK_SWITCH_EXCLUDED.contains(&id.as_str()) {
            continue;
        }
        let slot = set
            .iter()
            .position(|x| x == &id)
            .map(|i| (i + 1) as i32)
            .unwrap_or(0);
        let b = crate::brand::brand_for_with_logo(&id, &site.name, s.logos.get(&site.name).cloned());
        choices.push(crate::QuickSwitchChoice {
            id: id.into(),
            name: site.name.as_str().into(),
            slot,
            brand: b,
        });
    }
    s.quick_switch_choices.set_vec(choices);

    let names: Vec<&str> = set
        .iter()
        .filter_map(|id| {
            s.sites
                .iter()
                .find(|site| crate::brand::service_id(&site.name) == *id)
                .map(|site| site.name.as_str())
        })
        .collect();
    let prefix = if automatic { "Automatic" } else { "Custom" };
    let summary = if names.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} · {}", names.join(", "))
    };
    let qs = laptop.global::<crate::QuickSwitch>();
    qs.set_automatic(automatic);
    qs.set_summary(summary.into());
}

pub fn quick_switch_automatic_set() -> Vec<String> {
    super::STATE.with_borrow(|s| {
        let all: Vec<String> = s
            .sites
            .iter()
            .map(|site| crate::brand::service_id(&site.name))
            .collect();
        let recent: Vec<String> = s.recent.iter().map(|r| r.id.clone()).collect();
        crate::apps::quick_switch_set(&[], &recent, &all)
    })
}

pub fn quick_switch_custom() -> Vec<String> {
    super::STATE.with_borrow(|s| s.quick_switch_custom.clone())
}
