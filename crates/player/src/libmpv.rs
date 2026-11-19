//! mpv inside the tvdash window, through libmpv's client and render APIs.
//!
//! libmpv is loaded at runtime (`libmpv.so.2`), so tvdash still starts where
//! it's missing and falls back to mpv's own window ([`crate::mpv`]). Two halves:
//!
//! - [`Embedded`] is the player the controller drives: same controls and
//!   events as [`crate::mpv::Mpv`] (pause, seek, OSD text, [`MpvEvent`]s).
//! - [`RenderContext`] draws the video with OpenGL. It lives on the UI thread
//!   (libmpv requires it to be created and freed on one thread) and finds the
//!   player through a [`VideoSlot`].
//!
//! The mpv core is destroyed when the last [`Handle`] goes, which is always
//! after the render context was freed: the render context holds one.

use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::mpv::{MpvEvent, MpvOptions};

#[derive(Debug, Error)]
pub enum LibmpvError {
    #[error("libmpv isn't installed ({0})")]
    NotLoaded(String),
    #[error("libmpv: {0}")]
    Call(String),
}

pub type Result<T> = std::result::Result<T, LibmpvError>;

// ── C API (client.h, render.h, render_gl.h; API 2.x) ─────────────────

#[repr(C)]
struct MpvHandle {
    _private: [u8; 0],
}
#[repr(C)]
struct MpvRenderCtx {
    _private: [u8; 0],
}

#[repr(C)]
struct Event {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct EventLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}
#[repr(C)]
struct EventEndFile {
    reason: c_int,
    error: c_int,
}
#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct OpenGlInitParams {
    get_proc_address: unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}
#[repr(C)]
struct OpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

const EVENT_SHUTDOWN: c_int = 1;
const EVENT_LOG_MESSAGE: c_int = 2;
const EVENT_END_FILE: c_int = 7;
const EVENT_PROPERTY_CHANGE: c_int = 22;
const FORMAT_FLAG: c_int = 3;
const FORMAT_DOUBLE: c_int = 5;
const RENDER_PARAM_INVALID: c_int = 0;
const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const RENDER_PARAM_OPENGL_FBO: c_int = 3;
const RENDER_PARAM_FLIP_Y: c_int = 4;
const RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
const RENDER_UPDATE_FRAME: u64 = 1;

// Observer IDs, as in the IPC driver.
const OBS_TIME_POS: u64 = 1;
const OBS_PAUSE: u64 = 2;
const OBS_DURATION: u64 = 3;

type UpdateFn = unsafe extern "C" fn(ctx: *mut c_void);

struct Lib {
    _lib: libloading::Library,
    create: unsafe extern "C" fn() -> *mut MpvHandle,
    initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    set_option_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char) -> c_int,
    observe_property: unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int,
    request_log_messages: unsafe extern "C" fn(*mut MpvHandle, *const c_char) -> c_int,
    wait_event: unsafe extern "C" fn(*mut MpvHandle, f64) -> *mut Event,
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    client_api_version: unsafe extern "C" fn() -> std::ffi::c_ulong,
    render_create: unsafe extern "C" fn(*mut *mut MpvRenderCtx, *mut MpvHandle, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(*mut MpvRenderCtx, Option<UpdateFn>, *mut c_void),
    render_update: unsafe extern "C" fn(*mut MpvRenderCtx) -> u64,
    render_render: unsafe extern "C" fn(*mut MpvRenderCtx, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut MpvRenderCtx),
}

impl Lib {
    fn open() -> std::result::Result<Self, String> {
        let names = ["libmpv.so.2", "libmpv.so"];
        let mut last = String::new();
        for name in names {
            // SAFETY: loading libmpv runs its (well-behaved) initialisers only.
            match unsafe { libloading::Library::new(name) } {
                Ok(lib) => return Self::bind(lib).map_err(|e| format!("{name}: {e}")),
                Err(e) => last = e.to_string(),
            }
        }
        Err(last)
    }

