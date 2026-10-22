//! Worker-local OpenGL rendering with a bounded, asynchronous CPU readback ring.
//! This is not zero-copy: each delivered frame is downloaded once as RGBA8.
use super::{
    ffi::Param, library::Client, platform::GlContext, render::RenderedFrame, software::Frame,
};
use crate::{Error, Result};
use glow::HasContext;
use std::{
    ffi::{CStr, c_char, c_void},
    rc::Rc,
    sync::Arc,
    thread::{self, Thread},
    time::Instant,
};

#[derive(Clone)]
pub(super) struct GpuStats {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    /// CPU wall time, including any libmpv audio/video timing waits.
    pub render_call_ms: f64,
    /// CPU wall time for readback binds/pack state, glReadPixels, fence creation,
    /// flush and submission error checks. Excludes mpv rendering and later poll/copy.
    pub readback_submit_ms: f64,
    pub readback_copy_ms: f64,
    pub fence_latency_ms: f64,
    /// Cumulative bytes actually copied from mapped GPU buffers.
    pub readback_bytes: u64,
}

#[repr(C)]
struct OpenGLInit {
    get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: i32,
    w: i32,
    h: i32,
    internal_format: i32,
}

unsafe extern "C" fn get_proc_address(data: *mut c_void, name: *const c_char) -> *mut c_void {
    if data.is_null() || name.is_null() {
        return std::ptr::null_mut();
    }
    // The boxed context stays stable through mpv free. Contain even an unexpected
    // platform lookup panic: unwinding across libmpv's C callback would be UB.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        (&*data.cast::<GlContext>())
            .get_proc_address(CStr::from_ptr(name))
            .cast_mut()
    }))
    .unwrap_or(std::ptr::null_mut())
}
unsafe extern "C" fn wake(data: *mut c_void) {
    if !data.is_null() {
        unsafe { &*data.cast::<Thread>() }.unpark();
    }
}

struct MpvContext {
    client: Arc<Client>,
    ptr: *mut super::ffi::RenderContext,
    wake: Box<Thread>,
}
impl Drop for MpvContext {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.client.loaded.api.render_callback)(self.ptr, None, std::ptr::null_mut());
                (self.client.loaded.api.render_free)(self.ptr);
            }
        }
        // free synchronizes callbacks before the boxed Thread is released.
    }
}

struct Target {
    gl: Rc<glow::Context>,
    texture: Option<glow::NativeTexture>,
    fbo: Option<glow::NativeFramebuffer>,
    size: [u32; 2],
}
impl Target {
    fn new(gl: Rc<glow::Context>, size: [u32; 2]) -> Result<Self> {
        let mut target = Self {
            gl,
            texture: None,
            fbo: None,
            size,
        };
        unsafe {
            target.texture = Some(target.gl.create_texture().map_err(Error::Gpu)?);
            target.fbo = Some(target.gl.create_framebuffer().map_err(Error::Gpu)?);
            target.gl.bind_texture(glow::TEXTURE_2D, target.texture);
            target.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            target.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            target.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            target.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            target.gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            target.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size[0] as i32,
                size[1] as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            target.gl.bind_framebuffer(glow::FRAMEBUFFER, target.fbo);
            target.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                target.texture,
                0,
            );
            let status = target.gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(Error::Gpu(format!(
                    "RGBA8 framebuffer incomplete: 0x{status:04x}"
                )));
            }
            gl_error(&target.gl, "allocate RGBA8 framebuffer")?;
            target.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            target.gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(target)
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        unsafe {
            if let Some(fbo) = self.fbo.take() {
                self.gl.delete_framebuffer(fbo);
            }
            if let Some(texture) = self.texture.take() {
                self.gl.delete_texture(texture);
            }
        }
    }
}

struct Submission {
    fence: glow::NativeFence,
    generation: u64,
    size: [u32; 2],
    created_at: Instant,
    submitted_at: Instant,
    sequence: u64,
}
struct Slot {
    gl: Rc<glow::Context>,
    pbo: glow::NativeBuffer,
    submission: Option<Submission>,
}
impl Slot {
    fn new(gl: Rc<glow::Context>, bytes: usize) -> Result<Self> {
        let pbo = unsafe { gl.create_buffer() }.map_err(Error::Gpu)?;
        let slot = Self {
            gl,
            pbo,
            submission: None,
        };
        unsafe {
            slot.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(pbo));
            slot.gl
                .buffer_data_size(glow::PIXEL_PACK_BUFFER, bytes as i32, glow::STREAM_READ);
            slot.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
        }
        gl_error(&slot.gl, "allocate pixel pack buffer")?;
        Ok(slot)
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        unsafe {
            if let Some(submission) = self.submission.take() {
                self.gl.delete_sync(submission.fence);
            }
            self.gl.delete_buffer(self.pbo);
        }
    }
}

