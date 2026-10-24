//! Native, headless SDR BGRA8 interop test only: no GPUI, decoder, or HDR claims.
//!
//! Unsafe audit: CGL/CF objects have Create-rule ownership; Metal's imported
//! texture has new-rule ownership. All native calls and destruction happen on
//! this worker with its CGL context current. No IOSurface CPU mapping/upload is
//! used. The only CPU pixel access is the completed Metal test-readback buffer.
//! If a fence/command buffer cannot prove completion, the entire owner graph
//! (including framework library, context, surface, queue, and autorelease pool)
//! is intentionally leaked until process exit, never destroyed in flight.
//! Registry properties are from Apple's CGLTypes.h (macOS 10.13+, 140/141).
//! Reproduce on macOS: python3 experiments/mpv-platform-probes/prepare.py
//! then MTL_DEBUG_LAYER=1 cargo run --locked --manifest-path
//! target/mpv-platform-probes/Cargo.toml
//! Poll deadlines bound our waits, not a driver hanging inside a native API call.

use std::ffi::{c_char, c_void};
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use metal::foreign_types::ForeignType;
use metal::{
    Device, MTLBlitOption, MTLCommandBufferStatus, MTLOrigin, MTLPixelFormat, MTLResourceOptions,
    MTLSize, MTLStorageMode, MTLTextureType, MTLTextureUsage, Texture, TextureDescriptor,
};

use crate::{COLORS, HEIGHT, ROW_BYTES as ROW, WIDTH, matching_registry, validate_bgra};

type Ptr = *mut c_void;
const DEADLINE: Duration = Duration::from_secs(5);
// CGLTypes.h attributes, profiles, parameters, and renderer properties.
const CGL_PFA_OPENGL_PROFILE: i32 = 99;
const CGL_PROFILE_3_2_CORE: i32 = 0x3200;
const CGL_PFA_ACCELERATED: i32 = 73;
const CGL_PFA_NO_RECOVERY: i32 = 72;
const CGL_CP_CURRENT_RENDERER_ID: i32 = 309;
const CGL_RP_RENDERER_ID: i32 = 70;
const CGL_RP_ACCELERATED: i32 = 73;
// CGLTypes.h: registry ID halves available on macOS 10.13+.
const CGL_RP_REGISTRY_ID_LOW: i32 = 140;
const CGL_RP_REGISTRY_ID_HIGH: i32 = 141;
// Conservative sanity bound for driver-supplied renderer enumeration.
const MAX_RENDERERS: i32 = 256;

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
}
#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    fn IOSurfaceCreate(properties: Ptr) -> Ptr;
    static kIOSurfaceWidth: Ptr;
    static kIOSurfaceHeight: Ptr;
    static kIOSurfaceBytesPerElement: Ptr;
    static kIOSurfaceBytesPerRow: Ptr;
    static kIOSurfacePixelFormat: Ptr;
}
#[link(name = "objc")]
unsafe extern "C" {
    fn sel_registerName(name: *const c_char) -> Ptr;
    // The fixed signature is the exact ABI of newTextureWithDescriptor:iosurface:plane:.
    #[link_name = "objc_msgSend"]
    fn new_surface_texture(
        device: Ptr,
        selector: Ptr,
        descriptor: Ptr,
        surface: Ptr,
        plane: usize,
    ) -> Ptr;
    fn objc_autoreleasePoolPush() -> Ptr;
    fn objc_autoreleasePoolPop(pool: Ptr);
}

