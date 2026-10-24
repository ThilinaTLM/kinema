//! GPU surface SDR BGRA8 CGL -> IOSurface -> CVPixelBuffer transport.
//!
//! Native unsafe audit: one worker owns the CGL context and framework library
//! through libmpv render-context destruction. Every frame gets fresh storage;
//! nothing published is ever rendered into again. Create-rule CF/CV references
//! are retained until positive GL completion. GL names are retired on the next
//! begin/shutdown, with another fence after deletion before their owners drop.
//! A failed/expired wait poisons the context and quarantines its entire owner
//! graph until process exit. There is no CPU pixel mapping/readback or Metal
//! device selection here: identity must match GPUI's actual renderer registry ID.
//! This is not native runtime qualification, HDR, or a VideoToolbox claim.
//! Poll deadlines cannot interrupt a driver stuck inside a native API call.

use std::{
    cell::RefCell,
    ffi::{CStr, c_void},
    marker::PhantomData,
    ptr::{NonNull, null, null_mut},
    rc::Rc,
    time::{Duration, Instant},
};

use objc2_core_foundation::CFRetained;
use objc2_core_video::CVPixelBuffer;

use super::{GpuSurfaceContext, NativeFrame};

type Ptr = *mut c_void;
const COMPLETION_LIMIT: Duration = Duration::from_secs(2);
const MAX_RENDERERS: i32 = 256;
// Apple CGLTypes.h attributes/profile/parameters.
const CGL_PFA_OPENGL_PROFILE: i32 = 99;
const CGL_PROFILE_3_2_CORE: i32 = 0x3200;
const CGL_PFA_ACCELERATED: i32 = 73;
const CGL_PFA_NO_RECOVERY: i32 = 72;
const CGL_CP_CURRENT_RENDERER_ID: i32 = 309;
const CGL_RP_RENDERER_ID: i32 = 70;
const CGL_RP_ACCELERATED: i32 = 73;
// CGLTypes.h, available on macOS 10.13+.
const CGL_RP_REGISTRY_ID_LOW: i32 = 140;
const CGL_RP_REGISTRY_ID_HIGH: i32 = 141;
const GL_TEXTURE_RECTANGLE: u32 = 0x84F5;
const GL_RGBA8: u32 = 0x8058;
const GL_BGRA: u32 = 0x80E1;
const GL_UNSIGNED_INT_8_8_8_8_REV: u32 = 0x8367;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_MAX_RECTANGLE_TEXTURE_SIZE: u32 = 0x84F8;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_LINEAR: i32 = 0x2601;
const GL_SYNC_GPU_COMMANDS_COMPLETE: u32 = 0x9117;
const GL_ALREADY_SIGNALED: u32 = 0x911A;
const GL_TIMEOUT_EXPIRED: u32 = 0x911B;
const GL_CONDITION_SATISFIED: u32 = 0x911C;

#[link(name = "OpenGL", kind = "framework")]
unsafe extern "C" {
    fn CGLChoosePixelFormat(attrs: *const i32, format: *mut Ptr, count: *mut i32) -> i32;
    fn CGLDestroyPixelFormat(format: Ptr) -> i32;
    fn CGLCreateContext(format: Ptr, share: Ptr, context: *mut Ptr) -> i32;
    fn CGLDestroyContext(context: Ptr) -> i32;
    fn CGLSetCurrentContext(context: Ptr) -> i32;
    fn CGLGetCurrentContext() -> Ptr;
    fn CGLGetParameter(context: Ptr, parameter: i32, value: *mut i32) -> i32;
    fn CGLQueryRendererInfo(mask: u32, info: *mut Ptr, count: *mut i32) -> i32;
    fn CGLDescribeRenderer(info: Ptr, index: i32, property: i32, value: *mut i32) -> i32;
    fn CGLDestroyRendererInfo(info: Ptr) -> i32;
    fn CGLTexImageIOSurface2D(
        context: Ptr,
        target: u32,
        internal: u32,
        width: i32,
        height: i32,
        format: u32,
        ty: u32,
        surface: Ptr,
        plane: u32,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDictionaryCreateMutable(allocator: Ptr, capacity: isize, keys: Ptr, values: Ptr) -> Ptr;
    fn CFDictionarySetValue(dictionary: Ptr, key: *const c_void, value: *const c_void);
    fn CFNumberCreate(allocator: Ptr, ty: isize, value: *const c_void) -> Ptr;
    fn CFRelease(value: Ptr);
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;
    static kCFBooleanTrue: Ptr;
}
#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    fn IOSurfaceCreate(properties: Ptr) -> Ptr;
    static kIOSurfaceWidth: Ptr;
    static kIOSurfaceHeight: Ptr;
    static kIOSurfaceBytesPerElement: Ptr;
    static kIOSurfacePixelFormat: Ptr;
}
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    // Exact published C ABI; only the opaque IOSurface argument is handwritten,
    // avoiding an additional objc2-io-surface dependency for this narrow bridge.
    fn CVPixelBufferCreateWithIOSurface(
        allocator: Ptr,
        surface: Ptr,
        attributes: Ptr,
        buffer: *mut *mut CVPixelBuffer,
    ) -> i32;
    static kCVPixelBufferMetalCompatibilityKey: Ptr;
    static kCVPixelBufferOpenGLCompatibilityKey: Ptr;
}

