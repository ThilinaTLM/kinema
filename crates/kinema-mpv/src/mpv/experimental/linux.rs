//! Linux-only experimental Vulkan/GL opaque-FD buffer interop.
//!
//! Playback never maps a buffer: GL packs pixels into exported Vulkan memory and
//! Vulkan copies that buffer into an ordinary texture on the caller's device.
#![deny(unsafe_op_in_unsafe_fn)]

use ash::vk;
use libloading::Library;
use std::{
    cell::Cell,
    ffi::{CStr, c_char, c_void},
    mem::ManuallyDrop,
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    ptr,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

type Display = *mut c_void;
type Context = *mut c_void;
type Surface = *mut c_void;
type GetProc = unsafe extern "system" fn(*const c_char) -> *const c_void;

macro_rules! gl_api {
    ($($name:ident: $ty:ty = $symbol:literal;)*) => {
        pub(crate) struct Gl { $(pub $name: $ty,)* }
        impl Gl {
            unsafe fn load(get: GetProc) -> Result<Self, String> {
                Ok(Self { $($name: {
                    let p = unsafe { get(concat!($symbol, "\0").as_ptr().cast()) };
                    if p.is_null() { return Err(format!("missing GL entry point {}", $symbol)); }
                    unsafe { std::mem::transmute::<*const c_void, $ty>(p) }
                },)* })
            }
        }
    }
}
gl_api! {
    get_string_i: unsafe extern "system" fn(u32,u32)->*const u8 = "glGetStringi";
    get_integer: unsafe extern "system" fn(u32,*mut i32) = "glGetIntegerv";
    get_uuid: unsafe extern "system" fn(u32,u32,*mut u8) = "glGetUnsignedBytei_vEXT";
    get_error: unsafe extern "system" fn()->u32 = "glGetError";
    create_memory: unsafe extern "system" fn(i32,*mut u32) = "glCreateMemoryObjectsEXT";
    delete_memory: unsafe extern "system" fn(i32,*const u32) = "glDeleteMemoryObjectsEXT";
    import_memory: unsafe extern "system" fn(u32,u64,u32,i32) = "glImportMemoryFdEXT";
    gen_buffers: unsafe extern "system" fn(i32,*mut u32) = "glGenBuffers";
    delete_buffers: unsafe extern "system" fn(i32,*const u32) = "glDeleteBuffers";
    bind_buffer: unsafe extern "system" fn(u32,u32) = "glBindBuffer";
    storage: unsafe extern "system" fn(u32,isize,u32,u64) = "glBufferStorageMemEXT";
    gen_textures: unsafe extern "system" fn(i32,*mut u32) = "glGenTextures";
    delete_textures: unsafe extern "system" fn(i32,*const u32) = "glDeleteTextures";
    bind_texture: unsafe extern "system" fn(u32,u32) = "glBindTexture";
    tex_image: unsafe extern "system" fn(u32,i32,i32,i32,i32,i32,u32,u32,*const c_void) = "glTexImage2D";
    gen_fbos: unsafe extern "system" fn(i32,*mut u32) = "glGenFramebuffers";
    delete_fbos: unsafe extern "system" fn(i32,*const u32) = "glDeleteFramebuffers";
    bind_fbo: unsafe extern "system" fn(u32,u32) = "glBindFramebuffer";
    attach: unsafe extern "system" fn(u32,u32,u32,u32,i32) = "glFramebufferTexture2D";
    check_fbo: unsafe extern "system" fn(u32)->u32 = "glCheckFramebufferStatus";
    pixel_store: unsafe extern "system" fn(u32,i32) = "glPixelStorei";
    read_pixels: unsafe extern "system" fn(i32,i32,i32,i32,u32,u32,*mut c_void) = "glReadPixels";
    fence_sync: unsafe extern "system" fn(u32,u32)->*mut c_void = "glFenceSync";
    client_wait_sync: unsafe extern "system" fn(*mut c_void,u32,u64)->u32 = "glClientWaitSync";
    delete_sync: unsafe extern "system" fn(*mut c_void) = "glDeleteSync";
    flush: unsafe extern "system" fn() = "glFlush";
    gen_semaphores: unsafe extern "system" fn(i32,*mut u32) = "glGenSemaphoresEXT";
    delete_semaphores: unsafe extern "system" fn(i32,*const u32) = "glDeleteSemaphoresEXT";
    import_semaphore: unsafe extern "system" fn(u32,u32,i32) = "glImportSemaphoreFdEXT";
    wait_semaphore: unsafe extern "system" fn(u32,u32,*const u32,u32,*const u32,*const u32) = "glWaitSemaphoreEXT";
    signal_semaphore: unsafe extern "system" fn(u32,u32,*const u32,u32,*const u32,*const u32) = "glSignalSemaphoreEXT";
}

/// Owns a private surfaceless desktop-GL context; created and dropped on worker.
pub(crate) struct Egl {
    _library: ManuallyDrop<Library>,
    quarantined: Cell<bool>,
    initialize: unsafe extern "system" fn(Display, *mut i32, *mut i32) -> u32,
    render_fd: Option<OwnedFd>,
    pub gl: Gl,
    pub get_proc: GetProc,
    display: Display,
    native_device: *mut c_void,
    context: Context,
    surface: Surface,
    make_current: unsafe extern "system" fn(Display, Surface, Surface, Context) -> u32,
    get_current_context: unsafe extern "system" fn() -> Context,
    destroy_context: unsafe extern "system" fn(Display, Context) -> u32,
    destroy_surface: unsafe extern "system" fn(Display, Surface) -> u32,
    terminate: unsafe extern "system" fn(Display) -> u32,
}
impl Egl {
    pub fn new(expected_uuid: [u8; 16]) -> Result<Self, String> {
        // A multi-GPU host may default EGL to NVIDIA while wgpu uses Intel.
        // Enumerate native EGL devices and accept only the exact Vulkan UUID.
        unsafe {
            let library = Library::new("libEGL.so.1").map_err(|e| e.to_string())?;
            let get: GetProc = *library
                .get(b"eglGetProcAddress\0")
                .map_err(|e| e.to_string())?;
            let p = get(c"eglQueryDevicesEXT".as_ptr());
            if p.is_null() {
                return Err("EGL device enumeration required for exact GPU selection".into());
            }
            let query: unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32 =
                std::mem::transmute(p);
            let mut count = 0;
            if query(0, ptr::null_mut(), &mut count) == 0 || !(1..=64).contains(&count) {
                return Err("EGL device enumeration failed".into());
            }
            let mut devices = vec![ptr::null_mut(); count as usize];
            if query(count, devices.as_mut_ptr(), &mut count) == 0 {
                return Err("EGL device enumeration failed".into());
            }
            let mut errors = Vec::new();
            for device in devices.into_iter().take(count as usize) {
                match Self::for_device(device) {
                    Ok(mut egl) => match egl.verify_uuid(expected_uuid) {
                        Ok(()) => {
                            egl.open_render_node()?;
                            return Ok(egl);
                        }
                        Err(error) => errors.push(error),
                    },
                    Err(error) => errors.push(error),
                }
            }
            Err(format!(
                "no EGL device matches Vulkan UUID: {}",
                errors.join("; ")
            ))
        }
    }
    /// Node comes from the exact EGL device whose full UUID matched Vulkan.
    pub(crate) fn vaapi_render_node(&self) -> Result<std::path::PathBuf, String> {
        use std::os::{
            linux::fs::MetadataExt,
            unix::{ffi::OsStrExt, fs::FileTypeExt},
        };
        let node = unsafe {
            let p = (self.get_proc)(c"eglQueryDeviceStringEXT".as_ptr());
            if p.is_null() {
                return Err("vaapi-capability: EGL device string query unavailable".into());
            }
            let query: unsafe extern "system" fn(*mut c_void, i32) -> *const c_char =
                std::mem::transmute(p);
            let value = query(self.native_device, 0x3377);
            if value.is_null() {
                return Err("vaapi-capability: matched EGL device has no DRM render node".into());
            }
            std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
                CStr::from_ptr(value).to_bytes(),
            ))
        };
        if !node.is_absolute()
            || node.parent() != Some(std::path::Path::new("/dev/dri"))
            || !node
                .file_name()
                .is_some_and(|name| name.as_bytes().starts_with(b"renderD"))
        {
            return Err("vaapi-capability: invalid EGL DRM render-node path".into());
        }
        let canonical = node
            .canonicalize()
            .map_err(|_| "vaapi-capability: EGL DRM render node unavailable")?;
        let metadata = std::fs::metadata(&canonical)
            .map_err(|_| "vaapi-capability: EGL DRM render node unavailable")?;
        if canonical != node || !metadata.file_type().is_char_device() || metadata.st_rdev() == 0 {
            return Err(
                "vaapi-capability: EGL DRM render node is not a direct character device".into(),
            );
        }
        Ok(node)
    }
    fn open_render_node(&mut self) -> Result<(), String> {
        use std::os::{linux::fs::MetadataExt, unix::fs::FileTypeExt};
        let node = self.vaapi_render_node()?;
        let before = std::fs::metadata(&node).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&node)
            .map_err(|e| format!("open verified DRM render node: {e}"))?;
        let after = file.metadata().map_err(|e| e.to_string())?;
        if !after.file_type().is_char_device()
            || before.st_rdev() != after.st_rdev()
            || before.st_ino() != after.st_ino()
            || before.st_dev() != after.st_dev()
        {
            return Err("DRM render node changed during open".into());
        }
        self.render_fd = Some(file.into());
        Ok(())
    }
    fn for_device(native_device: *mut c_void) -> Result<Self, String> {
        // SAFETY: EGL ABI signatures below are from EGL 1.5; library is retained.
        unsafe {
            let library = Library::new("libEGL.so.1").map_err(|e| e.to_string())?;
            let get_proc: GetProc = *library
                .get(b"eglGetProcAddress\0")
                .map_err(|e| e.to_string())?;
            let platform: unsafe extern "system" fn(u32, *mut c_void, *const isize) -> Display =
                *library
                    .get(b"eglGetPlatformDisplay\0")
                    .map_err(|e| e.to_string())?;
            let initialize: unsafe extern "system" fn(Display, *mut i32, *mut i32) -> u32 =
                *library.get(b"eglInitialize\0").map_err(|e| e.to_string())?;
            let bind: unsafe extern "system" fn(u32) -> u32 =
                *library.get(b"eglBindAPI\0").map_err(|e| e.to_string())?;
            let choose: unsafe extern "system" fn(
                Display,
                *const i32,
                *mut *mut c_void,
                i32,
                *mut i32,
            ) -> u32 = *library
                .get(b"eglChooseConfig\0")
                .map_err(|e| e.to_string())?;
            let create_context: unsafe extern "system" fn(
                Display,
                *mut c_void,
                Context,
                *const i32,
            ) -> Context = *library
                .get(b"eglCreateContext\0")
                .map_err(|e| e.to_string())?;
            let create_surface: unsafe extern "system" fn(
                Display,
                *mut c_void,
                *const i32,
            ) -> Surface = *library
                .get(b"eglCreatePbufferSurface\0")
                .map_err(|e| e.to_string())?;
            let make_current = *library
                .get(b"eglMakeCurrent\0")
                .map_err(|e| e.to_string())?;
            let get_current_context = *library
                .get(b"eglGetCurrentContext\0")
                .map_err(|e| e.to_string())?;
            let destroy_context = *library
                .get(b"eglDestroyContext\0")
                .map_err(|e| e.to_string())?;
            let destroy_surface = *library
                .get(b"eglDestroySurface\0")
                .map_err(|e| e.to_string())?;
            let terminate = *library.get(b"eglTerminate\0").map_err(|e| e.to_string())?;
            let gl = Gl::load(get_proc)?;
            let display = platform(0x313f, native_device, ptr::null()); // EGL_PLATFORM_DEVICE_EXT
            if display.is_null() {
                return Err("surfaceless EGL display unavailable".into());
            }
            crate::mpv::platform::acquire_headless_display(initialize, display)
                .map_err(|e| e.to_string())?;
            let mut this = Self {
                _library: ManuallyDrop::new(library),
                quarantined: Cell::new(false),
                initialize,
                render_fd: None,
                gl,
                get_proc,
                display,
                native_device,
                context: ptr::null_mut(),
                surface: ptr::null_mut(),
                make_current,
                get_current_context,
                destroy_context,
                destroy_surface,
                terminate,
            };
            if bind(0x30a2) == 0 {
                return Err("EGL desktop OpenGL unavailable".into());
            }
            let attrs = [
                0x3033, 1, 0x3040, 8, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038,
            ];
            let mut config = ptr::null_mut();
            let mut count = 0;
            if choose(display, attrs.as_ptr(), &mut config, 1, &mut count) == 0 || count == 0 {
                return Err("no EGL OpenGL pbuffer config".into());
            }
            this.surface = create_surface(display, config, [0x3057, 1, 0x3056, 1, 0x3038].as_ptr());
            this.context = create_context(
                display,
                config,
                ptr::null_mut(),
                [0x3098, 3, 0x30fb, 3, 0x30fd, 1, 0x3038].as_ptr(),
            );
            if this.surface.is_null()
                || this.context.is_null()
                || (this.make_current)(display, this.surface, this.surface, this.context) == 0
            {
                return Err("EGL OpenGL context creation failed".into());
            }
            this.gl = Gl::load(get_proc)?;
            Ok(this)
        }
    }
    fn check(&self, operation: &str) -> Result<(), String> {
        let error = unsafe { (self.gl.get_error)() };
        if error == 0 {
            Ok(())
        } else {
            Err(format!("{operation}: GL error {error:#x}"))
        }
    }
    /// Fence only this worker's GL command stream. This does not acquire the
    /// shared PBO or consume Done: libmpv may have failed before pixel packing.
    fn complete_gl(&self) -> Result<(), String> {
        unsafe {
            if (self.get_current_context)() != self.context || self.context.is_null() {
                return Err("completion proof requires the private EGL context current".into());
            }
            let sync = (self.gl.fence_sync)(0x9117, 0);
            self.check("completion GL fence")?;
            if sync.is_null() {
                return Err("completion GL fence returned null".into());
            }
            // Explicitly submit libmpv writes even when finish_frame never ran.
            (self.gl.flush)();
            self.check("completion GL flush")?;
            let result = (self.gl.client_wait_sync)(sync, 0, 5_000_000_000);
            self.check("completion GL wait (5s)")?;
            if !matches!(result, 0x911a | 0x911c) {
                // Leave the sync/native resources with the quarantined context.
                return Err(format!("completion GL wait incomplete: {result:#x}"));
            }
            (self.gl.delete_sync)(sync);
            self.check("completion delete GL fence")
        }
    }
    fn verify_uuid(&self, expected: [u8; 16]) -> Result<(), String> {
        unsafe {
            let mut count = 0;
            (self.gl.get_integer)(0x821d, &mut count);
            let mut memory = false;
            let mut semaphore = false;
            for i in 0..count {
                let p = (self.gl.get_string_i)(0x1f03, i as u32);
                if p.is_null() {
                    continue;
                }
                let name = CStr::from_ptr(p.cast()).to_bytes();
                memory |= name == b"GL_EXT_memory_object_fd";
                semaphore |= name == b"GL_EXT_semaphore_fd";
            }
            if !memory || !semaphore {
                return Err("requires GL_EXT_memory_object_fd and GL_EXT_semaphore_fd".into());
            }
            let mut devices = 0;
            (self.gl.get_integer)(0x9596, &mut devices);
            // Multiple GL devices are deliberately not accepted by this narrow proof.
            if devices != 1 {
                return Err(format!(
                    "expected exactly one GL device UUID, got {devices}"
                ));
            }
            let mut uuid = [0; 16];
            (self.gl.get_uuid)(0x9597, 0, uuid.as_mut_ptr());
            self.check("GL device UUID")?;
            if uuid != expected {
                return Err(format!(
                    "GL/Vulkan device UUID mismatch: GL {uuid:02x?}, Vulkan {expected:02x?}"
                ));
            }
            Ok(())
        }
    }
}
impl Drop for Egl {
    fn drop(&mut self) {
        if self.quarantined.get() {
            // Retain the verified node as well as the EGL library/registry reference.
            std::mem::forget(self.render_fd.take());
            return;
        }
        unsafe {
            // A rejected enumeration candidate or same-thread lifecycle test
            // must not unbind another owner's context. Production EGL uses the
            // same guard; registry sharing alone does not protect current state.
            if !self.context.is_null() && (self.get_current_context)() == self.context {
                (self.make_current)(
                    self.display,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
            }
            if !self.context.is_null() {
                (self.destroy_context)(self.display, self.context);
            }
            if !self.surface.is_null() {
                (self.destroy_surface)(self.display, self.surface);
            }
            crate::mpv::platform::release_headless_display(
                self.initialize,
                self.terminate,
                self.display,
            );
            ManuallyDrop::drop(&mut self._library);
        }
    }
}

pub(crate) fn layout(size: [u32; 2]) -> Result<(u32, u64), String> {
    if size.contains(&0) || size.iter().any(|&n| n > i32::MAX as u32) {
        return Err("invalid render dimensions".into());
    }
    let row = size[0]
        .checked_mul(4)
        .and_then(|n| n.checked_add(255))
        .map(|n| n & !255)
        .ok_or("row stride overflow")?;
    let bytes = u64::from(row)
        .checked_mul(u64::from(size[1]))
        .ok_or("buffer size overflow")?;
    if bytes > isize::MAX as u64 {
        return Err("buffer exceeds GL address range".into());
    }
    Ok((row, bytes))
}

/// All GL resources must be destroyed before `egl`; all GPU work is completed
/// before return from copy. The HAL buffer alone owns Vulkan buffer + memory.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    NotShared,
    DoneSignaled,
    GlOwned,
    ReadySignaled,
    Acquiring,
    Releasing,
    VulkanOwned,
}

