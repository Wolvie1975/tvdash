//! Loopback HTTP endpoint (`POST http://127.0.0.1:47800/home`) for the
//! Brave "Back to tvdash" pill extension (§6.1).

use slint::{ComponentHandle, Weak};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc::UnboundedSender,
};
use tracing::{info, warn};
use tvdash_core::Command;

use crate::{LaptopWindow, Nav, tv_nav};

pub const PILL_PORT: u16 = 47800;

/// Bind `127.0.0.1:{port}` and listen for `POST /home`. Call inside a tokio runtime.
pub fn serve_pill(port: u16, cmd_tx: UnboundedSender<Command>, laptop: Weak<LaptopWindow>) {
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
                    tokio::spawn(handle_conn(stream, tx, weak));
                }
                Err(e) => warn!(error = %e, "pill listener accept failed"),
            }
        }
    });
}

async fn handle_conn(stream: TcpStream, cmd_tx: UnboundedSender<Command>, laptop: Weak<LaptopWindow>) {
    let (reader, mut writer) = stream.into_split();
    let mut buf = BufReader::new(reader);
    let mut request_line = String::new();
    if buf.read_line(&mut request_line).await.unwrap_or(0) == 0 {
        return;
    }

    // Drain headers
    let mut line = String::new();
    loop {
        line.clear();
        match buf.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" => break,
            Ok(_) => {}
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let (status, body) = if method == "OPTIONS" {
        ("204 No Content", "")
    } else if method == "POST" && path == "/home" {
        let _ = cmd_tx.send(Command::Home);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(l) = laptop.upgrade() {
                l.global::<Nav>().set_page("home".into());
                tv_nav::go_home(&l);
                let _ = l.window().show();
            }
        });
        ("200 OK", "ok\n")
    } else {
        ("404 Not Found", "not found\n")
    };

    let resp = format!(
        "HTTP/1.1 {status}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\n\
         Content-Type: text/plain\r\n\
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

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_conn(stream, tx, Weak::default()).await;
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
}