macro_rules! gl_api {
    ($($field:ident : $name:literal ($($arg:ty),*) -> $ret:ty;)+) => {
        struct Gl {
            $( $field: unsafe extern "C" fn($($arg),*) -> $ret, )+
            library: libloading::Library,
        }
        impl Gl {
            fn load() -> Result<Self, String> {
                // SAFETY: absolute system framework; signatures are Apple's GL ABI.
                unsafe {
                    let library = libloading::Library::new(
                        "/System/Library/Frameworks/OpenGL.framework/OpenGL"
                    ).map_err(|e| format!("load OpenGL framework: {e}"))?;
                    Ok(Self {
                        $($field: *library.get(concat!($name, "\0").as_bytes())
                            .map_err(|e| format!("load {}: {e}", $name))?,)+
                        library,
                    })
                }
            }
        }
    };
}
gl_api! {
    gen_textures: "glGenTextures" (i32, *mut u32) -> ();
    bind_texture: "glBindTexture" (u32, u32) -> ();
    delete_textures: "glDeleteTextures" (i32, *const u32) -> ();
    tex_parameter: "glTexParameteri" (u32, u32, i32) -> ();
    gen_framebuffers: "glGenFramebuffers" (i32, *mut u32) -> ();
    bind_framebuffer: "glBindFramebuffer" (u32, u32) -> ();
    framebuffer_texture: "glFramebufferTexture2D" (u32, u32, u32, u32, i32) -> ();
    check_framebuffer: "glCheckFramebufferStatus" (u32) -> u32;
    delete_framebuffers: "glDeleteFramebuffers" (i32, *const u32) -> ();
    fence_sync: "glFenceSync" (u32, u32) -> Ptr;
    client_wait_sync: "glClientWaitSync" (Ptr, u32, u64) -> u32;
    delete_sync: "glDeleteSync" (Ptr) -> ();
    flush: "glFlush" () -> ();
    get_integer: "glGetIntegerv" (u32, *mut i32) -> ();
    get_error: "glGetError" () -> u32;
}

/// Create-rule CF owner, used only before submission or inside Native's guard.
struct Cf(Ptr);
impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: exclusively owned Create-rule reference.
            unsafe { CFRelease(self.0) };
        }
    }
}

struct Frame {
    pixel_buffer: CFRetained<CVPixelBuffer>,
    surface: Cf,
    texture: u32,
    framebuffer: u32,
}

struct Native {
    gl: Gl,
    pixel_format: Ptr,
    context: Ptr,
    renderers: Ptr,
    renderer_id: i32,
    max_size: u32,
    active: Option<Frame>,
    retired: Option<Frame>,
    fence: Ptr,
    uncertain: bool,
    poisoned: bool,
}
impl Drop for Native {
    fn drop(&mut self) {
        // SAFETY: NativeInterop drops this only after shutdown positively completed
        // and retired all GL names, or before any GL submission during new().
        debug_assert!(!self.uncertain);
        debug_assert!(self.active.is_none() && self.retired.is_none());
        debug_assert!(self.fence.is_null());
        unsafe {
            if !self.context.is_null() {
                CGLSetCurrentContext(null_mut());
                CGLDestroyContext(self.context);
            }
            if !self.renderers.is_null() {
                CGLDestroyRendererInfo(self.renderers);
            }
            if !self.pixel_format.is_null() {
                CGLDestroyPixelFormat(self.pixel_format);
            }
        }
    }
}

