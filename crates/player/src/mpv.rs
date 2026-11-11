//! mpv full screen on the TV, controlled over its JSON IPC socket (M2).
//!
//! ```text
//! mpv --fullscreen --input-ipc-server=$XDG_RUNTIME_DIR/tvdash-mpv.sock \
//!     --fs-screen-name=HDMI-A-1 --force-window=yes --keep-open=no <url>
//! ```
//!
//! mpv's window covers the TV window, so the playback overlay (title, progress,
//! end time) is drawn with mpv's own OSD via [`Mpv::show_text`].

use std::{path::PathBuf, process::Stdio, time::Duration};

use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixStream, unix::OwnedWriteHalf},
    process::{Child, Command},
    sync::mpsc,
};
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum MpvError {
    #[error("couldn't start mpv ({binary}): {source}")]
    Spawn {
        binary: String,
        #[source]
        source: std::io::Error,
    },
    #[error("mpv exited before its IPC socket was ready ({0})")]
    ExitedEarly(String),
    #[error("timed out waiting for mpv's IPC socket")]
    SocketTimeout,
    #[error("mpv IPC: {0}")]
    Ipc(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MpvError>;

/// Path of the IPC socket: `$XDG_RUNTIME_DIR/tvdash-mpv.sock`.
pub fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("tvdash-mpv.sock")
}

#[derive(Debug, Clone)]
pub struct MpvOptions {
    pub binary: String,
    /// Connector name of the TV (`--fs-screen-name`); `None` = mpv's default.
    pub screen_name: Option<String>,
    pub title: String,
    /// Resume point.
    pub start_ms: Option<u64>,
    /// False for development on one display.
    pub fullscreen: bool,
}

/// Something mpv told us.
#[derive(Debug, Clone, PartialEq)]
pub enum MpvEvent {
    /// Playback position in seconds.
    Position(f64),
    /// Duration in seconds (known once the file is loaded).
    Duration(f64),
    Paused(bool),
    /// The file finished or was stopped; `reason` is mpv's (`eof`, `stop`, `quit`, `error`).
    Ended { reason: String },
    /// mpv quit / the IPC connection closed. Always the last event.
    Exited,
}

// Observer IDs for `observe_property`.
const OBS_TIME_POS: u64 = 1;
const OBS_PAUSE: u64 = 2;
const OBS_DURATION: u64 = 3;

pub fn build_args(opts: &MpvOptions, socket: &std::path::Path, url: &str) -> Vec<String> {
    let mut args = vec![
        format!("--input-ipc-server={}", socket.display()),
        "--force-window=yes".into(),
        "--keep-open=no".into(),
        "--idle=no".into(),
        "--really-quiet".into(),
        "--cursor-autohide=always".into(),
        format!("--title={}", opts.title),
        format!("--force-media-title={}", opts.title),
        // Overlay styling for TV distance (bottom-left, large).
        "--osd-font-size=44".into(),
        "--osd-border-size=2".into(),
        "--osd-align-x=left".into(),
        "--osd-align-y=bottom".into(),
        "--osd-margin-x=72".into(),
        "--osd-margin-y=64".into(),
        "--osd-duration=5000".into(),
    ];
    if opts.fullscreen {
        args.push("--fullscreen".into());
        if let Some(screen) = &opts.screen_name {
            args.push(format!("--fs-screen-name={screen}"));
        }
    }
    if let Some(ms) = opts.start_ms.filter(|&ms| ms > 0) {
        args.push(format!("--start={:.3}", ms as f64 / 1000.0));
    }
    args.push("--".into());
    args.push(url.into());
    args
}

/// Parse one line from the IPC socket into an event we care about.
pub fn parse_line(line: &str) -> Option<MpvEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    match v.get("event")?.as_str()? {
        "property-change" => {
            let data = v.get("data")?;
            match v.get("name")?.as_str()? {
                "time-pos" => data.as_f64().map(MpvEvent::Position),
                "duration" => data.as_f64().map(MpvEvent::Duration),
                "pause" => data.as_bool().map(MpvEvent::Paused),
                _ => None,
            }
        }
        "end-file" => Some(MpvEvent::Ended {
            reason: v.get("reason").and_then(Value::as_str).unwrap_or("unknown").to_owned(),
        }),
        "shutdown" => Some(MpvEvent::Exited),
        _ => None,
    }
}

/// A running mpv. Dropping it kills the process.
pub struct Mpv {
    child: Child,
    writer: OwnedWriteHalf,
    events: mpsc::UnboundedReceiver<MpvEvent>,
    exited: bool,
}

impl Mpv {
    /// Start mpv on `url` and connect to its IPC socket.
    pub async fn spawn(opts: &MpvOptions, url: &str) -> Result<Self> {
        let socket = socket_path();
        let _ = std::fs::remove_file(&socket); // stale socket from a crash

        let mut child = Command::new(&opts.binary)
            .args(build_args(opts, &socket, url))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| MpvError::Spawn {
                binary: opts.binary.clone(),
                source,
            })?;

