//! Worker-local, GPU-resident ANGLE/D3D11On12 bridge on the supplied WGPU
//! Dx12 device/queue. No decoder/playback qualification, HDR, or CPU pixel access.
//! Native DLL/COM/EGL owners are quarantined together if completion is unknown.
//!
//! Source state cycle: COMMON (fresh) -> explicit RENDER_TARGET -> ANGLE render
//! -> ReleaseWrappedResources/Flush COMMON -> fresh WGPU import/copy COPY_SOURCE
//! -> explicit RENDER_TARGET next frame. Acquire only marks hazard tracking.
//! WGPU-import tracker state must never be reused after out-of-band ANGLE writes.
//! Microsoft API references: D3D11On12CreateDevice, CreateWrappedResource,
//! AcquireWrappedResources, D3D11_CREATE_DEVICE_VIDEO_SUPPORT. The flag alone
//! is not proof of direct video decode: device/context/profile queries and the
//! parent's actual libmpv hwdec evidence are independently required.
use libloading::os::windows::Library;
use std::{
    cell::Cell,
    ffi::{CStr, c_void},
    future::Future,
    mem::ManuallyDrop,
    ptr::null_mut,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0, Direct3D11::*, Direct3D11on12::*, Direct3D12::*,
        Dxgi::Common::*,
    },
    core::{IUnknown, Interface},
};

type Ptr = *mut c_void;
type Bool = u32;
type Int = i32;
const NONE: Int = 0x3038;
const D3D11_DEVICE: u32 = 0x33a1;
const PLATFORM_DEVICE: u32 = 0x313f;
const D3D_TEXTURE: u32 = 0x33a3;
const TIMEOUT: Duration = Duration::from_secs(5);

trait At<T> {
    fn at(self, stage: &str) -> Result<T, String>;
}
impl<T> At<T> for windows::core::Result<T> {
    fn at(self, stage: &str) -> Result<T, String> {
        self.map_err(|e| format!("{stage}: HRESULT 0x{:08x}", e.code().0 as u32))
    }
}
fn required<T>(value: Option<T>, stage: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{stage}: null native result"))
}

// All signatures below match EGL 1.5, ANGLE extension headers and GLES2 gl2.h.
// DLL search uses explicit Config overrides or the executable's directory;
// no process-wide DLL search path or environment is modified.
struct Egl {
    _gles: Library,
    _egl: Library,
    get_error: unsafe extern "system" fn() -> Int,
    query_string: unsafe extern "system" fn(Ptr, Int) -> *const i8,
    create_device: unsafe extern "system" fn(u32, Ptr, *const isize) -> Ptr,
    release_device: unsafe extern "system" fn(Ptr) -> Bool,
    get_platform_display: unsafe extern "system" fn(u32, Ptr, *const Int) -> Ptr,
    initialize: unsafe extern "system" fn(Ptr, *mut Int, *mut Int) -> Bool,
    query_device_attrib: unsafe extern "system" fn(Ptr, Int, *mut isize) -> Bool,
    query_device_string: unsafe extern "system" fn(Ptr, Int) -> *const i8,
    choose_config: unsafe extern "system" fn(Ptr, *const Int, *mut Ptr, Int, *mut Int) -> Bool,
    bind_api: unsafe extern "system" fn(u32) -> Bool,
    create_context: unsafe extern "system" fn(Ptr, Ptr, Ptr, *const Int) -> Ptr,
    create_pbuffer: unsafe extern "system" fn(Ptr, u32, Ptr, Ptr, *const Int) -> Ptr,
    make_current: unsafe extern "system" fn(Ptr, Ptr, Ptr, Ptr) -> Bool,
    destroy_surface: unsafe extern "system" fn(Ptr, Ptr) -> Bool,
    destroy_context: unsafe extern "system" fn(Ptr, Ptr) -> Bool,
    terminate: unsafe extern "system" fn(Ptr) -> Bool,
    flush: unsafe extern "system" fn(),
    gl_error: unsafe extern "system" fn() -> u32,
    device: Ptr,
    display: Ptr,
    context: Ptr,
    surface: Ptr,
    bootstrap: Ptr,
    config: Ptr,
    get_proc: unsafe extern "system" fn(*const i8) -> Ptr,
    create_bootstrap: unsafe extern "system" fn(Ptr, Ptr, *const Int) -> Ptr,
}

