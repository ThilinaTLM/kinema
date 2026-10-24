use crate::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    ObjectFit, Pixels, Style, StyleRefinement, Styled, Window,
};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_core_foundation::CFRetained;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_core_video::{CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use refineable::Refineable;

/// An immutable, opaque experimental SDR GPU frame on the renderer's device.
///
/// The producer must finish GPU writes before publishing this frame and must not
/// mutate or reuse the texture while any frame reference exists. Use a default
/// 2D view of a single-layer, single-mip RGBA8Unorm/BGRA8Unorm texture on Linux/Windows,
/// or a nonplanar BGRA32 CVPixelBuffer on macOS. This API performs no
/// CPU import, synchronization, color conversion, or CPU readback. WGPU validates
/// device/view compatibility at binding; Apple imports with its actual device.
///
/// The owner stays alive through element/scene drawing. Wgpu tracks submitted
/// resources; Apple retains the frame through command-buffer completion.
/// Dropping a frame is NOT a presentation fence
/// and gives no guarantee about actual presentation time.
#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
pub struct GpuSurfaceFrame {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    view: wgpu::TextureView,
    #[cfg(target_os = "macos")]
    pixel_buffer: CFRetained<CVPixelBuffer>,
    size: [u32; 2],
    _owner: std::sync::Arc<dyn Send + Sync>,
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
impl GpuSurfaceFrame {
    /// Validate basic texture metadata and retain the immutable view and owner.
    /// The view itself must use the default format, mip, layer, and dimension.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn new(
        view: wgpu::TextureView,
        size: [u32; 2],
        owner: std::sync::Arc<dyn Send + Sync>,
    ) -> Result<Self, &'static str> {
        let texture = view.texture();
        if !valid_gpu_surface_size(size, [texture.width(), texture.height()]) {
            return Err("GPU surface dimensions must be nonzero and match the texture");
        }
        if !matches!(
            texture.format(),
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ) || texture.dimension() != wgpu::TextureDimension::D2
            || texture.depth_or_array_layers() != 1
            || texture.mip_level_count() != 1
            || texture.sample_count() != 1
            || !texture
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(
                "GPU surface requires a single-layer/mip/sample RGBA8Unorm or BGRA8Unorm sampled 2D texture",
            );
        }
        Ok(Self {
            view,
            size,
            _owner: owner,
        })
    }

    /// Retain a fresh immutable IOSurface-backed BGRA32 SDR buffer and its owner.
    /// Producer GPU work must be complete before publishing; no CPU pixel access
    /// or format conversion occurs here. The Apple renderer retains both this
    /// frame and its CoreVideo/Metal resources until command-buffer completion.
    #[cfg(target_os = "macos")]
    pub fn new(
        pixel_buffer: CFRetained<CVPixelBuffer>,
        size: [u32; 2],
        owner: std::sync::Arc<dyn Send + Sync>,
    ) -> Result<Self, &'static str> {
        use objc2_core_video::{
            CVPixelBufferGetIOSurface, CVPixelBufferGetPixelFormatType, CVPixelBufferIsPlanar,
            kCVPixelFormatType_32BGRA,
        };
        if !valid_gpu_surface_size(
            size,
            [
                CVPixelBufferGetWidth(&pixel_buffer) as u32,
                CVPixelBufferGetHeight(&pixel_buffer) as u32,
            ],
        ) {
            return Err("GPU surface dimensions must be nonzero and match the pixel buffer");
        }
        if CVPixelBufferGetPixelFormatType(&pixel_buffer) != kCVPixelFormatType_32BGRA
            || CVPixelBufferIsPlanar(&pixel_buffer)
            || CVPixelBufferGetIOSurface(Some(&pixel_buffer)).is_none()
        {
            return Err("GPU surface requires nonplanar BGRA32 SDR");
        }
        Ok(Self {
            pixel_buffer,
            size,
            _owner: owner,
        })
    }

    /// The retained CoreVideo buffer, for the experimental Apple renderer only.
    #[cfg(target_os = "macos")]
    #[doc(hidden)]
    pub fn pixel_buffer(&self) -> &CFRetained<CVPixelBuffer> {
        &self.pixel_buffer
    }

    /// The frame's pixel dimensions.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Immutable texture view, for the experimental renderer only.
    #[doc(hidden)]
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
fn valid_gpu_surface_size(size: [u32; 2], actual: [u32; 2]) -> bool {
    size[0] != 0
        && size[1] != 0
        && size[0] <= i32::MAX as u32
        && size[1] <= i32::MAX as u32
        && size == actual
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
impl std::fmt::Debug for GpuSurfaceFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuSurfaceFrame")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
impl PartialEq for GpuSurfaceFrame {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
impl Eq for GpuSurfaceFrame {}

/// Create an experimental GPU surface, using the usual styling and object fit.
#[cfg(all(
    any(target_os = "linux", target_os = "windows", target_os = "macos"),
    feature = "gpu-video-experiment"
))]
pub fn gpu_surface(frame: std::sync::Arc<GpuSurfaceFrame>) -> Surface {
    Surface {
        source: SurfaceSource::Gpu(frame),
        object_fit: ObjectFit::Contain,
        style: Default::default(),
    }
}

