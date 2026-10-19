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
    /// Card lift/scale on hover or keyboard focus.
    Hover,
    /// Backdrop changes and placeholder-to-content transitions.
    Crossfade,
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
            Motion::Hover => 140,
            Motion::Crossfade => 240,
            Motion::Disclose => 160,
            Motion::Flight => 320,
            Motion::Content => 160,
            Motion::Column => 200,
            Motion::PopupIn => 180,
            Motion::PopupOut => 70,
            Motion::Sheet => 280,
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
            // Includes Sheet/PopupIn: gentle spring-like settle without overshoot.
            _ => ease_out_cubic(t),
        }
    }
}

/// Effective duration: zero when reduced motion is active.
pub fn duration(motion: Motion, cx: &App) -> Duration {
    effective_duration(motion, reduced(cx))
}

fn effective_duration(motion: Motion, reduce_motion: bool) -> Duration {
    if reduce_motion {
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
    fn sheet_and_popup_settle_without_overshoot() {
        for motion in [Motion::Sheet, Motion::PopupIn] {
            let mut previous = 0.0;
            for step in 0..=100 {
                let value = motion.ease(step as f32 / 100.0);
                assert!((previous..=1.0).contains(&value));
                previous = value;
            }
            assert_eq!(previous, 1.0);
        }
    }

    #[test]
    fn reduced_motion_zeroes_every_duration() {
        for motion in [
            Motion::Feedback,
            Motion::Hover,
            Motion::Crossfade,
            Motion::Disclose,
            Motion::Flight,
            Motion::Content,
            Motion::Column,
            Motion::PopupIn,
            Motion::PopupOut,
            Motion::Sheet,
        ] {
            assert_eq!(effective_duration(motion, true), Duration::ZERO);
            assert_eq!(effective_duration(motion, false), motion.nominal());
        }
    }

    #[test]
    fn durations_match_spec() {
        assert_eq!(Motion::Hover.nominal(), Duration::from_millis(140));
        assert_eq!(Motion::Crossfade.nominal(), Duration::from_millis(240));
        assert_eq!(Motion::Sheet.nominal(), Duration::from_millis(280));
        assert_eq!(Motion::PopupIn.nominal(), Duration::from_millis(180));
        assert_eq!(Motion::Disclose.nominal(), Duration::from_millis(160));
        assert_eq!(Motion::Flight.nominal(), Duration::from_millis(320));
        assert_eq!(Motion::Content.delay(), Duration::from_millis(40));
    }
}
