//! Plex client — milestone M3.
//!
//! Plan: PIN sign-in (`POST https://plex.tv/api/v2/pins`, show the 4-char code for
//! plex.tv/link, poll `/pins/{id}` every 2 s), server discovery via
//! `/api/v2/resources?includeHttps=1` preferring a local connection, then
//! `/status/sessions`, `/library/onDeck`, `/library/recentlyAdded`, `/hubs/search`,
//! posters, `Part.key` streams and `/:/timeline` progress.
//!
//! Every server call sends `Accept: application/json`, `X-Plex-Token` and
//! `X-Plex-Client-Identifier`.

/// Product name sent as `X-Plex-Product`.
pub const PRODUCT: &str = tvdash_core::APP_NAME;