impl Egl {
    unsafe fn load(config: &crate::Config) -> Result<Self, String> {
        let directory = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_owned()))
            .ok_or("cannot locate executable directory")?;
        let gles_path = config
            .gles_library_path
            .clone()
            .unwrap_or_else(|| directory.join("libGLESv2.dll"));
        let egl_path = config
            .egl_library_path
            .clone()
            .unwrap_or_else(|| directory.join("libEGL.dll"));
        // Dependencies resolve beside each explicitly loaded DLL; never enable
        // legacy CWD lookup or mutate global DLL directories for another player.
        let gles: Library = unsafe { crate::mpv::library::open_native_library(&gles_path) }
            .map_err(|_| "cannot load ANGLE libGLESv2.dll; provision beside executable or set Config::gles_library_path")?.into();
        let egl: Library = unsafe { crate::mpv::library::open_native_library(&egl_path) }
            .map_err(|_| "cannot load ANGLE libEGL.dll; provision beside executable or set Config::egl_library_path")?.into();
        let get_proc: unsafe extern "system" fn(*const i8) -> Ptr =
            *unsafe { egl.get(b"eglGetProcAddress\0") }.map_err(|_| "missing eglGetProcAddress")?;
        macro_rules! symbol {
            ($library:expr, $name:literal) => {
                *unsafe { $library.get(concat!($name, "\0").as_bytes()) }
                    .map_err(|_| concat!("missing ANGLE export ", $name))?
            };
        }
        macro_rules! extension {
            ($name:literal, $ty:ty) => {{
                let address = unsafe { get_proc(concat!($name, "\0").as_ptr().cast()) };
                if address.is_null() {
                    return Err(concat!("missing ANGLE entry point ", $name).into());
                }
                unsafe { std::mem::transmute::<Ptr, $ty>(address) }
            }};
        }
        Ok(Self {
            get_error: symbol!(egl, "eglGetError"),
            query_string: symbol!(egl, "eglQueryString"),
            create_device: extension!(
                "eglCreateDeviceANGLE",
                unsafe extern "system" fn(u32, Ptr, *const isize) -> Ptr
            ),
            release_device: extension!(
                "eglReleaseDeviceANGLE",
                unsafe extern "system" fn(Ptr) -> Bool
            ),
            get_platform_display: extension!(
                "eglGetPlatformDisplayEXT",
                unsafe extern "system" fn(u32, Ptr, *const Int) -> Ptr
            ),
            query_device_attrib: extension!(
                "eglQueryDeviceAttribEXT",
                unsafe extern "system" fn(Ptr, Int, *mut isize) -> Bool
            ),
            query_device_string: extension!(
                "eglQueryDeviceStringEXT",
                unsafe extern "system" fn(Ptr, Int) -> *const i8
            ),
            initialize: symbol!(egl, "eglInitialize"),
            choose_config: symbol!(egl, "eglChooseConfig"),
            bind_api: symbol!(egl, "eglBindAPI"),
            create_context: symbol!(egl, "eglCreateContext"),
            create_pbuffer: symbol!(egl, "eglCreatePbufferFromClientBuffer"),
            make_current: symbol!(egl, "eglMakeCurrent"),
            destroy_surface: symbol!(egl, "eglDestroySurface"),
            destroy_context: symbol!(egl, "eglDestroyContext"),
            terminate: symbol!(egl, "eglTerminate"),
            flush: symbol!(gles, "glFlush"),
            gl_error: symbol!(gles, "glGetError"),
            _gles: gles,
            device: null_mut(),
            display: null_mut(),
            context: null_mut(),
            surface: null_mut(),
            bootstrap: null_mut(),
            config: null_mut(),
            get_proc,
            create_bootstrap: symbol!(egl, "eglCreatePbufferSurface"),
            _egl: egl,
        })
    }

    fn check(&self, result: Bool, stage: &str) -> Result<(), String> {
        if result == 0 {
            Err(format!("{stage}: EGL 0x{:04x}", unsafe {
                (self.get_error)()
            }))
        } else {
            Ok(())
        }
    }

    fn extensions(&self, display: Ptr, required: &[&str]) -> Result<(), String> {
        let pointer = unsafe { (self.query_string)(display, 0x3055) };
        if pointer.is_null() {
            return Err("ANGLE did not report required extensions".into());
        }
        let extensions = unsafe { CStr::from_ptr(pointer) }.to_bytes();
        for name in required {
            if !extensions
                .split(|b| *b == b' ')
                .any(|s| s == name.as_bytes())
            {
                return Err(format!("ANGLE missing required extension {name}"));
            }
        }
        Ok(())
    }

    unsafe fn initialize(&mut self, device: &ID3D11Device) -> Result<(), String> {
        self.extensions(
            null_mut(),
            &[
                "EGL_ANGLE_device_creation",
                "EGL_ANGLE_device_creation_d3d11",
                "EGL_EXT_platform_device",
                "EGL_EXT_device_query",
            ],
        )?;
        self.device = unsafe {
            (self.create_device)(D3D11_DEVICE, device.as_raw(), [NONE as isize].as_ptr())
        };
        self.check((!self.device.is_null()) as Bool, "eglCreateDeviceANGLE")?;
        self.display =
            unsafe { (self.get_platform_display)(PLATFORM_DEVICE, self.device, [NONE].as_ptr()) };
        self.check(
            (!self.display.is_null()) as Bool,
            "eglGetPlatformDisplayEXT",
        )?;
        self.check(
            unsafe { (self.initialize)(self.display, null_mut(), null_mut()) },
            "eglInitialize",
        )?;
        self.extensions(self.display, &["EGL_ANGLE_d3d_texture_client_buffer"])?;
        let device_extensions = unsafe { (self.query_device_string)(self.device, 0x3055) };
        if device_extensions.is_null()
            || !unsafe { CStr::from_ptr(device_extensions) }
                .to_bytes()
                .split(|b| *b == b' ')
                .any(|s| s == b"EGL_ANGLE_device_d3d")
        {
            return Err("ANGLE device missing EGL_ANGLE_device_d3d".into());
        }
        let mut queried = 0isize;
        self.check(
            unsafe { (self.query_device_attrib)(self.device, D3D11_DEVICE as Int, &mut queried) },
            "eglQueryDeviceAttribEXT",
        )?;
        // Compare canonical COM identities, not adapter names or interface aliases.
        let pointer = queried as Ptr;
        let actual = unsafe { ID3D11Device::from_raw_borrowed(&pointer) }
            .ok_or("ANGLE reported a null D3D11 device")?
            .cast::<IUnknown>()
            .at("ANGLE device IUnknown")?;
        let expected: IUnknown = device.cast().at("supplied device IUnknown")?;
        if actual.as_raw() != expected.as_raw() {
            return Err("ANGLE did not retain the supplied D3D11On12 device".into());
        }
        self.check(unsafe { (self.bind_api)(0x30a0) }, "eglBindAPI GLES")?;
        Ok(())
    }

    unsafe fn create_stable_context(&mut self) -> Result<(), String> {
        // EGL_RENDERABLE_TYPE=OPENGL_ES3_BIT_KHR; no ES2 fallback. libmpv
        // callbacks retain this context identity through every pbuffer resize.
        let attributes = [
            0x3033, 1, 0x3040, 0x40, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3021, 8, NONE,
        ];
        let mut count = 0;
        let mut config = null_mut();
        self.check(
            unsafe {
                (self.choose_config)(
                    self.display,
                    attributes.as_ptr(),
                    &mut config,
                    1,
                    &mut count,
                )
            },
            "eglChooseConfig GLES3 RGBA8",
        )?;
        self.config = config;
        if count != 1 || self.config.is_null() {
            return Err("ANGLE requires a GLES3 RGBA8 pbuffer config".into());
        }
        self.context = unsafe {
            (self.create_context)(
                self.display,
                self.config,
                null_mut(),
                [0x3098, 3, NONE].as_ptr(),
            )
        };
        self.check((!self.context.is_null()) as Bool, "eglCreateContext GLES3")?;
        self.bootstrap = unsafe {
            (self.create_bootstrap)(
                self.display,
                self.config,
                [0x3057, 4, 0x3056, 4, NONE].as_ptr(),
            )
        };
        self.check(
            (!self.bootstrap.is_null()) as Bool,
            "eglCreatePbufferSurface bootstrap",
        )?;
        self.current(self.bootstrap)
    }

    fn current(&self, surface: Ptr) -> Result<(), String> {
        self.check(
            unsafe { (self.make_current)(self.display, surface, surface, self.context) },
            "eglMakeCurrent stable context",
        )
    }

    unsafe fn replace_surface(&mut self, texture: &ID3D11Texture2D) -> Result<(), String> {
        self.current(self.bootstrap)?;
        if !self.surface.is_null() {
            self.check(
                unsafe { (self.destroy_surface)(self.display, self.surface) },
                "eglDestroySurface retired pbuffer",
            )?;
            self.surface = null_mut();
        }
        self.surface = unsafe {
            (self.create_pbuffer)(
                self.display,
                D3D_TEXTURE,
                texture.as_raw(),
                self.config,
                [NONE].as_ptr(),
            )
        };
        self.check(
            (!self.surface.is_null()) as Bool,
            "eglCreatePbufferFromClientBuffer wrapped D3D12 texture",
        )
    }

    fn get_proc_address(&self, name: &CStr) -> *const c_void {
        let address = unsafe { (self.get_proc)(name.as_ptr()) };
        if !address.is_null() {
            return address.cast_const();
        }
        unsafe { self._gles.get::<*const c_void>(name.to_bytes_with_nul()) }
            .map(|symbol| *symbol)
            .unwrap_or(std::ptr::null())
    }
}

