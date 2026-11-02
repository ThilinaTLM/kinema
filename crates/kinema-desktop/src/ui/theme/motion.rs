//! Motion values: durations, easings, delays, travel distances and interaction
//! timings. Pure values; the runtime lives in [`crate::ui::motion`].

use gpui::{Pixels, px};

/// The three animation durations.
pub mod duration {
    use std::time::Duration;

    /// Hover/pressed fills, toggles.
    pub const FAST: Duration = Duration::from_millis(90);
    /// Default: popups, disclosure, hover lift, column slide.
    pub const NORMAL: Duration = Duration::from_millis(180);
    /// Large surfaces: sheet, crossfade, flight, card overlay entry.
    pub const SLOW: Duration = Duration::from_millis(280);
}

/// Easing curves. `Out` is the standard decelerating curve and never
/// overshoots; `InOut` is the emphasized one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    Linear,
    Out,
    InOut,
}

impl Ease {
    /// Eased progress for linear progress `t`; input is clamped to `0..=1`.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::Out => 1.0 - (1.0 - t).powi(3),
            Ease::InOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

/// Start delays.
pub mod delay {
    use std::time::Duration;

    /// Extra start delay per successive row.
    pub const STAGGER: Duration = Duration::from_millis(30);
    /// Hover intent: overlays start this long after the lift.
    pub const INTENT: Duration = Duration::from_millis(60);
}

/// Travel distances for rise/slide/lift effects.
pub mod distance {
    use super::{Pixels, px};

    pub const SM: Pixels = px(4.0);
    pub const MD: Pixels = px(8.0);
    pub const LG: Pixels = px(12.0);
}

/// Interaction timings (not animations; unaffected by reduced motion).
pub mod timing {
    use std::time::Duration;

    /// Hover time before a tooltip shows.
    pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
    /// Quiet time before a typed search is submitted.
    pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);
    /// Idle time before scrollbars fade out.
    pub const SCROLLBAR_IDLE: Duration = Duration::from_millis(600);
    /// Pause after which menu type-ahead starts a new prefix.
    pub const TYPEAHEAD_RESET: Duration = Duration::from_millis(700);
    /// Wait before a loading placeholder (skeleton) appears.
    pub const SKELETON_DELAY: Duration = Duration::from_millis(150);
    /// Period of looping skeleton/thumb pulses.
    pub const PULSE_PERIOD: Duration = Duration::from_millis(1600);
    /// How long a "Copied" confirmation stays visible.
    pub const COPIED_FEEDBACK: Duration = Duration::from_millis(1500);
    /// Period of one indeterminate spinner/progress sweep.
    pub const SPIN_PERIOD: Duration = Duration::from_millis(1000);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_ordered() {
        assert!(duration::FAST < duration::NORMAL && duration::NORMAL < duration::SLOW);
    }

    #[test]
    fn easings_are_bounded_and_hit_the_ends() {
        for ease in [Ease::Linear, Ease::Out, Ease::InOut] {
            assert_eq!(ease.apply(0.0), 0.0);
            assert!((ease.apply(1.0) - 1.0).abs() < 1e-6);
            assert_eq!(ease.apply(-1.0), 0.0);
            assert_eq!(ease.apply(2.0), 1.0);
            let mut previous = 0.0;
            for step in 0..=100 {
                let value = ease.apply(step as f32 / 100.0);
                assert!((previous..=1.0).contains(&value), "{ease:?} at {step}");
                previous = value;
            }
        }
    }
}
