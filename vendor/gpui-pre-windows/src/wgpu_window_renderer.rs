//! Experimental HWND renderer. The D3D11 text rasterizer remains independent;
//! every scene primitive (including GPU video) is drawn/presented by WGPU Dx12.
//! No canvas, readback, D3D11 scene renderer, or device-recovery fallback.
use std::{cell::RefCell, num::NonZeroIsize, rc::Rc, sync::Arc};

use anyhow::{Context, Result, ensure};
use gpui::{DevicePixels, GpuSpecs, PlatformAtlas, Scene, Size, WindowBackgroundAppearance};
use gpui_wgpu::{GpuContext, WgpuRenderer, WgpuSurfaceConfig, wgpu};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    Win32WindowHandle, WindowHandle,
};
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::GetClientRect,
};

use crate::{DirectXDevices, DirectXRenderer};

/// A raw handle, not a COM wrapper: windows 0.61 and WGPU's windows 0.62 never
/// exchange interface types. WM_DESTROY releases the surface before HWND dies.
#[derive(Clone, Debug)]
struct HwndHost(NonZeroIsize);
impl HasWindowHandle for HwndHost {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: WindowsWindow owns the HWND; destroy() runs before its native
        // destruction. WgpuRenderer does not outlive this enclosing window state.
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(self.0)))
        })
    }
}
impl HasDisplayHandle for HwndHost {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}

thread_local! {
    // Cache one context on the native UI thread, as the platform already does
    // for its D3D11 text device. This is NOT a live-renderer registration: 0002's
    // weak registration disappears with the renderer core. Keeping the slot
    // also prevents a later window from replacing a lost device behind frames
    // still retained by a video worker after the original window closes.
    static WINDOWS_CONTEXT: GpuContext = Rc::new(RefCell::new(None));
}

pub(crate) struct WgpuWindowRenderer {
    renderer: WgpuRenderer,
    size: Size<DevicePixels>,
    force_redraw: bool,
    terminal: bool,
}
impl WgpuWindowRenderer {
    pub(crate) fn new(
        hwnd: HWND,
        _font_devices: &DirectXDevices,
        _disable_direct_composition: bool,
    ) -> Result<Self> {
        let host = HwndHost(NonZeroIsize::new(hwnd.0 as isize).context("null WGPU HWND")?);
        let mut rect = RECT::default();
        unsafe { GetClientRect(hwnd, &mut rect) }.context("query WGPU client rectangle")?;
        let size = gpui::size(
            DevicePixels((rect.right - rect.left).max(0)),
            DevicePixels((rect.bottom - rect.top).max(0)),
        );
        // Patch 0002's context factory adopts the active device/queue/gate across
        // windows, rather than constructing one independent device per HWND.
        let context = WINDOWS_CONTEXT.with(Clone::clone);
        ensure!(
            !context
                .borrow()
                .as_ref()
                .is_some_and(|context| context.device_lost()),
            "experimental Windows context was lost; restart required before creating another window"
        );
        let mut renderer = WgpuRenderer::new(
            context.clone(),
            &host,
            WgpuSurfaceConfig {
                size: gpui::size(
                    DevicePixels(size.width.0.max(1)),
                    DevicePixels(size.height.0.max(1)),
                ),
                transparent: false,
                preferred_present_mode: None,
            },
            None,
        )
        .context("create experimental HWND WGPU renderer")?;
        {
            let context = context.borrow();
            let context = context
                .as_ref()
                .context("missing actual HWND WGPU context")?;
            ensure!(
                context.adapter.get_info().backend == wgpu::Backend::Dx12,
                "experimental HWND renderer requires Dx12, not a fallback backend"
            );
            ensure!(
                context.adapter.get_info().device_type != wgpu::DeviceType::Cpu,
                "experimental HWND renderer rejects software adapters"
            );
        }
        validate_drawable_size(size, renderer.max_texture_size())?;
        renderer.set_subpixel_layout(DirectXRenderer::get_font_info().is_bgr);
        Ok(Self {
            renderer,
            size,
            force_redraw: true,
            terminal: false,
        })
    }

    fn ensure_live(&mut self) -> Result<()> {
        self.terminal |= self.renderer.device_lost();
        ensure!(
            !self.terminal,
            "experimental WGPU device unavailable; restart required (no in-place device replacement with live video frames)"
        );
        Ok(())
    }

    pub(crate) fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }

    pub(crate) fn gpu_specs(&self) -> Result<GpuSpecs> {
        self.renderer
            .gpu_specs()
            .context("experimental HWND WGPU specs unavailable")
    }

    pub(crate) fn resize(&mut self, size: Size<DevicePixels>) -> Result<()> {
        self.ensure_live()?;
        if let Err(error) = validate_drawable_size(size, self.renderer.max_texture_size()) {
            self.terminal = true;
            return Err(error);
        }
        self.size = size;
        self.force_redraw = true;
        // A minimized window must not acquire/present a synthetic 1x1 frame.
        // Keep its last surface configuration until restored to a nonzero size.
        if size.width.0 > 0 && size.height.0 > 0 {
            self.renderer.update_drawable_size(size);
        }
        Ok(())
    }

    pub(crate) fn draw(
        &mut self,
        scene: &Scene,
        appearance: WindowBackgroundAppearance,
    ) -> Result<()> {
        self.ensure_live()?;
        if self.size.width.0 == 0 || self.size.height.0 == 0 {
            return Ok(());
        }
        self.renderer
            .update_transparency(appearance != WindowBackgroundAppearance::Opaque);
        if !self.renderer.draw(scene) {
            self.force_redraw = true;
            self.ensure_live()?;
        }
        // WGPU draw owns acquisition, submission, and present exactly once.
        Ok(())
    }

    pub(crate) fn mark_drawable(&mut self) {
        // Never reset terminal device loss in response to a D3D11 font event.
    }

    pub(crate) fn needs_redraw(&mut self) -> bool {
        self.renderer.needs_redraw() | std::mem::take(&mut self.force_redraw)
    }

    pub(crate) fn handle_device_lost(&mut self, _font_devices: &DirectXDevices) -> Result<()> {
        // The platform's message concerns the independent D3D11 font/vsync
        // device. Never adopt it as the renderer device or call WGPU recover().
        self.ensure_live()?;
        self.force_redraw = true;
        Ok(())
    }

    pub(crate) fn destroy(&mut self) {
        self.terminal = true;
        self.renderer.destroy();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn render_to_image(
        &mut self,
        _scene: &Scene,
        _appearance: WindowBackgroundAppearance,
    ) -> Result<image::RgbaImage> {
        anyhow::bail!("CPU scene readback is unsupported by the experimental HWND renderer")
    }
}

fn validate_drawable_size(size: Size<DevicePixels>, limit: u32) -> Result<()> {
    ensure!(
        size.width.0 >= 0 && size.height.0 >= 0,
        "negative HWND drawable size"
    );
    ensure!(
        size.width.0 as u32 <= limit && size.height.0 as u32 <= limit,
        "HWND drawable exceeds WGPU texture limits"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawable_bounds_reject_negative_and_oversized_but_allow_minimized() {
        let valid =
            |w, h| validate_drawable_size(gpui::size(DevicePixels(w), DevicePixels(h)), 16384);
        assert!(valid(0, 0).is_ok());
        assert!(valid(3840, 2160).is_ok());
        assert!(valid(-1, 2160).is_err());
        assert!(valid(3840, -1).is_err());
        assert!(valid(16385, 2160).is_err());
    }
}