impl Drop for Egl {
    fn drop(&mut self) {
        unsafe {
            if !self.display.is_null() {
                (self.make_current)(self.display, null_mut(), null_mut(), null_mut());
                if !self.surface.is_null() {
                    (self.destroy_surface)(self.display, self.surface);
                }
                if !self.bootstrap.is_null() {
                    (self.destroy_surface)(self.display, self.bootstrap);
                }
                if !self.context.is_null() {
                    (self.destroy_context)(self.display, self.context);
                }
                (self.terminate)(self.display);
            }
            if !self.device.is_null() {
                (self.release_device)(self.device);
            }
        }
    }
}

struct Target {
    raw: ID3D12Resource,
    texture11: ID3D11Texture2D,
    wrapped: ID3D11Resource,
    _rtv: ID3D11RenderTargetView,
    size: [u32; 2],
    state: D3D12_RESOURCE_STATES,
}

/// All resources referenced by commands belong to this single graph. Neither
/// allocator/list recycling nor DLL unloading may precede a proven queue fence.
struct Native {
    egl: Egl,
    target: Option<Target>,
    retired: Vec<Target>,
    imported: Option<wgpu::Texture>,
    output: Option<wgpu::Texture>,
    commands: Vec<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)>,
    device11: Option<ID3D11Device>,
    context11: Option<ID3D11DeviceContext>,
    on12: Option<ID3D11On12Device>,
    video: Option<ID3D11VideoDevice>,
    video_context: Option<ID3D11VideoContext>,
    multithread: Option<ID3D11Multithread>,
    native_fence: ID3D12Fence,
    native_device: ID3D12Device,
    native_queue: ID3D12CommandQueue,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    gate: Arc<Mutex<()>>,
    pending: Cell<bool>,
    // None while pending means an early error prevented arming a completion
    // fence. An older signaled fence must NEVER release this owner graph.
    wait_target: Cell<Option<u64>>,
    next_fence: Cell<u64>,
    submission: Option<wgpu::SubmissionIndex>,
    active: bool,
}

