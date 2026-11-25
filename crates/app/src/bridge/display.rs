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
