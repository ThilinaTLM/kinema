//! Actual D3D12 -> D3D11On12 -> ANGLE GLES -> D3D12 test-readback.
//! No WGPU, window, decoder, or player integration.
use std::{
    ffi::{CStr, c_void},
    mem::ManuallyDrop,
    ptr::null_mut,
    time::{Duration, Instant},
};

use libloading::os::windows::Library;
use windows::{
    Win32::Graphics::{
        Direct3D::D3D_FEATURE_LEVEL_11_0,
        Direct3D11::*,
        Direct3D11on12::*,
        Direct3D12::*,
        Dxgi::{Common::*, *},
    },
    core::{IUnknown, Interface},
};

use crate::{COLORS, HEIGHT, WIDTH, validate_bgra_rows};

type Ptr = *mut c_void;
type Bool = u32;
type Int = i32;
const NONE: Int = 0x3038;
const D3D11_DEVICE: u32 = 0x33a1;
const PLATFORM_DEVICE: u32 = 0x313f;
const D3D_TEXTURE: u32 = 0x33a3;
const TIMEOUT: Duration = Duration::from_secs(5);
const INITIAL_STATE: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_COMMON;
const WRAP_IN_STATE: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_RENDER_TARGET;
const WRAP_OUT_STATE: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_COMMON;

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
// DLL search is restricted to the executable's directory and system defaults;
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
    clear_color: unsafe extern "system" fn(f32, f32, f32, f32),
    clear: unsafe extern "system" fn(u32),
    enable: unsafe extern "system" fn(u32),
    disable: unsafe extern "system" fn(u32),
    scissor: unsafe extern "system" fn(Int, Int, Int, Int),
    flush: unsafe extern "system" fn(),
    gl_error: unsafe extern "system" fn() -> u32,
    device: Ptr,
    display: Ptr,
    context: Ptr,
    surface: Ptr,
}

