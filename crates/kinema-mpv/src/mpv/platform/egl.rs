use libloading::Library;
use std::{
    collections::BTreeMap,
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    path::Path,
    ptr,
    rc::Rc,
    sync::{Mutex, OnceLock},
};

// EGL uses APIENTRY (__stdcall on 32-bit Windows), not the C calling convention.
type Display = *mut c_void;
type Handle = *mut c_void;
type Boolean = u32;
type Int = i32;
type GetPlatformDisplay = unsafe extern "system" fn(u32, *mut c_void, *const Int) -> Display;
type Initialize = unsafe extern "system" fn(Display, *mut Int, *mut Int) -> Boolean;
type Terminate = unsafe extern "system" fn(Display) -> Boolean;
type BindApi = unsafe extern "system" fn(u32) -> Boolean;
type ChooseConfig =
    unsafe extern "system" fn(Display, *const Int, *mut Handle, Int, *mut Int) -> Boolean;
type CreateContext = unsafe extern "system" fn(Display, Handle, Handle, *const Int) -> Handle;
type DestroyContext = unsafe extern "system" fn(Display, Handle) -> Boolean;
type CreateSurface = unsafe extern "system" fn(Display, Handle, *const Int) -> Handle;
type DestroySurface = unsafe extern "system" fn(Display, Handle) -> Boolean;
type MakeCurrent = unsafe extern "system" fn(Display, Handle, Handle, Handle) -> Boolean;
type GetCurrentContext = unsafe extern "system" fn() -> Handle;
type GetProcAddress = unsafe extern "system" fn(*const c_char) -> *const c_void;
type GetError = unsafe extern "system" fn() -> Int;
type QueryString = unsafe extern "system" fn(Display, Int) -> *const c_char;
const NONE: Int = 0x3038;

struct Api {
    egl: Library,
    gl: Library,
    initialize: Initialize,
    terminate: Terminate,
    bind_api: BindApi,
    choose_config: ChooseConfig,
    create_context: CreateContext,
    destroy_context: DestroyContext,
    create_surface: CreateSurface,
    destroy_surface: DestroySurface,
    make_current: MakeCurrent,
    get_current_context: GetCurrentContext,
    get_proc: GetProcAddress,
    get_error: GetError,
    query_string: QueryString,
}
impl Api {
    fn load(config: &crate::Config) -> crate::Result<Self> {
        #[cfg(target_os = "windows")]
        let (egl_names, gl_names): (&[&str], &[&str]) = (&["libEGL.dll"], &["libGLESv2.dll"]);
        #[cfg(target_os = "linux")]
        let (egl_names, gl_names): (&[&str], &[&str]) = (
            &["libEGL.so.1", "libEGL.so"],
            &["libGL.so.1", "libOpenGL.so.0"],
        );
        let egl = load_library(config.egl_library_path.as_deref(), egl_names)?;
        let gl = load_library(config.gles_library_path.as_deref(), gl_names)?;
        // All symbols are copied while their owning libraries remain in Api.
        unsafe {
            Ok(Self {
                initialize: symbol(&egl, b"eglInitialize\0")?,
                terminate: symbol(&egl, b"eglTerminate\0")?,
                bind_api: symbol(&egl, b"eglBindAPI\0")?,
                choose_config: symbol(&egl, b"eglChooseConfig\0")?,
                create_context: symbol(&egl, b"eglCreateContext\0")?,
                destroy_context: symbol(&egl, b"eglDestroyContext\0")?,
                create_surface: symbol(&egl, b"eglCreatePbufferSurface\0")?,
                destroy_surface: symbol(&egl, b"eglDestroySurface\0")?,
                make_current: symbol(&egl, b"eglMakeCurrent\0")?,
                get_current_context: symbol(&egl, b"eglGetCurrentContext\0")?,
                get_proc: symbol(&egl, b"eglGetProcAddress\0")?,
                get_error: symbol(&egl, b"eglGetError\0")?,
                query_string: symbol(&egl, b"eglQueryString\0")?,
                egl,
                gl,
            })
        }
    }
    fn error(&self, operation: &str) -> crate::Error {
        crate::Error::Gpu(format!("{operation}: EGL error {:#x}", unsafe {
            (self.get_error)()
        }))
    }
    fn platform_display(&self, platform: u32, native: *mut c_void, attrs: &[Int]) -> Display {
        // EXT takes EGLint attributes, unlike the EGL 1.5 EGLAttrib entrypoint.
        let name = c"eglGetPlatformDisplayEXT";
        let mut address = unsafe { (self.get_proc)(name.as_ptr()) };
        if address.is_null() {
            address = unsafe {
                self.egl
                    .get::<*const c_void>(name.to_bytes_with_nul())
                    .map(|s| *s)
                    .unwrap_or(ptr::null())
            };
        }
        if address.is_null() {
            return ptr::null_mut();
        }
        let get: GetPlatformDisplay = unsafe { std::mem::transmute(address) };
        unsafe { get(platform, native, attrs.as_ptr()) }
    }
}