    fn bind(lib: libloading::Library) -> std::result::Result<Self, libloading::Error> {
        // SAFETY: each symbol is looked up with its C signature from the libmpv 2.x headers.
        unsafe {
            Ok(Self {
                create: *lib.get(b"mpv_create\0")?,
                initialize: *lib.get(b"mpv_initialize\0")?,
                terminate_destroy: *lib.get(b"mpv_terminate_destroy\0")?,
                set_option_string: *lib.get(b"mpv_set_option_string\0")?,
                command: *lib.get(b"mpv_command\0")?,
                observe_property: *lib.get(b"mpv_observe_property\0")?,
                request_log_messages: *lib.get(b"mpv_request_log_messages\0")?,
                wait_event: *lib.get(b"mpv_wait_event\0")?,
                error_string: *lib.get(b"mpv_error_string\0")?,
                client_api_version: *lib.get(b"mpv_client_api_version\0")?,
                render_create: *lib.get(b"mpv_render_context_create\0")?,
                render_set_update_callback: *lib.get(b"mpv_render_context_set_update_callback\0")?,
                render_update: *lib.get(b"mpv_render_context_update\0")?,
                render_render: *lib.get(b"mpv_render_context_render\0")?,
                render_free: *lib.get(b"mpv_render_context_free\0")?,
                _lib: lib,
            })
        }
    }

    fn check(&self, what: &str, code: c_int) -> Result<()> {
        if code >= 0 {
            return Ok(());
        }
        // SAFETY: mpv_error_string returns a static string for any code.
        let msg = unsafe { CStr::from_ptr((self.error_string)(code)) }.to_string_lossy();
        Err(LibmpvError::Call(format!("{what}: {msg}")))
    }
}

static LIB: OnceLock<std::result::Result<Lib, String>> = OnceLock::new();

fn lib() -> Result<&'static Lib> {
    LIB.get_or_init(Lib::open).as_ref().map_err(|e| LibmpvError::NotLoaded(e.clone()))
}

/// Load libmpv now. `Ok("2.5")` (its client API version) when in-window
/// playback is possible.
pub fn load() -> Result<String> {
    let lib = lib()?;
    // SAFETY: no arguments; returns a packed version number.
    let v = unsafe { (lib.client_api_version)() };
    if v >> 16 != 2 {
        return Err(LibmpvError::NotLoaded(format!("unsupported client API {}.{}", v >> 16, v & 0xffff)));
    }
    Ok(format!("{}.{}", v >> 16, v & 0xffff))
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).expect("no NULs left")
}

// ── The mpv core ─────────────────────────────────────────────────────

/// One mpv core. Shared by the player, its event thread and the render context;
/// the core is destroyed when the last one is dropped.
pub struct Handle {
    raw: *mut MpvHandle,
    lib: &'static Lib,
    /// The file to play, loaded by [`Handle::start`]. With `vo=libmpv` that has to
    /// wait for the render context: video fails to start without one.
    pending: Mutex<Option<String>>,
}

