use super::{ffi::Param, library::Client};
use crate::{Error, Result};
use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    ffi::c_void,
    ptr::NonNull,
    sync::Arc,
    thread,
    time::Instant,
};

/// Tightly packed, top-to-bottom **RGBA8** pixels (opaque alpha), not GPUI's BGRA.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Frame {
    /// Consume this RGBA frame and return tightly packed BGRA8 pixels.
    ///
    /// Swaps red/blue in place, retaining the allocation, row order and alpha.
    /// `Player::take_frame` and the public `pixels` field remain RGBA8.
    pub fn into_bgra_pixels(mut self) -> Vec<u8> {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        self.pixels
    }
}

struct Surface {
    ptr: NonNull<u8>,
    layout: Layout,
    stride: usize,
    width: u32,
    height: u32,
}
/// Validate the actual native allocation, including 64-byte row padding.
/// This is pure so oversized inputs are rejected before any allocation.
fn surface_layout(width: u32, height: u32, max_frame_bytes: usize) -> Result<(Layout, usize)> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(Error::InvalidInput(
            "render dimensions must be positive signed 32-bit integers",
        ));
    }
    let row = (width as usize).checked_mul(4).ok_or(Error::Allocation)?;
    let stride = row.checked_add(63).ok_or(Error::Allocation)? & !63;
    let len = stride
        .checked_mul(height as usize)
        .ok_or(Error::Allocation)?;
    if len > max_frame_bytes {
        return Err(Error::InvalidInput(
            "CPU render surface including 64-byte row padding exceeds max_frame_bytes; select smaller dimensions or increase the guard",
        ));
    }
    let layout = Layout::from_size_align(len, 64).map_err(|_| Error::Allocation)?;
    Ok((layout, stride))
}
impl Surface {
    fn new(width: u32, height: u32, max_frame_bytes: usize) -> Result<Self> {
        let (layout, stride) = surface_layout(width, height, max_frame_bytes)?;
        // Initialize RGB/padding for blank or partially written surfaces. rgb0 alpha
        // may subsequently become undefined; frame() initializes it before reading.
        let ptr = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or(Error::Allocation)?;
        Ok(Self {
            ptr,
            layout,
            stride,
            width,
            height,
        })
    }
    fn frame(&mut self) -> Result<Frame> {
        let row = self.width as usize * 4;
        let len = row
            .checked_mul(self.height as usize)
            .ok_or(Error::Allocation)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(len)
            .map_err(|_| Error::Allocation)?;
        for y in 0..self.height as usize {
            let row_ptr = unsafe { self.ptr.as_ptr().add(y * self.stride) };
            // libmpv rgb0 promises initialized RGB, but explicitly leaves its
            // fourth byte undefined. Raw writes initialize every alpha BEFORE
            // constructing a &[u8]; never read those native bytes or row padding.
            for x in 0..self.width as usize {
                unsafe {
                    row_ptr.add(x * 4 + 3).write(255);
                }
            }
            let source = unsafe { std::slice::from_raw_parts(row_ptr, row) };
            pixels.extend_from_slice(source);
        }
        Ok(Frame {
            width: self.width,
            height: self.height,
            pixels,
        })
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

pub(super) fn capped_size(width: u32, height: u32, maximum: [u32; 2]) -> [u32; 2] {
    if width == 0 || height == 0 {
        return [0, 0];
    }
    let scale = (maximum[0] as f64 / width as f64)
        .min(maximum[1] as f64 / height as f64)
        .min(1.0);
    [
        (width as f64 * scale).floor().max(1.0) as u32,
        (height as f64 * scale).floor().max(1.0) as u32,
    ]
}

unsafe extern "C" fn update_callback(data: *mut c_void) {
    // Thread::unpark is nonblocking and does not unwind. Box stays alive through context free.
    unsafe { &*data.cast::<thread::Thread>() }.unpark();
}

/// The callback userdata and dynamic library remain live even during worker unwinding.
struct Context {
    client: Arc<Client>,
    ptr: *mut super::ffi::RenderContext,
    wake: Box<thread::Thread>,
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            (self.client.loaded.api.render_callback)(self.ptr, None, std::ptr::null_mut());
            (self.client.loaded.api.render_free)(self.ptr);
        }
        // wake is dropped only after free synchronizes outstanding callbacks.
    }
}

