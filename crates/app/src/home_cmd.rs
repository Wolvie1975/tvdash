//! Loopback HTTP endpoint (`http://127.0.0.1:47800`) for the Brave
//! "Back to tvdash" pill extension (`/home`, `/sleep`).

use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Weak};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc::UnboundedSender, watch},
};
use tracing::{info, warn};
use tvdash_core::{Command, SleepSnapshot};

use crate::{LaptopWindow, Nav, tv_nav};

pub const PILL_PORT: u16 = 47800;

/// What the Back pill extension shows in the Quick switch overlay (`GET /quick`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct QuickHttpState {
    /// `KeyboardEvent.key` that opens the overlay.
    pub key: String,
    /// Service id of the app on the TV right now, if any.
    pub current: Option<String>,
    pub tiles: Vec<QuickHttpTile>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuickHttpTile {
    pub id: String,
    pub name: String,
    /// Brand colours as `#RRGGBB`.
    pub fill: String,
    pub ink: String,
    /// The bundled SVG logo as text, if there is one (drawn inline by the extension).
    pub logo_svg: Option<String>,
}

/// Live state shared with the loopback HTTP server and `--sleep` socket.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SleepHttpState {
    pub phase: String,
    pub left: i64,
    pub ends_at: String,
    #[serde(skip)]
    pub label: String,
    #[serde(skip)]
    pub plex_playing: bool,
    #[serde(skip)]
    pub held: bool,
}

impl SleepHttpState {
    pub fn from_snapshot(s: &SleepSnapshot, plex_playing: bool) -> Self {
        Self {
            phase: if s.held { "off".into() } else { s.phase.clone() },
            left: s.left,
            ends_at: s.ends_at.clone(),
            label: s.label.clone(),
            plex_playing,
            held: s.held,
        }
    }

    pub fn off() -> Self {
        Self {
            phase: "off".into(),
            left: 0,
            ends_at: String::new(),
            label: "Off".into(),
            plex_playing: false,
            held: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SleepReq {
    Minutes { minutes: i64 },
    Action { action: SleepAction },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SleepAction {
    Extend,
    Now,
    Off,
    EndOfItem,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AlertHttpSide {
    pub name: String,
    pub init: String,
    pub color: String,
    pub right: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AlertHttpState {
    pub key: String,
    pub kind: String,
    pub kicker: String,
    pub why: String,
    pub away: AlertHttpSide,
    pub home: AlertHttpSide,
    pub meta: String,
    pub primary: Option<String>,
    pub closes_in: i64,
}

#[derive(Deserialize)]
struct AlertReq {
    key: String,
    action: String,
}

/// Bind `127.0.0.1:{port}` and listen for `/home`, `/sleep`, and `/alerts`. Call inside a tokio runtime.
pub fn serve_pill(
    port: u16,
    cmd_tx: UnboundedSender<Command>,
    laptop: Weak<LaptopWindow>,
    sleep_rx: watch::Receiver<SleepHttpState>,
    alert_rx: watch::Receiver<Option<AlertHttpState>>,
) {
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{port}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => {
                info!(%addr, "listening for Back-to-tvdash pill");
                l
            }
            Err(e) => {
                warn!(%addr, error = %e, "couldn't bind pill home endpoint");
                return;
            }
        };
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx = cmd_tx.clone();
                    let weak = laptop.clone();
                    let srx = sleep_rx.clone();
                    let arx = alert_rx.clone();
                    tokio::spawn(handle_conn(stream, tx, weak, srx, arx));
                }
                Err(e) => warn!(error = %e, "pill listener accept failed"),
            }
        }
    });
}

/// Only the Back-to-tvdash extension (and local tools like `curl`) may use the endpoint.
///
/// The extension's service worker sends `Origin: chrome-extension://<id>` and, thanks to its
/// `host_permissions`, needs no CORS headers. Web pages always send an `http(s)://` Origin on
/// POST and on cross-origin `fetch`, so they get a 403. Requests without an Origin come from
/// local processes, which could already control tvdash through its Unix socket. The Host check
/// stops DNS-rebinding pages that pretend to be on `127.0.0.1`.
fn request_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let origin_ok = match origin {
        None => true,
        Some(o) => o.starts_with("chrome-extension://") || o.starts_with("moz-extension://"),
    };
    let host_ok = match host {
        None => true,
        Some(h) => {
            let name = h.rsplit_once(':').map_or(h, |(name, _)| name);
            matches!(name, "127.0.0.1" | "localhost" | "[::1]")
        }
    };
    origin_ok && host_ok
}