        let stream = connect(&mut child, &socket).await?;
        let (read, writer) = stream.into_split();

        let (tx, events) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if let Some(ev) = parse_line(&line) {
                            let last = ev == MpvEvent::Exited;
                            if tx.send(ev).is_err() || last {
                                return;
                            }
                        } else {
                            debug!(line, "mpv ipc");
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        warn!(error = %e, "mpv IPC read failed");
                        break;
                    }
                }
            }
            let _ = tx.send(MpvEvent::Exited);
        });

        let mut mpv = Self {
            child,
            writer,
            events,
            exited: false,
        };
        mpv.command(json!(["observe_property", OBS_TIME_POS, "time-pos"])).await?;
        mpv.command(json!(["observe_property", OBS_PAUSE, "pause"])).await?;
        mpv.command(json!(["observe_property", OBS_DURATION, "duration"])).await?;
        Ok(mpv)
    }

    /// Next event; `None` after [`MpvEvent::Exited`] has been returned.
    pub async fn next_event(&mut self) -> Option<MpvEvent> {
        if self.exited {
            return None;
        }
        let ev = self.events.recv().await.unwrap_or(MpvEvent::Exited);
        if ev == MpvEvent::Exited {
            self.exited = true;
        }
        Some(ev)
    }

    async fn command(&mut self, args: Value) -> Result<()> {
        let mut line = json!({ "command": args }).to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        Ok(())
    }

    pub async fn set_pause(&mut self, paused: bool) -> Result<()> {
        self.command(json!(["set_property", "pause", paused])).await
    }

    pub async fn seek(&mut self, seconds: i64) -> Result<()> {
        self.command(json!(["seek", seconds, "relative"])).await
    }

    /// OSD text on the TV; `duration_ms: None` keeps it up until replaced.
    pub async fn show_text(&mut self, text: &str, duration_ms: Option<u32>) -> Result<()> {
        // mpv has no "forever"; a day is close enough for a paused overlay.
        let ms = duration_ms.unwrap_or(86_400_000);
        self.command(json!(["show-text", text, ms])).await
    }

    /// Ask mpv to quit, then make sure the process is gone.
    pub async fn quit(mut self) {
        let _ = self.command(json!(["quit"])).await;
        match tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await {
            Ok(_) => {}
            Err(_) => {
                warn!("mpv didn't quit; killing it");
                let _ = self.child.kill().await;
            }
        }
    }
}

async fn connect(child: &mut Child, socket: &std::path::Path) -> Result<UnixStream> {
    for _ in 0..100 {
        if let Some(status) = child.try_wait()? {
            return Err(MpvError::ExitedEarly(status.to_string()));
        }
        if let Ok(stream) = UnixStream::connect(socket).await {
            return Ok(stream);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill().await;
    Err(MpvError::SocketTimeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> MpvOptions {
        MpvOptions {
            binary: "mpv".into(),
            screen_name: Some("HDMI-A-1".into()),
            title: "Show".into(),
            start_ms: Some(90_500),
            fullscreen: true,
        }
    }

    #[test]
    fn args_include_screen_start_and_url_last() {
        let a = build_args(&opts(), std::path::Path::new("/run/x.sock"), "http://h/v");
        assert!(a.contains(&"--input-ipc-server=/run/x.sock".to_string()));
        assert!(a.contains(&"--fullscreen".to_string()));
        assert!(a.contains(&"--fs-screen-name=HDMI-A-1".to_string()));
        assert!(a.contains(&"--start=90.500".to_string()));
        assert_eq!(&a[a.len() - 2..], ["--", "http://h/v"]);
    }

    #[test]
    fn windowed_args_skip_fullscreen() {
        let o = MpvOptions {
            fullscreen: false,
            start_ms: None,
            ..opts()
        };
        let a = build_args(&o, std::path::Path::new("/s"), "u");
        assert!(!a.iter().any(|x| x.starts_with("--fs") || x == "--fullscreen" || x.starts_with("--start")));
    }

    #[test]
    fn parses_events() {
        assert_eq!(
            parse_line(r#"{"event":"property-change","id":1,"name":"time-pos","data":12.5}"#),
            Some(MpvEvent::Position(12.5))
        );
        assert_eq!(
            parse_line(r#"{"event":"property-change","id":2,"name":"pause","data":true}"#),
            Some(MpvEvent::Paused(true))
        );
        assert_eq!(
            parse_line(r#"{"event":"property-change","id":1,"name":"time-pos"}"#),
            None,
            "no data before the file loads"
        );
        assert_eq!(
            parse_line(r#"{"event":"end-file","reason":"eof","playlist_entry_id":1}"#),
            Some(MpvEvent::Ended { reason: "eof".into() })
        );
        assert_eq!(parse_line(r#"{"event":"shutdown"}"#), Some(MpvEvent::Exited));
        assert_eq!(parse_line(r#"{"request_id":0,"error":"success"}"#), None);
        assert_eq!(parse_line("garbage"), None);
    }
}
