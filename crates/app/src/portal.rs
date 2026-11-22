//! Desktop portal integration (`org.freedesktop.portal.Settings` color-scheme).

use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, warn};
use tvdash_core::AppEvent;
use zbus::zvariant::OwnedValue;

#[zbus::proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait PortalSettings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;

    #[zbus(signal)]
    fn setting_changed(&self, namespace: String, key: String, value: OwnedValue) -> zbus::Result<()>;
}

/// XDG color-scheme: 1 = PreferDark, 2 = PreferLight, 0 = NoPreference.
fn is_dark(val: &OwnedValue) -> Option<bool> {
    let code: u32 = val.downcast_ref().ok()?;
    Some(code != 2)
}

/// Read the initial system color scheme and watch for changes.
pub async fn watch_color_scheme(events: UnboundedSender<AppEvent>) {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "no session D-Bus for portal color-scheme");
            return;
        }
    };
    let proxy = match PortalSettingsProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            debug!(error = %e, "no portal Settings proxy");
            return;
        }
    };

    if let Ok(val) = proxy.read_one("org.freedesktop.appearance", "color-scheme").await
        && let Some(dark) = is_dark(&val)
    {
        let _ = events.send(AppEvent::SystemColorScheme(dark));
    }

    let mut changes = match proxy.receive_setting_changed().await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "couldn't subscribe to portal SettingChanged");
            return;
        }
    };

    while let Some(sig) = changes.next().await {
        let Ok(args) = sig.args() else { continue };
        if args.namespace == "org.freedesktop.appearance"
            && args.key == "color-scheme"
            && let Some(dark) = is_dark(&args.value)
        {
            let _ = events.send(AppEvent::SystemColorScheme(dark));
        }
    }
}

/// Inhibits GNOME's screen blanking/idle via `org.freedesktop.portal.Inhibit`
/// while video is playing on the TV. Dropping it (or calling `release`) closes
/// the portal request handle.
pub struct IdleInhibit {
    conn: zbus::Connection,
    request_path: zbus::zvariant::OwnedObjectPath,
}

impl IdleInhibit {
    /// Request an idle inhibit (`flags = 8`). Returns `None` if the portal isn't available.
    pub async fn acquire(reason: &str) -> Option<Self> {
        let conn = zbus::Connection::session().await.ok()?;
        let mut options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
            std::collections::HashMap::new();
        options.insert("reason", zbus::zvariant::Value::from(reason));
        let reply = conn
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Inhibit"),
                "Inhibit",
                &("", 8u32, options),
            )
            .await
            .ok()?;
        let request_path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize().ok()?;
        debug!(path = %request_path, "portal idle inhibit acquired");
        Some(Self { conn, request_path })
    }

    pub async fn release(self) {
        let _ = self
            .conn
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                self.request_path.as_str(),
                Some("org.freedesktop.portal.Request"),
                "Close",
                &(),
            )
            .await;
        debug!(path = %self.request_path, "portal idle inhibit released");
    }
}