/// Worker-local, deliberately !Send/!Sync. Keep alive through mpv_render_context_free.
/// The parent owns the single mpv client, cancellation/generations and three-frame
/// mailbox permits; this transport never launches a second client or worker.
pub(super) struct NativeInterop {
    native: RefCell<Option<Box<Native>>>,
    _worker: PhantomData<Rc<()>>,
}

impl NativeInterop {
    pub(super) fn new(context: &GpuSurfaceContext) -> Result<Self, String> {
        if context.registry_id == 0 {
            return Err(
                "GPU surface macOS transport requires GPUI's actual Metal registry ID".into(),
            );
        }
        let this = Self {
            native: RefCell::new(Some(Box::new(Native {
                gl: Gl::load()?,
                pixel_format: null_mut(),
                context: null_mut(),
                renderers: null_mut(),
                renderer_id: 0,
                max_size: 0,
                active: None,
                retired: None,
                fence: null_mut(),
                uncertain: false,
                poisoned: false,
            }))),
            _worker: PhantomData,
        };
        {
            let mut owner = this.native.borrow_mut();
            let n = owner.as_mut().unwrap();
            // SAFETY: this invocation owns every out-pointer and all objects stay
            // on the invoking worker; this guards cleanup on every error path.
            unsafe {
                let attributes = [
                    CGL_PFA_OPENGL_PROFILE,
                    CGL_PROFILE_3_2_CORE,
                    CGL_PFA_ACCELERATED,
                    CGL_PFA_NO_RECOVERY,
                    0,
                ];
                let mut count = 0;
                cgl(
                    CGLChoosePixelFormat(attributes.as_ptr(), &mut n.pixel_format, &mut count),
                    "choose accelerated CGL core 3.2",
                )?;
                if n.pixel_format.is_null() || count < 1 {
                    return Err("no accelerated CGL core 3.2 pixel format".into());
                }
                cgl(
                    CGLCreateContext(n.pixel_format, null_mut(), &mut n.context),
                    "create CGL context",
                )?;
                if n.context.is_null() {
                    return Err("CGLCreateContext returned null".into());
                }
                cgl(CGLSetCurrentContext(n.context), "make CGL context current")?;
                if CGLGetCurrentContext() != n.context {
                    return Err("owned CGL context is not current".into());
                }
                cgl(
                    CGLGetParameter(n.context, CGL_CP_CURRENT_RENDERER_ID, &mut n.renderer_id),
                    "query active CGL renderer",
                )?;
                cgl(
                    CGLQueryRendererInfo(u32::MAX, &mut n.renderers, &mut count),
                    "query CGL registry identities",
                )?;
                if n.renderers.is_null() || !(1..=MAX_RENDERERS).contains(&count) {
                    return Err("CGL renderer enumeration returned invalid object/count".into());
                }
                let mut identities = Vec::new();
                for index in 0..count {
                    let mut renderer = 0;
                    cgl(
                        CGLDescribeRenderer(n.renderers, index, CGL_RP_RENDERER_ID, &mut renderer),
                        "CGL renderer ID",
                    )?;
                    if renderer != n.renderer_id {
                        continue;
                    }
                    let (mut accelerated, mut low, mut high) = (0, 0, 0);
                    cgl(
                        CGLDescribeRenderer(
                            n.renderers,
                            index,
                            CGL_RP_ACCELERATED,
                            &mut accelerated,
                        ),
                        "CGL renderer acceleration",
                    )?;
                    cgl(
                        CGLDescribeRenderer(n.renderers, index, CGL_RP_REGISTRY_ID_LOW, &mut low),
                        "CGL registry low (macOS 10.13+)",
                    )?;
                    cgl(
                        CGLDescribeRenderer(n.renderers, index, CGL_RP_REGISTRY_ID_HIGH, &mut high),
                        "CGL registry high (macOS 10.13+)",
                    )?;
                    if accelerated == 0 {
                        return Err("active CGL renderer is not hardware accelerated".into());
                    }
                    identities.push((u64::from(high as u32) << 32) | u64::from(low as u32));
                }
                verify_identity(context.registry_id, &identities)?;
                (n.gl.get_integer)(GL_MAX_RECTANGLE_TEXTURE_SIZE, &mut count);
                gl_ok(n, "query rectangle texture limit")?;
                if count <= 0 {
                    return Err("invalid CGL rectangle texture limit".into());
                }
                n.max_size = count as u32;
            }
        }
        Ok(this)
    }

