//! One running tvdash per user, reachable on `$XDG_RUNTIME_DIR/tvdash.sock`.
//!
//! `tvdash --home` (bound to a GNOME keyboard shortcut) sends `home` to the
//! running app, which reaches it even while mpv or the browser has focus. A
//! second plain `tvdash` finds the socket answering and exits instead of
//! starting a second copy.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::{
    io::{AsyncBufReadExt, BufReader as TokioBufReader},
    net::UnixListener,
};
use tracing::{debug, info, warn};

/// Messages the socket understands, one per line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// Back to Home: stop playback / close the browser, show the Home screen.
    Home,
    /// Just checking whether an instance is running.
    Ping,
}

impl Message {
    fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Ping => "ping",
        }
    }

    fn parse(line: &str) -> Option<Self> {
        match line.trim() {
            "home" => Some(Self::Home),
            "ping" => Some(Self::Ping),
            _ => None,
        }
    }
}

pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    dir.join("tvdash.sock")
}

/// Send `msg` to a running instance. `Ok(true)` = it answered.
pub fn send(path: &Path, msg: Message) -> std::io::Result<bool> {
    let mut stream = match UnixStream::connect(path) {
        Ok(s) => s,
        Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => {
            return Ok(false);
        }
        Err(e) => return Err(e),
    };
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    writeln!(stream, "{}", msg.as_str())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim() == "ok")
}

/// Listen for messages; `on_message` runs for each one (on the tokio runtime).
/// A stale socket file from a crashed run is replaced. Call inside a runtime.
pub fn listen(path: PathBuf, on_message: impl Fn(Message) + Send + Sync + 'static) -> std::io::Result<()> {
    if path.exists() {
        // Nobody answered before we got here (main checks), so it's stale.
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path)?;
    info!(path = %path.display(), "listening for tvdash --home");
    let on_message = std::sync::Arc::new(on_message);
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "instance socket accept failed");
                    continue;
                }
            };
            let on_message = on_message.clone();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                if TokioBufReader::new(read).read_line(&mut line).await.is_err() {
                    return;
                }
                let reply = match Message::parse(&line) {
                    Some(m) => {
                        debug!(message = ?m, "instance socket");
                        on_message(m);
                        "ok\n"
                    }
                    None => "unknown\n",
                };
                use tokio::io::AsyncWriteExt;
                let _ = write.write_all(reply.as_bytes()).await;
            });
        }
    });
    Ok(())
}

/// Remove the socket file on exit.
pub fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test(flavor = "multi_thread")]
    async fn home_reaches_the_running_instance() {
        let path = std::env::temp_dir().join(format!("tvdash-sock-{}", std::process::id()));
        assert!(!send(&path, Message::Ping).unwrap(), "nothing running yet");
        std::fs::write(&path, b"stale").unwrap(); // a crashed run left its socket behind
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        listen(path.clone(), move |m| g.lock().unwrap().push(m)).unwrap();
        let p = path.clone();
        let answered =
            tokio::task::spawn_blocking(move || (send(&p, Message::Ping), send(&p, Message::Home))).await.unwrap();
        assert!(answered.0.unwrap() && answered.1.unwrap());
        assert_eq!(*got.lock().unwrap(), [Message::Ping, Message::Home]);
        cleanup(&path);
    }
}
