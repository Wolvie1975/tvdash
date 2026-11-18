//! In-window video for the TV-only layout. The controller puts the playing
//! libmpv core in a [`VideoSlot`]; this draws its frames, on the UI thread
//! inside Slint's OpenGL context, into a texture that the TV shell shows as
//! `TvUi.video` behind Home (or full screen).
//!
//! If the render context can't be created (no usable GL), the controller is
//! told (`Command::VideoFailed`) and plays the item in mpv's own window instead.

use std::sync::Arc;

use glow::HasContext;
use slint::{BorrowedOpenGLTextureBuilder, BorrowedOpenGLTextureOrigin, ComponentHandle, GraphicsAPI, RenderingState};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info, warn};
use tvdash_core::Command;
use tvdash_player::libmpv::{RenderContext, VideoSlot};

use crate::{LaptopWindow, TvUi};

/// The texture mpv draws into, at the window's size.
struct Target {
    texture: glow::NativeTexture,
    fbo: glow::NativeFramebuffer,
    w: i32,
    h: i32,
}

#[derive(Default)]
struct Video {
    gl: Option<glow::Context>,
    target: Option<Target>,
    /// The render context and the slot id it belongs to.
    ctx: Option<(u64, RenderContext)>,
    /// A slot id whose render context failed (don't retry it every frame).
    failed: Option<u64>,
}

/// Draw `slot`'s video in `laptop` (the main window). Call once at startup.
pub fn install(laptop: &LaptopWindow, slot: Arc<VideoSlot>, commands: UnboundedSender<Command>) {
    let weak = laptop.as_weak();
    let wake = move || {
        let _ = weak.upgrade_in_event_loop(|l| l.window().request_redraw());
    };
    slot.set_notify(wake.clone());

    let weak = laptop.as_weak();
    let mut v = Video::default();
    let installed = laptop.window().set_rendering_notifier(move |state, api| {
        let Some(laptop) = weak.upgrade() else { return };
        match (state, api) {
            (RenderingState::BeforeRendering, GraphicsAPI::NativeOpenGL { get_proc_address }) => {
                v.before_rendering(&laptop, &slot, get_proc_address, &wake, &commands);
            }
            (RenderingState::RenderingTeardown, _) => v.teardown(),
            _ => {}
        }
    });
    match installed {
        Ok(()) => debug!("in-window video ready"),
        Err(e) => warn!(error = ?e, "no rendering notifier; videos play in mpv's own window"),
    }
}