fn load_library(override_path: Option<&Path>, names: &[&str]) -> crate::Result<Library> {
    if let Some(path) = override_path {
        return unsafe { crate::mpv::library::open_native_library(path) }
            .map_err(|e| crate::Error::Gpu(format!("loading {}: {e}", path.display())));
    }
    let mut errors = Vec::new();
    for name in names {
        match unsafe { crate::mpv::library::open_native_library(Path::new(name)) } {
            Ok(lib) => return Ok(lib),
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    Err(crate::Error::Gpu(format!(
        "native GL library unavailable: {}",
        errors.join("; ")
    )))
}
unsafe fn symbol<T: Copy>(lib: &Library, name: &[u8]) -> crate::Result<T> {
    unsafe { lib.get::<T>(name) }
        .map(|s| *s)
        .map_err(|e| crate::Error::Gpu(format!("missing native GL symbol: {e}")))
}

// EGLInitialize is idempotent, not reference counted. Library identity is the
// initialize entrypoint address; each retained context keeps that library loaded.
// Initialize/final terminate are serialized here, but no GL or mpv calls occur
// under the mutex. Destruction of worker-local contexts/surfaces happens outside it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct DisplayKey {
    initialize: usize,
    display: usize,
}
struct DisplayEntry {
    references: usize,
    native: Option<usize>,
}
#[derive(Default)]
struct DisplayRegistry(BTreeMap<DisplayKey, DisplayEntry>);
impl DisplayRegistry {
    fn acquire(
        &mut self,
        key: DisplayKey,
        native: Option<usize>,
        initialize: impl FnOnce() -> crate::Result<()>,
    ) -> crate::Result<()> {
        if let Some(entry) = self.0.get_mut(&key) {
            // Independently owned X11/Wayland connections normally have distinct
            // EGLDisplays. Reject provider aliasing rather than disconnecting the
            // originating connection while another worker still needs it. Shared
            // null-native ANGLE/surfaceless displays do not have this lifetime issue.
            if entry.native.is_some() || native.is_some() {
                return Err(crate::Error::Gpu("EGL display aliases independently owned native connections; concurrent players require distinct native EGL displays or headless contexts".into()));
            }
            entry.references = entry
                .references
                .checked_add(1)
                .ok_or_else(|| crate::Error::Gpu("EGL display reference count overflow".into()))?;
        } else {
            initialize()?;
            self.0.insert(
                key,
                DisplayEntry {
                    references: 1,
                    native,
                },
            );
        }
        Ok(())
    }
    fn release(&mut self, key: DisplayKey, terminate: impl FnOnce()) {
        let Some(entry) = self.0.get_mut(&key) else {
            return;
        };
        if entry.references > 1 {
            entry.references -= 1;
        } else {
            self.0.remove(&key);
            terminate();
        }
    }
}
fn displays() -> &'static Mutex<DisplayRegistry> {
    static DISPLAYS: OnceLock<Mutex<DisplayRegistry>> = OnceLock::new();
    DISPLAYS.get_or_init(|| Mutex::new(DisplayRegistry::default()))
}

