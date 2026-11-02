//! Desktop v3 motion runtime (see `docs/design/desktop-v3.md` §2.6).
//!
//! Values live in [`crate::ui::theme::motion`]; this module turns them into
//! named [`transition`] presets, the shared [`presence`] lifecycle and the
//! interaction helpers. It is the only place that may call GPUI's animation
//! APIs (`Animation::new`, `with_animation`, `with_spring`, `SpringConfig`).
//! Reduced motion resolves every preset to zero duration.

use std::time::Duration;

use gpui::App;

pub mod card_hover;
pub mod interaction;
pub mod presence;
pub mod transition;
pub mod tween;

pub use card_hover::{CardHover, CardHoverValues};
pub use presence::{Closed, Phase, Presence};
pub use transition::{AnimateExt, Axis, Effect, Resolved, Transition};
pub use tween::Tween;

use crate::ui::theme::theme;

/// True when the effective reduced-motion preference is on.
pub fn reduced(cx: &App) -> bool {
    cx.try_global::<crate::ui::theme::Theme>().is_some() && theme(cx).reduce_motion
}

/// Resolve a preset against the current reduced-motion preference.
pub fn resolve(preset: Transition, cx: &App) -> Resolved {
    preset.resolved(reduced(cx))
}

/// Effective duration of a preset: zero when reduced motion is active.
pub fn duration(preset: Transition, cx: &App) -> Duration {
    resolve(preset, cx).duration
}

/// Linear interpolation helper for animated geometry.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}
