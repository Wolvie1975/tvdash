//! Keep the laptop screen from blanking while something plays on the TV:
//! an idle inhibitor from the desktop portal (`org.freedesktop.portal.Inhibit`).
//! Blocking D-Bus calls: run them off the UI thread and off the async workers
//! (`spawn_blocking`).

use std::collections::HashMap;

use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{OwnedObjectPath, Value},
};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
/// Inhibit flags: 8 = idle (the screen blanking / screensaver).
const IDLE: u32 = 8;

/// Held while the screen must stay on; [`Inhibitor::release`] (or dropping the
/// connection) lets GNOME blank it again.
pub struct Inhibitor {
    conn: Connection,
    request: OwnedObjectPath,
}

impl Inhibitor {
    pub fn start(reason: &str) -> zbus::Result<Self> {
        let conn = Connection::session()?;
        let proxy = Proxy::new(&conn, PORTAL, PATH, "org.freedesktop.portal.Inhibit")?;
        let options: HashMap<&str, Value> = HashMap::from([("reason", Value::from(reason))]);
        let request: OwnedObjectPath = proxy.call("Inhibit", &("", IDLE, options))?;
        Ok(Self { conn, request })
    }

    pub fn release(self) -> zbus::Result<()> {
        let proxy = Proxy::new(&self.conn, PORTAL, self.request.as_ref(), "org.freedesktop.portal.Request")?;
        proxy.call::<_, _, ()>("Close", &())
    }
}