impl Video {
    fn before_rendering(
        &mut self,
        laptop: &LaptopWindow,
        slot: &VideoSlot,
        get_proc_address: &dyn Fn(&std::ffi::CStr) -> *const std::ffi::c_void,
        wake: &(impl Fn() + Send + Sync + Clone + 'static),
        commands: &UnboundedSender<Command>,
    ) {
        let gl = self.gl.get_or_insert_with(|| {
            // SAFETY: the GL context is current during the notifier.
            unsafe { glow::Context::from_loader_function_cstr(|name| get_proc_address(name)) }
        });
        let ui = laptop.global::<TvUi>();

        // Follow the slot: a new playback, or none.
        let current = slot.current();
        let current_id = current.as_ref().map(|(id, _)| *id);
        if self.ctx.as_ref().map(|(id, _)| *id) != current_id {
            let had = self.ctx.take().is_some(); // frees the render context (UI thread, as required)
            if had {
                drop_target(gl, &mut self.target);
            }
            match current {
                Some((id, handle)) if self.failed != Some(id) => {
                    match RenderContext::new(handle, get_proc_address, wake.clone()) {
                        Ok(ctx) => {
                            info!("video drawing in the window");
                            self.ctx = Some((id, ctx));
                            let on_home = ui.get_screen() == "home";
                            ui.set_video_on(true);
                            ui.set_video_full(!on_home);
                            crate::tv_nav::sync(laptop);
                        }
                        Err(e) => {
                            error!(error = %e, "couldn't draw video in the window");
                            self.failed = Some(id);
                            let _ = commands.send(Command::VideoFailed(e.to_string()));
                        }
                    }
                }
                _ if had => {
                    ui.set_video_on(false);
                    ui.set_video_full(false);
                    ui.set_video(slint::Image::default());
                    crate::tv_nav::sync(laptop);
                }
                _ => {}
            }
        }
        let Some((_, ctx)) = &self.ctx else { return };

        // A texture the size of the window.
        let size = laptop.window().size();
        let (w, h) = (size.width.max(1) as i32, size.height.max(1) as i32);
        let resized = self.target.as_ref().is_none_or(|t| (t.w, t.h) != (w, h));
        if resized {
            drop_target(gl, &mut self.target);
            // SAFETY: GL context current.
            match unsafe { create_target(gl, w, h) } {
                Ok(t) => {
                    // SAFETY: the texture stays alive until replaced or torn down, and the
                    // image is replaced (or cleared) before then.
                    let image = unsafe {
                        BorrowedOpenGLTextureBuilder::new_gl_2d_rgba_texture(t.texture.0, (w as u32, h as u32).into())
                    }
                    .origin(BorrowedOpenGLTextureOrigin::TopLeft)
                    .build();
                    self.target = Some(t);
                    ui.set_video(image);
                }
                Err(e) => {
                    error!(error = %e, "couldn't create the video texture");
                    return;
                }
            }
        }
        let Some(t) = &self.target else { return };

        if ctx.wants_frame() || resized {
            // SAFETY: GL context current; mpv draws into our FBO and we put back
            // what Slint's renderer had bound.
            unsafe {
                let previous = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
                let mut viewport = [0i32; 4];
                gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
                // mpv expects OpenGL's default state; Slint's renderer leaves some of it
                // changed (pixel unpacking for its glyph uploads, blending, scissoring).
                let unpack =
                    [glow::UNPACK_ROW_LENGTH, glow::UNPACK_SKIP_PIXELS, glow::UNPACK_SKIP_ROWS, glow::UNPACK_ALIGNMENT]
                        .map(|p| (p, gl.get_parameter_i32(p)));
                for (p, _) in unpack {
                    gl.pixel_store_i32(p, if p == glow::UNPACK_ALIGNMENT { 4 } else { 0 });
                }
                gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
                gl.active_texture(glow::TEXTURE0);
                gl.disable(glow::BLEND);
                gl.disable(glow::SCISSOR_TEST);
                gl.disable(glow::STENCIL_TEST);
                gl.disable(glow::DEPTH_TEST);
                gl.disable(glow::CULL_FACE);
                gl.color_mask(true, true, true, true);
                if let Err(e) = ctx.render(t.fbo.0.get(), w, h) {
                    warn!(error = %e, "video frame");
                }
                for (p, v) in unpack {
                    gl.pixel_store_i32(p, v);
                }
                let previous = std::num::NonZeroU32::new(previous as u32).map(glow::NativeFramebuffer);
                gl.bind_framebuffer(glow::FRAMEBUFFER, previous);
                gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            }
        }
    }

    /// The GL context is going away: free everything that lives in it.
    fn teardown(&mut self) {
        self.ctx = None;
        if let Some(gl) = &self.gl {
            drop_target(gl, &mut self.target);
        }
        self.gl = None;
    }
}

fn drop_target(gl: &glow::Context, target: &mut Option<Target>) {
    if let Some(old) = target.take() {
        // SAFETY: GL context current; these objects are ours.
        unsafe {
            gl.delete_framebuffer(old.fbo);
            gl.delete_texture(old.texture);
        }
    }
}

/// An RGBA texture with a framebuffer around it.
///
/// # Safety
/// The GL context must be current.
unsafe fn create_target(gl: &glow::Context, w: i32, h: i32) -> Result<Target, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        gl.bind_texture(glow::TEXTURE_2D, None);

        let previous = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        let previous = std::num::NonZeroU32::new(previous as u32).map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, previous);
        if status != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_framebuffer(fbo);
            gl.delete_texture(texture);
            return Err(format!("framebuffer incomplete (0x{status:x})"));
        }
        Ok(Target { texture, fbo, w, h })
    }
}
