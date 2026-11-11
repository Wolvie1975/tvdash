//! Moves `AppEvent`s from background tasks onto the UI thread.

use slint::Weak;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::debug;
use tvdash_core::AppEvent;

use crate::{LaptopWindow, Theme, TvWindow};
use slint::ComponentHandle;

pub async fn forward(mut rx: UnboundedReceiver<AppEvent>, laptop: Weak<LaptopWindow>, tv: Weak<TvWindow>) {
    while let Some(event) = rx.recv().await {
        let (laptop, tv) = (laptop.clone(), tv.clone());
        let delivered = slint::invoke_from_event_loop(move || {
            if let (Some(laptop), Some(tv)) = (laptop.upgrade(), tv.upgrade()) {
                apply(&laptop, &tv, event);
            }
        });
        if delivered.is_err() {
            break; // event loop is gone; app is shutting down
        }
    }
}

/// Runs on the UI thread.
fn apply(laptop: &LaptopWindow, _tv: &TvWindow, event: AppEvent) {
    let theme = laptop.global::<Theme>();
    match event {
        AppEvent::KeyringStatus(Ok(())) => {
            laptop.set_keyring_status("GNOME Keyring connected".into());
            laptop.set_keyring_dot(theme.get_ok());
        }
        AppEvent::KeyringStatus(Err(e)) => {
            laptop.set_keyring_status(format!("Keyring unavailable: {e}").into());
            laptop.set_keyring_dot(theme.get_alert());
        }
        other => debug!(event = ?other, "event not handled yet"),
    }
}