pub(super) struct NativeInterop {
    native: Option<Box<Native>>,
}

impl NativeInterop {
    pub(super) fn new(
        context: &super::GpuSurfaceContext,
        config: &crate::Config,
    ) -> Result<Self, String> {
        let device = Arc::clone(&context.device);
        let queue = Arc::clone(&context.queue);
        let (native_device, native_queue) = {
            let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() }
                .ok_or("GPU surface Windows playback requires the actual GPUI WGPU Dx12 device; Vulkan is unsupported")?;
            let hal_queue = unsafe { queue.as_hal::<wgpu_hal::api::Dx12>() }
                .ok_or("GPU surface Windows playback requires the supplied WGPU Dx12 queue")?;
            (hal_device.raw_device().clone(), hal_queue.as_raw().clone())
        };
        // Queue identity comes from the supplied WGPU queue, not Device::raw_queue
        // (which is a presentation queue detail) and never a separately selected
        // adapter. Verify the queue's owning COM device canonically as well.
        let mut queue_device: Option<ID3D12Device> = None;
        unsafe { native_queue.GetDevice(&mut queue_device) }.at("Dx12 queue GetDevice")?;
        let queue_device = required(queue_device, "Dx12 queue device")?;
        if queue_device
            .cast::<IUnknown>()
            .at("queue device identity")?
            .as_raw()
            != native_device
                .cast::<IUnknown>()
                .at("WGPU device identity")?
                .as_raw()
        {
            return Err("supplied WGPU Dx12 device/queue identities differ".into());
        }
        let native_fence = unsafe { native_device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }
            .at("Create native interop fence")?;
        let mut interop = Self {
            native: Some(Box::new(Native {
                egl: unsafe { Egl::load(config)? },
                target: None,
                retired: Vec::new(),
                imported: None,
                output: None,
                commands: Vec::new(),
                device11: None,
                context11: None,
                on12: None,
                video: None,
                video_context: None,
                multithread: None,
                native_fence,
                native_device,
                native_queue,
                device,
                queue,
                gate: Arc::clone(&context.queue_gate),
                pending: Cell::new(false),
                wait_target: Cell::new(None),
                next_fence: Cell::new(1),
                submission: None,
                active: false,
            })),
        };
        let n = interop
            .native
            .as_mut()
            .ok_or("native initialization lost owner")?;
        // Even ANGLE initialization can enqueue native work. Quarantine all
        // partial initialization on errors once the supplied queue is in use.
        n.mark_pending();
        {
            let gate = Arc::clone(&n.gate);
            let _guard = gate.lock().map_err(|_| "GPU queue gate poisoned")?;
            let mut device11 = None;
            let mut context11 = None;
            // VIDEO_SUPPORT requests modern video-capable driver creation. It
            // alone proves no decoding support: query the video API/profiles too.
            // A rejecting driver is a capability failure, not a software retry.
            let created = unsafe {
                D3D11On12CreateDevice(
                    &n.native_device,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT.0 | D3D11_CREATE_DEVICE_VIDEO_SUPPORT.0,
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    Some(&[Some(n.native_queue.cast().at("native queue IUnknown")?)]),
                    0,
                    Some(&mut device11),
                    Some(&mut context11),
                    None,
                )
            };
            n.device11 = device11;
            n.context11 = context11;
            created.at("D3D11On12CreateDevice BGRA+VIDEO_SUPPORT (direct d3d11va required)")?;
            if n.device11.is_none() || n.context11.is_none() {
                return Err("D3D11On12 returned a null device or immediate context".into());
            }
            let immediate = n
                .context11
                .as_ref()
                .ok_or("missing D3D11On12 immediate context")?;
            n.video_context = Some(
                immediate
                    .cast()
                    .at("ID3D11VideoContext: direct d3d11va unavailable")?,
            );
            n.multithread = Some(
                immediate
                    .cast()
                    .at("ID3D11Multithread for shared ANGLE/video context")?,
            );
            let multithread = n
                .multithread
                .as_ref()
                .ok_or("missing multithread protection")?;
            unsafe {
                let _previous_protection = multithread.SetMultithreadProtected(true);
            }
            if !unsafe { multithread.GetMultithreadProtected() }.as_bool() {
                return Err(
                    "D3D11On12 immediate context cannot enable multithread protection".into(),
                );
            }
            let d11 = n.device11.as_ref().ok_or("missing D3D11On12 device")?;
            n.on12 = Some(d11.cast().at("ID3D11On12Device")?);
            n.video = Some(
                d11.cast()
                    .at("ID3D11VideoDevice: direct d3d11va unavailable")?,
            );
            let video = n.video.as_ref().ok_or("missing video capability")?;
            let profiles = unsafe { video.GetVideoDecoderProfileCount() };
            if profiles == 0 || profiles > 4096 {
                return Err(
                    "D3D11On12 exposes no usable decoder profiles; direct d3d11va unavailable"
                        .into(),
                );
            }
            let mut nv12 = false;
            for index in 0..profiles {
                let profile = unsafe { video.GetVideoDecoderProfile(index) }
                    .at("enumerate D3D11On12 video decoder profiles")?;
                nv12 |= unsafe { video.CheckVideoDecoderFormat(&profile, DXGI_FORMAT_NV12) }
                    .at("query D3D11On12 NV12 decoder format")?
                    .as_bool();
            }
            if !nv12 {
                return Err(
                    "D3D11On12 has no NV12 decoder profile; direct d3d11va unavailable".into(),
                );
            }
            // This establishes API capability only, not codec/video qualification.
            unsafe {
                n.egl.initialize(d11)?;
                n.egl.create_stable_context()?;
            }
            unsafe {
                n.context11
                    .as_ref()
                    .ok_or("missing immediate context")?
                    .Flush();
            }
            n.signal()?;
        }
        n.wait_complete()?;
        Ok(interop)
    }

    pub(super) fn get_proc_address(&self, name: &CStr) -> *const c_void {
        self.native
            .as_ref()
            .map(|n| n.egl.get_proc_address(name))
            .unwrap_or(std::ptr::null())
    }

    pub(super) fn drm_render_fd(&self) -> Option<i32> {
        None
    }

    pub(super) fn begin_frame(&mut self, size: [u32; 2]) -> Result<i32, String> {
        let n = self
            .native
            .as_mut()
            .ok_or("native interop already shut down")?;
        if n.active || n.pending.get() {
            return Err("previous Windows interop frame has not completed".into());
        }
        let limit = n.device.limits().max_texture_dimension_2d;
        validate_size(size, limit)?;
        let resize = n.target.as_ref().is_none_or(|t| t.size != size);
        if resize {
            // No source can still be referenced: finish_frame waits before
            // publishing its ordinary output texture. EGL context is unchanged.
            n.create_target(size)?;
        }
        let gate = Arc::clone(&n.gate);
        let _guard = gate.lock().map_err(|_| "GPU queue gate poisoned")?;
        n.mark_pending();
        let target = n.target.as_ref().ok_or("missing native frame target")?;
        let allocator: ID3D12CommandAllocator = unsafe {
            n.native_device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        }
        .at("Create frame transition allocator")?;
        let list: ID3D12GraphicsCommandList = unsafe {
            n.native_device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .at("Create frame transition command list")?;
        unsafe {
            transition(
                &list,
                &target.raw,
                target.state,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            );
        }
        unsafe { list.Close() }.at("Close frame entry transition")?;
        let command: ID3D12CommandList = list.cast().at("frame entry command interface")?;
        n.commands.push((allocator, list));
        unsafe {
            n.native_queue.ExecuteCommandLists(&[Some(command)]);
        }
        // Acquire is ONLY hazard tracking. The explicit ordered queue barrier
        // above establishes the InState, both initially and after a WGPU copy.
        unsafe {
            n.on12
                .as_ref()
                .ok_or("missing D3D11On12 device")?
                .AcquireWrappedResources(&[Some(target.wrapped.clone())]);
        }
        n.egl.current(n.egl.surface)?;
        n.target.as_mut().ok_or("missing target")?.state = D3D12_RESOURCE_STATE_RENDER_TARGET;
        n.active = true;
        // The MPV render API accepts FBO 0 with this real GLES3 pbuffer current.
        Ok(0)
    }

    pub(super) fn finish_frame(&mut self) -> Result<super::NativeFrame, String> {
        let n = self
            .native
            .as_mut()
            .ok_or("native interop already shut down")?;
        if !n.active || !n.pending.get() {
            return Err("finish_frame requires an active native frame".into());
        }
        // These scopes are worker-thread-local in WGPU 29. Capture allocation,
        // import, encoder, and submit failures instead of publishing an invalid
        // texture when the application's uncaptured-error hook is non-panicking.
        let out_of_memory = n.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = n.device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = n.device.push_error_scope(wgpu::ErrorFilter::Validation);
        {
            let gate = Arc::clone(&n.gate);
            let _guard = gate.lock().map_err(|_| "GPU queue gate poisoned")?;
            unsafe {
                (n.egl.flush)();
                let error = (n.egl.gl_error)();
                if error != 0 {
                    return Err(format!("libmpv ANGLE render GL error 0x{error:04x}"));
                }
                // Unbind the wrapped render target before releasing ownership.
                n.egl.check(
                    (n.egl.make_current)(n.egl.display, null_mut(), null_mut(), null_mut()),
                    "eglMakeCurrent unbind rendered pbuffer",
                )?;
                let target = n.target.as_mut().ok_or("missing rendered target")?;
                n.on12
                    .as_ref()
                    .ok_or("missing D3D11On12 device")?
                    .ReleaseWrappedResources(&[Some(target.wrapped.clone())]);
                n.context11
                    .as_ref()
                    .ok_or("missing immediate context")?
                    .Flush();
                target.state = D3D12_RESOURCE_STATE_COMMON;
            }
            // Restore the SAME context on a persistent non-wrapped bootstrap
            // pbuffer. MPV callbacks/free always have a current stable context,
            // without leaving the released source bound as a render target.
            n.egl.current(n.egl.bootstrap)?;
            let target = n.target.as_ref().ok_or("missing rendered source")?;
            let extent = wgpu::Extent3d {
                width: target.size[0],
                height: target.size[1],
                depth_or_array_layers: 1,
            };
            let descriptor = wgpu::TextureDescriptor {
                label: Some("GPU surface ANGLE native BGRA source"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            };
            // SAFETY: source originates on the actual WGPU device, GL has fully
            // initialized it, Release+Flush established COMMON, and the fresh
            // import's initial tracker state is COMMON. Retain through completion.
            let hal = unsafe {
                wgpu_hal::dx12::Device::texture_from_raw(
                    target.raw.clone(),
                    descriptor.format,
                    descriptor.dimension,
                    extent,
                    1,
                    1,
                )
            };
            n.imported = Some(unsafe {
                n.device
                    .create_texture_from_hal::<wgpu_hal::api::Dx12>(hal, &descriptor)
            });
            n.output = Some(n.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("GPU surface embedded mpv BGRA GPU frame"),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                ..descriptor
            }));
            let mut encoder = n
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("GPU surface ANGLE-to-GPUI GPU copy"),
                });
            encoder.copy_texture_to_texture(
                n.imported
                    .as_ref()
                    .ok_or("missing imported source")?
                    .as_image_copy(),
                n.output
                    .as_ref()
                    .ok_or("missing GPU output")?
                    .as_image_copy(),
                extent,
            );
            n.submission = Some(n.queue.submit([encoder.finish()]));
            // HAL's explicit COPY_SRC use leaves the raw source in COPY_SOURCE.
            // The next begin_frame transitions that exact state to RENDER_TARGET.
            n.target.as_mut().ok_or("missing copied source")?.state =
                D3D12_RESOURCE_STATE_COPY_SOURCE;
            n.signal()?;
        }
        // Never hold the shared queue gate during native/WGPU completion waits.
        n.wait_complete()?;
        let diagnostics_deadline = Instant::now() + TIMEOUT;
        resolve_scope(validation.pop(), &n.device, diagnostics_deadline)?;
        resolve_scope(internal.pop(), &n.device, diagnostics_deadline)?;
        resolve_scope(out_of_memory.pop(), &n.device, diagnostics_deadline)?;
        n.commands.clear();
        n.imported = None;
        n.submission = None;
        n.active = false;
        let texture = n.output.take().ok_or("missing completed GPU frame")?;
        Ok(super::NativeFrame::Texture(texture))
    }

    pub(super) fn complete_submissions(&self) -> Result<(), String> {
        let n = self
            .native
            .as_ref()
            .ok_or("native interop already shut down")?;
        if n.pending.get() {
            n.wait_complete()?;
        }
        // MakeCurrent can implicitly flush ANGLE even without a GL draw. The
        // parent gates its MPV calls separately; never nest that guard here.
        let _guard = n.gate.lock().map_err(|_| "GPU queue gate poisoned")?;
        let restored = n.egl.current(n.egl.bootstrap);
        if restored.is_err() {
            // A failed MakeCurrent may still have flushed partial ANGLE work;
            // an older completed frame fence cannot authorize destruction.
            n.mark_pending();
        }
        restored
    }

    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        if self.native.is_none() {
            return Ok(());
        }
        self.complete_submissions()?;
        let n = self
            .native
            .as_ref()
            .ok_or("native interop already shut down")?;
        // The parent frees its MPV render context before shutdown. That free can
        // enqueue GL cleanup AFTER the last frame's fence, so explicitly flush
        // and fence once more, rather than treating that older fence as proof.
        n.mark_pending();
        {
            let gate = Arc::clone(&n.gate);
            let _guard = gate.lock().map_err(|_| "GPU queue gate poisoned")?;
            unsafe {
                (n.egl.flush)();
                n.context11
                    .as_ref()
                    .ok_or("missing immediate context during shutdown")?
                    .Flush();
            }
            n.signal()?;
        }
        n.wait_complete()?;
        drop_completed_native(
            self.native
                .take()
                .ok_or("native interop already shut down")?,
        )
    }
}

