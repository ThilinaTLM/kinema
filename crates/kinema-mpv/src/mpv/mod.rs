mod ffi;
#[cfg(feature = "gpu-render")]
mod gpu;
mod library;
#[cfg(feature = "gpu-render")]
mod platform;
mod player;
mod render;
mod software;

pub use player::{Config, Player};
pub use software::Frame;
