//! Jellyfin client — milestone M2.
//!
//! Plan: Quick Connect sign-in (`/QuickConnect/Initiate` → poll `/QuickConnect/Connect`
//! → `/Users/AuthenticateWithQuickConnect`), password fallback, resume/latest/search,
//! posters, `/Videos/{id}/stream?static=true`, and progress via `/Sessions/Playing*`
//! (positions in ticks). Every request carries:
//!
//! ```text
//! Authorization: MediaBrowser Client="tvdash", Device="tv-laptop", DeviceId="<uuid>",
//!                Version="0.1.0", Token="<token>"
//! ```
//!
//! Confirm per-user routes against the server's `/api-docs/swagger` (10.9 moved some).

/// Build the `Authorization` header value Jellyfin expects.
pub fn auth_header(device_id: &str, token: Option<&str>) -> String {
    let mut h = format!(
        r#"MediaBrowser Client="{}", Device="tv-laptop", DeviceId="{device_id}", Version="{}""#,
        tvdash_core::APP_NAME,
        tvdash_core::APP_VERSION,
    );
    if let Some(t) = token {
        h.push_str(&format!(r#", Token="{t}""#));
    }
    h
}