    pub(super) fn get_proc_address(&self, name: &CStr) -> *const c_void {
        let owner = self.native.borrow();
        let Some(n) = owner.as_ref() else {
            return null();
        };
        if n.poisoned {
            return null();
        }
        // SAFETY: returned address remains valid while the native owner/library
        // lives; parent must free the mpv render context before shutdown/drop.
        unsafe {
            n.gl.library
                .get::<*const c_void>(name.to_bytes_with_nul())
                .map(|p| *p)
                .unwrap_or(null())
        }
    }

    pub(super) fn drm_render_fd(&self) -> Option<i32> {
        None
    }

    pub(super) fn begin_frame(&mut self, size: [u32; 2]) -> Result<i32, String> {
        let mut owner = self.native.borrow_mut();
        let n = owner.as_mut().ok_or("macOS interop has shut down")?;
        check_current(n)?;
        validate_size(size, n.max_size)?;
        if n.active.is_some() {
            return Err("previous macOS frame has not been finished".into());
        }
        retire(n)?;
        // Surface and pixel-buffer construction does not map or write pixels.
        n.active = Some(make_frame(size)?);
        // Any native setup/render work from here onward must be completed or
        // quarantined, including partial FBO creation and mpv render errors.
        n.uncertain = true;
        // SAFETY: the active frame owns all storage and the checked context is
        // current. Driver-reported names are stored before any fallible operation.
        unsafe {
            let frame = n.active.as_mut().unwrap();
            (n.gl.gen_textures)(1, &mut frame.texture);
            (n.gl.bind_texture)(GL_TEXTURE_RECTANGLE, frame.texture);
            (n.gl.tex_parameter)(GL_TEXTURE_RECTANGLE, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
            (n.gl.tex_parameter)(GL_TEXTURE_RECTANGLE, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
            cgl(
                CGLTexImageIOSurface2D(
                    n.context,
                    GL_TEXTURE_RECTANGLE,
                    GL_RGBA8,
                    size[0] as i32,
                    size[1] as i32,
                    GL_BGRA,
                    GL_UNSIGNED_INT_8_8_8_8_REV,
                    frame.surface.0,
                    0,
                ),
                "bind IOSurface to GL rectangle",
            )?;
            (n.gl.gen_framebuffers)(1, &mut frame.framebuffer);
            (n.gl.bind_framebuffer)(GL_FRAMEBUFFER, frame.framebuffer);
            (n.gl.framebuffer_texture)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_RECTANGLE,
                frame.texture,
                0,
            );
            if frame.texture == 0 || frame.framebuffer == 0 || frame.framebuffer > i32::MAX as u32 {
                return Err("invalid GL texture/framebuffer name".into());
            }
            if (n.gl.check_framebuffer)(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE {
                return Err("IOSurface GL rectangle framebuffer incomplete".into());
            }
            let fbo = frame.framebuffer as i32;
            gl_ok(n, "configure IOSurface framebuffer")?;
            Ok(fbo)
        }
    }

    pub(super) fn finish_frame(&mut self) -> Result<NativeFrame, String> {
        let mut owner = self.native.borrow_mut();
        let n = owner.as_mut().ok_or("macOS interop has shut down")?;
        if n.active.is_none() {
            return Err("no active macOS interop frame".into());
        }
        complete(n)?;
        // No flip/copy: CV/Metal row zero corresponds to GL y=0. The parent's mpv
        // FBO render orientation contract must account for this, as on Linux.
        let buffer = n.active.as_ref().unwrap().pixel_buffer.clone();
        debug_assert!(n.retired.is_none());
        n.retired = n.active.take();
        // Keep GL names and an extra surface/CV reference until next begin or
        // shutdown. Published storage is immutable, never reused for rendering.
        Ok(NativeFrame::PixelBuffer(buffer))
    }

    pub(super) fn complete_submissions(&self) -> Result<(), String> {
        let mut owner = self.native.borrow_mut();
        match owner.as_mut() {
            Some(n) => complete(n),
            None => Ok(()),
        }
    }

    /// Parent must call only after mpv_render_context_free; its final GL commands
    /// are fenced here even when no frame is active. Idempotent after success.
    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        let mut owner = self.native.borrow_mut();
        let Some(n) = owner.as_mut() else {
            return Ok(());
        };
        if !n.context.is_null() {
            complete(n)?;
            // An unfinished/cancelled frame remains owned until completion too.
            if n.active.is_some() {
                debug_assert!(n.retired.is_none());
                n.retired = n.active.take();
            }
            retire(n)?;
        }
        owner.take();
        Ok(())
    }
}

impl Drop for NativeInterop {
    fn drop(&mut self) {
        if self.shutdown().is_err() {
            // Every required owner is inside this box: context/library, CF/CV
            // storage, names, fence. No in-flight storage is freed on failure.
            if let Some(native) = self.native.get_mut().take() {
                std::mem::forget(native);
            }
        }
    }
}

fn cgl(code: i32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation}: CGL error {code}"))
    }
}