impl Drop for NativeInterop {
    fn drop(&mut self) {
        if let Some(native) = self.native.take() {
            if native.pending.get() {
                // Includes native and WGPU device/queue, GL context, all DLLs,
                // source/output/import, allocators, command lists, and fence.
                std::mem::forget(native);
            } else {
                let _ = drop_completed_native(native);
            }
        }
    }
}

/// EGL destruction, D3D11 context release, and DLL unload may implicitly flush.
/// Serialize the entire known-complete native destruction path with GPUI. No
/// fence/poll wait is performed here; poisoned serialization quarantines owners.
fn drop_completed_native(native: Box<Native>) -> Result<(), String> {
    let gate = Arc::clone(&native.gate);
    match gate.lock() {
        Ok(_guard) => {
            drop(native);
            Ok(())
        }
        Err(_) => {
            std::mem::forget(native);
            Err("GPU queue gate poisoned during native teardown; owners quarantined".into())
        }
    }
}

impl Native {
    fn mark_pending(&self) {
        self.wait_target.set(None);
        self.pending.set(true);
    }

    fn signal(&self) -> Result<(), String> {
        let value = self.next_fence.get();
        let next = value
            .checked_add(1)
            .ok_or("native fence sequence exhausted")?;
        unsafe { self.native_queue.Signal(&self.native_fence, value) }
            .at("signal interop native queue fence")?;
        self.next_fence.set(next);
        self.wait_target.set(Some(value));
        Ok(())
    }