// Field order is intentional: mpv, then all GL objects, then glow, then native context.
// Rc additionally keeps glow alive through partial construction failures. !Send/!Sync.
pub(super) struct GpuRenderer {
    mpv: MpvContext,
    target: Option<Target>,
    slots: Vec<Slot>,
    gl: Rc<glow::Context>,
    platform: Box<GlContext>,
    // glow 0.16 discards glUnmapBuffer's GLboolean. Keep the ABI-correct raw
    // entrypoint so corrupted/invalidated readbacks cannot become delivered frames.
    unmap_buffer: unsafe extern "system" fn(u32) -> u8,
    max_texture_size: u32,
    max_frame_bytes: usize,
    generation: Option<u64>,
    sequence: u64,
    stats: GpuStats,
}

impl GpuRenderer {
    pub(super) fn new(client: Arc<Client>, config: &crate::Config) -> Result<Self> {
        let platform = Box::new(GlContext::new(config)?);
        // glow panics on missing entrypoints; validate every function we call first.
        for name in REQUIRED_PROCS {
            let name = CStr::from_bytes_with_nul(name).map_err(|e| Error::Gpu(e.to_string()))?;
            if platform.get_proc_address(name).is_null() {
                return Err(Error::Gpu(format!(
                    "required GL entrypoint {} is unavailable",
                    name.to_string_lossy()
                )));
            }
        }
        let unmap_address = platform.get_proc_address(c"glUnmapBuffer");
        if unmap_address.is_null() {
            return Err(Error::Gpu(
                "required GL entrypoint glUnmapBuffer is unavailable".into(),
            ));
        }
        // SAFETY: the native platform resolved the validated OpenGL entrypoint.
        // GLenum is u32, GLboolean is u8, and APIENTRY uses the system ABI.
        let unmap_buffer = unsafe {
            std::mem::transmute::<*const c_void, unsafe extern "system" fn(u32) -> u8>(
                unmap_address,
            )
        };
        let gl = Rc::new(unsafe {
            glow::Context::from_loader_function(|name| {
                // GL symbol names contain no embedded nul. Avoid unwinding through any callback.
                std::ffi::CString::new(name)
                    .ok()
                    .map_or(std::ptr::null(), |name| platform.get_proc_address(&name))
            })
        });
        let (vendor, renderer, version, major, max_texture_size) = unsafe {
            (
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION),
                gl.get_parameter_i32(glow::MAJOR_VERSION),
                gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE),
            )
        };
        gl_error(&gl, "query GL capabilities")?;
        if major < 3 || max_texture_size <= 0 {
            return Err(Error::Gpu(format!(
                "OpenGL 3 / GLES 3 and valid texture limits required (version: {version})"
            )));
        }
        if platform.is_gles() != gl.version().is_embedded {
            return Err(Error::Gpu(
                "native context and GL API profile disagree".into(),
            ));
        }
        if software_renderer(&format!("{vendor} {renderer}")) {
            return Err(Error::Gpu(format!(
                "software GL implementation rejected: {vendor} / {renderer}"
            )));
        }
        // Check FBO capability before mpv initialization, so Auto can safely fall back.
        let target = Target::new(gl.clone(), [1, 1])?;
        let mut this = Self {
            mpv: MpvContext {
                client,
                ptr: std::ptr::null_mut(),
                wake: Box::new(thread::current()),
            },
            target: Some(target),
            slots: Vec::with_capacity(2),
            gl,
            platform,
            unmap_buffer,
            max_texture_size: max_texture_size as u32,
            max_frame_bytes: config.max_frame_bytes,
            generation: None,
            sequence: 0,
            stats: GpuStats {
                vendor,
                renderer,
                version,
                render_call_ms: 0.0,
                readback_submit_ms: 0.0,
                readback_copy_ms: 0.0,
                fence_latency_ms: 0.0,
                readback_bytes: 0,
            },
        };
        // Exercise PBO allocation and fence entrypoints before reporting successful startup.
        this.slots.push(Slot::new(this.gl.clone(), 4)?);
        this.slots.push(Slot::new(this.gl.clone(), 4)?);
        let fence = unsafe { this.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) }
            .map_err(Error::Gpu)?;
        unsafe {
            this.gl.delete_sync(fence);
        }
        gl_error(&this.gl, "validate readback capabilities")?;
        let mut init = OpenGLInit {
            get_proc_address,
            get_proc_address_ctx: (&mut *this.platform as *mut GlContext).cast(),
        };
        let mut params = vec![
            Param::new(1, c"opengl".as_ptr().cast_mut().cast()),
            Param::new(2, (&mut init as *mut OpenGLInit).cast()),
        ];
        if let Some((kind, display)) = this.platform.native_display() {
            if !matches!(kind, 8 | 9) || display.is_null() {
                return Err(Error::Gpu("invalid native display render parameter".into()));
            }
            params.push(Param::new(kind, display));
        }
        params.push(Param::end());
        let code = unsafe {
            (this.mpv.client.loaded.api.render_create)(
                &mut this.mpv.ptr,
                this.mpv.client.handle,
                params.as_mut_ptr(),
            )
        };
        this.mpv
            .client
            .loaded
            .check(code, "create OpenGL render context")?;
        if this.mpv.ptr.is_null() {
            return Err(Error::Gpu(
                "libmpv returned a null OpenGL render context".into(),
            ));
        }
        unsafe {
            (this.mpv.client.loaded.api.render_callback)(
                this.mpv.ptr,
                Some(wake),
                (&*this.mpv.wake as *const Thread).cast_mut().cast(),
            );
        }
        Ok(this)
    }

    pub(super) fn update(&self) -> u64 {
        unsafe { (self.mpv.client.loaded.api.render_update)(self.mpv.ptr) }
    }

    pub(super) fn render(&mut self, size: [u32; 2], generation: u64) -> Result<bool> {
        let bytes = frame_bytes(size, self.max_texture_size, self.max_frame_bytes)?;
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.size != size)
            || self.generation != Some(generation)
        {
            // Deleting in-flight GL objects defers their storage reclamation until commands
            // finish; no CPU fence wait and no stale-generation downloads are necessary.
            self.slots.clear();
            self.target = None;
            self.target = Some(Target::new(self.gl.clone(), size)?);
            self.slots.push(Slot::new(self.gl.clone(), bytes)?);
            self.slots.push(Slot::new(self.gl.clone(), bytes)?);
            self.generation = Some(generation);
        }
        let Some(index) = free_slot(self.slots.iter().map(|slot| slot.submission.is_some())) else {
            // libmpv must still consume this update, preserving timing without a readback.
            self.skip()?;
            return Ok(false);
        };
        let target = self
            .target
            .as_ref()
            .ok_or_else(|| Error::Gpu("missing render target".into()))?;
        let mut fbo = Fbo {
            fbo: target
                .fbo
                .ok_or_else(|| Error::Gpu("missing framebuffer".into()))?
                .0
                .get() as i32,
            w: size[0] as i32,
            h: size[1] as i32,
            internal_format: glow::RGBA8 as i32,
        };
        // This is an offscreen FBO, not a GL default framebuffer. With FLIP_Y=0,
        // libmpv puts the image's top row at the FBO's low-y edge. glReadPixels
        // reads low-y first, directly producing Frame's top-to-bottom RGBA order.
        // Do not flip again on the CPU; native_color_orientation verifies corners.
        let mut flip_y = 0_i32;
        let mut params = [
            Param::new(3, (&mut fbo as *mut Fbo).cast()),
            Param::new(4, (&mut flip_y as *mut i32).cast()),
            Param::end(),
        ];
        let created_at = Instant::now();
        let code =
            unsafe { (self.mpv.client.loaded.api.render)(self.mpv.ptr, params.as_mut_ptr()) };
        self.stats.render_call_ms = created_at.elapsed().as_secs_f64() * 1000.0;
        self.mpv.client.loaded.check(code, "OpenGL render")?;
        gl_error(&self.gl, "libmpv OpenGL render")?;
        let submit_started = Instant::now();
        let submit_result = (|| -> Result<()> {
            let slot = &mut self.slots[index];
            unsafe {
                self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, target.fbo);
                self.gl.read_buffer(glow::COLOR_ATTACHMENT0);
                self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(slot.pbo));
                self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
                self.gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
                self.gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 0);
                self.gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, 0);
                self.gl.read_pixels(
                    0,
                    0,
                    size[0] as i32,
                    size[1] as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::BufferOffset(0),
                );
                self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
                self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            }
            gl_error(&self.gl, "submit RGBA8 readback")?;
            let fence = unsafe { self.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) }
                .map_err(Error::Gpu)?;
            slot.submission = Some(Submission {
                fence,
                generation,
                size,
                created_at,
                submitted_at: Instant::now(),
                sequence: self.sequence,
            });
            self.sequence = self.sequence.wrapping_add(1);
            // Flush, not finish: ensure fences progress even during paused playback.
            unsafe {
                self.gl.flush();
            }
            gl_error(&self.gl, "submit readback fence")?;
            // Fence latency begins after submission CPU work (including flush), so
            // the new submission timing and later polling latency do not overlap.
            if let Some(submission) = slot.submission.as_mut() {
                submission.submitted_at = Instant::now();
            }
            Ok(())
        })();
        // Preserve this cost even when a submission operation returns an error.
        self.stats.readback_submit_ms = submit_started.elapsed().as_secs_f64() * 1000.0;
        submit_result?;
        Ok(true)
    }

    pub(super) fn poll(&mut self) -> Result<Option<RenderedFrame>> {
        let result = self.poll_readback();
        if result.is_err() {
            // Every mapping path unbinds before returning. Retire all fences and
            // buffers with this context current; never retain an invalid readback
            // for a later poll, even if the caller inspects/reuses the renderer.
            self.slots.clear();
            self.generation = None;
        }
        result
    }

    fn poll_readback(&mut self) -> Result<Option<RenderedFrame>> {
        let Some(index) = oldest_slot(&self.slots) else {
            return Ok(None);
        };
        let slot = &mut self.slots[index];
        let submission = slot
            .submission
            .as_ref()
            .ok_or_else(|| Error::Gpu("missing readback submission".into()))?;
        let status = unsafe { self.gl.client_wait_sync(submission.fence, 0, 0) };
        match status {
            glow::TIMEOUT_EXPIRED => return Ok(None),
            glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => {}
            _ => return Err(Error::Gpu(format!("readback fence failed: 0x{status:04x}"))),
        }
        self.stats.fence_latency_ms = submission.submitted_at.elapsed().as_secs_f64() * 1000.0;
        let bytes = frame_bytes(submission.size, self.max_texture_size, self.max_frame_bytes)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(bytes)
            .map_err(|_| Error::Allocation)?;
        let copy_started = Instant::now();
        let unmap_result = unsafe {
            self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(slot.pbo));
            let mapped = self.gl.map_buffer_range(
                glow::PIXEL_PACK_BUFFER,
                0,
                bytes as i32,
                glow::MAP_READ_BIT,
            );
            if mapped.is_null() {
                self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
                return Err(Error::Gpu(
                    "completed pixel pack buffer mapped to null".into(),
                ));
            }
            // Allocation was reserved before mapping: copying cannot allocate or panic.
            std::ptr::copy_nonoverlapping(mapped, pixels.as_mut_ptr(), bytes);
            pixels.set_len(bytes);
            let valid = (self.unmap_buffer)(glow::PIXEL_PACK_BUFFER);
            self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            valid
        };
        self.stats.readback_bytes = self.stats.readback_bytes.saturating_add(bytes as u64);
        // GL_FALSE invalidates the entire data store and need not set a GL error.
        // On failure pixels are dropped and poll retires the ring without delivery.
        validate_unmap(unmap_result)?;
        normalize_rgba(&mut pixels);
        self.stats.readback_copy_ms = copy_started.elapsed().as_secs_f64() * 1000.0;
        gl_error(&self.gl, "copy completed RGBA8 readback")?;
        let submission = slot
            .submission
            .take()
            .ok_or_else(|| Error::Gpu("missing completed readback".into()))?;
        unsafe {
            self.gl.delete_sync(submission.fence);
        }
        Ok(Some(RenderedFrame {
            frame: Frame {
                width: submission.size[0],
                height: submission.size[1],
                pixels,
            },
            generation: submission.generation,
            created_at: submission.created_at,
        }))
    }

    pub(super) fn skip(&mut self) -> Result<()> {
        let mut skip = 1_i32;
        let mut params = [Param::new(13, (&mut skip as *mut i32).cast()), Param::end()];
        let started = Instant::now();
        let code =
            unsafe { (self.mpv.client.loaded.api.render)(self.mpv.ptr, params.as_mut_ptr()) };
        self.stats.render_call_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.mpv.client.loaded.check(code, "skip GPU frame")
    }
    pub(super) fn pending(&self) -> bool {
        self.slots.iter().any(|slot| slot.submission.is_some())
    }
    pub(super) fn stats(&self) -> GpuStats {
        self.stats.clone()
    }
}