fn verify_identity(expected: u64, candidates: &[u64]) -> Result<(), String> {
    match candidates {
        [id] if expected != 0 && *id == expected => Ok(()),
        _ => Err("active accelerated CGL registry identity is missing, ambiguous, or differs from GPUI Metal renderer".into()),
    }
}

fn validate_size(size: [u32; 2], maximum: u32) -> Result<(), String> {
    if size
        .iter()
        .any(|&v| v == 0 || v > maximum || v > i32::MAX as u32)
    {
        Err("GPU surface macOS frame dimensions exceed GL rectangle capabilities".into())
    } else {
        Ok(())
    }
}

fn check_current(n: &mut Native) -> Result<(), String> {
    if n.poisoned {
        return Err("macOS interop poisoned; native owners quarantined".into());
    }
    // SAFETY: worker-confined owned context; no display/context update or
    // automatic GPU-switching attribute is requested anywhere in this module.
    let result = unsafe {
        if CGLGetCurrentContext() != n.context {
            Err("macOS interop context is no longer current".into())
        } else {
            let mut renderer = 0;
            cgl(
                CGLGetParameter(n.context, CGL_CP_CURRENT_RENDERER_ID, &mut renderer),
                "recheck CGL renderer",
            )
            .and_then(|()| {
                if renderer == n.renderer_id {
                    Ok(())
                } else {
                    Err("active CGL renderer changed; refusing adapter mismatch".into())
                }
            })
        }
    };
    if result.is_err() {
        n.poisoned = true;
        n.uncertain = true;
    }
    result
}

fn gl_ok(n: &Native, operation: &str) -> Result<(), String> {
    // SAFETY: caller has verified that this worker's context is current.
    let error = unsafe { (n.gl.get_error)() };
    if error == 0 {
        Ok(())
    } else {
        Err(format!("{operation}: GL error {error:#x}"))
    }
}

fn complete(n: &mut Native) -> Result<(), String> {
    check_current(n)?;
    // Always fence, including after libmpv render-context destruction: that code
    // may submit GL work outside begin/finish. Never use glFinish/unbounded wait.
    n.uncertain = true;
    // SAFETY: native owns its context/fence and retains all submitted storage.
    unsafe {
        if !n.fence.is_null() {
            n.poisoned = true;
            return Err("prior GL fence completion unknown; native owners quarantined".into());
        }
        n.fence = (n.gl.fence_sync)(GL_SYNC_GPU_COMMANDS_COMPLETE, 0);
        (n.gl.flush)();
        if n.fence.is_null() {
            n.poisoned = true;
            return Err("GL fence allocation failed; native owners quarantined".into());
        }
        let deadline = Instant::now() + COMPLETION_LIMIT;
        loop {
            match (n.gl.client_wait_sync)(n.fence, 0, 1_000_000) {
                GL_ALREADY_SIGNALED | GL_CONDITION_SATISFIED => break,
                GL_TIMEOUT_EXPIRED if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                status => {
                    n.poisoned = true;
                    return Err(format!(
                        "GL completion unproven ({status:#x}); native owners quarantined"
                    ));
                }
            }
        }
        (n.gl.delete_sync)(n.fence);
        n.fence = null_mut();
        n.uncertain = false;
    }
    gl_ok(n, "complete GL submissions")
}