macro_rules! gl_api {
    ($($field:ident : $name:literal ($($arg:ty),*) -> $ret:ty;)+) => {
        struct Gl {
            $( $field: unsafe extern "C" fn($($arg),*) -> $ret, )+
            _library: libloading::Library,
        }
        impl Gl {
            unsafe fn load() -> Result<Self, String> {
                // SAFETY: system framework path; typed symbols are the published GL ABI.
                unsafe {
                    let library = libloading::Library::new(
                        "/System/Library/Frameworks/OpenGL.framework/OpenGL"
                    ).map_err(|e| e.to_string())?;
                    Ok(Self {
                        $($field: *library.get(concat!($name, "\0").as_bytes())
                            .map_err(|e| e.to_string())?,)+
                        _library: library,
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
    enable: "glEnable" (u32) -> ();
    disable: "glDisable" (u32) -> ();
    scissor: "glScissor" (i32, i32, i32, i32) -> ();
    clear_color: "glClearColor" (f32, f32, f32, f32) -> ();
    clear: "glClear" (u32) -> ();
    fence_sync: "glFenceSync" (u32, u32) -> Ptr;
    client_wait_sync: "glClientWaitSync" (Ptr, u32, u64) -> u32;
    delete_sync: "glDeleteSync" (Ptr) -> ();
    flush: "glFlush" () -> ();
    get_error: "glGetError" () -> u32;
}

struct Native {
    gl: Gl,
    pool: Ptr,
    pixel_format: Ptr,
    context: Ptr,
    renderers: Ptr,
    surface: Ptr,
    texture: u32,
    framebuffer: u32,
    fence: Ptr,
    device: Option<Device>,
    imported: Option<Texture>,
    copied: Option<Texture>,
    queue: Option<metal::CommandQueue>,
    command: Option<metal::CommandBuffer>,
    readback: Option<metal::Buffer>,
}
impl Drop for Native {
    fn drop(&mut self) {
        // SAFETY: Guard permits destruction only before submission or after
        // positive completion. Context remains current until GL deletion ends.
        unsafe {
            self.command.take();
            self.readback.take();
            self.copied.take();
            self.imported.take();
            self.queue.take();
            self.device.take();
            if !self.fence.is_null() {
                (self.gl.delete_sync)(self.fence);
            }
            if self.framebuffer != 0 {
                (self.gl.delete_framebuffers)(1, &self.framebuffer);
            }
            if self.texture != 0 {
                (self.gl.delete_textures)(1, &self.texture);
            }
            if !self.context.is_null() {
                CGLSetCurrentContext(null_mut());
                CGLDestroyContext(self.context);
            }
            if !self.surface.is_null() {
                CFRelease(self.surface);
            }
            if !self.renderers.is_null() {
                CGLDestroyRendererInfo(self.renderers);
            }
            if !self.pixel_format.is_null() {
                CGLDestroyPixelFormat(self.pixel_format);
            }
            objc_autoreleasePoolPop(self.pool);
        }
    }
}
struct Guard {
    native: Option<Box<Native>>,
    uncertain: bool,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.uncertain {
            // Deliberate process-lifetime quarantine, including on unwinding.
            if let Some(native) = self.native.take() {
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

/// Called only on the CLI's dedicated native worker, never the UI/render thread.
pub(super) fn run() -> Result<(), String> {
    // SAFETY: all native objects are confined to this invocation/worker. Guard
    // owns every submitted resource before any fallible operation can unwind.
    unsafe { probe() }
}

unsafe fn probe() -> Result<(), String> {
    // This block centralizes FFI; individual ownership/synchronization boundaries
    // are called out below. No pointers escape except intentional quarantine.
    unsafe {
        let gl = Gl::load()?;
        let mut guard = Guard {
            native: Some(Box::new(Native {
                gl,
                pool: objc_autoreleasePoolPush(),
                pixel_format: null_mut(),
                context: null_mut(),
                renderers: null_mut(),
                surface: null_mut(),
                texture: 0,
                framebuffer: 0,
                fence: null_mut(),
                device: None,
                imported: None,
                copied: None,
                queue: None,
                command: None,
                readback: None,
            })),
            uncertain: false,
        };
        let n = guard.native.as_mut().unwrap();
        // kCGLPFAOpenGLProfile/Core3.2, Accelerated, NoRecovery. In particular,
        // do not enable software recovery or automatic GPU switching.
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
            "choose accelerated core 3.2",
        )?;
        if n.pixel_format.is_null() || count < 1 {
            return Err("no accelerated core 3.2 pixel format".into());
        }
        cgl(
            CGLCreateContext(n.pixel_format, null_mut(), &mut n.context),
            "create CGL context",
        )?;
        if n.context.is_null() {
            return Err("CGLCreateContext returned a null context".into());
        }
        cgl(CGLSetCurrentContext(n.context), "make CGL context current")?;
        if CGLGetCurrentContext() != n.context {
            return Err("CGL current context does not match the owned context".into());
        }

        let mut active = 0;
        cgl(
            CGLGetParameter(n.context, CGL_CP_CURRENT_RENDERER_ID, &mut active),
            "active renderer ID",
        )?;
        cgl(
            CGLQueryRendererInfo(u32::MAX, &mut n.renderers, &mut count),
            "query renderer identities",
        )?;
        if n.renderers.is_null() || !(1..=MAX_RENDERERS).contains(&count) {
            return Err("CGL renderer enumeration returned a null object or invalid count".into());
        }
        let mut identities = Vec::new();
        for index in 0..count {
            let mut id = 0;
            cgl(
                CGLDescribeRenderer(n.renderers, index, CGL_RP_RENDERER_ID, &mut id),
                "renderer ID",
            )?;
            if id != active {
                continue;
            }
            let (mut accelerated, mut low, mut high) = (0, 0, 0);
            cgl(
                CGLDescribeRenderer(n.renderers, index, CGL_RP_ACCELERATED, &mut accelerated),
                "renderer acceleration",
            )?;
            cgl(
                CGLDescribeRenderer(n.renderers, index, CGL_RP_REGISTRY_ID_LOW, &mut low),
                "renderer registry low",
            )?;
            cgl(
                CGLDescribeRenderer(n.renderers, index, CGL_RP_REGISTRY_ID_HIGH, &mut high),
                "renderer registry high",
            )?;
            if accelerated == 0 {
                return Err("active CGL renderer is not accelerated".into());
            }
            identities.push((
                id as u32,
                (u64::from(high as u32) << 32) | u64::from(low as u32),
            ));
        }
        let registry = matching_registry(active as u32, &identities)?;
        let mut devices: Vec<_> = Device::all()
            .into_iter()
            .filter(|d| d.registry_id() == registry)
            .collect();
        if devices.len() != 1 {
            return Err("no unique Metal device with the active CGL registry ID".into());
        }
        n.device = devices.pop();

        n.surface = make_surface()?;
        (n.gl.gen_textures)(1, &mut n.texture);
        (n.gl.bind_texture)(0x84F5, n.texture); // GL_TEXTURE_RECTANGLE
        (n.gl.tex_parameter)(0x84F5, 0x2801, 0x2600); // MIN_FILTER, NEAREST
        (n.gl.tex_parameter)(0x84F5, 0x2800, 0x2600);
        cgl(
            CGLTexImageIOSurface2D(
                n.context,
                0x84F5,
                0x8058,
                WIDTH as i32,
                HEIGHT as i32,
                0x80E1,
                0x8367,
                n.surface,
                0,
            ),
            "bind BGRA8 IOSurface to GL rectangle",
        )?;
        (n.gl.gen_framebuffers)(1, &mut n.framebuffer);
        (n.gl.bind_framebuffer)(0x8D40, n.framebuffer);
        (n.gl.framebuffer_texture)(0x8D40, 0x8CE0, 0x84F5, n.texture, 0);
        if (n.gl.check_framebuffer)(0x8D40) != 0x8CD5 {
            return Err("IOSurface framebuffer incomplete".into());
        }
        gl_ok(&n.gl, "IOSurface framebuffer setup")?;
        (n.gl.disable)(0x8DB9); // FRAMEBUFFER_SRGB: exact linear UNORM bytes
        (n.gl.disable)(0x0BD0); // DITHER
        (n.gl.enable)(0x0C11); // SCISSOR_TEST
        // Mark uncertain BEFORE the first GPU write, including panic paths.
        guard.uncertain = true;
        for (index, rgba) in COLORS.iter().enumerate() {
            (n.gl.scissor)((index % 2 * 2) as i32, (index / 2 * 2) as i32, 2, 2);
            (n.gl.clear_color)(
                rgba[0] as f32 / 255.0,
                rgba[1] as f32 / 255.0,
                rgba[2] as f32 / 255.0,
                1.0,
            );
            (n.gl.clear)(0x4000); // GPU clears, no CPU texture upload
        }
        n.fence = (n.gl.fence_sync)(0x9117, 0); // SYNC_GPU_COMMANDS_COMPLETE
        (n.gl.flush)();
        if n.fence.is_null() {
            return Err("GL fence creation failed; owners quarantined".into());
        }
        let deadline = Instant::now() + DEADLINE;
        loop {
            match (n.gl.client_wait_sync)(n.fence, 0, 1_000_000) {
                0x911A | 0x911C => break, // ALREADY_SIGNALED / CONDITION_SATISFIED
                0x911B if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(1)),
                status => {
                    return Err(format!(
                        "GL completion unproven ({status:#x}); owners quarantined"
                    ));
                }
            }
        }
        guard.uncertain = false;
        gl_ok(&n.gl, "GL pattern/fence")?;
        // No context update, drawable, or renderer switch occurred since identity
        // selection; nevertheless reject a changed active renderer before import.
        let mut after = 0;
        cgl(
            CGLGetParameter(n.context, CGL_CP_CURRENT_RENDERER_ID, &mut after),
            "recheck active renderer",
        )?;
        if after != active {
            return Err("CGL renderer changed during probe".into());
        }

        let device = n.device.as_ref().unwrap();
        let descriptor = TextureDescriptor::new();
        descriptor.set_texture_type(MTLTextureType::D2);
        descriptor.set_width(WIDTH as u64);
        descriptor.set_height(HEIGHT as u64);
        descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        descriptor.set_storage_mode(if device.has_unified_memory() {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Managed
        });
        descriptor.set_usage(MTLTextureUsage::ShaderRead);
        let raw = new_surface_texture(
            device.as_ptr().cast(),
            sel_registerName(c"newTextureWithDescriptor:iosurface:plane:".as_ptr()),
            descriptor.as_ptr().cast(),
            n.surface,
            0,
        );
        if raw.is_null() {
            return Err("Metal rejected IOSurface BGRA8 import".into());
        }
        n.imported = Some(Texture::from_ptr(raw.cast()));
        descriptor.set_storage_mode(MTLStorageMode::Private);
        n.copied = Some(device.new_texture(&descriptor));
        n.queue = Some(device.new_command_queue());
        n.readback =
            Some(device.new_buffer((ROW * HEIGHT) as u64, MTLResourceOptions::StorageModeShared));
        n.command = Some(n.queue.as_ref().unwrap().new_command_buffer().to_owned());
        let command = n.command.as_ref().unwrap();
        let encoder = command.new_blit_command_encoder();
        encoder.copy_from_texture(
            n.imported.as_ref().unwrap(),
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
            MTLSize {
                width: WIDTH as u64,
                height: HEIGHT as u64,
                depth: 1,
            },
            n.copied.as_ref().unwrap(),
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
        );
        encoder.copy_from_texture_to_buffer(
            n.copied.as_ref().unwrap(),
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
            MTLSize {
                width: WIDTH as u64,
                height: HEIGHT as u64,
                depth: 1,
            },
            n.readback.as_ref().unwrap(),
            0,
            ROW as u64,
            (ROW * HEIGHT) as u64,
            MTLBlitOption::empty(),
        );
        encoder.end_encoding();
        guard.uncertain = true;
        command.commit();
        let deadline = Instant::now() + DEADLINE;
        loop {
            match command.status() {
                MTLCommandBufferStatus::Completed => break,
                MTLCommandBufferStatus::Error => {
                    return Err("Metal GPU error; owners quarantined".into());
                }
                _ if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(1)),
                _ => return Err("Metal completion timeout; owners quarantined".into()),
            }
        }
        guard.uncertain = false;
        let data = std::slice::from_raw_parts(
            n.readback.as_ref().unwrap().contents().cast::<u8>(),
            ROW * HEIGHT,
        );
        validate_bgra(data, ROW)?;
        // Diagnostics on stderr only: shared CLI owns the fixed passed label.
        eprintln!(
            "macOS native test-readback: exact CGL/Metal registry identity {registry:#x}; BGRA8, GL y=0 == Metal row 0; not GPUI/playback/HDR qualification"
        );
        Ok(())
    }
}

unsafe fn gl_ok(gl: &Gl, operation: &str) -> Result<(), String> {
    let error = unsafe { (gl.get_error)() };
    if error == 0 {
        Ok(())
    } else {
        Err(format!("{operation}: GL error {error:#x}"))
    }
}

unsafe fn make_surface() -> Result<Ptr, String> {
    unsafe {
        // CFType callbacks retain the temporary CFNumbers, released below.
        let dictionary = CFDictionaryCreateMutable(
            null_mut(),
            0,
            (&raw const kCFTypeDictionaryKeyCallBacks).cast_mut().cast(),
            (&raw const kCFTypeDictionaryValueCallBacks)
                .cast_mut()
                .cast(),
        );
        if dictionary.is_null() {
            return Err("IOSurface dictionary allocation failed".into());
        }
        let values = [
            (kIOSurfaceWidth, WIDTH as i64),
            (kIOSurfaceHeight, HEIGHT as i64),
            (kIOSurfaceBytesPerElement, 4),
            (kIOSurfaceBytesPerRow, ROW as i64),
            (
                kIOSurfacePixelFormat,
                i64::from(u32::from_be_bytes(*b"BGRA")),
            ),
        ];
        for (key, value) in values {
            let number = CFNumberCreate(null_mut(), 4, (&value as *const i64).cast()); // kCFNumberSInt64Type
            if number.is_null() {
                CFRelease(dictionary);
                return Err("IOSurface property allocation failed".into());
            }
            CFDictionarySetValue(dictionary, key, number);
            CFRelease(number);
        }
        let surface = IOSurfaceCreate(dictionary);
        CFRelease(dictionary);
        if surface.is_null() {
            Err("IOSurfaceCreate failed".into())
        } else {
            Ok(surface)
        }
    }
}
