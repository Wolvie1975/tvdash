//! Settings › Accounts (Plex, TMDB) and Streaming rows, the site
//! editor's TMDB lookups, and first-run setup.

use chrono::{DateTime, Utc};
use slint::ComponentHandle;
use tvdash_core::models::{AccountStatus, SignInState, SourceKind};

use super::{STATE, TOAST_FOR, UiState, home, on_setup, show_toast};
use crate::{AccountRow, Accounts, LaptopWindow, Setup, SiteEditor, SiteRow};

pub(super) fn source_id(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Jellyfin => "jellyfin",
        SourceKind::Plex => "plex",
    }
}

pub(super) fn code_instructions(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Jellyfin => "On a signed-in Jellyfin app, open Settings › Quick Connect and enter this code.",
        SourceKind::Plex => "On any device, go to plex.tv/link and enter this code.",
    }
}

// ── Events ──────────────────────────────────────────────────────────

pub(super) fn on_sign_in(s: &mut UiState, laptop: &LaptopWindow, kind: SourceKind, state: SignInState) {
    match &state {
        SignInState::Done | SignInState::Cancelled => {
            s.sign_in.remove(&kind);
        }
        _ => {
            s.sign_in.insert(kind, state.clone());
        }
    }
    if state == SignInState::Done {
        show_toast(s, laptop, format!("Signed in to {}", kind.label()), TOAST_FOR);
        // Setup moves on by itself once a step's sign-in succeeds.
        let setup = laptop.global::<Setup>();
        let step = setup.get_step();
        if on_setup(laptop) && ((kind == SourceKind::Plex && step == 0) || (kind == SourceKind::Jellyfin && step == 1))
        {
            setup.set_step(step + 1);
        }
    }
    home::update_cw_state(s, laptop);
}

pub(super) fn on_tmdb(s: &mut UiState, laptop: &LaptopWindow, status: AccountStatus) {
    let acc = laptop.global::<Accounts>();
    if acc.get_editing() == "tmdb" {
        match &status {
            AccountStatus::Unreachable(e) => acc.set_edit_error(e.as_str().into()),
            _ => {
                acc.set_editing("".into());
                acc.set_edit_error("".into());
            }
        }
    }
    s.tmdb = Some(status);
    refresh(s, laptop);
}

/// TMDB providers found for the site open in the editor.
pub(super) fn on_provider_matches(laptop: &LaptopWindow, name: &str, ids: &[u32], names: &[String]) {
    let ed = laptop.global::<SiteEditor>();
    if ed.get_open() && ed.get_name().trim().eq_ignore_ascii_case(name.trim()) {
        if ids.is_empty() {
            ed.set_note(format!("TMDB has no provider called “{name}”. Enter its IDs by hand.").into());
        } else {
            let list: Vec<String> = ids.iter().map(u32::to_string).collect();
            ed.set_providers(list.join(", ").into());
            ed.set_note(format!("Found: {}", names.join(", ")).into());
        }
    }
}

// ── Rows ────────────────────────────────────────────────────────────

/// A button: (label, action id). `NONE` = no button.
type Button<'a> = (&'a str, &'a str);
const NONE: Button = ("", "");

struct Row<'a> {
    id: &'a str,
    name: &'a str,
    status: String,
    detail: String,
    state: &'a str,
    primary: Button<'a>,
    secondary: Button<'a>,
}

impl From<Row<'_>> for AccountRow {
    fn from(r: Row<'_>) -> Self {
        AccountRow {
            id: r.id.into(),
            name: r.name.into(),
            status: r.status.into(),
            detail: r.detail.into(),
            state: r.state.into(),
            primary: r.primary.0.into(),
            primary_id: r.primary.1.into(),
            secondary: r.secondary.0.into(),
            secondary_id: r.secondary.1.into(),
            input_hint: "".into(),
        }
    }
}

fn tmdb_row(s: &UiState) -> AccountRow {
    let row = |status: &str, detail: String, state: &str, primary, secondary| -> AccountRow {
        AccountRow {
            input_hint: "TMDB API Read Access Token (themoviedb.org › Settings › API)".into(),
            ..Row {
                id: "tmdb",
                name: "TMDB · Where to watch",
                status: status.into(),
                detail,
                state,
                primary,
                secondary,
            }
            .into()
        }
    };
    let (replace, remove) = (("Replace token", "edit"), ("Remove", "remove"));
    match &s.tmdb {
        Some(AccountStatus::Connected) => {
            row("Connected", "Search, where things stream, and service logos".into(), "ok", NONE, replace)
        }
        Some(AccountStatus::Expired) => row("TMDB rejected the token", String::new(), "warn", replace, remove),
        Some(AccountStatus::Unreachable(e)) => {
            row("Couldn't use that token", e.clone(), "warn", ("Add token", "edit"), NONE)
        }
        _ => row(
            "Not set up",
            "Paste your TMDB API Read Access Token to search where things stream".into(),
            "off",
            ("Add token", "edit"),
            NONE,
        ),
    }
}