fn retire(n: &mut Native) -> Result<(), String> {
    if n.retired.is_none() {
        return Ok(());
    }
    check_current(n)?;
    n.uncertain = true;
    // SAFETY: rendering completed before retirement. Retain CF/CV storage even
    // after deletion until a subsequent fence proves deletion work complete.
    unsafe {
        let frame = n.retired.as_mut().unwrap();
        (n.gl.bind_framebuffer)(GL_FRAMEBUFFER, 0);
        (n.gl.bind_texture)(GL_TEXTURE_RECTANGLE, 0);
        if frame.framebuffer != 0 {
            (n.gl.delete_framebuffers)(1, &frame.framebuffer);
            frame.framebuffer = 0;
        }
        if frame.texture != 0 {
            (n.gl.delete_textures)(1, &frame.texture);
            frame.texture = 0;
        }
    }
    complete(n)?;
    n.retired.take();
    Ok(())
}

fn dictionary() -> Result<Cf, String> {
    // SAFETY: callback addresses are the actual exported CFType callback structs;
    // byte declarations are used only to take their addresses, never dereferenced.
    let ptr = unsafe {
        CFDictionaryCreateMutable(
            null_mut(),
            0,
            (&raw const kCFTypeDictionaryKeyCallBacks).cast_mut().cast(),
            (&raw const kCFTypeDictionaryValueCallBacks)
                .cast_mut()
                .cast(),
        )
    };
    if ptr.is_null() {
        Err("CF dictionary allocation failed".into())
    } else {
        Ok(Cf(ptr))
    }
}

fn make_frame(size: [u32; 2]) -> Result<Frame, String> {
    let properties = dictionary()?;
    // SAFETY: owned dictionaries use CFType callbacks, retaining each number.
    // IOSurface computes its native row/alloc alignment; no CPU pointer is used.
    unsafe {
        let values = [
            (kIOSurfaceWidth, i64::from(size[0])),
            (kIOSurfaceHeight, i64::from(size[1])),
            (kIOSurfaceBytesPerElement, 4),
            (
                kIOSurfacePixelFormat,
                i64::from(u32::from_be_bytes(*b"BGRA")),
            ),
        ];
        for (key, value) in values {
            let number = Cf(CFNumberCreate(null_mut(), 4, (&value as *const i64).cast()));
            if number.0.is_null() {
                return Err("IOSurface property allocation failed".into());
            }
            CFDictionarySetValue(properties.0, key, number.0);
        }
        let surface = Cf(IOSurfaceCreate(properties.0));
        if surface.0.is_null() {
            return Err("BGRA8 IOSurface creation failed".into());
        }
        let attributes = dictionary()?;
        CFDictionarySetValue(
            attributes.0,
            kCVPixelBufferMetalCompatibilityKey,
            kCFBooleanTrue,
        );
        CFDictionarySetValue(
            attributes.0,
            kCVPixelBufferOpenGLCompatibilityKey,
            kCFBooleanTrue,
        );
        let mut raw = null_mut();
        let result =
            CVPixelBufferCreateWithIOSurface(null_mut(), surface.0, attributes.0, &mut raw);
        let buffer = NonNull::new(raw).map(|p| CFRetained::from_raw(p));
        if result != 0 {
            return Err(format!("CVPixelBuffer IOSurface bridge failed: {result}"));
        }
        let pixel_buffer = buffer.ok_or("CVPixelBuffer bridge returned null")?;
        Ok(Frame {
            pixel_buffer,
            surface,
            texture: 0,
            framebuffer: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_identity_is_exact_unique_and_nonzero() {
        assert!(verify_identity(42, &[42]).is_ok());
        assert!(verify_identity(42, &[43]).is_err());
        assert!(verify_identity(42, &[42, 42]).is_err());
        assert!(verify_identity(42, &[]).is_err());
        assert!(verify_identity(0, &[0]).is_err());
    }

    #[test]
    fn dimensions_fail_closed_before_native_allocation() {
        assert!(validate_size([3840, 2160], 16384).is_ok());
        assert!(validate_size([0, 2160], 16384).is_err());
        assert!(validate_size([16385, 2160], 16384).is_err());
        assert!(validate_size([u32::MAX, 1], u32::MAX).is_err());
    }
}