async fn handle_conn(
    stream: TcpStream,
    cmd_tx: UnboundedSender<Command>,
    laptop: Weak<LaptopWindow>,
    mut sleep_rx: watch::Receiver<SleepHttpState>,
    alert_rx: watch::Receiver<Option<AlertHttpState>>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut buf = BufReader::new(reader);
    let mut request_line = String::new();
    if buf.read_line(&mut request_line).await.unwrap_or(0) == 0 {
        return;
    }

    // Read headers & Content-Length.
    let mut content_len: usize = 0;
    let mut origin: Option<String> = None;
    let mut host: Option<String> = None;
    let mut line = String::new();
    loop {
        line.clear();
        match buf.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {
                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim();
                    if k.eq_ignore_ascii_case("content-length") {
                        content_len = v.trim().parse().unwrap_or(0).min(16 * 1024);
                    } else if k.eq_ignore_ascii_case("origin") {
                        origin = Some(v.trim().to_owned());
                    } else if k.eq_ignore_ascii_case("host") {
                        host = Some(v.trim().to_owned());
                    }
                }
            }
        }
    }

    let mut req_body = vec![0u8; content_len];
    if content_len > 0 && buf.read_exact(&mut req_body).await.is_err() {
        return;
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let (status, content_type, body) = if !request_allowed(origin.as_deref(), host.as_deref()) {
        warn!(?origin, ?host, %path, "pill endpoint: rejected request from a web page");
        ("403 Forbidden", "text/plain", "forbidden\n".into())
    } else if method == "OPTIONS" {
        ("204 No Content", "text/plain", String::new())
    } else if method == "POST" && path == "/home" {
        let _ = cmd_tx.send(Command::Home);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(l) = laptop.upgrade() {
                l.global::<Nav>().set_page("home".into());
                tv_nav::go_home(&l);
                let _ = l.window().show();
            }
        });
        ("200 OK", "text/plain", "ok\n".into())
    } else if method == "GET" && path == "/sleep" {
        let st = sleep_rx.borrow().clone();
        let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
        ("200 OK", "application/json", json)
    } else if method == "POST" && path == "/sleep" {
        match serde_json::from_slice::<SleepReq>(&req_body) {
            Ok(SleepReq::Minutes { minutes }) if (1..=480).contains(&minutes) => {
                let _ = cmd_tx.send(Command::SleepSetMinutes(minutes));
                let _ = tokio::time::timeout(std::time::Duration::from_millis(200), sleep_rx.changed()).await;
                let st = sleep_rx.borrow().clone();
                let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
                ("200 OK", "application/json", json)
            }
            Ok(SleepReq::Minutes { .. }) => {
                ("400 Bad Request", "application/json", r#"{"error":"minutes must be 1..=480"}"#.into())
            }
            Ok(SleepReq::Action { action }) => {
                let plex_playing = sleep_rx.borrow().plex_playing;
                match action {
                    SleepAction::EndOfItem if !plex_playing => (
                        "400 Bad Request",
                        "application/json",
                        r#"{"error":"end_of_item requires Plex playback"}"#.into(),
                    ),
                    SleepAction::EndOfItem => {
                        let _ = cmd_tx.send(Command::SleepSetEndOfItem);
                        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), sleep_rx.changed()).await;
                        let st = sleep_rx.borrow().clone();
                        let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
                        ("200 OK", "application/json", json)
                    }
                    SleepAction::Extend => {
                        let _ = cmd_tx.send(Command::SleepExtend);
                        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), sleep_rx.changed()).await;
                        let st = sleep_rx.borrow().clone();
                        let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
                        ("200 OK", "application/json", json)
                    }
                    SleepAction::Now => {
                        let _ = cmd_tx.send(Command::SleepNow);
                        let st = sleep_rx.borrow().clone();
                        let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
                        ("200 OK", "application/json", json)
                    }
                    SleepAction::Off => {
                        let _ = cmd_tx.send(Command::SleepOff);
                        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), sleep_rx.changed()).await;
                        let st = sleep_rx.borrow().clone();
                        let json = serde_json::to_string(&st).unwrap_or_else(|_| "{}".into());
                        ("200 OK", "application/json", json)
                    }
                }
            }
            Err(_) => ("400 Bad Request", "application/json", r#"{"error":"invalid request"}"#.into()),
        }
    } else if method == "GET" && path == "/alerts" {
        let st = alert_rx.borrow().clone();
        let json = match st {
            Some(a) => serde_json::to_string(&a).unwrap_or_else(|_| "null".into()),
            None => "null".into(),
        };
        ("200 OK", "application/json", json)
    } else if method == "POST" && path == "/alerts" {
        match serde_json::from_slice::<AlertReq>(&req_body) {
            Ok(req) if req.action == "primary" || req.action == "dismiss" => {
                let key = req.key;
                let action = req.action;
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(l) = laptop.upgrade() {
                        crate::bridge::alerts::on_http_action(&l, &key, &action);
                    }
                });
                ("204 No Content", "text/plain", String::new())
            }
            _ => ("400 Bad Request", "application/json", r#"{"error":"invalid alert request"}"#.into()),
        }
    } else {
        ("404 Not Found", "text/plain", "not found\n".into())
    };

    let resp = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = writer.write_all(resp.as_bytes()).await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn post_home_dispatches_command_home() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (_sleep_tx, sleep_rx) = watch::channel(SleepHttpState::off());
        let (_alert_tx, alert_rx) = watch::channel(None);

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_conn(stream, tx, Weak::default(), sleep_rx, alert_rx).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"POST /home HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).await.unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        let cmd = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(cmd, Command::Home));
    }

    #[tokio::test]
    async fn sleep_get_and_post() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (sleep_tx, sleep_rx) = watch::channel(SleepHttpState {
            phase: "counting".into(),
            left: 2521,
            ends_at: "23:42".into(),
            label: "43 min".into(),
            plex_playing: false,
            held: false,
        });

        tokio::spawn(async move {
            let (_alert_tx, alert_rx) = watch::channel(None);
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
                tokio::spawn(handle_conn(stream, tx.clone(), Weak::default(), sleep_rx.clone(), alert_rx.clone()));
            }
        });

        // GET /sleep
        let mut c = TcpStream::connect(addr).await.unwrap();
        c.write_all(b"GET /sleep HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await.unwrap();
        let mut resp = String::new();
        c.read_to_string(&mut resp).await.unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        assert!(resp.contains(r#""phase":"counting""#));
        assert!(resp.contains(r#""left":2521"#));

        // POST /sleep {"minutes": 30}
        let mut c2 = TcpStream::connect(addr).await.unwrap();
        let body = br#"{"minutes":30}"#;
        c2.write_all(
            format!("POST /sleep HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes(),
        )
        .await
        .unwrap();
        c2.write_all(body).await.unwrap();
        let cmd = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
        assert!(matches!(cmd, Command::SleepSetMinutes(30)));
        drop(sleep_tx);
    }

    #[test]
    fn only_the_extension_and_local_tools_are_allowed() {
        let ext = Some("chrome-extension://abcdefghijklmnop");
        let host = Some("127.0.0.1:47800");
        assert!(request_allowed(ext, host));
        assert!(request_allowed(None, host), "curl / local scripts");
        assert!(request_allowed(None, None));
        assert!(request_allowed(ext, Some("localhost:47800")));
        assert!(!request_allowed(Some("https://www.netflix.com"), host), "a web page");
        assert!(!request_allowed(Some("null"), host), "sandboxed iframe");
        assert!(!request_allowed(None, Some("evil.example:47800")), "DNS rebinding");
    }

    #[tokio::test]
    async fn web_page_post_is_rejected() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (_sleep_tx, sleep_rx) = watch::channel(SleepHttpState::off());
        let (_alert_tx, alert_rx) = watch::channel(None);

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_conn(stream, tx, Weak::default(), sleep_rx, alert_rx).await;
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(
                b"POST /home HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://ads.example\r\nContent-Length: 0\r\n\r\n",
            )
            .await
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).await.unwrap();
        assert!(resp.starts_with("HTTP/1.1 403"), "{resp}");
        assert!(!resp.contains("Access-Control-Allow-Origin"));
        assert!(rx.try_recv().is_err(), "no command sent");
    }
}