fn media_account_row(s: &UiState, kind: SourceKind) -> AccountRow {
    let server = s.servers.get(&kind).cloned().unwrap_or_default();
    let sign_out = ("Sign out", "sign-out");
    let edit_url = if kind == SourceKind::Jellyfin { ("Server URL", "edit") } else { NONE };
    let input_hint = if kind == SourceKind::Jellyfin { "http://host:8096".into() } else { String::new() };
    let row = |status: String, detail: String, state, primary, secondary| AccountRow {
        input_hint: input_hint.clone().into(),
        ..Row {
            id: source_id(kind),
            name: kind.label(),
            status,
            detail,
            state,
            primary,
            secondary,
        }
        .into()
    };
    match (s.sign_in.get(&kind), s.accounts.get(&kind)) {
        (Some(SignInState::Starting), _) => {
            row("Contacting the server…".into(), server, "busy", ("Cancel", "cancel"), NONE)
        }
        (Some(SignInState::Code(code)), _) => {
            row(format!("Code {code}"), code_instructions(kind).into(), "busy", ("Cancel", "cancel"), NONE)
        }
        (Some(SignInState::Failed(e)), _) => {
            row(format!("Sign-in failed: {e}"), server, "warn", ("Try again", "sign-in"), edit_url)
        }
        (_, Some(AccountStatus::Connected)) => row("Connected".into(), server, "ok", edit_url, sign_out),
        (_, Some(AccountStatus::Expired)) => {
            row("Sign-in expired".into(), server, "warn", ("Reconnect", "sign-in"), sign_out)
        }
        (_, Some(AccountStatus::Unreachable(e))) => {
            row("Can't reach the server".into(), e.clone(), "warn", ("Retry", "retry"), sign_out)
        }
        _ => row("Not connected".into(), server, "off", ("Sign in", "sign-in"), edit_url),
    }
}

fn site_status(opened: Option<&DateTime<Utc>>) -> String {
    match opened {
        None => "Not signed in yet".into(),
        Some(t) => match (Utc::now() - *t).num_days() {
            0 => "Set up · opened today".into(),
            1 => "Set up · opened yesterday".into(),
            n => format!("Set up · opened {n} days ago"),
        },
    }
}

fn tautulli_row(s: &UiState) -> AccountRow {
    let input_hint = "http://host:8181 | API_KEY".into();
    if s.tautulli_configured {
        AccountRow {
            input_hint,
            ..Row {
                id: "tautulli",
                name: "Tautulli · Popular on server",
                status: "Connected".into(),
                detail: s.tautulli_url.clone(),
                state: "ok",
                primary: ("Change", "edit"),
                secondary: ("Remove", "remove"),
            }
            .into()
        }
    } else {
        AccountRow {
            input_hint,
            ..Row {
                id: "tautulli",
                name: "Tautulli · Popular on server",
                status: "Not set up".into(),
                detail: "Add your Tautulli URL and API key for Popular on Republic City".into(),
                state: "off",
                primary: ("Set up", "edit"),
                secondary: NONE,
            }
            .into()
        }
    }
}

/// Rebuild Settings › Accounts and Settings › Streaming (and setup, if it's showing).
pub(super) fn refresh(s: &UiState, laptop: &LaptopWindow) {
    s.account_rows.set_vec(vec![
        media_account_row(s, SourceKind::Plex),
        media_account_row(s, SourceKind::Jellyfin),
        tmdb_row(s),
        tautulli_row(s),
    ]);
    let sites: Vec<SiteRow> = s
        .sites
        .iter()
        .enumerate()
        .map(|(i, site)| {
            let opened = s.streaming.get(&site.name);
            let (bg, fg) = home::site_colors(s, i);
            SiteRow {
                index: i as i32,
                name: site.name.as_str().into(),
                url: site.url.as_str().into(),
                status: site_status(opened).into(),
                opened: opened.is_some(),
                hidden: site.hidden,
                monogram: home::monogram(&site.name).into(),
                bg,
                fg,
            }
        })
        .collect();
    s.site_rows.set_vec(sites);
    if on_setup(laptop) {
        refresh_setup(s, laptop);
    }
}

// ── First-run setup ─────────────────────────────────────────────────

/// (state, code, message) for a sign-in step on the setup page.
fn setup_sign_in(s: &UiState, kind: SourceKind) -> (&'static str, String, String) {
    match (s.sign_in.get(&kind), s.accounts.get(&kind)) {
        (Some(SignInState::Starting), _) => ("busy", String::new(), String::new()),
        (Some(SignInState::Code(c)), _) => ("code", c.clone(), String::new()),
        (Some(SignInState::Failed(e)), _) => ("error", String::new(), format!("Sign-in failed: {e}")),
        (_, Some(AccountStatus::Connected)) => {
            let server = s.servers.get(&kind).map(|u| format!(" ({u})")).unwrap_or_default();
            ("done", String::new(), format!("Connected to {}{server}", kind.label()))
        }
        _ => ("idle", String::new(), String::new()),
    }
}

pub(super) fn refresh_setup(s: &UiState, laptop: &LaptopWindow) {
    let setup = laptop.global::<Setup>();
    let (st, code, msg) = setup_sign_in(s, SourceKind::Plex);
    setup.set_plex_state(st.into());
    setup.set_plex_code(code.into());
    setup.set_plex_message(msg.into());
    let (st, code, msg) = setup_sign_in(s, SourceKind::Jellyfin);
    setup.set_jellyfin_state(st.into());
    setup.set_jellyfin_code(code.into());
    setup.set_jellyfin_message(msg.into());
}

/// Run the setup page's derived state now (e.g. right after navigating to it).
pub fn sync_setup(laptop: &LaptopWindow) {
    STATE.with_borrow(|s| refresh_setup(s, laptop));
}
