mod diagnostics;
mod ffi;
#[cfg(test)]
mod ffi_tests;
#[cfg(feature = "gpu-render")]
mod gpu;
#[cfg(feature = "gpu-surface")]
mod gpu_surface;
mod library;
mod metadata;
#[cfg(feature = "gpu-render")]
mod platform;
mod player;
mod render;
mod software;

pub use player::{Config, MAX_VOLUME, Player};
pub use software::Frame;

#[cfg(feature = "gpu-surface")]
pub use gpu_surface::{GpuSurfaceContext, GpuSurfaceFrame};