fn free_slot(occupied: impl IntoIterator<Item = bool>) -> Option<usize> {
    occupied.into_iter().position(|occupied| !occupied)
}
fn oldest_slot(slots: &[Slot]) -> Option<usize> {
    oldest_sequence(
        slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.submission.as_ref().map(|s| (index, s.sequence))),
    )
}
fn oldest_sequence(sequences: impl IntoIterator<Item = (usize, u64)>) -> Option<usize> {
    // Wrapping subtraction orders the two live submissions, including sequence wrap.
    sequences
        .into_iter()
        .reduce(|a, b| {
            if b.1.wrapping_sub(a.1) < (1_u64 << 63) {
                a
            } else {
                b
            }
        })
        .map(|s| s.0)
}
fn frame_bytes(size: [u32; 2], max_texture: u32, max_bytes: usize) -> Result<usize> {
    if size.contains(&0) || size.iter().any(|&n| n > max_texture || n > i32::MAX as u32) {
        return Err(Error::Gpu(format!(
            "invalid render size {}x{} (GL texture limit {max_texture})",
            size[0], size[1]
        )));
    }
    let bytes = (size[0] as usize)
        .checked_mul(size[1] as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| Error::Gpu("RGBA8 render size overflow".into()))?;
    // glow's PBO size/map interfaces accept signed 32-bit byte counts.
    if bytes > max_bytes || bytes > i32::MAX as usize {
        return Err(Error::Gpu(format!(
            "RGBA8 frame requires {bytes} bytes, exceeding configured/GL readback limit ({max_bytes})"
        )));
    }
    Ok(bytes)
}
// Unflipped mpv offscreen-FBO readback is already top-to-bottom RGBA8.
// Preserve row/color order and normalize only the public opaque-alpha contract.
fn normalize_rgba(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}
fn software_renderer(identity: &str) -> bool {
    let identity = identity.to_ascii_lowercase();
    [
        "llvmpipe",
        "softpipe",
        "swiftshader",
        "warp",
        "software rasterizer",
        "software renderer",
        "gdi generic",
        "microsoft basic render",
        "lavapipe",
        "swrast",
    ]
    .iter()
    .any(|name| identity.contains(name))
}
fn validate_unmap(valid: u8) -> Result<()> {
    if valid == 0 {
        Err(Error::Gpu(
            "glUnmapBuffer returned GL_FALSE: pixel pack buffer contents were invalidated; frame discarded. Recreate the player with Cpu rendering if this persists.".into(),
        ))
    } else {
        Ok(())
    }
}
fn gl_error(gl: &glow::Context, operation: &str) -> Result<()> {
    let error = unsafe { gl.get_error() };
    if error == glow::NO_ERROR {
        Ok(())
    } else {
        Err(Error::Gpu(format!("{operation}: GL error 0x{error:04x}")))
    }
}
const REQUIRED_PROCS: &[&[u8]] = &[
    b"glGetString\0",
    b"glGetStringi\0",
    b"glGetIntegerv\0",
    b"glGetError\0",
    b"glGenTextures\0",
    b"glDeleteTextures\0",
    b"glBindTexture\0",
    b"glTexParameteri\0",
    b"glTexImage2D\0",
    b"glGenFramebuffers\0",
    b"glDeleteFramebuffers\0",
    b"glBindFramebuffer\0",
    b"glFramebufferTexture2D\0",
    b"glCheckFramebufferStatus\0",
    b"glReadBuffer\0",
    b"glGenBuffers\0",
    b"glDeleteBuffers\0",
    b"glBindBuffer\0",
    b"glBufferData\0",
    b"glReadPixels\0",
    b"glPixelStorei\0",
    b"glFenceSync\0",
    b"glDeleteSync\0",
    b"glClientWaitSync\0",
    b"glMapBufferRange\0",
    b"glUnmapBuffer\0",
    b"glFlush\0",
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unmap_validity_checked_independently_of_gl_errors() {
        let error = validate_unmap(0).unwrap_err();
        assert!(matches!(error, Error::Gpu(_)));
        assert!(error.to_string().contains("GL_FALSE"));
        assert!(validate_unmap(1).is_ok());
    }
    #[test]
    fn dimension_and_byte_guards() {
        assert_eq!(
            frame_bytes([3840, 2160], 16384, 64 * 1024 * 1024).unwrap(),
            33_177_600
        );
        assert!(frame_bytes([0, 720], 16384, usize::MAX).is_err());
        assert!(frame_bytes([16385, 1], 16384, usize::MAX).is_err());
        assert!(frame_bytes([1920, 1080], 16384, 1024).is_err());
        assert!(frame_bytes([u32::MAX, u32::MAX], u32::MAX, usize::MAX).is_err());
    }
    #[test]
    fn top_down_color_order_preserved_with_opaque_alpha() {
        let mut pixels = vec![
            0, 0, 255, 0, 0, 255, 0, 42, 255, 0, 0, 1, 255, 255, 255, 128,
        ];
        normalize_rgba(&mut pixels);
        assert_eq!(
            pixels,
            [
                0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255
            ]
        );
    }
    #[test]
    fn odd_height_and_single_pixel() {
        let mut pixels = vec![1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0];
        normalize_rgba(&mut pixels);
        assert_eq!(pixels, [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255]);
        normalize_rgba(&mut pixels[..4]);
        assert_eq!(pixels[..4], [1, 2, 3, 255]);
    }
    #[test]
    fn bounded_ring_and_submission_order() {
        assert_eq!(free_slot([false, false]), Some(0));
        assert_eq!(free_slot([true, false]), Some(1));
        assert_eq!(free_slot([true, true]), None);
        assert_eq!(free_slot([false, true]), Some(0));
        assert_eq!(oldest_sequence([]), None);
        assert_eq!(oldest_sequence([(0, 5), (1, 4)]), Some(1));
        assert_eq!(oldest_sequence([(0, 4), (1, 5)]), Some(0));
        assert_eq!(oldest_sequence([(0, 0), (1, u64::MAX)]), Some(1));
        assert_eq!(oldest_sequence([(0, u64::MAX), (1, 0)]), Some(0));
    }
    /// Opt in with: cargo test --no-default-features --features gpu-render
    /// mpv::gpu::tests::native_readback_smoke -- --ignored --nocapture
    /// Requires installed libmpv and a hardware-backed native GL context. This
    /// checks blank-frame readback/resize/teardown, not decoding or media colors.
    #[test]
    #[ignore = "requires native hardware GL and installed libmpv"]
    fn native_readback_smoke() {
        for _ in 0..3 {
            let loaded = super::super::library::Loaded::load(None).expect("load native libmpv");
            let handle = unsafe { (loaded.api.create)() };
            assert!(!handle.is_null(), "mpv_create returned null");
            let client = Arc::new(Client { loaded, handle });
            // All client operations precede render-context creation. The render
            // implementation itself only calls the libmpv render API.
            let code =
                unsafe { (client.loaded.api.option)(handle, c"vo".as_ptr(), c"libmpv".as_ptr()) };
            client.loaded.check(code, "smoke vo option").unwrap();
            let code = unsafe { (client.loaded.api.initialize)(handle) };
            client.loaded.check(code, "smoke initialize").unwrap();
            let mut renderer = GpuRenderer::new(client, &crate::Config::default())
                .unwrap_or_else(|e| panic!("native GPU capability unavailable: {e}"));
            let stats = renderer.stats();
            eprintln!(
                "native GL: {} / {} / {}",
                stats.vendor, stats.renderer, stats.version
            );
            assert!(renderer.render([2, 3], 1).unwrap());
            assert!(renderer.render([2, 3], 1).unwrap());
            assert!(
                !renderer.render([2, 3], 1).unwrap(),
                "ring must remain bounded"
            );
            assert!(renderer.pending());
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            while renderer.pending() {
                if let Some(frame) = renderer.poll().unwrap() {
                    assert_eq!(frame.generation, 1);
                    assert_eq!([frame.frame.width, frame.frame.height], [2, 3]);
                    assert_eq!(frame.frame.pixels.len(), 24);
                    assert!(frame.frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
                }
                assert!(
                    Instant::now() < deadline,
                    "native readback fence did not complete"
                );
                thread::sleep(std::time::Duration::from_millis(1));
            }
            let stats = renderer.stats();
            assert_eq!(stats.readback_bytes, 48);
            assert!(stats.readback_submit_ms.is_finite());
            assert!(stats.readback_submit_ms >= 0.0);
            assert!(renderer.render([2, 3], 1).unwrap());
            assert!(renderer.render([3, 2], 2).unwrap()); // retire pending old-generation slot
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(frame) = renderer.poll().unwrap() {
                    assert_eq!(frame.generation, 2);
                    assert_eq!([frame.frame.width, frame.frame.height], [3, 2]);
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "resized readback fence did not complete"
                );
                thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(renderer.stats().readback_bytes, 72);
        }
    }
    #[test]
    #[ignore = "requires native hardware GL, libmpv and its PPM decoder"]
    fn native_color_orientation() {
        struct TemporaryImage(std::path::PathBuf);
        impl Drop for TemporaryImage {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let image = TemporaryImage(std::env::temp_dir().join(format!(
            "mpv-gpui-orientation-{}-{}.ppm", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )));
        let mut ppm = b"P6\n2 2\n255\n".to_vec();
        ppm.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        std::fs::write(&image.0, ppm).unwrap();
        let loaded = super::super::library::Loaded::load(None).expect("load native libmpv");
        let handle = unsafe { (loaded.api.create)() };
        assert!(!handle.is_null());
        let client = Arc::new(Client { loaded, handle });
        for (name, value) in [
            (c"vo", c"libmpv"),
            (c"pause", c"yes"),
            (c"image-display-duration", c"inf"),
        ] {
            let code = unsafe { (client.loaded.api.option)(handle, name.as_ptr(), value.as_ptr()) };
            client
                .loaded
                .check(code, "native color test option")
                .unwrap();
        }
        let code = unsafe { (client.loaded.api.initialize)(handle) };
        client
            .loaded
            .check(code, "native color test initialize")
            .unwrap();
        let mut renderer = GpuRenderer::new(client.clone(), &crate::Config::default())
            .unwrap_or_else(|e| panic!("native GPU capability unavailable: {e}"));
        // Simulate the real control/render split: no client calls on the GL worker.
        let path = std::ffi::CString::new(image.0.to_str().unwrap()).unwrap();
        thread::spawn(move || {
            let args = [c"loadfile".as_ptr(), path.as_ptr(), std::ptr::null()];
            let code = unsafe { (client.loaded.api.command)(client.handle, 1, args.as_ptr()) };
            client
                .loaded
                .check(code, "native color test loadfile")
                .unwrap();
        })
        .join()
        .unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let mut last_pixels = Vec::new();
        loop {
            if renderer.update() & 1 != 0 {
                renderer.render([2, 2], 1).unwrap();
            }
            if let Some(frame) = renderer.poll().unwrap() {
                last_pixels = frame.frame.pixels;
                let matches = last_pixels
                    .chunks_exact(4)
                    .zip([[255_u8, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]])
                    .all(|(actual, expected)| {
                        actual[3] == 255
                            && actual[..3]
                                .iter()
                                .zip(expected)
                                .all(|(&a, e)| a.abs_diff(e) < 40)
                    });
                if matches {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "native RGBA color/orientation mismatch: {last_pixels:?}"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn software_implementations_rejected() {
        for name in [
            "Mesa llvmpipe",
            "softpipe",
            "Google SwiftShader",
            "ANGLE WARP",
            "GDI Generic",
        ] {
            assert!(software_renderer(name));
        }
        assert!(!software_renderer("AMD Radeon"));
        assert!(!software_renderer("Intel Iris Xe"));
    }
}