/// macOS name for the owned CoreVideo GPU surface frame.
#[cfg(all(target_os = "macos", feature = "gpu-video-experiment"))]
pub type MacGpuSurfaceFrame = GpuSurfaceFrame;

/// A source of a surface's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceSource {
    /// An immutable SDR GPU frame on the active renderer device.
    #[cfg(all(
        any(target_os = "linux", target_os = "windows", target_os = "macos"),
        feature = "gpu-video-experiment"
    ))]
    Gpu(std::sync::Arc<GpuSurfaceFrame>),
    /// A CoreVideo image buffer
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Surface(CFRetained<CVPixelBuffer>),
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
impl From<CFRetained<CVPixelBuffer>> for SurfaceSource {
    fn from(value: CFRetained<CVPixelBuffer>) -> Self {
        SurfaceSource::Surface(value)
    }
}

/// A surface element.
pub struct Surface {
    source: SurfaceSource,
    object_fit: ObjectFit,
    style: StyleRefinement,
}

/// Create a new surface element.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn surface(source: impl Into<SurfaceSource>) -> Surface {
    Surface {
        source: source.into(),
        object_fit: ObjectFit::Contain,
        style: Default::default(),
    }
}

impl Surface {
    /// Set the object fit for the image.
    pub fn object_fit(mut self, object_fit: ObjectFit) -> Self {
        self.object_fit = object_fit;
        self
    }
}

impl Element for Surface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        #[cfg_attr(
            not(any(target_os = "macos", target_os = "ios")),
            allow(unused_variables)
        )]
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        #[cfg_attr(
            not(any(target_os = "macos", target_os = "ios")),
            allow(unused_variables)
        )]
        window: &mut Window,
        _: &mut App,
    ) {
        match &self.source {
            #[cfg(all(
                any(target_os = "linux", target_os = "windows", target_os = "macos"),
                feature = "gpu-video-experiment"
            ))]
            SurfaceSource::Gpu(frame) => {
                let [width, height] = frame.size();
                let size = crate::size(
                    crate::DevicePixels(width as i32),
                    crate::DevicePixels(height as i32),
                );
                let new_bounds = self.object_fit.get_bounds(bounds, size);
                window.paint_gpu_surface(new_bounds, frame.clone());
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            SurfaceSource::Surface(surface) => {
                let size = crate::size(
                    CVPixelBufferGetWidth(surface).into(),
                    CVPixelBufferGetHeight(surface).into(),
                );
                let new_bounds = self.object_fit.get_bounds(bounds, size);
                // TODO: Add support for corner_radii
                window.paint_surface(new_bounds, surface.clone());
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
}

impl IntoElement for Surface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Styled for Surface {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[cfg(all(test, target_os = "linux", feature = "gpu-video-experiment"))]
mod gpu_surface_tests {
    #[test]
    fn dimensions_must_be_nonzero_and_match() {
        assert!(super::valid_gpu_surface_size([640, 360], [640, 360]));
        assert!(!super::valid_gpu_surface_size([0, 360], [0, 360]));
        assert!(!super::valid_gpu_surface_size([640, 0], [640, 0]));
        assert!(!super::valid_gpu_surface_size([640, 360], [360, 640]));
        let oversized = [i32::MAX as u32 + 1, 1];
        assert!(!super::valid_gpu_surface_size(oversized, oversized));
    }
}
