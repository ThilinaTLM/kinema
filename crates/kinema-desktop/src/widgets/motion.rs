//! Desktop v3 motion tokens (see `docs/design/desktop-v3.md` §2.6).
//!
//! Always obtain durations through [`duration`] so reduced motion is honored.

use std::time::Duration;

use gpui::App;

use super::tokens::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// Hover/pressed fills and borders.
    Feedback,
    /// Card hover panel, artwork zoom, artwork fade-in.
    Disclose,
    /// Shared poster flight (both directions).
    Flight,
    /// Detail content fade + rise after landing.
    Content,
    /// Action column slide (episodes <-> streams).
    Column,
    /// Menus, context menus, popovers opening.
    PopupIn,
    /// Menus, context menus, popovers closing.
    PopupOut,
    /// Modal sheet + scrim.
    Sheet,
}

impl Motion {
    /// Nominal duration, ignoring reduced motion.
    pub const fn nominal(self) -> Duration {
        Duration::from_millis(match self {
            Motion::Feedback => 80,
            Motion::Disclose => 160,
            Motion::Flight => 320,
            Motion::Content => 160,
            Motion::Column => 200,
            Motion::PopupIn => 100,
            Motion::PopupOut => 70,
            Motion::Sheet => 160,
        })
    }

    /// Delay before the animation starts (only `Content` has one).
    pub const fn delay(self) -> Duration {
        match self {
            Motion::Content => Duration::from_millis(40),
            _ => Duration::ZERO,
        }
    }

    /// Easing curve for this token.
    pub fn ease(self, t: f32) -> f32 {
        match self {
            Motion::Feedback => t.clamp(0.0, 1.0),
            Motion::Flight => ease_in_out_cubic(t),
            _ => ease_out_cubic(t),
        }
    }
}

/// Effective duration: zero when reduced motion is active.
pub fn duration(motion: Motion, cx: &App) -> Duration {
    if reduced(cx) {
        Duration::ZERO
    } else {
        motion.nominal()
    }
}

/// True when the effective reduced-motion preference is on.
pub fn reduced(cx: &App) -> bool {
    cx.try_global::<super::tokens::Theme>().is_some() && theme(cx).reduce_motion
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Linear interpolation helper for animated geometry.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_are_bounded_and_monotonic_at_ends() {
        for f in [ease_out_cubic, ease_in_out_cubic] {
            assert_eq!(f(0.0), 0.0);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
            assert_eq!(f(-1.0), 0.0);
            assert!(f(0.25) < f(0.75));
        }
    }

    #[test]
    fn durations_match_spec() {
        assert_eq!(Motion::Disclose.nominal(), Duration::from_millis(160));
        assert_eq!(Motion::Flight.nominal(), Duration::from_millis(320));
        assert_eq!(Motion::Content.delay(), Duration::from_millis(40));
    }
}
