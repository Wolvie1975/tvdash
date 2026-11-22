//! UI-thread Sports Alerts queue, 12 s countdown, 3 s gap, priority & hold rules,
//! and `/alerts` HTTP state sync.

use std::{collections::VecDeque, time::Duration};

use chrono::{DateTime, Timelike, Utc};
use chrono_tz::America::Chicago;
use slint::{Color, ComponentHandle, ModelRc, Timer, TimerMode, VecModel, Weak};
use tokio::sync::{mpsc::UnboundedSender, watch};
use tvdash_core::{
    Command,
    config::DuringVideo,
    events::{SportsAlertDto, SportsAlertsStatusSnapshot},
};
use tvdash_events::format::team_initials;

use super::{STATE, UiState};
use crate::{
    Artwork, Dash, FollowedTeamRow, LaptopWindow, LibraryLanding, SiteEditor, SleepState,
    SportsAlertSide, SportsAlertState, SportsAlertsSettings, StarredGameRow, TvWindow,
    home_cmd::{AlertHttpSide, AlertHttpState},
};

const AUTO_CLOSE_SECS: i64 = 12;
const QUEUE_GAP_SECS: i64 = 3;
const FINAL_GRACE_SECS: i64 = 30 * 60;

#[derive(Default)]
pub struct SportsAlertsUi {
    pub queue: VecDeque<SportsAlertDto>,
    pub showing: Option<SportsAlertDto>,
    pub closes_in: i64,
    pub last_closed_at: Option<i64>,
    pub status: SportsAlertsStatusSnapshot,
    pub alert_http_tx: Option<watch::Sender<Option<AlertHttpState>>>,
    pub alert_status_tx: Option<watch::Sender<String>>,
    pub cmd_tx: Option<UnboundedSender<Command>>,
    pub timer: Option<Timer>,
    pub tv_weak: Weak<TvWindow>,
}

pub fn set_cmd_tx(cmd_tx: UnboundedSender<Command>) {
    STATE.with_borrow_mut(|s| s.sports_alerts.cmd_tx = Some(cmd_tx));
}

pub fn set_channels(
    alert_http_tx: watch::Sender<Option<AlertHttpState>>,
    alert_status_tx: watch::Sender<String>,
) {
    STATE.with_borrow_mut(|s| {
        s.sports_alerts.alert_http_tx = Some(alert_http_tx);
        s.sports_alerts.alert_status_tx = Some(alert_status_tx);
    });
}

pub fn start_timer(laptop: Weak<LaptopWindow>, tv: Weak<TvWindow>) {
    let tv_for_state = tv.clone();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let (Some(l), Some(t)) = (laptop.upgrade(), tv.upgrade()) else {
            return;
        };
        STATE.with_borrow_mut(|s| on_tick(s, &l, &t));
    });
    STATE.with_borrow_mut(|s| {
        s.sports_alerts.timer = Some(timer);
        s.sports_alerts.tv_weak = tv_for_state;
    });
}

pub fn on_alerts(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, alerts: Vec<SportsAlertDto>) {
    for a in alerts {
        let dup_showing = s.sports_alerts.showing.as_ref().is_some_and(|cur| cur.key == a.key);
        let dup_queued = s.sports_alerts.queue.iter().any(|cur| cur.key == a.key);
        if !dup_showing && !dup_queued {
            s.sports_alerts.queue.push_back(a);
        }
    }
    promote_next(s, laptop, tv);
}

pub fn on_status(
    s: &mut UiState,
    laptop: &LaptopWindow,
    status: SportsAlertsStatusSnapshot,
) {
    let summary = format_cli_status(&status);
    if let Some(tx) = &s.sports_alerts.alert_status_tx {
        let _ = tx.send(summary);
    }
    s.sports_alerts.status = status;
    sync_settings_ui(s, laptop);
}

pub fn reset_countdown(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if s.sports_alerts.showing.is_some() {
            s.sports_alerts.closes_in = AUTO_CLOSE_SECS;
            laptop.global::<SportsAlertState>().set_closes_in(AUTO_CLOSE_SECS as i32);
            if let Some(t) = s.sports_alerts.tv_weak.upgrade() {
                t.global::<SportsAlertState>().set_closes_in(AUTO_CLOSE_SECS as i32);
            }
            publish_http(s);
        }
    });
}

pub fn on_primary(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if let Some(t) = s.sports_alerts.tv_weak.upgrade() {
            close_current(s, laptop, &t, true);
        }
    });
}

pub fn on_dismiss(laptop: &LaptopWindow) {
    STATE.with_borrow_mut(|s| {
        if let Some(t) = s.sports_alerts.tv_weak.upgrade() {
            close_current(s, laptop, &t, false);
        }
    });
}

