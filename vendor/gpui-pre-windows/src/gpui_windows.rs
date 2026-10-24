#![cfg(target_os = "windows")]

mod clipboard;
mod destination_list;
mod dialog;
mod direct_manipulation;
mod direct_write;
#[cfg_attr(feature = "gpu-video-experiment", allow(dead_code, reason = "legacy types retained for native Windows font rasterization"))]
mod directx_atlas;
#[cfg_attr(feature = "gpu-video-experiment", allow(dead_code, reason = "legacy types retained for native Windows font rasterization"))]
mod directx_devices;
#[cfg_attr(feature = "gpu-video-experiment", allow(dead_code, reason = "legacy types retained for native Windows font rasterization"))]
mod directx_renderer;
mod dispatcher;
mod display;
mod events;
mod keyboard;
mod platform;
mod system_notifications;
mod system_settings;
mod util;
mod vsync;
mod window;
mod wrapper;
#[cfg(feature = "gpu-video-experiment")]
mod wgpu_window_renderer;
#[cfg(feature = "gpu-video-experiment")]
pub(crate) use wgpu_window_renderer::WgpuWindowRenderer as WindowRenderer;
#[cfg(not(feature = "gpu-video-experiment"))]
pub(crate) use directx_renderer::DirectXRenderer as WindowRenderer;

pub(crate) use clipboard::*;
pub(crate) use destination_list::*;
pub(crate) use direct_write::*;
pub(crate) use directx_atlas::*;
pub(crate) use directx_devices::*;
pub(crate) use directx_renderer::*;
pub(crate) use dispatcher::*;
pub(crate) use display::*;
pub(crate) use events::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
pub(crate) use system_notifications::*;
pub(crate) use system_settings::*;
pub(crate) use util::*;
pub(crate) use vsync::*;
pub(crate) use window::*;
pub(crate) use wrapper::*;

pub use platform::WindowsPlatform;

pub(crate) use windows::Win32::Foundation::HWND;