    fn wait_complete(&self) -> Result<(), String> {
        let value = self
            .wait_target
            .get()
            .ok_or("GPU completion unproven after early failure; all interop owners quarantined")?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let completed = unsafe { self.native_fence.GetCompletedValue() };
            if completed == u64::MAX {
                return Err("Dx12 device lost; all native interop owners quarantined".into());
            }
            if completed >= value {
                break;
            }
            unsafe { self.native_device.GetDeviceRemovedReason() }
                .at("Dx12 device health during completion")?;
            if Instant::now() >= deadline {
                return Err("Dx12 interop fence timeout (5s); all GPU owners quarantined".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(index) = self.submission.clone() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            self.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: Some(remaining),
                })
                .map_err(
                    |_| "WGPU interop submission completion failed; all GPU owners quarantined",
                )?;
        }
        unsafe { self.native_device.GetDeviceRemovedReason() }
            .at("Dx12 device health after completion")?;
        self.pending.set(false);
        Ok(())
    }

    fn create_target(&mut self, size: [u32; 2]) -> Result<(), String> {
        // All EGL/D3D11 calls can implicitly submit, including MakeCurrent and
        // wrapping/view creation. Gate the entire setup, not just the last flush.
        self.mark_pending();
        let gate = Arc::clone(&self.gate);
        let guard = gate.lock().map_err(|_| "GPU queue gate poisoned")?;
        self.egl.current(self.egl.bootstrap)?;
        let mut raw: Option<ID3D12Resource> = None;
        unsafe {
            self.native_device.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                    Width: u64::from(size[0]),
                    Height: size[1],
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
                    Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut raw,
            )
        }
        .at("create native BGRA8 source texture")?;
        let raw = required(raw, "native BGRA source")?;
        let mut texture11: Option<ID3D11Texture2D> = None;
        unsafe {
            self.on12
                .as_ref()
                .ok_or("missing D3D11On12 device")?
                .CreateWrappedResource(
                    &raw,
                    &D3D11_RESOURCE_FLAGS {
                        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0)
                            as u32,
                        ..Default::default()
                    },
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_COMMON,
                    &mut texture11,
                )
        }
        .at("wrap native BGRA source via D3D11On12")?;
        let texture11 = required(texture11, "wrapped source texture")?;
        let wrapped = texture11.cast().at("wrapped ID3D11Resource")?;
        let mut rtv = None;
        unsafe {
            self.device11
                .as_ref()
                .ok_or("missing D3D11 device")?
                .CreateRenderTargetView(&texture11, None, Some(&mut rtv))
        }
        .at("create wrapped source RTV")?;
        let target = Target {
            raw,
            texture11,
            wrapped,
            _rtv: required(rtv, "wrapped RTV")?,
            size,
            state: D3D12_RESOURCE_STATE_COMMON,
        };
        // Install ownership before EGL can enqueue initialization work. Keep the
        // old target alive until its old EGL surface has actually been destroyed.
        if let Some(previous) = self.target.replace(target) {
            self.retired.push(previous);
        }
        // No reference to previous can be submitted now: old native work was
        // waited before resize and EGL is on bootstrap, not the retired surface.
        unsafe {
            self.egl.replace_surface(
                &self
                    .target
                    .as_ref()
                    .ok_or("missing resized target")?
                    .texture11,
            )?;
        }
        // Retired targets stay in Native on every early error or panic.
        // They may still be referenced by failed EGL surface destruction.
        unsafe {
            self.context11
                .as_ref()
                .ok_or("missing immediate context")?
                .Flush();
        }
        self.signal()?;
        drop(guard);
        self.wait_complete()?;
        // Native resource release can also flush deferred D3D11 bookkeeping.
        let _guard = gate.lock().map_err(|_| "GPU queue gate poisoned")?;
        self.retired.clear();
        Ok(())
    }
}