impl Egl {
    unsafe fn load() -> Result<Self, String> {
        let directory = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_owned()))
            .ok_or("cannot locate probe executable directory")?;
        const FLAGS: u32 = 0x00000100 | 0x00001000; // DLL_LOAD_DIR | DEFAULT_DIRS
        let gles = unsafe { Library::load_with_flags(directory.join("libGLESv2.dll"), FLAGS) }
            .map_err(|_| "cannot load executable-adjacent ANGLE libGLESv2.dll")?;
        let egl = unsafe { Library::load_with_flags(directory.join("libEGL.dll"), FLAGS) }
            .map_err(|_| "cannot load executable-adjacent ANGLE libEGL.dll")?;
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
            clear_color: symbol!(gles, "glClearColor"),
            clear: symbol!(gles, "glClear"),
            enable: symbol!(gles, "glEnable"),
            disable: symbol!(gles, "glDisable"),
            scissor: symbol!(gles, "glScissor"),
            flush: symbol!(gles, "glFlush"),
            gl_error: symbol!(gles, "glGetError"),
            _gles: gles,
            _egl: egl,
            device: null_mut(),
            display: null_mut(),
            context: null_mut(),
            surface: null_mut(),
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

    unsafe fn attach(&mut self, texture: &ID3D11Texture2D) -> Result<(), String> {
        // EGL_SURFACE_TYPE PBUFFER_BIT, RENDERABLE_TYPE OPENGL_ES2_BIT,
        // RGBA 8/8/8/8. No sRGB or channel reinterpretation.
        let attributes = [
            0x3033, 1, 0x3040, 4, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3021, 8, NONE,
        ];
        let mut config = null_mut();
        let mut count = 0;
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
            "eglChooseConfig",
        )?;
        if count != 1 || config.is_null() {
            return Err("no ANGLE RGBA8 pbuffer config".into());
        }
        self.context = unsafe {
            (self.create_context)(self.display, config, null_mut(), [0x3098, 2, NONE].as_ptr())
        };
        self.check((!self.context.is_null()) as Bool, "eglCreateContext GLES2")?;
        self.surface = unsafe {
            (self.create_pbuffer)(
                self.display,
                D3D_TEXTURE,
                texture.as_raw(),
                config,
                [NONE].as_ptr(),
            )
        };
        self.check(
            (!self.surface.is_null()) as Bool,
            "eglCreatePbufferFromClientBuffer D3D texture",
        )?;
        self.check(
            unsafe { (self.make_current)(self.display, self.surface, self.surface, self.context) },
            "eglMakeCurrent",
        )
    }

    unsafe fn render(&self) -> Result<(), String> {
        unsafe {
            (self.disable)(0x0bd0); // GL_DITHER: exact UNORM8 conversion
            (self.enable)(0x0c11); // GL_SCISSOR_TEST
            for (i, c) in COLORS.iter().enumerate() {
                (self.scissor)((i % 2 * 2) as Int, (i / 2 * 2) as Int, 2, 2);
                (self.clear_color)(
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                    1.0,
                );
                (self.clear)(0x00004000); // GL_COLOR_BUFFER_BIT
            }
            (self.flush)();
            let error = (self.gl_error)();
            if error != 0 {
                return Err(format!("ANGLE GL clear: error 0x{error:04x}"));
            }
        }
        Ok(())
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

/// COM/DLL/EGL owners are quarantined on any error after GPU submission.
/// Returning a timeout is NOT permission to destroy resources still in flight.
/// This intentional leak lasts to process exit; callers must not retry forever.
struct Owners {
    objects: ManuallyDrop<Vec<IUnknown>>,
    egl: ManuallyDrop<Egl>,
    pending: bool,
}
impl Owners {
    fn keep<T: Interface>(&mut self, object: &T) -> Result<(), String> {
        self.objects.push(object.cast().at("retain native owner")?);
        Ok(())
    }
}
impl Drop for Owners {
    fn drop(&mut self) {
        if !self.pending {
            unsafe {
                ManuallyDrop::drop(&mut self.egl);
                ManuallyDrop::drop(&mut self.objects);
            }
        }
    }
}

pub(super) fn run() -> Result<(), String> {
    eprintln!(
        "probe backend=ANGLE-D3D11On12-D3D12 mode=test-readback debug-layer=not-enabled debug-messages=not-collected"
    );
    // All APIs, immediate context calls, bounded waits, and cleanup are worker-only.
    unsafe { run_native() }
}

unsafe fn run_native() -> Result<(), String> {
    let mut owners = Owners {
        objects: ManuallyDrop::new(Vec::new()),
        egl: ManuallyDrop::new(unsafe { Egl::load()? }),
        pending: false,
    };
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.at("CreateDXGIFactory1")?;
    let mut selected = None;
    for index in 0..32 {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => {
                return Err(format!(
                    "EnumAdapters1: HRESULT 0x{:08x}",
                    e.code().0 as u32
                ));
            }
        };
        let description = unsafe { adapter.GetDesc1() }.at("GetDesc1")?;
        if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let mut device: Option<ID3D12Device> = None;
        if unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.is_ok() {
            selected = Some((adapter, required(device, "D3D12CreateDevice")?));
            break;
        }
    }
    let (adapter, device) =
        selected.ok_or("no hardware D3D12 adapter; software fallback forbidden")?;
    owners.keep(&adapter)?;
    owners.keep(&device)?;
    let metadata = unsafe { adapter.GetDesc1() }.at("selected adapter metadata")?;
    eprintln!(
        "probe hardware-adapter vendor-id=0x{:04x} device-id=0x{:04x}",
        metadata.VendorId, metadata.DeviceId
    );
    let queue: ID3D12CommandQueue = unsafe {
        device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        })
    }
    .at("CreateCommandQueue")?;
    owners.keep(&queue)?;
    let mut device11 = None;
    let mut context11 = None;
    unsafe {
        D3D11On12CreateDevice(
            &device,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT.0,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            Some(&[Some(queue.cast().at("queue IUnknown")?)]),
            0,
            Some(&mut device11),
            Some(&mut context11),
            None,
        )
    }
    .at("D3D11On12CreateDevice")?;
    let device11 = required(device11, "D3D11On12 device")?;
    let context11 = required(context11, "D3D11On12 immediate context")?;
    let on12: ID3D11On12Device = device11.cast().at("ID3D11On12Device")?;
    owners.keep(&device11)?;
    owners.keep(&context11)?;
    owners.keep(&on12)?;
    unsafe {
        owners.egl.initialize(&device11)?;
    }

    let texture_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Width: WIDTH as u64,
        Height: HEIGHT as u32,
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
    };
    let mut texture: Option<ID3D12Resource> = None;
    unsafe {
        device.CreateCommittedResource(
            &D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_DEFAULT,
                ..Default::default()
            },
            D3D12_HEAP_FLAG_NONE,
            &texture_desc,
            INITIAL_STATE,
            None,
            &mut texture,
        )
    }
    .at("CreateCommittedResource BGRA8")?;
    let texture = required(texture, "D3D12 BGRA8 texture")?;
    owners.keep(&texture)?;
    let mut wrapped: Option<ID3D11Texture2D> = None;
    unsafe {
        on12.CreateWrappedResource(
            &texture,
            &D3D11_RESOURCE_FLAGS {
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                ..Default::default()
            },
            WRAP_IN_STATE,
            WRAP_OUT_STATE,
            &mut wrapped,
        )
    }
    .at("CreateWrappedResource")?;
    let wrapped = required(wrapped, "wrapped D3D11 texture")?;
    owners.keep(&wrapped)?;
    // An actual RTV is created, not merely a declared bind flag. ANGLE also
    // creates its own RTV for this exact wrapped ID3D11Texture2D.
    let mut rtv = None;
    unsafe { device11.CreateRenderTargetView(&wrapped, None, Some(&mut rtv)) }
        .at("CreateRenderTargetView")?;
    let rtv = required(rtv, "wrapped texture RTV")?;
    owners.keep(&rtv)?;
    let wrapped_resource: ID3D11Resource = wrapped.cast().at("wrapped ID3D11Resource")?;
    owners.keep(&wrapped_resource)?;

    let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut total_bytes = 0;
    unsafe {
        device.GetCopyableFootprints(
            &texture_desc,
            0,
            1,
            0,
            Some(&mut footprint),
            None,
            None,
            Some(&mut total_bytes),
        );
    }
    if total_bytes == 0 || total_bytes > 65536 || footprint.Footprint.RowPitch < (WIDTH * 4) as u32
    {
        return Err("invalid D3D12 test-readback footprint".into());
    }
    let mut readback: Option<ID3D12Resource> = None;
    unsafe {
        device.CreateCommittedResource(
            &D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_READBACK,
                ..Default::default()
            },
            D3D12_HEAP_FLAG_NONE,
            &D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                Width: total_bytes,
                Height: 1,
                DepthOrArraySize: 1,
                MipLevels: 1,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                ..Default::default()
            },
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
            &mut readback,
        )
    }
    .at("CreateCommittedResource test-readback")?;
    let readback = required(readback, "readback buffer")?;
    owners.keep(&readback)?;
    let allocator: ID3D12CommandAllocator =
        unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
            .at("CreateCommandAllocator")?;
    owners.keep(&allocator)?;
    let list: ID3D12GraphicsCommandList =
        unsafe { device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None) }
            .at("CreateCommandList")?;
    owners.keep(&list)?;
    let fence: ID3D12Fence =
        unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.at("CreateFence")?;
    owners.keep(&fence)?;

    // AcquireWrappedResources only marks hazard tracking; it does not establish
    // InState. Submit COMMON -> RENDER_TARGET ourselves on the exact queue used
    // by D3D11On12. The separate initialization list/allocator stay owned until
    // the final queue fence, so neither is reset while in flight.
    let initial_allocator: ID3D12CommandAllocator =
        unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
            .at("Create initial transition allocator")?;
    owners.keep(&initial_allocator)?;
    let initial_list: ID3D12GraphicsCommandList = unsafe {
        device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &initial_allocator, None)
    }
    .at("Create initial transition list")?;
    owners.keep(&initial_list)?;
    unsafe {
        transition(&initial_list, &texture, INITIAL_STATE, WRAP_IN_STATE);
    }
    unsafe { initial_list.Close() }.at("Close initial transition list")?;
    let initial_command: ID3D12CommandList =
        initial_list.cast().at("initial command list interface")?;
    // From here every error preserves the complete ownership graph.
    owners.pending = true;
    unsafe {
        queue.ExecuteCommandLists(&[Some(initial_command)]);
        on12.AcquireWrappedResources(&[Some(wrapped_resource.clone())]);
    }
    unsafe {
        owners.egl.attach(&wrapped)?;
        owners.egl.render()?;
        // Force context unbinding/flush before handing resource ownership back
        // to native D3D12. Any unbind failure leaves all owners quarantined.
        owners.egl.check(
            (owners.egl.make_current)(owners.egl.display, null_mut(), null_mut(), null_mut()),
            "eglMakeCurrent unbind",
        )?;
    }
    unsafe {
        on12.ReleaseWrappedResources(&[Some(wrapped_resource)]);
        context11.Flush();
    }
    // ReleaseWrappedResources + Flush transitions the wrapped resource to COMMON.
    // Explicit COMMON -> COPY_SOURCE barrier precedes the native GPU readback.
    unsafe {
        transition(
            &list,
            &texture,
            WRAP_OUT_STATE,
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        );
    }
    let mut source = D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(texture.clone())),
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            SubresourceIndex: 0,
        },
    };
    let mut destination = D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(readback.clone())),
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: footprint,
        },
    };
    unsafe {
        list.CopyTextureRegion(&destination, 0, 0, 0, &source, None);
        ManuallyDrop::drop(&mut source.pResource);
        ManuallyDrop::drop(&mut destination.pResource);
        list.Close().at("Close copy command list")?;
        queue.ExecuteCommandLists(&[Some(list.cast().at("copy command list interface")?)]);
        queue.Signal(&fence, 1).at("D3D12 queue Signal")?;
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let value = unsafe { fence.GetCompletedValue() };
        if value == u64::MAX {
            return Err("D3D12 device removed during completion; owners quarantined".into());
        }
        if value >= 1 {
            break;
        }
        unsafe { device.GetDeviceRemovedReason() }.at("D3D12 device health during wait")?;
        if Instant::now() >= deadline {
            return Err(
                "D3D12 fence timeout (5s); native owners quarantined until process exit".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    owners.pending = false;
    unsafe { device.GetDeviceRemovedReason() }.at("D3D12 completion health")?;
    let mut mapped = null_mut();
    unsafe {
        readback.Map(
            0,
            Some(&D3D12_RANGE {
                Begin: 0,
                End: total_bytes as usize,
            }),
            Some(&mut mapped),
        )
    }
    .at("Map test-readback")?;
    if mapped.is_null() {
        unsafe {
            readback.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        }
        return Err("Map test-readback returned null".into());
    }
    let row_pitch = footprint.Footprint.RowPitch as usize;
    // D3D12 total footprint need not include padding on the final row. Build a
    // bounded CPU test image, copying only the initialized pixel bytes per row.
    let mut pixels = vec![0; row_pitch * HEIGHT];
    for y in 0..HEIGHT {
        let start = footprint.Offset as usize + y * row_pitch;
        if start + WIDTH * 4 > total_bytes as usize {
            unsafe {
                readback.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
            }
            return Err("test-readback row exceeds D3D12 footprint".into());
        }
        let row = unsafe { std::slice::from_raw_parts(mapped.cast::<u8>().add(start), WIDTH * 4) };
        pixels[y * row_pitch..y * row_pitch + WIDTH * 4].copy_from_slice(row);
    }
    unsafe {
        readback.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
    }
    // ANGLE Clear11 uses scissor.y directly for ordinary D3D pbuffers; unlike
    // a present-path-fast window, native row zero is GLES y=0. Assert this
    // contract rather than accepting either orientation after detection.
    validate_bgra_rows(&pixels, row_pitch, true)
}

/// Record an explicit native transition; command-list recording borrows the
/// resource. Owners retains it until the queue fence, not this temporary clone.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_requires_explicit_entry_transition_and_common_release() {
        assert_eq!(INITIAL_STATE, D3D12_RESOURCE_STATE_COMMON);
        assert_eq!(WRAP_IN_STATE, D3D12_RESOURCE_STATE_RENDER_TARGET);
        assert_ne!(INITIAL_STATE, WRAP_IN_STATE);
        assert_eq!(WRAP_OUT_STATE, D3D12_RESOURCE_STATE_COMMON);
        assert_ne!(WRAP_OUT_STATE, D3D12_RESOURCE_STATE_COPY_SOURCE);
    }
}
