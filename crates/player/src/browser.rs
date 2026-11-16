//! Streaming sites in a dedicated, full-screen browser profile (M4).
//!
//! ```text
//! brave-browser --user-data-dir=$HOME/.local/share/tvdash/browser \
//!   --app=<url> --start-fullscreen --no-first-run
//! ```
//!
//! Placement: native Wayland windows can't choose an output, so by default Chrome
//! runs through XWayland (`--ozone-platform=x11`) with `--window-position` set to
//! the TV's position, which GNOME honours for X11 windows.
//!
//! The browser runs in its own process group; [`Browser::close`] sends SIGTERM to
//! the whole group (Chrome saves cookies and sign-ins on SIGTERM), then SIGKILL if
//! it hasn't exited after a few seconds.
//!
//! Any Chromium-based browser works if it has Widevine DRM: Brave (enable it when
//! prompted), Google Chrome, Edge, Vivaldi. Fedora's `chromium` lacks Widevine, so
//! streaming sites won't play in it. Firefox works with DRM enabled.

use std::{
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};

use thiserror::Error;
use tokio::process::{Child, Command};
use tracing::{info, warn};
use tvdash_core::models::Geometry;

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("couldn't create the browser profile at {path}: {source}")]
    Profile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{binary} isn't installed. Install it, or set [browser] binary in config.toml")]
    NotInstalled { binary: String },
    #[error("couldn't start {binary}: {source}")]
    Spawn {
        binary: String,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, BrowserError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    /// Brave, Chrome, Edge, Vivaldi, Chromium: anything Chromium-based.
    Chromium,
    Firefox,
}

/// Brave ships Widevine switched off and asks before enabling it.
pub fn is_brave(binary: &str) -> bool {
    Path::new(binary).file_name().and_then(|n| n.to_str()).unwrap_or(binary).contains("brave")
}

impl BrowserKind {
    pub fn detect(binary: &str) -> Self {
        let name = Path::new(binary).file_name().and_then(|n| n.to_str()).unwrap_or(binary);
        if name.contains("firefox") { Self::Firefox } else { Self::Chromium }
    }
}

#[derive(Debug, Clone)]
pub struct BrowserOptions {
    pub binary: String,
    pub profile_dir: PathBuf,
    /// Chromium only: run through XWayland so the position below is honoured.
    pub use_xwayland: bool,
    /// Where to open (the TV output). `None` = wherever the desktop puts it.
    pub geometry: Option<Geometry>,
    /// False for development on one display.
    pub fullscreen: bool,
}

pub fn build_args(opts: &BrowserOptions, url: &str) -> Vec<String> {
    let profile = opts.profile_dir.display();
    match BrowserKind::detect(&opts.binary) {
        BrowserKind::Chromium => {
            let mut a = vec![
                format!("--user-data-dir={profile}"),
                format!("--app={url}"),
                "--no-first-run".into(),
                "--no-default-browser-check".into(),
                "--disable-session-crashed-bubble".into(),
                "--autoplay-policy=no-user-gesture-required".into(),
                "--class=tvdash-browser".into(),
            ];
            if opts.fullscreen {
                a.push("--start-fullscreen".into());
            }
            if opts.use_xwayland {
                a.push("--ozone-platform=x11".into());
            }
            if let Some(g) = opts.geometry {
                a.push(format!("--window-position={},{}", g.x, g.y));
                a.push(format!("--window-size={},{}", g.width, g.height));
            }
            a
        }
        BrowserKind::Firefox => {
            // Firefox can't be positioned; it opens where GNOME puts it.
            let mut a = vec!["--new-instance".into(), "--profile".into(), profile.to_string()];
            if opts.fullscreen {
                a.push("--kiosk".into());
            }
            a.push(url.into());
            a
        }
    }
}

/// A running browser. Dropping it kills the process group unless [`Browser::detach`] is called.
pub struct Browser {
    child: Option<Child>,
    pgid: Option<i32>,
    started: Instant,
    detached: bool,
    /// Site name, for logs and the UI.
    pub site: String,
}

fn is_pid_alive(pid: i32) -> bool {
    // SAFETY: kill(pid, 0) checks process existence without sending a signal.
    unsafe { libc::kill(pid, 0) == 0 }
}