/// Temporary transition descriptors retain their resource only while recording;
/// Native owns both resource and command list through the final queue fence.
unsafe fn transition(
    list: &ID3D12GraphicsCommandList,
    texture: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) {
    let mut barrier = D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(texture.clone())),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    };
    unsafe {
        list.ResourceBarrier(std::slice::from_ref(&barrier));
        ManuallyDrop::drop(&mut (*barrier.Anonymous.Transition).pResource);
    }
}

/// Validate before native allocation, integer casts, or any queue submission.
fn validate_size(size: [u32; 2], limit: u32) -> Result<(), String> {
    if size.contains(&0) || size.iter().any(|&v| v > limit || v > i32::MAX as u32) {
        Err("GPU surface video target size is outside GPU limits".into())
    } else {
        Ok(())
    }
}

struct WorkerWake(std::thread::Thread);
impl Wake for WorkerWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Drain only this thread's WGPU error scope with a bounded worker wait. Never
/// include driver strings/paths in public diagnostics or hold the queue gate.
fn resolve_scope(
    future: impl Future<Output = Option<wgpu::Error>>,
    device: &wgpu::Device,
    deadline: Instant,
) -> Result<(), String> {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(WorkerWake(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(None) => return Ok(()),
            Poll::Ready(Some(_)) => {
                return Err("WGPU rejected GPU surface native import/GPU copy".into());
            }
            Poll::Pending => {}
        }
        if Instant::now() >= deadline {
            return Err("WGPU native import/GPU copy diagnostics timed out (5s)".into());
        }
        device
            .poll(wgpu::PollType::Poll)
            .map_err(|_| "WGPU poll failed resolving GPU surface copy diagnostics")?;
        std::thread::park_timeout(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_target_before_native_work() {
        assert!(validate_size([1920, 1080], 16384).is_ok());
        assert!(validate_size([3840, 2160], 16384).is_ok());
        assert!(validate_size([0, 1080], 16384).is_err());
        assert!(validate_size([1920, 0], 16384).is_err());
        assert!(validate_size([16385, 1080], 16384).is_err());
        assert!(validate_size([u32::MAX, 1], u32::MAX).is_err());
    }
}