/// Acquire a shared, null-native EGL display. The caller must retain the library
/// containing both entrypoints until release (or forever if quarantined).
///
/// # Safety
/// `display` and `initialize` must belong to the same live EGL implementation.
#[cfg(all(feature = "gpu-surface", target_os = "linux"))]
pub(in crate::mpv) unsafe fn acquire_headless_display(
    initialize: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> u32,
    display: *mut c_void,
) -> crate::Result<()> {
    displays()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .acquire(
            DisplayKey {
                initialize: initialize as usize,
                display: display as usize,
            },
            None,
            || {
                if unsafe { initialize(display, ptr::null_mut(), ptr::null_mut()) } == 0 {
                    Err(crate::Error::Gpu(
                        "headless EGL initialization failed".into(),
                    ))
                } else {
                    Ok(())
                }
            },
        )
}

/// Release exactly one successful acquisition; never call for quarantined work.
///
/// # Safety
/// Entry points/display must match the acquisition and remain live throughout.
#[cfg(all(feature = "gpu-surface", target_os = "linux"))]
pub(in crate::mpv) unsafe fn release_headless_display(
    initialize: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> u32,
    terminate: unsafe extern "system" fn(*mut c_void) -> u32,
    display: *mut c_void,
) {
    displays()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .release(
            DisplayKey {
                initialize: initialize as usize,
                display: display as usize,
            },
            || unsafe {
                terminate(display);
            },
        );
}