pub fn on_http_action(laptop: &LaptopWindow, key: &str, action: &str) {
    STATE.with_borrow_mut(|s| {
        if s.sports_alerts.showing.as_ref().is_some_and(|a| a.key == key)
            && let Some(t) = s.sports_alerts.tv_weak.upgrade()
        {
            close_current(s, laptop, &t, action == "primary");
        }
    });
}

fn on_tick(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow) {
    if s.sports_alerts.showing.is_some() {
        // Keep HTTP vs Slint synced if browser opens/closes mid-card.
        if s.sports_alerts.closes_in <= 1 {
            close_current(s, laptop, tv, false);
            return;
        }
        s.sports_alerts.closes_in -= 1;
        let rem = s.sports_alerts.closes_in;
        if s.browser.is_none() {
            for st in [laptop.global::<SportsAlertState>(), tv.global::<SportsAlertState>()] {
                st.set_closes_in(rem as i32);
            }
        }
        publish_http(s);
    } else {
        promote_next(s, laptop, tv);
    }
}

fn is_held(s: &UiState) -> bool {
    if s.sports_alerts.status.held {
        return true;
    }
    s.sports_alerts.status.config.during_video == DuringVideo::Hold
        && (s.player.is_some() || s.browser.is_some())
}

fn blocked_by_priority(laptop: &LaptopWindow) -> bool {
    let sl = laptop.global::<SleepState>();
    if sl.get_picker_open() || (sl.get_phase() == "warn" && !sl.get_held()) {
        return true;
    }
    let dash = laptop.global::<Dash>();
    if dash.get_next_ep_open() || dash.get_search_open() {
        return true;
    }
    if laptop.global::<LibraryLanding>().get_watching_now_open() {
        return true;
    }
    laptop.global::<SiteEditor>().get_open() || laptop.global::<Artwork>().get_open()
}

fn promote_next(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow) {
    if s.sports_alerts.showing.is_some() || is_held(s) || blocked_by_priority(laptop) {
        return;
    }
    let now = Utc::now().timestamp();
    if let Some(closed_at) = s.sports_alerts.last_closed_at {
        if now - closed_at < QUEUE_GAP_SECS {
            return;
        }
    }
    while let Some(a) = s.sports_alerts.queue.pop_front() {
        let stale = if a.kind == "soon" {
            a.start.is_some_and(|st| st <= now)
        } else {
            now - a.queued_at > FINAL_GRACE_SECS
        };
        if stale {
            continue;
        }
        if !a.key.starts_with("test:")
            && let Some(tx) = &s.sports_alerts.cmd_tx
        {
            let _ = tx.send(Command::SportsAlertShown(a.key.clone()));
        }
        s.sports_alerts.closes_in = AUTO_CLOSE_SECS;
        s.sports_alerts.showing = Some(a.clone());
        if s.browser.is_some() {
            hide_slint_cards(laptop, tv);
            publish_http(s);
        } else {
            show_slint_card(laptop, tv, &a, AUTO_CLOSE_SECS);
            if let Some(tx) = &s.sports_alerts.alert_http_tx {
                let _ = tx.send(None);
            }
        }
        break;
    }
}

fn close_current(s: &mut UiState, laptop: &LaptopWindow, tv: &TvWindow, run_primary: bool) {
    let Some(cur) = s.sports_alerts.showing.take() else {
        return;
    };
    s.sports_alerts.last_closed_at = Some(Utc::now().timestamp());
    hide_slint_cards(laptop, tv);
    if let Some(tx) = &s.sports_alerts.alert_http_tx {
        let _ = tx.send(None);
    }
    if run_primary && let Some(tx) = &s.sports_alerts.cmd_tx {
        let _ = tx.send(Command::SportsAlertPrimary {
            kind: cur.kind,
            source_url: cur.source_url,
        });
    }
}

fn hide_slint_cards(laptop: &LaptopWindow, tv: &TvWindow) {
    for st in [laptop.global::<SportsAlertState>(), tv.global::<SportsAlertState>()] {
        st.set_visible(false);
        st.set_alert_key("".into());
    }
}

fn parse_hex_color(hex: &str) -> Color {
    let h = hex.trim().trim_start_matches('#');
    if h.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        )
    {
        return Color::from_rgb_u8(r, g, b);
    }
    Color::from_rgb_u8(0x25, 0x2A, 0x31)
}

fn to_slint_side(side: &tvdash_core::events::SportsAlertSideDto) -> SportsAlertSide {
    SportsAlertSide {
        name: side.name.as_str().into(),
        initials: side.initials.as_str().into(),
        color: parse_hex_color(&side.color),
        logo: Default::default(),
        has_logo: false,
        right: side.right.as_str().into(),
        winner: side.winner,
        muted: side.muted,
    }
}

