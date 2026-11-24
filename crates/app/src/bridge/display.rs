//! Display settings & Dashboard UI picker model builders.

use slint::{ComponentHandle, ModelRc, VecModel};
use tvdash_core::config::DashboardUi;

use crate::{DashboardChoice, Display, LaptopWindow};

pub fn dashboard_ui_label(ui: DashboardUi) -> String {
    format!("{} · {}", ui.name(), ui.sub())
}

pub fn dashboard_choices(current: DashboardUi) -> Vec<DashboardChoice> {
    DashboardUi::ALL
        .iter()
        .map(|&ui| DashboardChoice {
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