/// A positive proof requires every native submission domain, in this order.
/// No external semaphore operations or mpv destruction occur in this helper.
fn prove_completion(
    gl: impl FnOnce() -> Result<(), String>,
    raw: impl FnOnce() -> Result<(), String>,
    wgpu: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    gl()?;
    raw()?;
    wgpu()
}

fn completed_raw_phase(phase: Phase) -> Result<Phase, String> {
    match phase {
        Phase::Acquiring => Ok(Phase::VulkanOwned),
        Phase::Releasing => Ok(Phase::DoneSignaled),
        _ => Err("pending raw fence has no submitted ownership transition".into()),
    }
}

struct Allocation {
    egl: Rc<Egl>,
    pub fbo: u32,
    color: u32,
    pbo: u32,
    memory: u32,
    buffer: Option<wgpu::Buffer>,
    raw_buffer: vk::Buffer,
    // Owns allocation until transferred into the HAL buffer.
    pending_memory: vk::DeviceMemory,
    raw: ash::Device,
    raw_queue: vk::Queue,
    queue_gate: Arc<Mutex<()>>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    raw_pending: AtomicBool,
    hal_owned: bool,
    shutdown_complete: bool,
    phase: Cell<Phase>,
    ready: vk::Semaphore,
    done: vk::Semaphore,
    gl_ready: u32,
    gl_done: u32,
    family: u32,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    size: [u32; 2],
    row: u32,
    bytes: u64,
}
impl Allocation {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        queue_gate: Arc<Mutex<()>>,
        size: [u32; 2],
        existing_egl: Option<Rc<Egl>>,
    ) -> Result<Self, String> {
        let (row, bytes) = layout(size)?;
        if size
            .iter()
            .any(|&n| n > device.limits().max_texture_dimension_2d)
            || bytes > device.limits().max_buffer_size
        {
            return Err("render dimensions exceed device limits".into());
        }
        unsafe {
            let hal = device
                .as_hal::<wgpu_hal::api::Vulkan>()
                .ok_or("requires a Vulkan wgpu device")?;
            let raw = hal.raw_device().clone();
            let instance = hal.shared_instance();
            let physical = hal.raw_physical_device();
            let mut ids = vk::PhysicalDeviceIDProperties::default();
            let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut ids);
            instance
                .raw_instance()
                .get_physical_device_properties2(physical, &mut properties);
            if properties.properties.vendor_id != 0x8086 {
                return Err("this proof currently supports Intel Mesa only".into());
            }
            let egl = match existing_egl {
                Some(egl) => {
                    egl.verify_uuid(ids.device_uuid)?;
                    egl
                }
                None => Rc::new(Egl::new(ids.device_uuid)?),
            };
            if !hal
                .enabled_device_extensions()
                .contains(&ash::khr::external_memory_fd::NAME)
            {
                return Err("Vulkan external_memory_fd was not enabled".into());
            }
            if !hal
                .enabled_device_extensions()
                .contains(&ash::khr::external_semaphore_fd::NAME)
            {
                return Err("Vulkan external_semaphore_fd was not enabled (experimental HAL patch required)".into());
            }
            let mut semaphore_caps = vk::ExternalSemaphoreProperties::default();
            instance
                .raw_instance()
                .get_physical_device_external_semaphore_properties(
                    physical,
                    &vk::PhysicalDeviceExternalSemaphoreInfo::default()
                        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD),
                    &mut semaphore_caps,
                );
            if !semaphore_caps
                .external_semaphore_features
                .contains(vk::ExternalSemaphoreFeatureFlags::EXPORTABLE)
            {
                return Err("Vulkan opaque-FD semaphore export unsupported".into());
            }
            let semaphore_api =
                ash::khr::external_semaphore_fd::Device::new(instance.raw_instance(), &raw);
            let family = hal.queue_family_index();
            let raw_queue = hal.raw_queue();
            let mut external = vk::ExternalMemoryBufferCreateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            let info = vk::BufferCreateInfo::default()
                .size(bytes)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .push_next(&mut external);
            let mut capability = vk::ExternalBufferProperties::default();
            instance
                .raw_instance()
                .get_physical_device_external_buffer_properties(
                    physical,
                    &vk::PhysicalDeviceExternalBufferInfo::default()
                        .usage(info.usage)
                        .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD),
                    &mut capability,
                );
            let features = capability
                .external_memory_properties
                .external_memory_features;
            if !features.contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE)
                || features.contains(vk::ExternalMemoryFeatureFlags::DEDICATED_ONLY)
            {
                return Err("opaque-FD buffer export unsupported or dedicated-only".into());
            }
            let memory_properties = instance
                .raw_instance()
                .get_physical_device_memory_properties(physical);
            let fd_api = ash::khr::external_memory_fd::Device::new(instance.raw_instance(), &raw);
            drop(hal);
            let raw_buffer = raw
                .create_buffer(&info, None)
                .map_err(|e| format!("create external buffer: {e}"))?;
            let mut this = Self {
                egl,
                fbo: 0,
                color: 0,
                pbo: 0,
                memory: 0,
                buffer: None,
                raw_buffer,
                pending_memory: vk::DeviceMemory::null(),
                raw,
                raw_queue,
                queue_gate,
                pool: vk::CommandPool::null(),
                command: vk::CommandBuffer::null(),
                fence: vk::Fence::null(),
                raw_pending: AtomicBool::new(false),
                hal_owned: false,
                shutdown_complete: false,
                phase: Cell::new(Phase::NotShared),
                ready: vk::Semaphore::null(),
                done: vk::Semaphore::null(),
                gl_ready: 0,
                gl_done: 0,
                family,
                device,
                queue,
                size,
                row,
                bytes,
            };
            let req = this.raw.get_buffer_memory_requirements(raw_buffer);
            let index = (0..memory_properties.memory_type_count)
                .find(|&i| {
                    req.memory_type_bits & (1 << i) != 0
                        && memory_properties.memory_types[i as usize]
                            .property_flags
                            .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                })
                .ok_or("no exportable device-local memory type")?;
            let mut export = vk::ExportMemoryAllocateInfo::default()
                .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
            this.pending_memory = this
                .raw
                .allocate_memory(
                    &vk::MemoryAllocateInfo::default()
                        .allocation_size(req.size)
                        .memory_type_index(index)
                        .push_next(&mut export),
                    None,
                )
                .map_err(|e| format!("allocate external memory: {e}"))?;
            this.raw
                .bind_buffer_memory(raw_buffer, this.pending_memory, 0)
                .map_err(|e| format!("bind external memory: {e}"))?;
            let fd = OwnedFd::from_raw_fd(
                fd_api
                    .get_memory_fd(
                        &vk::MemoryGetFdInfoKHR::default()
                            .memory(this.pending_memory)
                            .handle_type(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD),
                    )
                    .map_err(|e| format!("export memory FD: {e}"))?,
            );
            this.pool = this
                .raw
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .map_err(|e| e.to_string())?;
            this.command = this
                .raw
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(this.pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .map_err(|e| e.to_string())?[0];
            this.fence = this
                .raw
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(|e| e.to_string())?;
            for ready in [true, false] {
                let mut export = vk::ExportSemaphoreCreateInfo::default()
                    .handle_types(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);
                let semaphore = this
                    .raw
                    .create_semaphore(
                        &vk::SemaphoreCreateInfo::default().push_next(&mut export),
                        None,
                    )
                    .map_err(|e| e.to_string())?;
                if ready {
                    this.ready = semaphore;
                } else {
                    this.done = semaphore;
                }
                let fd = OwnedFd::from_raw_fd(
                    semaphore_api
                        .get_semaphore_fd(
                            &vk::SemaphoreGetFdInfoKHR::default()
                                .semaphore(semaphore)
                                .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD),
                        )
                        .map_err(|e| e.to_string())?,
                );
                let gl_name = if ready {
                    &mut this.gl_ready
                } else {
                    &mut this.gl_done
                };
                (this.egl.gl.gen_semaphores)(1, gl_name);
                this.egl.check("create GL external semaphore")?;
                (this.egl.gl.import_semaphore)(*gl_name, 0x9586, fd.as_raw_fd());
                this.egl.check("import external semaphore FD")?;
                let _ = fd.into_raw_fd(); // successful import transfers ownership
            }
            // Fill ALL padding before the fully-initialized HAL import contract.
            // First release signals Done, which GL consumes before first use.
            this.raw_handoff(false, true)?;
            // Mark ownership BEFORE passing raw resources across the HAL API:
            // unwinding must never free these copied Vulkan handles twice.
            this.hal_owned = true;
            let memory = std::mem::replace(&mut this.pending_memory, vk::DeviceMemory::null());
            let buffer =
                wgpu_hal::vulkan::Buffer::from_raw_managed(raw_buffer, memory, 0, req.size);
            this.buffer = Some(this.device.create_buffer_from_hal::<wgpu_hal::api::Vulkan>(
                buffer,
                &wgpu::BufferDescriptor {
                    label: Some("external GL pixel-pack buffer"),
                    size: bytes,
                    usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            ));
            this.pending_memory = vk::DeviceMemory::null();
            let gl = &this.egl.gl;
            (gl.create_memory)(1, &mut this.memory);
            this.egl.check("create GL memory object")?;
            // EXT_external_objects_fd transfers ownership only on success.
            (gl.import_memory)(this.memory, req.size, 0x9586, fd.as_raw_fd());
            this.egl.check("import opaque memory FD")?;
            let _ = fd.into_raw_fd();
            (gl.gen_buffers)(1, &mut this.pbo);
            this.egl.check("generate shared PBO")?;
            (gl.bind_buffer)(0x88eb, this.pbo);
            (gl.storage)(0x88eb, bytes as isize, this.memory, 0);
            (gl.bind_buffer)(0x88eb, 0);
            (gl.gen_textures)(1, &mut this.color);
            this.egl.check("generate FBO color texture")?;
            (gl.bind_texture)(0x0de1, this.color);
            (gl.tex_image)(
                0x0de1,
                0,
                0x8058,
                size[0] as i32,
                size[1] as i32,
                0,
                0x1908,
                0x1401,
                ptr::null(),
            );
            (gl.gen_fbos)(1, &mut this.fbo);
            this.egl.check("generate mpv FBO")?;
            (gl.bind_fbo)(0x8d40, this.fbo);
            (gl.attach)(0x8d40, 0x8ce0, 0x0de1, this.color, 0);
            if (gl.check_fbo)(0x8d40) != 0x8cd5 {
                return Err("incomplete mpv framebuffer".into());
            }
            this.egl.check("create shared PBO and mpv FBO")?;
            Ok(this)
        }
    }
    unsafe fn barrier(&self, cmd: vk::CommandBuffer, acquire: bool) {
        let (src, dst, src_access, dst_access) = if acquire {
            (
                vk::QUEUE_FAMILY_EXTERNAL,
                self.family,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_READ,
            )
        } else {
            (
                self.family,
                vk::QUEUE_FAMILY_EXTERNAL,
                vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::empty(),
            )
        };
        let barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(src_access)
            .dst_access_mask(dst_access)
            .src_queue_family_index(src)
            .dst_queue_family_index(dst)
            .buffer(self.raw_buffer)
            .offset(0)
            .size(self.bytes);
        unsafe {
            self.raw.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
    }
    /// Queue operations are externally synchronized with GPUI's submissions and
    /// presentation. The gate covers only submission, never fence/GPU waiting.
    fn raw_handoff(&self, acquire: bool, initialize: bool) -> Result<(), String> {
        unsafe {
            self.raw
                .reset_fences(&[self.fence])
                .map_err(|e| e.to_string())?;
            self.raw
                .reset_command_buffer(self.command, vk::CommandBufferResetFlags::empty())
                .map_err(|e| e.to_string())?;
            self.raw
                .begin_command_buffer(
                    self.command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|e| e.to_string())?;
            if initialize {
                self.raw
                    .cmd_fill_buffer(self.command, self.raw_buffer, 0, self.bytes, 0);
            }
            self.barrier(self.command, acquire);
            self.raw
                .end_command_buffer(self.command)
                .map_err(|e| e.to_string())?;
            let commands = [self.command];
            let ready = [self.ready];
            let done = [self.done];
            let stages = [vk::PipelineStageFlags::ALL_COMMANDS];
            let mut submit = vk::SubmitInfo::default().command_buffers(&commands);
            submit = if acquire {
                submit.wait_semaphores(&ready).wait_dst_stage_mask(&stages)
            } else {
                submit.signal_semaphores(&done)
            };
            {
                let _gate = self.queue_gate.lock().map_err(|_| "queue gate poisoned")?;
                self.raw
                    .queue_submit(self.raw_queue, &[submit], self.fence)
                    .map_err(|e| format!("external buffer handoff submit: {e}"))?;
            }
            self.phase.set(if acquire {
                Phase::Acquiring
            } else {
                Phase::Releasing
            });
            self.raw_pending.store(true, Ordering::Release);
            self.drain_raw()
        }
    }
    /// Retire a previously submitted raw handoff without re-signaling or
    /// consuming either semaphore. A timeout leaves phase/pending unchanged.
    fn drain_raw(&self) -> Result<(), String> {
        if self.raw_pending.load(Ordering::Acquire) {
            let completed = completed_raw_phase(self.phase.get())?;
            unsafe {
                self.raw
                    .wait_for_fences(&[self.fence], true, 5_000_000_000)
                    .map_err(|e| format!("completion raw fence (5s): {e}"))?;
            }
            self.phase.set(completed);
            self.raw_pending.store(false, Ordering::Release);
        }
        Ok(())
    }
    pub(crate) fn complete_submissions(&self) -> Result<(), String> {
        if self.egl.quarantined.get() {
            return Err("completion proof unavailable: EGL context quarantined".into());
        }
        let result = prove_completion(
            || self.egl.complete_gl(),
            || self.drain_raw(),
            || {
                self.device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(5)),
                    })
                    .map_err(|e| format!("worker retirement wait: {e}"))?;
                Ok(())
            },
        );
        if result.is_err() {
            self.egl.quarantined.set(true);
        }
        result
    }
    fn wait(&self, encoder: wgpu::CommandEncoder) -> Result<(), String> {
        let command = encoder.finish();
        let submission = {
            let _gate = self.queue_gate.lock().map_err(|_| "queue gate poisoned")?;
            self.queue.submit([command])
        };
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(5)),
            })
            .map_err(|e| format!("worker GPU wait failed: {e}"))?;
        Ok(())
    }
    /// Fallible, worker-only shutdown. Unknown/in-flight resources are never
    /// destroyed on timeout; Drop quarantines them for the rest of the process.
    /// This is a fatal-probe safety policy, NOT driver-hang recovery.
    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        let result = self.finish_shutdown();
        if result.is_err() {
            // A failed retirement during resize must not later retry an
            // uncertain semaphore consume when the parent tears libmpv down.
            self.egl.quarantined.set(true);
        }
        result
    }
    fn finish_shutdown(&mut self) -> Result<(), String> {
        unsafe {
            self.complete_submissions()?;
            if self.phase.get() == Phase::ReadySignaled {
                self.raw_handoff(true, false)?;
            }
            if self.phase.get() == Phase::DoneSignaled {
                (self.egl.gl.wait_semaphore)(
                    self.gl_done,
                    1,
                    &self.pbo,
                    0,
                    ptr::null(),
                    ptr::null(),
                );
                self.egl.check("shutdown consume Done")?;
                self.phase.set(Phase::GlOwned);
            }
            // mpv_render_context_free may have enqueued cleanup after the
            // pre-free proof; consuming Done above may also enqueue GL work.
            if let Err(error) = self.egl.complete_gl() {
                self.egl.quarantined.set(true);
                return Err(error);
            }
            self.shutdown_complete = true;
            Ok(())
        }
    }
    pub fn copy_frame(&self) -> Result<wgpu::Texture, String> {
        unsafe {
            let gl = &self.egl.gl;
            (gl.wait_semaphore)(self.gl_done, 1, &self.pbo, 0, ptr::null(), ptr::null());
            self.egl.check("GL wait Done semaphore")?;
            self.phase.set(Phase::GlOwned);
            (gl.bind_fbo)(0x8d40, self.fbo);
            (gl.bind_buffer)(0x88eb, self.pbo);
            (gl.pixel_store)(0x0d05, 4); // PACK_ALIGNMENT
            (gl.pixel_store)(0x0d02, (self.row / 4) as i32); // PACK_ROW_LENGTH
            (gl.read_pixels)(
                0,
                0,
                self.size[0] as i32,
                self.size[1] as i32,
                0x1908,
                0x1401,
                ptr::null_mut(),
            );
            (gl.bind_buffer)(0x88eb, 0);
            (gl.pixel_store)(0x0d02, 0);
            self.egl.check("GL pixel pack")?;
            (gl.signal_semaphore)(self.gl_ready, 1, &self.pbo, 0, ptr::null(), ptr::null());
            self.egl.check("GL signal Ready semaphore")?;
            self.phase.set(Phase::ReadySignaled);
            (gl.flush)();
            self.egl.check("GL shared-PBO read + semaphore signal")?;
            self.raw_handoff(true, false)?;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("immutable mpv GPU frame"),
                size: wgpu::Extent3d {
                    width: self.size[0],
                    height: self.size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: self.buffer.as_ref().unwrap(),
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.row),
                        rows_per_image: Some(self.size[1]),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                texture.size(),
            );
            self.wait(encoder)?;
            self.raw_handoff(false, false)?; // release to GL and signal Done
            Ok(texture)
        }
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.shutdown_complete {
            // Nonblocking fallback, including initialization failures/unwind.
            // Keep native context/library/device and all resources alive rather
            // than freeing anything that an untracked raw submit might use.
            self.egl.quarantined.set(true);
            std::mem::forget(self.buffer.take());
            std::mem::forget(self.device.clone());
            std::mem::forget(self.queue.clone());
            eprintln!(
                "GPU experiment: incomplete shutdown; GPU resources quarantined until process exit"
            );
            return;
        }
        unsafe {
            let gl = &self.egl.gl;
            (gl.delete_fbos)(1, &self.fbo);
            (gl.delete_textures)(1, &self.color);
            (gl.delete_buffers)(1, &self.pbo);
            (gl.delete_memory)(1, &self.memory);
            (gl.delete_semaphores)(1, &self.gl_ready);
            (gl.delete_semaphores)(1, &self.gl_done);
            if self.pool != vk::CommandPool::null() {
                self.raw.destroy_command_pool(self.pool, None);
            }
            if self.fence != vk::Fence::null() {
                self.raw.destroy_fence(self.fence, None);
            }
            for semaphore in [self.ready, self.done] {
                if semaphore != vk::Semaphore::null() {
                    self.raw.destroy_semaphore(semaphore, None);
                }
            }
            if !self.hal_owned {
                self.raw.destroy_buffer(self.raw_buffer, None);
                if self.pending_memory != vk::DeviceMemory::null() {
                    self.raw.free_memory(self.pending_memory, None);
                }
            }
            // HAL frees Vulkan allocation exactly once, only after GL releases it.
            drop(self.buffer.take());
            let _ = self.device.poll(wgpu::PollType::Poll);
        }
    }
}

