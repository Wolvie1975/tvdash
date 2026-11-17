//! Lightweight local HTTP phone remote for TV-only mode.
//!
//! Serves a mobile-friendly remote page at `http://<host>:8765/` and accepts
//! `POST /cmd` requests for D-pad navigation, playback controls, Home, and
//! Search typing from a phone on the same network.

use slint::{ComponentHandle, SharedString, Weak, platform::Key};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc::UnboundedSender,
};
use tracing::{info, warn};
use tvdash_core::Command;

use crate::{Dash, LaptopWindow, Nav, TvUi, Watch, tv_nav};

pub const DEFAULT_PORT: u16 = 8765;

#[derive(Debug, Clone, PartialEq)]
enum RemoteAction {
    Key(Key),
    Home,
    PlayPause,
    Seek(i64),
    CycleAudio,
    CycleSub,
    VolumeDelta(i64),
    Stop,
    Search(String),
}

fn parse_action(body: &str) -> Option<RemoteAction> {
    let cmd = body.trim();
    if let Some(q) = cmd.strip_prefix("search:") {
        let q = q.trim().to_owned();
        return (!q.is_empty()).then_some(RemoteAction::Search(q));
    }
    match cmd {
        "up" => Some(RemoteAction::Key(Key::UpArrow)),
        "down" => Some(RemoteAction::Key(Key::DownArrow)),
        "left" => Some(RemoteAction::Key(Key::LeftArrow)),
        "right" => Some(RemoteAction::Key(Key::RightArrow)),
        "ok" => Some(RemoteAction::Key(Key::Return)),
        "back" => Some(RemoteAction::Key(Key::Escape)),
        "home" => Some(RemoteAction::Home),
        "play-pause" => Some(RemoteAction::PlayPause),
        "seek-back" => Some(RemoteAction::Seek(-10)),
        "seek-fwd" => Some(RemoteAction::Seek(30)),
        "audio" => Some(RemoteAction::CycleAudio),
        "sub" => Some(RemoteAction::CycleSub),
        "vol-down" => Some(RemoteAction::VolumeDelta(-5)),
        "vol-up" => Some(RemoteAction::VolumeDelta(5)),
        "stop" => Some(RemoteAction::Stop),
        _ => None,
    }
}

fn dispatch(action: RemoteAction, tx: &UnboundedSender<Command>, laptop: &Weak<LaptopWindow>) {
    match action {
        RemoteAction::Key(k) => {
            let weak = laptop.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(l) = weak.upgrade() {
                    tv_nav::key(&l, &SharedString::from(k));
                }
            });
        }
        RemoteAction::Home => {
            let _ = tx.send(Command::Home);
            let weak = laptop.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(l) = weak.upgrade() {
                    l.global::<Nav>().set_page("home".into());
                    tv_nav::go_home(&l);
                }
            });
        }
        RemoteAction::PlayPause => {
            let weak = laptop.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(l) = weak.upgrade() {
                    l.global::<Dash>().invoke_transport("play-pause".into());
                }
            });
        }
        RemoteAction::Seek(secs) => {
            let _ = tx.send(Command::Seek(secs));
        }
        RemoteAction::CycleAudio => {
            let _ = tx.send(Command::CycleAudio);
        }
        RemoteAction::CycleSub => {
            let _ = tx.send(Command::CycleSub);
        }
        RemoteAction::VolumeDelta(delta) => {
            let _ = tx.send(Command::VolumeDelta(delta));
        }
        RemoteAction::Stop => {
            let _ = tx.send(Command::Stop);
        }
        RemoteAction::Search(query) => {
            let weak = laptop.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(l) = weak.upgrade() {
                    let ui = l.global::<TvUi>();
                    ui.set_ambient(false);
                    ui.set_video_full(false);
                    ui.set_screen("search".into());
                    let w = l.global::<Watch>();
                    w.set_detail_open(false);
                    w.set_query(query.as_str().into());
                    w.invoke_search(query.into());
                    tv_nav::sync(&l);
                }
            });
        }
    }
}

pub fn serve(port: u16, tx: UnboundedSender<Command>, laptop: Weak<LaptopWindow>) {
    tokio::spawn(async move {
        let listener = match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(l) => l,
            Err(e) => {
                warn!(port, error = %e, "phone remote disabled (couldn't bind)");
                return;
            }
        };
        info!(port, "phone remote listening");
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx = tx.clone();
                    let laptop = laptop.clone();
                    tokio::spawn(async move {
                        let _ = handle_conn(stream, &tx, &laptop).await;
                    });
                }
                Err(e) => {
                    warn!(error = %e, "phone remote accept failed");
                    break;
                }
            }
        }
    });
}