pub(super) struct CpuRenderer {
    guard: Context,
    surface: Option<Surface>,
    pub render_call_ms: f64,
    pub copy_ms: f64,
    max_frame_bytes: usize,
}
impl CpuRenderer {
    pub fn new(client: Arc<Client>, max_frame_bytes: usize) -> Result<Self> {
        let api = &client.loaded.api;
        let mut context = std::ptr::null_mut();
        let mut params = [
            Param::new(1, c"sw".as_ptr().cast_mut().cast()),
            Param::end(),
        ];
        let code = unsafe { (api.render_create)(&mut context, client.handle, params.as_mut_ptr()) };
        client
            .loaded
            .check(code, "create software render context")?;
        let guard = Context {
            client,
            ptr: context,
            wake: Box::new(thread::current()),
        };
        unsafe {
            (guard.client.loaded.api.render_callback)(
                context,
                Some(update_callback),
                (&*guard.wake as *const thread::Thread).cast_mut().cast(),
            )
        };
        Ok(Self {
            guard,
            surface: None,
            render_call_ms: 0.0,
            copy_ms: 0.0,
            max_frame_bytes,
        })
    }
    pub fn update(&self) -> u64 {
        unsafe { (self.guard.client.loaded.api.render_update)(self.guard.ptr) }
    }
    pub fn skip(&self) -> Result<()> {
        let mut skip = 1_i32;
        let mut params = [Param::new(13, (&mut skip as *mut i32).cast()), Param::end()];
        self.guard.client.loaded.check(
            unsafe { (self.guard.client.loaded.api.render)(self.guard.ptr, params.as_mut_ptr()) },
            "skip hidden frame",
        )
    }
    pub fn render(&mut self, size: [u32; 2]) -> Result<Frame> {
        if self
            .surface
            .as_ref()
            .is_none_or(|s| [s.width, s.height] != size)
        {
            self.surface = Some(Surface::new(size[0], size[1], self.max_frame_bytes)?);
        }
        let s = self.surface.as_mut().ok_or(Error::Allocation)?;
        let mut dimensions = [size[0] as i32, size[1] as i32];
        let mut params = [
            Param::new(17, dimensions.as_mut_ptr().cast()),
            Param::new(18, c"rgb0".as_ptr().cast_mut().cast()),
            Param::new(19, (&mut s.stride as *mut usize).cast()),
            Param::new(20, s.ptr.as_ptr().cast()),
            Param::end(),
        ];
        let render_started = Instant::now();
        self.guard.client.loaded.check(
            unsafe { (self.guard.client.loaded.api.render)(self.guard.ptr, params.as_mut_ptr()) },
            "software render",
        )?;
        self.render_call_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        let copy_started = Instant::now();
        let frame = s.frame();
        self.copy_ms = copy_started.elapsed().as_secs_f64() * 1000.0;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consuming_bgra_conversion_preserves_alpha_order_and_allocation() {
        let frame = Frame {
            width: 2,
            height: 2,
            pixels: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 12, 34, 56, 255,
            ],
        };
        let original_ptr = frame.pixels.as_ptr();
        let pixels = frame.into_bgra_pixels();
        assert_eq!(pixels.as_ptr(), original_ptr);
        assert_eq!(
            pixels,
            [
                0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 56, 34, 12, 255
            ]
        );
    }
    #[test]
    fn padded_surface_byte_guard_without_allocating() {
        // Packed pixels fit in 4 MB, but native rows require 64 MB.
        assert!(
            matches!(surface_layout(1, 1_000_000, 4_000_000), Err(Error::InvalidInput(message)) if message.contains("row padding"))
        );
        assert_eq!(
            surface_layout(1, 1_000_000, 64_000_000).unwrap().0.size(),
            64_000_000
        );
        assert!(surface_layout(1, 16_777_216, 64 * 1024 * 1024).is_err());
        assert_eq!(
            surface_layout(3840, 2160, 64 * 1024 * 1024)
                .unwrap()
                .0
                .size(),
            3840 * 2160 * 4
        );
        assert!(surface_layout(i32::MAX as u32, i32::MAX as u32, 64 * 1024 * 1024).is_err());
    }
    #[test]
    fn padded_rgba_and_alignment() {
        let mut s = Surface::new(3, 2, 128).unwrap();
        assert_eq!(s.ptr.as_ptr() as usize % 64, 0);
        assert_eq!(s.stride, 64);
        unsafe {
            std::ptr::copy_nonoverlapping([12, 34, 56, 0].as_ptr(), s.ptr.as_ptr().add(64), 4);
        }
        let frame = s.frame().unwrap();
        assert_eq!(frame.pixels.len(), 24);
        assert_eq!(&frame.pixels[12..16], &[12, 34, 56, 255]);
    }
    #[test]
    fn undefined_native_alpha_is_initialized_before_copy_and_padding_is_excluded() {
        let mut surface = Surface::new(2, 2, 128).unwrap();
        let colors = [[1, 2, 3], [10, 20, 30], [100, 110, 120], [200, 210, 220]];
        unsafe {
            surface
                .ptr
                .as_ptr()
                .write_bytes(0xa5, surface.layout.size());
            for (index, color) in colors.iter().enumerate() {
                let pixel = surface
                    .ptr
                    .as_ptr()
                    .add(index / 2 * surface.stride + index % 2 * 4);
                std::ptr::copy_nonoverlapping(color.as_ptr(), pixel, 3);
                // Model rgb0's genuinely undefined fourth byte, not merely zero.
                pixel
                    .add(3)
                    .cast::<std::mem::MaybeUninit<u8>>()
                    .write(std::mem::MaybeUninit::uninit());
            }
        }
        let frame = surface.frame().unwrap();
        assert_eq!(frame.pixels.len(), 16);
        for (pixel, color) in frame.pixels.chunks_exact(4).zip(colors) {
            assert_eq!(&pixel[..3], &color);
            assert_eq!(pixel[3], 255);
        }
        for y in 0..2 {
            let native = unsafe {
                std::slice::from_raw_parts(
                    surface.ptr.as_ptr().add(y * surface.stride),
                    surface.stride,
                )
            };
            assert_eq!(native[3], 255);
            assert_eq!(native[7], 255);
            assert!(native[8..].iter().all(|&byte| byte == 0xa5));
        }
    }
    #[test]
    fn dimensions_and_caps() {
        assert!(Surface::new(0, 1, usize::MAX).is_err());
        assert!(Surface::new(u32::MAX, u32::MAX, usize::MAX).is_err());
        assert_eq!(capped_size(1920, 1080, [1280, 720]), [1280, 720]);
        assert_eq!(capped_size(1000, 2000, [1280, 720]), [360, 720]);
        assert_eq!(capped_size(0, 200, [1280, 720]), [0, 0]);
    }
}