fn show_slint_card(laptop: &LaptopWindow, tv: &TvWindow, a: &SportsAlertDto, closes_in: i64) {
    let away = to_slint_side(&a.away);
    let home = to_slint_side(&a.home);
    for st in [laptop.global::<SportsAlertState>(), tv.global::<SportsAlertState>()] {
        st.set_alert_key(a.key.as_str().into());
        st.set_kicker(a.kicker.as_str().into());
        st.set_kicker_final(a.kicker_final);
        st.set_why(a.why.as_str().into());
        st.set_away(away.clone());
        st.set_home(home.clone());
        st.set_meta(a.meta.as_str().into());
        st.set_primary(a.primary.as_deref().unwrap_or("").into());
        st.set_closes_in(closes_in as i32);
        st.set_focus(0);
        st.set_visible(true);
    }
}

fn publish_http(s: &UiState) {
    let Some(tx) = &s.sports_alerts.alert_http_tx else {
        return;
    };
    if s.browser.is_none() {
        let _ = tx.send(None);
        return;
    }
    let payload = s.sports_alerts.showing.as_ref().map(|a| AlertHttpState {
        key: a.key.clone(),
        kind: a.kind.clone(),
        kicker: a.kicker.clone(),
        why: a.why.clone(),
        away: AlertHttpSide {
            name: a.away.name.clone(),
            init: a.away.initials.clone(),
            color: a.away.color.clone(),
            right: a.away.right.clone(),
        },
        home: AlertHttpSide {
            name: a.home.name.clone(),
            init: a.home.initials.clone(),
            color: a.home.color.clone(),
            right: a.home.right.clone(),
        },
        meta: a.meta.clone(),
        primary: a.primary.clone(),
        closes_in: s.sports_alerts.closes_in,
    });
    let _ = tx.send(payload);
}

fn format_cli_status(st: &SportsAlertsStatusSnapshot) -> String {
    let poll = match (&st.last_poll_error, st.last_poll_at) {
        (Some(err), _) => format!("error: {err}"),
        (None, Some(ts)) => format!("ok at {}", ts),
        (None, None) => "not polled yet".into(),
    };
    let next = st.next_eligible.as_deref().unwrap_or("none");
    format!(
        "Followed teams: {} · Starred games: {} · Next eligible: {} · Last poll: {}",
        st.followed.len(),
        st.starred.len(),
        next,
        poll
    )
}

fn format_updated_time(ts: i64) -> String {
    let Some(utc) = DateTime::<Utc>::from_timestamp(ts, 0) else {
        return String::new();
    };
    let local = utc.with_timezone(&Chicago);
    let (_, h12) = local.hour12();
    let ampm = if local.hour() < 12 { "AM" } else { "PM" };
    format!("{}:{:02} {}", h12, local.minute(), ampm)
}

fn team_color(name: &str) -> Color {
    const PALETTE: [&str; 8] = [
        "#0051BA", "#C8102E", "#1D428A", "#006BB6", "#154734", "#512888", "#CC0000", "#002D62",
    ];
    let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    parse_hex_color(PALETTE[hash % PALETTE.len()])
}

pub fn sync_settings_ui(s: &UiState, laptop: &LaptopWindow) {
    let st = &s.sports_alerts.status;
    let ui = laptop.global::<SportsAlertsSettings>();
    ui.set_soon_minutes(st.config.soon_minutes as i32);
    ui.set_final_on(st.config.final_on);
    ui.set_during_video(match st.config.during_video {
        DuringVideo::Show => "show".into(),
        DuringVideo::Hold => "hold".into(),
    });

    let updated = st
        .follows_updated_at
        .map(format_updated_time)
        .filter(|t| !t.is_empty());
    let (note, has_err) = if st.follows_error.is_some() {
        let msg = match updated {
            Some(t) => format!("Couldn't reach Sports SQL · showing the list from {t}"),
            None => "Couldn't reach Sports SQL".into(),
        };
        (msg, true)
    } else {
        let mut msg = match updated {
            Some(t) => format!("Managed in MyNewsFeed · updated {t}"),
            None => "Managed in MyNewsFeed".into(),
        };
        if st.unmapped_count > 0 {
            msg.push_str(&format!(" · {} unmapped", st.unmapped_count));
        }
        (msg, false)
    };
    ui.set_follows_note(note.into());
    ui.set_follows_error(has_err);

    let teams: Vec<FollowedTeamRow> = st
        .followed
        .iter()
        .map(|t| FollowedTeamRow {
            name: t.name.as_str().into(),
            initials: team_initials(&t.name).into(),
            color: team_color(&t.name),
            league_label: t.league_label.as_str().into(),
        })
        .collect();
    ui.set_followed_teams(ModelRc::new(VecModel::from(teams)));

    let starred: Vec<StarredGameRow> = st
        .starred
        .iter()
        .map(|g| StarredGameRow {
            event_id: g.event_id.as_str().into(),
            matchup: g.matchup.as_str().into(),
            meta: g.meta.as_str().into(),
        })
        .collect();
    ui.set_starred_games(ModelRc::new(VecModel::from(starred)));
}
