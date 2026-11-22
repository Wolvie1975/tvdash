// Dev tool: play a video inside the TV-only window through libmpv, the way
// tvdash does (same drawing code, src/video.rs), without accounts or servers.
//
//   cargo run --example video -- <file or URL>            # play; close the window to stop
//   cargo run --example video -- <file or URL> <dir>      # also save <dir>/full.png and
//                                                          # <dir>/home.png, then quit
//
// Keys: Space pause, ←/→ seek, Enter overlay, Esc Home over the video (again: back).
// Try it without a file: av://lavfi:testsrc2=size=1280x720:rate=25
use std::{path::PathBuf, time::Duration};

use anyhow::Context;
use slint::{ComponentHandle, Timer, TimerMode};
use tokio::sync::mpsc;
use tvdash_player::{
    libmpv::{Embedded, VideoSlot},
    mpv::{MpvEvent, MpvOptions},
};

slint::include_modules!();

#[path = "../src/video.rs"]
mod video;

/// src/video.rs asks the TV navigation to recheck focus; nothing to focus here.
mod tv_nav {
    pub fn sync(_: &crate::LaptopWindow) {}
}

/// A step in snapshot mode: what to do with the window.
type Step = Box<dyn Fn(&LaptopWindow)>;

enum Control {
    Pause,
    Seek(i64),
    Overlay,
    Quit,
}

fn snapshot(win: &LaptopWindow, path: &std::path::Path) -> anyhow::Result<()> {
    let shot = win.window().take_snapshot().context("snapshot")?;
    let img = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_bytes().to_vec()).context("pixels")?;
    img.save(path)?;
    println!("saved {}", path.display());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let mut args = std::env::args().skip(1);
    let url = args.next().context("usage: cargo run --example video -- <file or URL> [snapshot dir]")?;
    let snaps = args.next().map(PathBuf::from);
    let version = tvdash_player::libmpv::load()?;
    println!("libmpv {version}");

    let rt = tokio::runtime::Runtime::new()?;
    slint::BackendSelector::new().backend_name("winit".into()).require_opengl().select()?;
    let win = LaptopWindow::new()?;
    win.global::<Nav>().set_layout("tv".into());
    win.global::<TvUi>().set_layout_tv(true);
    win.window().set_size(slint::LogicalSize::new(1280.0, 720.0));

    let slot = VideoSlot::new();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    video::install(&win, None, slot.clone(), cmd_tx);
    rt.spawn(async move {
        while let Some(c) = cmd_rx.recv().await {
            println!("the window asked: {c:?}");
        }
    });

    let opts = MpvOptions {
        binary: String::new(),
        screen_name: None,
        title: "tvdash video test".into(),
        start_ms: None,
        fullscreen: false,
    };
    let mut player = {
        let _rt = rt.enter();
        Embedded::spawn(&opts, &url)?
    };
    slot.attach(player.handle());

    // The player: controls in, events printed; quits the window when mpv ends.
    let (ctl, mut ctl_rx) = mpsc::unbounded_channel::<Control>();
    let weak = win.as_weak();
    rt.spawn(async move {
        let mut paused = false;
        loop {
            tokio::select! {
                c = ctl_rx.recv() => match c {
                    Some(Control::Pause) => { paused = !paused; let _ = player.set_pause(paused); }
                    Some(Control::Seek(s)) => { let _ = player.seek(s); }
                    Some(Control::Overlay) => { let _ = player.show_text("tvdash video test", Some(3000)); }
                    Some(Control::Quit) | None => { slot.detach(); player.quit().await; break; }
                },
                e = player.next_event() => match e {
                    Some(MpvEvent::Position(_)) => {}
                    Some(MpvEvent::Exited) | None => { println!("mpv ended"); slot.detach(); break; }
                    Some(other) => println!("{other:?}"),
                },
            }
        }
        let _ = weak.upgrade_in_event_loop(|_| {
            let _ = slint::quit_event_loop();
        });
    });

    let (c, weak) = (ctl.clone(), win.as_weak());
    win.global::<TvUi>().on_key(move |text| {
        let Some(win) = weak.upgrade() else { return false };
        let ui = win.global::<TvUi>();
        let key = |k: slint::platform::Key| text == slint::SharedString::from(k);
        let control = match text.as_str() {
            " " => Control::Pause,
            "\n" | "\r" => Control::Overlay,
            _ if key(slint::platform::Key::LeftArrow) => Control::Seek(-10),
            _ if key(slint::platform::Key::RightArrow) => Control::Seek(30),
            _ if key(slint::platform::Key::Escape) => {
                ui.set_video_full(!ui.get_video_full());
                return true;
            }
            _ => return false,
        };
        let _ = c.send(control);
        true
    });

    // Snapshot mode: full screen at 3 s, Home over the video at 5 s, then quit.
    let timers: Vec<Timer> = match snaps {
        None => Vec::new(),
        Some(dir) => {
            std::fs::create_dir_all(&dir)?;
            let steps: [(u64, Step); 3] = [
                (
                    3,
                    Box::new({
                        let d = dir.clone();
                        move |w| {
                            if let Err(e) = snapshot(w, &d.join("full.png")) {
                                eprintln!("{e:#}");
                            }
                            w.global::<TvUi>().set_video_full(false);
                        }
                    }),
                ),
                (
                    5,
                    Box::new({
                        let d = dir.clone();
                        move |w| {
                            if let Err(e) = snapshot(w, &d.join("home.png")) {
                                eprintln!("{e:#}");
                            }
                        }
                    }),
                ),
                (
                    6,
                    Box::new({
                        let c = ctl.clone();
                        move |_| {
                            let _ = c.send(Control::Quit);
                        }
                    }),
                ),
            ];
            steps
                .into_iter()
                .map(|(secs, f)| {
                    let (t, weak) = (Timer::default(), win.as_weak());
                    t.start(TimerMode::SingleShot, Duration::from_secs(secs), move || {
                        if let Some(w) = weak.upgrade() {
                            f(&w);
                        }
                    });
                    t
                })
                .collect()
        }
    };

    win.show()?;
    slint::run_event_loop_until_quit()?;
    drop(timers);
    let _ = ctl.send(Control::Quit);
    rt.shutdown_timeout(Duration::from_secs(3));
    Ok(())
}