pub(crate) struct GlContext {
    api: Api,
    display: Display,
    context: Handle,
    surface: Handle,
    initialized: bool,
    // Destroy EGL before disconnecting the independently owned native connection.
    native: Option<NativeDisplay>,
    _worker_only: PhantomData<Rc<()>>,
}
impl GlContext {
    pub(crate) fn new(config: &crate::Config) -> crate::Result<Self> {
        Self::new_with_native(config, true)
    }
    fn new_with_native(config: &crate::Config, _use_native: bool) -> crate::Result<Self> {
        let api = Api::load(config)?;
        let mut result = Self {
            api,
            display: ptr::null_mut(),
            context: ptr::null_mut(),
            surface: ptr::null_mut(),
            initialized: false,
            native: None,
            _worker_only: PhantomData,
        };
        #[cfg(target_os = "linux")]
        {
            let mut errors = Vec::new();
            let candidates = if _use_native {
                NativeDisplay::connect_candidates()
            } else {
                Vec::new()
            };
            for native in candidates {
                let platform = if native.mpv_parameter == 9 {
                    0x31D8
                } else {
                    0x31D5
                };
                result.display = result
                    .api
                    .platform_display(platform, native.pointer, &[NONE]);
                result.native = Some(native);
                match result.initialize() {
                    Ok(()) => return Ok(result),
                    Err(e) => errors.push(e.to_string()),
                }
                result.release_egl();
                result.native = None;
            }
            result.display = result
                .api
                .platform_display(0x31DD, ptr::null_mut(), &[NONE]); // EGL_PLATFORM_SURFACELESS_MESA
            result.initialize().map_err(|e| {
                crate::Error::Gpu(format!(
                    "offscreen EGL initialization failed: {e}; native display attempts: {}",
                    errors.join("; ")
                ))
            })?;
        }
        #[cfg(target_os = "windows")]
        {
            // Explicit ANGLE D3D11 hardware device: no implicit WARP/default backend.
            result.display = result.api.platform_display(
                0x3202,
                ptr::null_mut(),
                &[0x3203, 0x3208, 0x3209, 0x320A, NONE],
            );
            if result.display.is_null() {
                return Err(result.api.error(
                    "ANGLE D3D11 hardware display unavailable (install trusted ANGLE DLLs)",
                ));
            }
            result.initialize()?;
        }
        Ok(result)
    }
    fn display_key(&self) -> DisplayKey {
        DisplayKey {
            initialize: self.api.initialize as usize,
            display: self.display as usize,
        }
    }
    fn initialize(&mut self) -> crate::Result<()> {
        if self.display.is_null() {
            return Err(self.api.error("eglInitialize (null display)"));
        }
        if !self.initialized {
            displays()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .acquire(
                    self.display_key(),
                    self.native.as_ref().map(|native| native.pointer as usize),
                    || {
                        if unsafe {
                            (self.api.initialize)(self.display, ptr::null_mut(), ptr::null_mut())
                        } == 0
                        {
                            Err(self.api.error("eglInitialize"))
                        } else {
                            Ok(())
                        }
                    },
                )?;
            self.initialized = true;
        }
        unsafe {
            let gles = self.is_gles();
            if (self.api.bind_api)(if gles { 0x30A0 } else { 0x30A2 }) == 0 {
                return Err(self.api.error("eglBindAPI"));
            }
            let attrs = [
                0x3033,
                1,
                0x3040,
                if gles { 0x40 } else { 8 },
                0x3024,
                8,
                0x3023,
                8,
                0x3022,
                8,
                0x3021,
                8,
                NONE,
            ];
            let mut config = ptr::null_mut();
            let mut count = 0;
            if (self.api.choose_config)(self.display, attrs.as_ptr(), &mut config, 1, &mut count)
                == 0
                || count == 0
            {
                return Err(self.api.error("eglChooseConfig (RGBA8 offscreen GL3)"));
            }
            let context_attrs = if gles {
                vec![0x3098, 3, NONE]
            } else {
                vec![0x3098, 3, 0x30FB, 2, 0x30FD, 1, NONE]
            };
            self.context = (self.api.create_context)(
                self.display,
                config,
                ptr::null_mut(),
                context_attrs.as_ptr(),
            );
            if self.context.is_null() {
                return Err(self.api.error("eglCreateContext (GL 3.2 core / GLES 3)"));
            }
            self.surface = (self.api.create_surface)(
                self.display,
                config,
                [0x3057, 1, 0x3056, 1, NONE].as_ptr(),
            );
            if self.surface.is_null() {
                let extensions = (self.api.query_string)(self.display, 0x3055);
                let supported = !extensions.is_null()
                    && CStr::from_ptr(extensions)
                        .to_bytes()
                        .split(|b| *b == b' ')
                        .any(|e| e == b"EGL_KHR_surfaceless_context");
                if !supported {
                    return Err(self
                        .api
                        .error("eglCreatePbufferSurface (surfaceless unavailable)"));
                }
            }
            if (self.api.make_current)(self.display, self.surface, self.surface, self.context) == 0
            {
                return Err(self.api.error("eglMakeCurrent"));
            }
        }
        Ok(())
    }
    pub(crate) fn get_proc_address(&self, name: &CStr) -> *const c_void {
        let address = unsafe { (self.api.get_proc)(name.as_ptr()) };
        if !address.is_null() {
            return address;
        }
        unsafe {
            self.api
                .gl
                .get::<*const c_void>(name.to_bytes_with_nul())
                .map(|s| *s)
                .unwrap_or(ptr::null())
        }
    }
    pub(crate) fn native_display(&self) -> Option<(i32, *mut c_void)> {
        self.native.as_ref().map(|n| (n.mpv_parameter, n.pointer))
    }
    pub(crate) fn is_gles(&self) -> bool {
        cfg!(target_os = "windows")
    }
    fn release_egl(&mut self) {
        if self.display.is_null() {
            return;
        }
        if self.initialized {
            unsafe {
                // Another context may be current on this same worker (especially in
                // lifecycle tests). Never clear a different player's current context.
                if !self.context.is_null() && (self.api.get_current_context)() == self.context {
                    (self.api.make_current)(
                        self.display,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                    );
                }
                if !self.context.is_null() {
                    (self.api.destroy_context)(self.display, self.context);
                }
                if !self.surface.is_null() {
                    (self.api.destroy_surface)(self.display, self.surface);
                }
            }
            displays()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .release(self.display_key(), || unsafe {
                    (self.api.terminate)(self.display);
                });
            self.initialized = false;
        }
        self.context = ptr::null_mut();
        self.surface = ptr::null_mut();
        self.display = ptr::null_mut();
    }
}
impl Drop for GlContext {
    fn drop(&mut self) {
        self.release_egl();
    }
}