async fn handle_conn(
    stream: TcpStream,
    tx: &UnboundedSender<Command>,
    laptop: &Weak<LaptopWindow>,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case("content-length")
        {
            content_length = v.trim().parse().unwrap_or(0).min(4096);
        }
    }

    match (method, path) {
        ("GET", "/") => {
            let body = REMOTE_HTML.as_bytes();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            writer.write_all(header.as_bytes()).await?;
            writer.write_all(body).await?;
        }
        ("POST", "/cmd") => {
            let mut buf = vec![0u8; content_length];
            if content_length > 0 {
                reader.read_exact(&mut buf).await?;
            }
            let body = String::from_utf8_lossy(&buf);
            let status = if let Some(action) = parse_action(&body) {
                dispatch(action, tx, laptop);
                "200 OK"
            } else {
                "400 Bad Request"
            };
            let resp = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            writer.write_all(resp.as_bytes()).await?;
        }
        _ => {
            writer
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await?;
        }
    }
    writer.flush().await
}

const REMOTE_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,user-scalable=no">
<title>tvdash remote</title>
<style>
  :root { color-scheme: dark; }
  body { margin: 0; font-family: system-ui, sans-serif; background: #0B0D10; color: #E8EAED; display: flex; flex-direction: column; align-items: center; padding: 18px; gap: 18px; }
  h1 { margin: 0; font-size: 18px; font-weight: 600; color: #9BA3AE; letter-spacing: .04em; }
  .search { display: flex; gap: 8px; width: 100%; max-width: 360px; }
  input { flex: 1; background: #161A20; color: #E8EAED; border: 1px solid #2A303A; border-radius: 12px; padding: 12px 14px; font-size: 16px; }
  button { background: #1A1F26; color: #E8EAED; border: 1px solid #2A303A; border-radius: 14px; font-size: 16px; font-weight: 600; padding: 14px; cursor: pointer; touch-action: manipulation; }
  button:active { background: #2D3542; transform: scale(.97); }
  .primary { background: #E8EAED; color: #0B0D10; border-color: #E8EAED; }
  .dpad { display: grid; grid-template-columns: repeat(3, 84px); grid-template-rows: repeat(3, 68px); gap: 10px; }
  .row { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; width: 100%; max-width: 360px; }
  .media { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; width: 100%; max-width: 360px; }
</style>
</head>
<body>
  <h1>TVDASH REMOTE</h1>
  <form class="search" onsubmit="event.preventDefault(); send('search:' + q.value); q.blur();">
    <input id="q" type="search" placeholder="Search on TV…">
    <button class="primary" type="submit">Go</button>
  </form>
  <div class="row">
    <button onclick="send('back')">Back</button>
    <button class="primary" onclick="send('home')">Home</button>
  </div>
  <div class="dpad">
    <div></div><button onclick="send('up')">▲</button><div></div>
    <button onclick="send('left')">◀</button><button class="primary" onclick="send('ok')">OK</button><button onclick="send('right')">▶</button>
    <div></div><button onclick="send('down')">▼</button><div></div>
  </div>
  <div class="media">
    <button onclick="send('seek-back')">−10s</button>
    <button class="primary" onclick="send('play-pause')">⏯</button>
    <button onclick="send('seek-fwd')">+30s</button>
    <button onclick="send('stop')">Stop</button>
    <button onclick="send('audio')">Audio</button>
    <button onclick="send('sub')">Subs</button>
    <button onclick="send('vol-down')">Vol −</button>
    <button onclick="send('vol-up')">Vol +</button>
  </div>
<script>
  function send(c) {
    fetch('/cmd', { method: 'POST', body: c }).catch(() => {});
  }
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_commands() {
        assert_eq!(parse_action("up"), Some(RemoteAction::Key(Key::UpArrow)));
        assert_eq!(parse_action("ok\n"), Some(RemoteAction::Key(Key::Return)));
        assert_eq!(parse_action("home"), Some(RemoteAction::Home));
        assert_eq!(parse_action("audio"), Some(RemoteAction::CycleAudio));
        assert_eq!(parse_action("vol-down"), Some(RemoteAction::VolumeDelta(-5)));
        assert_eq!(parse_action("search: the matrix "), Some(RemoteAction::Search("the matrix".into())));
        assert_eq!(parse_action("search:   "), None);
        assert_eq!(parse_action("unknown"), None);
    }

    #[tokio::test]
    async fn serves_html_and_accepts_post_cmd() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let weak = Weak::<LaptopWindow>::default();
        tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                handle_conn(stream, &tx, &weak).await.unwrap();
            }
        });

        let mut get = TcpStream::connect(addr).await.unwrap();
        get.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
        let mut resp = String::new();
        get.read_to_string(&mut resp).await.unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        assert!(resp.contains("TVDASH REMOTE"));

        let mut post = TcpStream::connect(addr).await.unwrap();
        post.write_all(b"POST /cmd HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\n\r\nhome").await.unwrap();
        let mut post_resp = String::new();
        post.read_to_string(&mut post_resp).await.unwrap();
        assert!(post_resp.starts_with("HTTP/1.1 200 OK"));
        assert!(matches!(rx.recv().await, Some(Command::Home)));
    }
}