impl Browser {
    pub async fn launch(opts: &BrowserOptions, site: &str, url: &str) -> Result<Self> {
        std::fs::create_dir_all(&opts.profile_dir)
            .map_err(|source| BrowserError::Profile { path: opts.profile_dir.clone(), source })?;
        let child = Command::new(&opts.binary)
            .args(build_args(opts, url))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0) // own group, so close() reaches every helper process
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| match source.kind() {
                std::io::ErrorKind::NotFound => BrowserError::NotInstalled { binary: opts.binary.clone() },
                _ => BrowserError::Spawn { binary: opts.binary.clone(), source },
            })?;
        let pgid = child.id().map(|p| p as i32);
        info!(site, binary = %opts.binary, pid = ?pgid, "browser started");
        Ok(Self { child: Some(child), pgid, started: Instant::now(), detached: false, site: site.to_owned() })
    }

    /// Reattach to an already-running browser process if it is still alive.
    pub fn reconnect(pid: i32, site: String) -> Option<Self> {
        if !is_pid_alive(pid) {
            return None;
        }
        Some(Self { child: None, pgid: Some(pid), started: Instant::now(), detached: false, site })
    }

    pub fn pid(&self) -> Option<i32> {
        self.pgid
    }

    /// Keep the browser running when this handle is dropped (e.g. during re-exec).
    pub fn detach(mut self) {
        self.detached = true;
        if let Some(child) = self.child.take() {
            std::mem::forget(child);
        }
    }

    /// Resolves when the browser exits on its own.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        if let Some(child) = &mut self.child {
            return child.wait().await;
        }
        if let Some(pid) = self.pgid {
            while is_pid_alive(pid) {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            Ok(ExitStatus::from_raw(0))
        }
    }

    /// Exited almost immediately: Chrome does this when another instance already
    /// owns the profile and it just handed the URL over.
    pub fn exited_immediately(&self) -> bool {
        self.started.elapsed() < Duration::from_secs(3)
    }

    /// SIGTERM the main browser process so it flushes cookies and sign-ins,
    /// then SIGKILL the process group if anything hasn't exited within 5 s.
    pub async fn close(mut self) {
        self.detached = true;
        self.signal_main(libc::SIGTERM);
        if let Some(mut child) = self.child.take() {
            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                Ok(_) => info!(site = %self.site, "browser closed"),
                Err(_) => {
                    warn!(site = %self.site, "browser ignored SIGTERM; killing it");
                    self.signal(libc::SIGKILL);
                    let _ = child.wait().await;
                }
            }
        } else if let Some(pid) = self.pgid {
            let deadline = Instant::now() + Duration::from_secs(5);
            while is_pid_alive(pid) && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if is_pid_alive(pid) {
                warn!(site = %self.site, "browser ignored SIGTERM; killing it");
                self.signal(libc::SIGKILL);
            }
        }
        // Helpers can outlive the main process for a moment; make sure they go too.
        self.signal(libc::SIGKILL);
    }

    fn signal_main(&self, sig: i32) {
        if let Some(pid) = self.pgid {
            // SAFETY: plain syscall; targets only the main browser process so
            // it can flush cookies/storage before tearing down its helpers.
            unsafe {
                libc::kill(pid, sig);
            }
        }
    }

    fn signal(&self, sig: i32) {
        if let Some(pgid) = self.pgid {
            // SAFETY: plain syscall; a negative pid targets the process group we created.
            unsafe {
                libc::kill(-pgid, sig);
            }
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if !self.detached {
            self.signal_main(libc::SIGTERM);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(binary: &str) -> BrowserOptions {
        BrowserOptions {
            binary: binary.into(),
            profile_dir: "/p".into(),
            use_xwayland: true,
            geometry: Some(Geometry { x: 1920, y: 0, width: 3840, height: 2160 }),
            fullscreen: true,
        }
    }

    #[test]
    fn chrome_args() {
        let a = build_args(&opts("/usr/bin/brave-browser"), "https://www.netflix.com");
        for want in [
            "--user-data-dir=/p",
            "--app=https://www.netflix.com",
            "--start-fullscreen",
            "--no-first-run",
            "--ozone-platform=x11",
            "--window-position=1920,0",
            "--window-size=3840,2160",
        ] {
            assert!(a.contains(&want.to_string()), "missing {want}: {a:?}");
        }
    }

    #[test]
    fn chrome_native_wayland_windowed() {
        let o = BrowserOptions { use_xwayland: false, geometry: None, fullscreen: false, ..opts("google-chrome") };
        let a = build_args(&o, "u");
        assert!(!a.iter().any(|x| x.contains("ozone") || x.contains("window-") || x.contains("fullscreen")));
    }

    #[test]
    fn detects_brave() {
        assert!(is_brave("/usr/bin/brave-browser"));
        assert!(is_brave("brave-browser-beta"));
        assert!(!is_brave("google-chrome"));
        assert_eq!(BrowserKind::detect("brave-browser"), BrowserKind::Chromium);
    }

    #[test]
    fn firefox_args() {
        assert_eq!(BrowserKind::detect("/opt/firefox/firefox"), BrowserKind::Firefox);
        let a = build_args(&opts("firefox"), "https://tv.apple.com");
        assert_eq!(a, ["--new-instance", "--profile", "/p", "--kiosk", "https://tv.apple.com"]);
    }
}
