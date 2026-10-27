//! Native contexts are created, used and dropped on the render worker only.
#[cfg(target_os = "macos")]
mod cgl;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod egl;

#[cfg(target_os = "macos")]
pub(crate) use cgl::GlContext;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) use egl::GlContext;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod unsupported {
    use std::ffi::{CStr, c_void};
    pub(crate) struct GlContext(std::marker::PhantomData<std::rc::Rc<()>>);
    impl GlContext {
        pub(crate) fn new(_: &crate::Config) -> crate::Result<Self> {
            Err(crate::Error::Gpu(
                "native GL contexts are unsupported on this platform".into(),
            ))
        }
        pub(crate) fn get_proc_address(&self, _: &CStr) -> *const c_void {
            std::ptr::null()
        }
        pub(crate) fn native_display(&self) -> Option<(i32, *mut c_void)> {
            None
        }
        pub(crate) fn is_gles(&self) -> bool {
            false
        }
    }
}
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub(crate) use unsupported::GlContext;

#[cfg(all(feature = "gpu-surface", target_os = "linux"))]
pub(super) use egl::{acquire_headless_display, release_headless_display};
