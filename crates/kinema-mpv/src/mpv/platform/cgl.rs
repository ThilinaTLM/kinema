use libloading::Library;
use std::{
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    ptr,
    rc::Rc,
};

type Context = *mut c_void;
type PixelFormat = *mut c_void;
type CglError = i32;

// CGL uses the C ABI; attributes and GLint are 32-bit, not pointer-sized.
#[link(name = "OpenGL", kind = "framework")]
unsafe extern "C" {
    fn CGLChoosePixelFormat(
        attributes: *const i32,
        format: *mut PixelFormat,
        count: *mut i32,
    ) -> CglError;
    fn CGLDestroyPixelFormat(format: PixelFormat) -> CglError;
    fn CGLCreateContext(format: PixelFormat, share: Context, context: *mut Context) -> CglError;
    fn CGLDestroyContext(context: Context) -> CglError;
    fn CGLSetCurrentContext(context: Context) -> CglError;
    fn CGLGetCurrentContext() -> Context;
    fn CGLErrorString(error: CglError) -> *const c_char;
}

fn error(operation: &str, code: CglError) -> crate::Error {
    let text = unsafe { CGLErrorString(code) };
    let detail = if text.is_null() {
        "unknown CGL error".into()
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy()
    };
    crate::Error::Gpu(format!("{operation}: {detail} ({code})"))
}

pub(crate) struct GlContext {
    context: Context,
    gl: Library,
    _worker_only: PhantomData<Rc<()>>,
}
impl GlContext {
    pub(crate) fn new(_: &crate::Config) -> crate::Result<Self> {
        let gl = unsafe { Library::new("/System/Library/Frameworks/OpenGL.framework/OpenGL") }
            .map_err(|e| crate::Error::Gpu(format!("loading OpenGL framework: {e}")))?;
        // Core 3.2, accelerated and no software fallback. Rendering is into the
        // GPU backend's FBO; CGL has no drawable, NSWindow or pbuffer here.
        let attributes = [99, 0x3200, 73, 72, 8, 24, 11, 8, 0];
        let mut format = ptr::null_mut();
        let mut count = 0;
        let code = unsafe { CGLChoosePixelFormat(attributes.as_ptr(), &mut format, &mut count) };
        if code != 0 || format.is_null() || count == 0 {
            if !format.is_null() {
                unsafe {
                    CGLDestroyPixelFormat(format);
                }
            }
            return Err(error("CGLChoosePixelFormat (accelerated core 3.2)", code));
        }
        let mut context = ptr::null_mut();
        let code = unsafe { CGLCreateContext(format, ptr::null_mut(), &mut context) };
        unsafe {
            CGLDestroyPixelFormat(format);
        }
        if code != 0 || context.is_null() {
            if !context.is_null() {
                unsafe {
                    CGLDestroyContext(context);
                }
            }
            return Err(error("CGLCreateContext", code));
        }
        let result = Self {
            context,
            gl,
            _worker_only: PhantomData,
        };
        let code = unsafe { CGLSetCurrentContext(context) };
        if code != 0 {
            return Err(error("CGLSetCurrentContext", code));
        }
        if unsafe { CGLGetCurrentContext() } != context {
            return Err(crate::Error::Gpu(
                "CGL current context verification failed".into(),
            ));
        }
        Ok(result)
    }
    pub(crate) fn get_proc_address(&self, name: &CStr) -> *const c_void {
        unsafe {
            self.gl
                .get::<*const c_void>(name.to_bytes_with_nul())
                .map(|s| *s)
                .unwrap_or(ptr::null())
        }
    }
    pub(crate) fn native_display(&self) -> Option<(i32, *mut c_void)> {
        None
    }
    pub(crate) fn is_gles(&self) -> bool {
        false
    }
}
impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            if CGLGetCurrentContext() == self.context {
                CGLSetCurrentContext(ptr::null_mut());
            }
            CGLDestroyContext(self.context);
        }
    }
}