enum NativeClose {
    #[cfg(target_os = "linux")]
    Wayland(unsafe extern "C" fn(*mut c_void)),
    #[cfg(target_os = "linux")]
    X11(unsafe extern "C" fn(*mut c_void) -> Int),
}
struct NativeDisplay {
    pointer: *mut c_void,
    mpv_parameter: i32,
    close: NativeClose,
    _library: Library,
}
// XInitThreads must precede our first Xlib call, and must run for each loaded
// library lifetime rather than caching success in a Rust Once. Embedders that
// call Xlib themselves must initialize threading before their own first call;
// this provider cannot retroactively repair an already-used external Xlib.
#[cfg(target_os = "linux")]
fn initialize_xlib_threads(initialize: Option<impl FnOnce() -> Int>) -> bool {
    static INITIALIZATION: Mutex<()> = Mutex::new(());
    let Some(initialize) = initialize else {
        return false;
    };
    let _guard = INITIALIZATION.lock().unwrap_or_else(|e| e.into_inner());
    initialize() != 0
}

impl NativeDisplay {
    #[cfg(target_os = "linux")]
    fn connect_candidates() -> Vec<Self> {
        let mut candidates = Vec::new();
        if std::env::var_os("WAYLAND_DISPLAY").is_some()
            && let Ok(lib) = unsafe { Library::new("libwayland-client.so.0") }
        {
            unsafe {
                let connect = lib.get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(
                    b"wl_display_connect\0",
                );
                let close =
                    lib.get::<unsafe extern "C" fn(*mut c_void)>(b"wl_display_disconnect\0");
                if let (Ok(connect), Ok(close)) = (connect, close) {
                    let pointer = connect(ptr::null());
                    let close = *close;
                    if !pointer.is_null() {
                        candidates.push(Self {
                            pointer,
                            mpv_parameter: 9,
                            close: NativeClose::Wayland(close),
                            _library: lib,
                        });
                    }
                }
            }
        }
        if std::env::var_os("DISPLAY").is_some()
            && let Ok(lib) = unsafe { Library::new("libX11.so.6") }
        {
            unsafe {
                let initialize = lib.get::<unsafe extern "C" fn() -> Int>(b"XInitThreads\0");
                let connect = lib
                    .get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"XOpenDisplay\0");
                let close = lib.get::<unsafe extern "C" fn(*mut c_void) -> Int>(b"XCloseDisplay\0");
                let threaded = initialize_xlib_threads(initialize.ok().map(|init| move || init()));
                // Missing/failed thread support skips X11 instead of opening an
                // unsafe connection. No native, EGL or mpv work runs under the lock.
                if threaded && let (Ok(connect), Ok(close)) = (connect, close) {
                    let pointer = connect(ptr::null());
                    let close = *close;
                    if !pointer.is_null() {
                        candidates.push(Self {
                            pointer,
                            mpv_parameter: 8,
                            close: NativeClose::X11(close),
                            _library: lib,
                        });
                    }
                }
            }
        }
        candidates
    }
}
impl Drop for NativeDisplay {
    fn drop(&mut self) {
        match self.close {
            #[cfg(target_os = "linux")]
            NativeClose::Wayland(close) => unsafe {
                close(self.pointer);
            },
            #[cfg(target_os = "linux")]
            NativeClose::X11(close) => unsafe {
                close(self.pointer);
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[cfg(target_os = "linux")]
    #[test]
    fn xlib_thread_initialization_requires_support_and_repeats_after_reload() {
        assert!(!initialize_xlib_threads(None::<fn() -> Int>));
        assert!(!initialize_xlib_threads(Some(|| 0)));
        let calls = Cell::new(0);
        for _ in 0..3 {
            assert!(initialize_xlib_threads(Some(|| {
                calls.set(calls.get() + 1);
                1
            })));
        }
        assert_eq!(calls.get(), 3);
    }

    fn key(library: usize, display: usize) -> DisplayKey {
        DisplayKey {
            initialize: library,
            display,
        }
    }

    #[test]
    fn shared_display_initializes_once_and_terminates_only_on_final_release() {
        let mut registry = DisplayRegistry::default();
        let initialize_calls = Cell::new(0);
        let terminate_calls = Cell::new(0);
        let display = key(1, 2);
        for _ in 0..3 {
            registry
                .acquire(display, None, || {
                    initialize_calls.set(initialize_calls.get() + 1);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(initialize_calls.get(), 1);
        for remaining in [2, 1, 0] {
            registry.release(display, || terminate_calls.set(terminate_calls.get() + 1));
            assert_eq!(terminate_calls.get(), usize::from(remaining == 0));
            assert_eq!(
                registry.0.get(&display).map_or(0, |entry| entry.references),
                remaining
            );
        }
        // No retained initialization means no termination (including double release).
        registry.release(display, || panic!("unretained display terminated"));
        registry
            .acquire(display, None, || {
                initialize_calls.set(initialize_calls.get() + 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(initialize_calls.get(), 2);
        registry.release(display, || terminate_calls.set(terminate_calls.get() + 1));
        assert_eq!(terminate_calls.get(), 2);
    }

    #[test]
    fn failed_initialization_and_distinct_libraries_are_isolated() {
        let mut registry = DisplayRegistry::default();
        let first = key(1, 5);
        let other_library = key(2, 5);
        assert!(
            registry
                .acquire(first, None, || Err(crate::Error::Gpu(
                    "mock initialization failure".into()
                )))
                .is_err()
        );
        registry.release(first, || panic!("failed initialization terminated"));
        assert!(registry.0.is_empty());
        registry.acquire(first, None, || Ok(())).unwrap();
        assert!(
            registry
                .acquire(other_library, None, || Err(crate::Error::Gpu(
                    "other library failed".into()
                )))
                .is_err()
        );
        registry.release(other_library, || panic!("other library terminated"));
        assert_eq!(registry.0[&first].references, 1);
        registry.acquire(other_library, None, || Ok(())).unwrap();
        registry.release(first, || {});
        assert_eq!(registry.0[&other_library].references, 1);
        registry.release(other_library, || {});
        assert!(registry.0.is_empty());
    }

    #[cfg(all(feature = "gpu-surface", target_os = "linux"))]
    #[test]
    fn gpu_surface_and_production_headless_acquisitions_share_registry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static INITIALIZE: AtomicUsize = AtomicUsize::new(0);
        static TERMINATE: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "system" fn initialize(_: Display, _: *mut Int, _: *mut Int) -> Boolean {
            INITIALIZE.fetch_add(1, Ordering::SeqCst);
            1
        }
        unsafe extern "system" fn terminate(_: Display) -> Boolean {
            TERMINATE.fetch_add(1, Ordering::SeqCst);
            1
        }
        // Synthetic display is never dereferenced; no EGL/GPU libraries are loaded.
        let display = ptr::dangling_mut::<c_void>();
        let key = DisplayKey {
            initialize: initialize as *const () as usize,
            display: display as usize,
        };
        displays()
            .lock()
            .unwrap()
            .acquire(key, None, || {
                unsafe {
                    initialize(display, ptr::null_mut(), ptr::null_mut());
                }
                Ok(())
            })
            .unwrap();
        unsafe { acquire_headless_display(initialize, display) }.unwrap();
        assert_eq!(INITIALIZE.load(Ordering::SeqCst), 1);
        unsafe {
            release_headless_display(initialize, terminate, display);
        }
        assert_eq!(TERMINATE.load(Ordering::SeqCst), 0);
        displays().lock().unwrap().release(key, || unsafe {
            terminate(display);
        });
        assert_eq!(TERMINATE.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn independently_owned_native_connections_cannot_alias_a_live_display() {
        let mut registry = DisplayRegistry::default();
        let display = key(1, 2);
        registry.acquire(display, Some(7), || Ok(())).unwrap();
        assert!(
            registry
                .acquire(display, Some(8), || panic!("live display reinitialized"))
                .is_err()
        );
        assert!(
            registry
                .acquire(display, None, || panic!("live display reinitialized"))
                .is_err()
        );
        assert_eq!(registry.0[&display].references, 1);
        registry.release(display, || {});
        registry.acquire(display, None, || Ok(())).unwrap();
        assert!(
            registry
                .acquire(display, Some(7), || panic!("live display reinitialized"))
                .is_err()
        );
        registry.release(display, || {});
    }

    #[test]
    fn synchronized_workers_share_one_initialization() {
        use std::sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
        };
        let registry = Arc::new(Mutex::new(DisplayRegistry::default()));
        let initialized = Arc::new(AtomicUsize::new(0));
        let terminated = Arc::new(AtomicUsize::new(0));
        let acquired = Arc::new(Barrier::new(4));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let (registry, initialized, terminated, acquired) = (
                    registry.clone(),
                    initialized.clone(),
                    terminated.clone(),
                    acquired.clone(),
                );
                std::thread::spawn(move || {
                    registry
                        .lock()
                        .unwrap()
                        .acquire(key(1, 2), None, || {
                            initialized.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        })
                        .unwrap();
                    acquired.wait();
                    registry.lock().unwrap().release(key(1, 2), || {
                        terminated.fetch_add(1, Ordering::SeqCst);
                    });
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(initialized.load(Ordering::SeqCst), 1);
        assert_eq!(terminated.load(Ordering::SeqCst), 1);
        assert!(registry.lock().unwrap().0.is_empty());
    }

    #[test]
    #[ignore = "requires native surfaceless EGL or Windows ANGLE libraries"]
    fn native_headless_context_survives_other_context_drop() {
        let config = crate::Config::default();
        let first = GlContext::new_with_native(&config, false)
            .unwrap_or_else(|e| panic!("headless EGL capability unavailable: {e}"));
        let second = GlContext::new_with_native(&config, false)
            .unwrap_or_else(|e| panic!("second headless EGL context unavailable: {e}"));
        assert!(first.native.is_none() && second.native.is_none());
        assert_eq!(
            first.display, second.display,
            "test requires shared headless EGLDisplay"
        );
        assert_eq!(
            unsafe { (second.api.get_current_context)() },
            second.context
        );
        let address = second.get_proc_address(c"glGetString");
        assert!(!address.is_null());
        let get_string = unsafe {
            std::mem::transmute::<*const c_void, unsafe extern "system" fn(u32) -> *const c_char>(
                address,
            )
        };
        let before = unsafe { get_string(0x1F02) }; // GL_VERSION
        assert!(!before.is_null());
        let version = unsafe { CStr::from_ptr(before) }.to_owned();
        drop(first);
        // A setup failure after retaining the same display must release only its
        // own reference, and must not clear the already-current second context.
        unsafe extern "system" fn fail_choose_config(
            _: Display,
            _: *const Int,
            _: *mut Handle,
            _: Int,
            _: *mut Int,
        ) -> Boolean {
            0
        }
        let mut failed = GlContext {
            api: Api::load(&config).unwrap(),
            display: second.display,
            context: ptr::null_mut(),
            surface: ptr::null_mut(),
            initialized: false,
            native: None,
            _worker_only: PhantomData,
        };
        failed.api.choose_config = fail_choose_config;
        assert!(failed.initialize().is_err());
        assert!(failed.initialized);
        drop(failed);
        assert_eq!(
            unsafe { (second.api.get_current_context)() },
            second.context
        );
        let after = unsafe { get_string(0x1F02) };
        assert!(
            !after.is_null(),
            "remaining context lost after another player dropped"
        );
        assert_eq!(unsafe { CStr::from_ptr(after) }, version.as_c_str());
        assert_ne!(
            unsafe {
                (second.api.make_current)(
                    second.display,
                    second.surface,
                    second.surface,
                    second.context,
                )
            },
            0,
            "remaining EGLDisplay was terminated"
        );
        eprintln!(
            "shared headless EGL lifecycle verified: {}",
            version.to_string_lossy()
        );
        drop(second);
        // A final release must permit a fresh initialization of that display.
        let third = GlContext::new_with_native(&config, false).unwrap();
        let address = third.get_proc_address(c"glGetString");
        assert!(!address.is_null());
        let get_string = unsafe {
            std::mem::transmute::<*const c_void, unsafe extern "system" fn(u32) -> *const c_char>(
                address,
            )
        };
        assert!(!unsafe { get_string(0x1F02) }.is_null());
        drop(third);
    }
}