// SAFETY: the libmpv client API is thread-safe for everything used here
// (commands, options, waiting for events from one thread).
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    /// Load the file (once; later calls do nothing).
    fn start(&self) -> Result<()> {
        let url = self.pending.lock().unwrap_or_else(|e| e.into_inner()).take();
        match url {
            Some(url) => self.command(&["loadfile", &url]),
            None => Ok(()),
        }
    }

    fn set_option(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstring(name), cstring(value));
        // SAFETY: valid handle and NUL-terminated strings for the duration of the call.
        let code = unsafe { (self.lib.set_option_string)(self.raw, n.as_ptr(), v.as_ptr()) };
        self.lib.check(&format!("option {name}={value}"), code)
    }

    fn command(&self, args: &[&str]) -> Result<()> {
        let owned: Vec<CString> = args.iter().map(|a| cstring(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        // SAFETY: NULL-terminated array of C strings that outlive the call.
        let code = unsafe { (self.lib.command)(self.raw, ptrs.as_mut_ptr()) };
        self.lib.check(&format!("command {}", args.first().unwrap_or(&"")), code)
    }

    fn observe(&self, id: u64, name: &str, format: c_int) -> Result<()> {
        let n = cstring(name);
        // SAFETY: valid handle; mpv copies the name.
        let code = unsafe { (self.lib.observe_property)(self.raw, id, n.as_ptr(), format) };
        self.lib.check(&format!("observe {name}"), code)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: last reference; any render context was freed before (it holds a Handle).
        unsafe { (self.lib.terminate_destroy)(self.raw) };
        debug!("libmpv core destroyed");
    }
}

/// Turn one libmpv event into ours. `None` = not interesting.
///
/// # Safety
/// `ev` must be the event mpv_wait_event just returned on this thread.
unsafe fn translate(ev: &Event) -> Option<MpvEvent> {
    unsafe {
        match ev.event_id {
            EVENT_SHUTDOWN => Some(MpvEvent::Exited),
            EVENT_PROPERTY_CHANGE => {
                let p = &*(ev.data as *const EventProperty);
                if p.data.is_null() {
                    return None; // no value yet (before the file loads)
                }
                match (ev.reply_userdata, p.format) {
                    (OBS_TIME_POS, FORMAT_DOUBLE) => Some(MpvEvent::Position(*(p.data as *const f64))),
                    (OBS_DURATION, FORMAT_DOUBLE) => Some(MpvEvent::Duration(*(p.data as *const f64))),
                    (OBS_PAUSE, FORMAT_FLAG) => Some(MpvEvent::Paused(*(p.data as *const c_int) != 0)),
                    _ => None,
                }
            }
            EVENT_END_FILE => {
                let e = &*(ev.data as *const EventEndFile);
                let reason = match e.reason {
                    0 => "eof",
                    2 => "stop",
                    3 => "quit",
                    4 => "error",
                    5 => "redirect",
                    _ => "unknown",
                };
                Some(MpvEvent::Ended { reason: reason.into() })
            }
            EVENT_LOG_MESSAGE => {
                let m = &*(ev.data as *const EventLogMessage);
                let s = |p: *const c_char| {
                    if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
                };
                (s(m.level) == "error")
                    .then(|| MpvEvent::Error { prefix: s(m.prefix), text: s(m.text).trim().to_owned() })
            }
            _ => None,
        }
    }
}

/// mpv's own log lines (MPV_LOG) into ours, under the `mpv` target.
///
/// # Safety
/// `m` must point to a live mpv_event_log_message.
unsafe fn log_through(m: &EventLogMessage) {
    let s = |p: *const c_char| {
        if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
    };
    let (level, prefix, text) = (s(m.level), s(m.prefix), s(m.text));
    let text = text.trim_end();
    match level.as_str() {
        "fatal" | "error" => warn!(target: "mpv", "[{prefix}] {text}"),
        "warn" => info!(target: "mpv", "[{prefix}] {text}"),
        _ => debug!(target: "mpv", "[{prefix}] {text}"),
    }
}

// ── The player ───────────────────────────────────────────────────────

/// mpv playing inside the tvdash window. The controller's side.
pub struct Embedded {
    handle: Arc<Handle>,
    events: mpsc::UnboundedReceiver<MpvEvent>,
    exited: bool,
}

impl Embedded {
    /// Start playing `url`, drawing through a render context (`vo=libmpv`).
    pub fn spawn(opts: &MpvOptions, url: &str) -> Result<Self> {
        Self::spawn_with(opts, url, &[])
    }

    /// Like [`Embedded::spawn`], with extra mpv options (tests use `vo=null`).
    pub fn spawn_with(opts: &MpvOptions, url: &str, extra: &[(&str, &str)]) -> Result<Self> {
        let lib = lib()?;
        // SAFETY: plain constructor; NULL on out-of-memory.
        let raw = unsafe { (lib.create)() };
        if raw.is_null() {
            return Err(LibmpvError::Call("mpv_create failed".into()));
        }
        let handle = Arc::new(Handle { raw, lib, pending: Mutex::new(Some(url.to_owned())) });
        let title = opts.title.clone();
        let start = opts.start_ms.filter(|&ms| ms > 0).map(|ms| format!("{:.3}", ms as f64 / 1000.0));
        let mut options: Vec<(&str, &str)> = vec![
            ("vo", "libmpv"),
            ("hwdec", "auto-safe"),
            ("keep-open", "no"),
            // Wait for the file (it loads once the render context exists); the event
            // thread quits the core when it ends.
            ("idle", "yes"),
            // tvdash handles the keys and the mouse.
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
            ("terminal", "no"),
            ("audio-client-name", "tvdash"),
            ("title", &title),
            ("force-media-title", &title),
            // Overlay styling for TV distance (bottom-left, large), as with mpv's window.
            ("osd-font-size", "44"),
            ("osd-border-size", "2"),
            ("osd-align-x", "left"),
            ("osd-align-y", "bottom"),
            ("osd-margin-x", "72"),
            ("osd-margin-y", "64"),
            ("osd-duration", "5000"),
        ];
        if let Some(s) = &start {
            options.push(("start", s));
        }
        options.extend_from_slice(extra);
        for (name, value) in options {
            handle.set_option(name, value)?;
        }
        // SAFETY: freshly created handle, options set.
        lib.check("initialize", unsafe { (lib.initialize)(handle.raw) })?;
        handle.observe(OBS_TIME_POS, "time-pos", FORMAT_DOUBLE)?;
        handle.observe(OBS_PAUSE, "pause", FORMAT_FLAG)?;
        handle.observe(OBS_DURATION, "duration", FORMAT_DOUBLE)?;
        // Errors become events (e.g. "Failed to initialize a decoder for codec 'hevc'");
        // MPV_LOG=v (or info, warn, debug…) also passes mpv's own log to ours.
        let level = cstring(&std::env::var("MPV_LOG").unwrap_or_else(|_| "error".into()));
        // SAFETY: valid handle and string.
        lib.check("log messages", unsafe { (lib.request_log_messages)(handle.raw, level.as_ptr()) })?;

        let (tx, events) = mpsc::unbounded_channel();
        let waiter = handle.clone();
        std::thread::Builder::new()
            .name("tvdash-mpv-events".into())
            .spawn(move || {
                loop {
                    // SAFETY: only this thread waits on the handle; the event stays valid until the next wait.
                    let ev = unsafe { &*(waiter.lib.wait_event)(waiter.raw, -1.0) };
                    if ev.event_id == EVENT_LOG_MESSAGE {
                        // SAFETY: a log message event's data is an mpv_event_log_message.
                        unsafe { log_through(&*(ev.data as *const EventLogMessage)) };
                    }
                    // SAFETY: `ev` was just returned by mpv_wait_event on this thread.
                    if let Some(e) = unsafe { translate(ev) } {
                        let last = e == MpvEvent::Exited;
                        // One file per core: when it's over, so is the core (then Exited follows).
                        let quit = matches!(&e, MpvEvent::Ended { reason } if reason != "redirect");
                        if tx.send(e).is_err() && !last {
                            // Nobody listens any more: shut the core down.
                            let _ = waiter.command(&["quit"]);
                        }
                        if last {
                            break;
                        }
                        if quit {
                            let _ = waiter.command(&["quit"]);
                        }
                    }
                }
            })
            .map_err(|e| LibmpvError::Call(format!("event thread: {e}")))?;

        info!("libmpv ready; playing once the window draws");
        Ok(Self { handle, events, exited: false })
    }

    /// Start playing now. The render context does this when it's created; call it
    /// yourself only without one (`vo=null`).
    pub fn start(&self) -> Result<()> {
        self.handle.start()
    }

    /// The core, for the render context ([`VideoSlot::attach`]).
    pub fn handle(&self) -> Arc<Handle> {
        self.handle.clone()
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

    pub fn set_pause(&self, paused: bool) -> Result<()> {
        self.handle.command(&["set", "pause", if paused { "yes" } else { "no" }])
    }

    pub fn seek(&self, seconds: i64) -> Result<()> {
        self.handle.command(&["seek", &seconds.to_string(), "relative"])
    }

    pub fn seek_to(&self, seconds: u64) -> Result<()> {
        self.handle.command(&["seek", &seconds.to_string(), "absolute"])
    }

    pub fn cycle_audio(&self) -> Result<()> {
        self.handle.command(&["cycle", "audio"])
    }

    pub fn cycle_sub(&self) -> Result<()> {
        self.handle.command(&["cycle", "sub"])
    }

    pub fn add_volume(&self, delta: i64) -> Result<()> {
        self.handle.command(&["add", "volume", &delta.to_string()])
    }

    /// OSD text on the video; `duration_ms: None` keeps it up until replaced.
    pub fn show_text(&self, text: &str, duration_ms: Option<u32>) -> Result<()> {
        let ms = duration_ms.unwrap_or(86_400_000);
        self.handle.command(&["show-text", text, &ms.to_string()])
    }

    /// Ask mpv to quit and wait (up to 3 s) for the core to shut down.
    pub async fn quit(mut self) {
        if self.exited {
            return;
        }
        if let Err(e) = self.handle.command(&["quit"]) {
            warn!(error = %e, "libmpv quit failed");
        }
        let wait = async {
            while let Some(ev) = self.events.recv().await {
                if ev == MpvEvent::Exited {
                    break;
                }
            }
        };
        if tokio::time::timeout(Duration::from_secs(3), wait).await.is_err() {
            warn!("libmpv didn't shut down within 3 s");
        }
    }
}

// ── Handing the player to the UI ─────────────────────────────────────

/// Where the controller puts the playing core for the UI thread to draw.
/// `notify` (set by the UI) wakes the UI whenever it changes.
#[derive(Default)]
pub struct VideoSlot {
    current: Mutex<Option<(u64, Arc<Handle>)>>,
    next_id: AtomicU64,
    notify: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl VideoSlot {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The UI's wake-up (called from any thread).
    pub fn set_notify(&self, f: impl Fn() + Send + Sync + 'static) {
        let _ = self.notify.set(Box::new(f));
    }

    fn changed(&self) {
        if let Some(f) = self.notify.get() {
            f();
        }
    }

    pub fn attach(&self, handle: Arc<Handle>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some((id, handle));
        self.changed();
    }

    pub fn detach(&self) {
        let old = self.current.lock().unwrap_or_else(|e| e.into_inner()).take();
        if old.is_some() {
            self.changed();
        }
    }

    /// (id, core) of what's playing; the id changes with each new playback.
    pub fn current(&self) -> Option<(u64, Arc<Handle>)> {
        self.current.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

// ── Drawing ──────────────────────────────────────────────────────────

/// Draws a core's video with OpenGL. Create, use and drop it on the UI thread,
/// with the GL context current.
pub struct RenderContext {
    raw: *mut MpvRenderCtx,
    lib: &'static Lib,
    update: *mut Box<dyn Fn() + Send + Sync>,
    _handle: Arc<Handle>,
}

unsafe extern "C" fn proc_address(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: `ctx` is the `&&dyn Fn` passed to mpv_render_context_create, which only
    // looks up functions during that call.
    unsafe {
        let f = &*(ctx as *const &dyn Fn(&CStr) -> *const c_void);
        f(CStr::from_ptr(name)) as *mut c_void
    }
}

unsafe extern "C" fn update_trampoline(ctx: *mut c_void) {
    // SAFETY: `ctx` is the boxed callback owned by the RenderContext, freed only
    // after mpv_render_context_free (after which mpv no longer calls this).
    unsafe { (*(ctx as *const Box<dyn Fn() + Send + Sync>))() }
}

impl RenderContext {
    /// `get_proc_address` is the GL loader (from Slint's rendering notifier).
    /// `on_update` runs on an mpv thread when a new frame is ready: it must only
    /// wake the UI (no libmpv calls).
    pub fn new(
        handle: Arc<Handle>,
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        on_update: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let lib = handle.lib;
        let api = cstring("opengl");
        let mut loader: &dyn Fn(&CStr) -> *const c_void = get_proc_address;
        let mut init = OpenGlInitParams {
            get_proc_address: proc_address,
            get_proc_address_ctx: &mut loader as *mut &dyn Fn(&CStr) -> *const c_void as *mut c_void,
        };
        let mut params = [
            RenderParam { kind: RENDER_PARAM_API_TYPE, data: api.as_ptr() as *mut c_void },
            RenderParam { kind: RENDER_PARAM_OPENGL_INIT_PARAMS, data: &mut init as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        let mut raw = std::ptr::null_mut();
        // SAFETY: params are valid for the call; the GL context is current (caller's contract).
        let code = unsafe { (lib.render_create)(&mut raw, handle.raw, params.as_mut_ptr()) };
        lib.check("render context", code)?;
        let update: *mut Box<dyn Fn() + Send + Sync> = Box::into_raw(Box::new(Box::new(on_update)));
        // SAFETY: `update` stays alive until after mpv_render_context_free.
        unsafe { (lib.render_set_update_callback)(raw, Some(update_trampoline), update as *mut c_void) };
        let ctx = Self { raw, lib, update, _handle: handle.clone() };
        handle.start()?; // video can start now
        Ok(ctx)
    }

    /// True when a new frame should be drawn (call after `on_update` fired).
    pub fn wants_frame(&self) -> bool {
        // SAFETY: valid context, UI thread.
        unsafe { (self.lib.render_update)(self.raw) & RENDER_UPDATE_FRAME != 0 }
    }

    /// Draw the current frame into framebuffer `fbo` (`w` × `h` pixels), top row
    /// first in memory (what a texture shown with a top-left origin needs).
    pub fn render(&self, fbo: u32, w: i32, h: i32) -> Result<()> {
        let mut target = OpenGlFbo { fbo: fbo as c_int, w, h, internal_format: 0 };
        let mut flip: c_int = 0;
        // Don't hold up the UI thread until the frame's display time.
        let mut block: c_int = 0;
        let mut params = [
            RenderParam { kind: RENDER_PARAM_OPENGL_FBO, data: &mut target as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_FLIP_Y, data: &mut flip as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_BLOCK_FOR_TARGET_TIME, data: &mut block as *mut _ as *mut c_void },
            RenderParam { kind: RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        // SAFETY: valid context and params, GL context current.
        let code = unsafe { (self.lib.render_render)(self.raw, params.as_mut_ptr()) };
        self.lib.check("render", code)
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        // SAFETY: same thread as creation (caller's contract); after this mpv no
        // longer calls `on_update`, so the box can go.
        unsafe {
            (self.lib.render_free)(self.raw);
            drop(Box::from_raw(self.update));
        }
    }
}

#[cfg(test)]
mod tests {
    //! Against the real libmpv (`apt install libmpv2` / `dnf install mpv-libs`),
    //! with no video or audio output. Run with `--ignored`.

    use super::*;

    const CLIP: &str = "av://lavfi:testsrc=duration=4:size=320x240:rate=25";

    fn opts(start_ms: Option<u64>) -> MpvOptions {
        MpvOptions { binary: String::new(), screen_name: None, title: "Test".into(), start_ms, fullscreen: true }
    }

    async fn until(p: &mut Embedded, mut want: impl FnMut(&MpvEvent) -> bool) -> MpvEvent {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let ev = p.next_event().await.expect("events ended early");
                if want(&ev) {
                    return ev;
                }
            }
        })
        .await
        .expect("timed out")
    }

    #[tokio::test]
    #[ignore = "needs libmpv; run with --ignored"]
    async fn plays_pauses_seeks_and_ends() {
        load().expect("libmpv");
        let null = [("vo", "null"), ("ao", "null")];
        let mut p = Embedded::spawn_with(&opts(Some(1000)), CLIP, &null).unwrap();
        p.start().unwrap();
        until(&mut p, |e| matches!(e, MpvEvent::Duration(_))).await; // lavfi reports one frame's length
        until(&mut p, |e| matches!(e, MpvEvent::Position(s) if *s >= 1.0)).await; // started at the resume point
        p.set_pause(true).unwrap();
        until(&mut p, |e| *e == MpvEvent::Paused(true)).await;
        p.show_text("overlay", Some(1000)).unwrap();
        p.seek(1).unwrap();
        p.set_pause(false).unwrap();
        until(&mut p, |e| *e == MpvEvent::Paused(false)).await;
        assert_eq!(
            until(&mut p, |e| matches!(e, MpvEvent::Ended { .. })).await,
            MpvEvent::Ended { reason: "eof".into() }
        );
        assert_eq!(until(&mut p, |_| true).await, MpvEvent::Exited, "the core shuts down after the file");
        assert_eq!(p.next_event().await, None);
    }

    #[tokio::test]
    #[ignore = "needs libmpv; run with --ignored"]
    async fn quit_and_slot() {
        load().expect("libmpv");
        let p = Embedded::spawn_with(&opts(None), CLIP, &[("vo", "null"), ("ao", "null")]).unwrap();
        p.start().unwrap();
        p.start().unwrap(); // only the first loads
        let slot = VideoSlot::new();
        let woke = Arc::new(AtomicU64::new(0));
        let w = woke.clone();
        slot.set_notify(move || {
            w.fetch_add(1, Ordering::Relaxed);
        });
        slot.attach(p.handle());
        let (id, _) = slot.current().unwrap();
        slot.detach();
        slot.detach(); // nothing attached: no wake-up
        assert_eq!((id, woke.load(Ordering::Relaxed), slot.current().is_none()), (1, 2, true));
        tokio::time::timeout(Duration::from_secs(5), p.quit()).await.expect("quit in time");
    }

    #[test]
    #[ignore = "needs libmpv; run with --ignored"]
    fn bad_option_is_an_error() {
        load().expect("libmpv");
        let err = Embedded::spawn_with(&opts(None), CLIP, &[("vo", "null"), ("no-such-option", "1")]).err().unwrap();
        assert!(err.to_string().contains("no-such-option"), "{err}");
    }
}