/// Worker-local native backend. The context/proc addresses outlive every target
/// allocation, including resolution changes while libmpv retains its GL callback.
pub(super) struct Interop {
    allocation: Option<Allocation>,
    egl: Rc<Egl>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    queue_gate: Arc<Mutex<()>>,
    stopped: bool,
}
impl Interop {
    pub(super) fn new(context: &super::ExperimentalGpuContext) -> Result<Self, String> {
        let allocation = Allocation::new(
            context.device.clone(),
            context.queue.clone(),
            context.queue_gate.clone(),
            [1, 1],
            None,
        )?;
        Ok(Self {
            egl: allocation.egl.clone(),
            allocation: Some(allocation),
            device: context.device.clone(),
            queue: context.queue.clone(),
            queue_gate: context.queue_gate.clone(),
            stopped: false,
        })
    }
    pub(super) fn get_proc_address(&self, name: &CStr) -> *const c_void {
        unsafe { (self.egl.get_proc)(name.as_ptr()) }
    }
    pub(super) fn drm_render_fd(&self) -> Option<i32> {
        self.egl.render_fd.as_ref().map(AsRawFd::as_raw_fd)
    }
    pub(super) fn begin_frame(&mut self, size: [u32; 2]) -> Result<i32, String> {
        if self.stopped || self.egl.quarantined.get() {
            return Err("experimental GPU context is stopped or quarantined".into());
        }
        // Validate before retiring a usable target. No context is recreated here.
        let (_, bytes) = layout(size)?;
        if size
            .iter()
            .any(|&n| n > self.device.limits().max_texture_dimension_2d)
            || bytes > self.device.limits().max_buffer_size
        {
            return Err("render dimensions exceed device limits".into());
        }
        if self.allocation.as_ref().is_none_or(|a| a.size != size) {
            if let Some(allocation) = self.allocation.as_mut() {
                allocation.shutdown()?;
            }
            drop(self.allocation.take());
            self.allocation = Some(Allocation::new(
                self.device.clone(),
                self.queue.clone(),
                self.queue_gate.clone(),
                size,
                Some(self.egl.clone()),
            )?);
        }
        Ok(self.allocation.as_ref().ok_or("missing GPU target")?.fbo as i32)
    }
    pub(super) fn finish_frame(&mut self) -> Result<super::NativeFrame, String> {
        if self.stopped || self.egl.quarantined.get() {
            return Err("experimental GPU context is stopped or quarantined".into());
        }
        self.allocation
            .as_ref()
            .ok_or("missing GPU target")?
            .copy_frame()
            .map(super::NativeFrame::Texture)
    }
    pub(super) fn complete_submissions(&self) -> Result<(), String> {
        match &self.allocation {
            Some(allocation) => allocation.complete_submissions(),
            None => {
                if self.egl.quarantined.get() {
                    return Err("completion proof unavailable: EGL context quarantined".into());
                }
                let result = prove_completion(
                    || self.egl.complete_gl(),
                    || Ok(()),
                    || {
                        self.device
                            .poll(wgpu::PollType::Wait {
                                submission_index: None,
                                timeout: Some(Duration::from_secs(5)),
                            })
                            .map_err(|e| format!("worker retirement wait: {e}"))?;
                        Ok(())
                    },
                );
                if result.is_err() {
                    self.egl.quarantined.set(true);
                }
                result
            }
        }
    }
    /// Called only after libmpv frees its render context. EGL and its verified DRM
    /// FD remain owned until this backend is dropped on the same worker.
    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        self.stopped = true;
        if let Some(allocation) = self.allocation.as_mut() {
            allocation.shutdown()?;
        } else {
            self.complete_submissions()?;
        }
        drop(self.allocation.take());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Phase, completed_raw_phase, layout, prove_completion};
    use std::cell::RefCell;

    #[test]
    fn completion_proof_requires_all_domains_and_stops_at_first_failure() {
        for fail_at in [None, Some(0), Some(1), Some(2)] {
            let calls = RefCell::new(Vec::new());
            let step = |index| {
                calls.borrow_mut().push(index);
                if fail_at == Some(index) {
                    Err("mock incomplete submission".into())
                } else {
                    Ok(())
                }
            };
            let proof = prove_completion(|| step(0), || step(1), || step(2));
            assert_eq!(proof.is_ok(), fail_at.is_none());
            let last = fail_at.unwrap_or(2);
            assert_eq!(*calls.borrow(), (0..=last).collect::<Vec<_>>());
        }
    }

    #[test]
    fn raw_fence_completion_records_only_the_submitted_transition() {
        assert!(matches!(
            completed_raw_phase(Phase::Acquiring),
            Ok(Phase::VulkanOwned)
        ));
        assert!(matches!(
            completed_raw_phase(Phase::Releasing),
            Ok(Phase::DoneSignaled)
        ));
        // In particular, a completion proof must not consume Done or invent a
        // Ready acquisition: only explicit handoff/shutdown changes these states.
        for phase in [
            Phase::NotShared,
            Phase::DoneSignaled,
            Phase::GlOwned,
            Phase::ReadySignaled,
            Phase::VulkanOwned,
        ] {
            assert!(completed_raw_phase(phase).is_err());
        }
    }
    #[test]
    fn padded_rows() {
        assert_eq!(layout([65, 3]).unwrap(), (512, 1536));
        assert_eq!(layout([64, 1]).unwrap(), (256, 256));
    }
    #[test]
    fn rejects_invalid_or_overflowing_dimensions() {
        for size in [[0, 1], [1, 0], [u32::MAX, 1], [i32::MAX as u32, 1]] {
            assert!(layout(size).is_err());
        }
    }
}
