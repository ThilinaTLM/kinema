//! Named transition presets and the [`AnimateExt`] element helpers.
//!
//! A preset is `{ duration, ease, delay, effect }` built from
//! [`crate::ui::theme::motion`] values. Callers never touch GPUI animation types:
//! they pick a preset and apply it with [`AnimateExt`]. Reduced motion resolves
//! every preset to zero duration, which renders the final state unwrapped.

use std::time::Duration;

use gpui::{prelude::*, *};

use super::resolve;
use crate::ui::theme::motion::{Ease, delay, distance, duration};

/// Axis of a [`Effect::Slide`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// What a transition does to the element besides timing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Timing only; the caller drives the visual (fills, geometry).
    None,
    /// Opacity 0 → 1.
    Fade,
    /// Fade in while travelling up the given distance into place.
    Rise(Pixels),
    /// Travel along an axis into place (no fade).
    Slide(Axis, Pixels),
    /// Raise by the given distance (hover lift).
    Lift(Pixels),
}

impl Effect {
    /// Apply the effect at `shown` (0 = hidden/rest, 1 = fully shown/active).
    pub fn apply<E: Styled>(self, el: E, shown: f32) -> E {
        let shown = shown.clamp(0.0, 1.0);
        match self {
            Effect::None => el,
            Effect::Fade => el.opacity(shown),
            Effect::Rise(d) => el.opacity(shown).top(d * (1.0 - shown)),
            Effect::Slide(Axis::X, d) => el.left(d * (1.0 - shown)),
            Effect::Slide(Axis::Y, d) => el.top(d * (1.0 - shown)),
            Effect::Lift(d) => el.top(-(d * shown)),
        }
    }

    /// Travel distance of the effect (zero for `None`/`Fade`).
    pub fn distance(self) -> Pixels {
        match self {
            Effect::Rise(d) | Effect::Slide(_, d) | Effect::Lift(d) => d,
            Effect::None | Effect::Fade => Pixels::ZERO,
        }
    }
}

/// A named motion preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub duration: Duration,
    pub ease: Ease,
    pub delay: Duration,
    pub effect: Effect,
}

const fn preset(duration: Duration, ease: Ease, delay: Duration, effect: Effect) -> Transition {
    Transition {
        duration,
        ease,
        delay,
        effect,
    }
}

/// Hover/pressed fills and toggles: fast, linear, no travel.
pub const FEEDBACK: Transition = preset(duration::FAST, Ease::Linear, Duration::ZERO, Effect::None);
/// Hover state layer and card lift.
pub const HOVER: Transition = preset(
    duration::NORMAL,
    Ease::Out,
    Duration::ZERO,
    Effect::Lift(distance::SM),
);
/// Disclosure panels, artwork zoom/fade-in.
pub const DISCLOSE: Transition = preset(duration::NORMAL, Ease::Out, Duration::ZERO, Effect::Fade);
/// Popover, menu and tooltip entering.
pub const POPUP_IN: Transition = preset(
    duration::NORMAL,
    Ease::Out,
    Duration::ZERO,
    Effect::Rise(distance::SM),
);
/// Popover, menu and tooltip leaving.
pub const POPUP_OUT: Transition = preset(
    duration::FAST,
    Ease::Out,
    Duration::ZERO,
    Effect::Rise(distance::SM),
);
/// Detail content landing after navigation.
pub const CONTENT: Transition = preset(
    duration::NORMAL,
    Ease::Out,
    delay::STAGGER,
    Effect::Rise(distance::MD),
);
/// Action column / page slide (episodes ↔ streams, carousel paging).
pub const COLUMN: Transition = preset(
    duration::NORMAL,
    Ease::Out,
    Duration::ZERO,
    Effect::Slide(Axis::X, distance::LG),
);
/// Backdrop changes and placeholder-to-content transitions.
pub const CROSSFADE: Transition = preset(duration::SLOW, Ease::Out, Duration::ZERO, Effect::Fade);
/// Modal sheet and its scrim.
pub const SHEET: Transition = preset(
    duration::SLOW,
    Ease::Out,
    Duration::ZERO,
    Effect::Rise(distance::LG),
);
/// Shared poster flight (both directions); geometry is caller-driven.
pub const FLIGHT: Transition = preset(duration::SLOW, Ease::InOut, Duration::ZERO, Effect::None);
/// Poster-card hover scrim and text rows entering: starts after hover intent;
/// rows add [`delay::STAGGER`] each.
pub const CARD_OVERLAY: Transition = preset(
    duration::SLOW,
    Ease::Out,
    delay::INTENT,
    Effect::Rise(distance::MD),
);
/// Poster-card hover scrim and text rows leaving: together, no delay.
pub const CARD_OVERLAY_OUT: Transition = preset(
    duration::NORMAL,
    Ease::Out,
    Duration::ZERO,
    Effect::Rise(distance::MD),
);

/// Every preset by name, for enumeration in tests.
#[cfg(test)]
pub(crate) const ALL: [(&str, Transition); 12] = [
    ("FEEDBACK", FEEDBACK),
    ("HOVER", HOVER),
    ("DISCLOSE", DISCLOSE),
    ("POPUP_IN", POPUP_IN),
    ("POPUP_OUT", POPUP_OUT),
    ("CONTENT", CONTENT),
    ("COLUMN", COLUMN),
    ("CROSSFADE", CROSSFADE),
    ("SHEET", SHEET),
    ("FLIGHT", FLIGHT),
    ("CARD_OVERLAY", CARD_OVERLAY),
    ("CARD_OVERLAY_OUT", CARD_OVERLAY_OUT),
];

/// A preset with the reduced-motion preference applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    pub duration: Duration,
    pub delay: Duration,
    pub ease: Ease,
    pub effect: Effect,
}

impl Transition {
    /// Apply the reduced-motion preference: reduced → zero duration and delay.
    pub fn resolved(self, reduced: bool) -> Resolved {
        let (duration, delay) = if reduced {
            (Duration::ZERO, Duration::ZERO)
        } else {
            (self.duration, self.delay)
        };
        Resolved {
            duration,
            delay,
            ease: self.ease,
            effect: self.effect,
        }
    }
}

impl Resolved {
    /// Zero duration: render the final state, do not wrap in an animation.
    pub fn is_instant(&self) -> bool {
        self.duration.is_zero()
    }

    /// Delay plus duration.
    pub fn total(&self) -> Duration {
        self.delay + self.duration
    }

    /// Eased progress (0..=1) for linear progress `t` over [`Self::total`],
    /// holding at 0 during the delay.
    pub fn progress_at(&self, t: f32) -> f32 {
        if self.is_instant() {
            return 1.0;
        }
        let total = self.total().as_secs_f32();
        let elapsed = t.clamp(0.0, 1.0) * total;
        self.ease
            .apply((elapsed - self.delay.as_secs_f32()) / self.duration.as_secs_f32())
    }

    /// Eased progress for a wall-clock `elapsed` since the start (frame-driven
    /// animations that do not use GPUI's animation wrapper).
    pub fn progress(&self, elapsed: Duration) -> f32 {
        if self.is_instant() {
            1.0
        } else {
            self.progress_at(elapsed.as_secs_f32() / self.total().as_secs_f32())
        }
    }
}

/// Preset-driven animation for elements. The returned element can no longer be
/// styled. A zero (reduced-motion) duration renders the final state directly.
pub trait AnimateExt: IntoElement + Sized + 'static {
    /// Run the preset's effect from hidden to shown.
    fn animate_enter(self, id: impl Into<ElementId>, preset: Transition, cx: &App) -> AnyElement
    where
        Self: Styled,
    {
        self.animate_with(id, preset, cx, move |el, t| preset.effect.apply(el, t))
    }

    /// Run the preset's effect from shown to hidden. Under reduced motion the
    /// final (hidden) state renders; owners normally stop rendering instead.
    fn animate_exit(self, id: impl Into<ElementId>, preset: Transition, cx: &App) -> AnyElement
    where
        Self: Styled,
    {
        self.animate_with(id, preset, cx, move |el, t| {
            preset.effect.apply(el, 1.0 - t)
        })
    }

    /// Custom properties: `f` receives eased progress 0 → 1.
    fn animate_with(
        self,
        id: impl Into<ElementId>,
        preset: Transition,
        cx: &App,
        f: impl Fn(Self, f32) -> Self + 'static,
    ) -> AnyElement {
        let r = resolve(preset, cx);
        if r.is_instant() {
            return f(self, 1.0).into_any_element();
        }
        self.with_animation(id, Animation::new(r.total()), move |el, t| {
            f(el, r.progress_at(t))
        })
        .into_any_element()
    }

    /// Like [`Self::animate_with`], but always wrapped so the subtree keeps its
    /// element identity across states. When `run` is false, or motion is
    /// reduced, the animation lasts 1 ms: an instant switch.
    fn animate_stable(
        self,
        id: impl Into<ElementId>,
        preset: Transition,
        run: bool,
        cx: &App,
        f: impl Fn(Self, f32) -> Self + 'static,
    ) -> AnyElement {
        let r = resolve(preset, cx);
        let total = if run && !r.is_instant() {
            r.total()
        } else {
            Duration::from_millis(1) // token-exempt: 1 ms sentinel so with_animation completes at once; not a design duration
        };
        self.with_animation(id, Animation::new(total), move |el, t| {
            f(el, r.progress_at(t))
        })
        .into_any_element()
    }

    /// Endless loop over `period` (pulses, sweeps); `f` receives linear phase
    /// 0 → 1. Static (phase 0) under reduced motion; callers whose static
    /// look differs from phase 0 branch on [`super::reduced`] first.
    fn animate_loop(
        self,
        id: impl Into<ElementId>,
        period: Duration,
        cx: &App,
        f: impl Fn(Self, f32) -> Self + 'static,
    ) -> AnyElement {
        if super::reduced(cx) {
            return f(self, 0.0).into_any_element();
        }
        self.with_animation(id, Animation::new(period).repeat(), f)
            .into_any_element()
    }
}

impl<E: IntoElement + 'static> AnimateExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn every_preset_resolves_to_zero_under_reduced_motion() {
        for (name, preset) in ALL {
            let r = preset.resolved(true);
            assert_eq!(r.duration, Duration::ZERO, "{name} duration");
            assert_eq!(r.delay, Duration::ZERO, "{name} delay");
            assert!(r.is_instant(), "{name}");
            assert_eq!(r.progress(Duration::ZERO), 1.0, "{name} snaps to final");
            assert!(!preset.resolved(false).is_instant(), "{name} animates");
        }
    }

    #[test]
    fn presets_use_only_token_durations() {
        for (name, preset) in ALL {
            assert!(
                [duration::FAST, duration::NORMAL, duration::SLOW].contains(&preset.duration),
                "{name}"
            );
        }
    }

    #[test]
    fn preset_values_match_the_plan() {
        assert_eq!(FEEDBACK.duration, Duration::from_millis(90));
        assert_eq!(HOVER.effect, Effect::Lift(px(4.0)));
        assert_eq!(POPUP_IN.duration, Duration::from_millis(180));
        assert_eq!(POPUP_OUT.duration, Duration::from_millis(90));
        assert_eq!(CONTENT.delay, Duration::from_millis(30));
        assert_eq!(SHEET.effect, Effect::Rise(px(12.0)));
        assert_eq!(FLIGHT.ease, Ease::InOut);
        assert_eq!(CARD_OVERLAY.delay, Duration::from_millis(60));
        assert_eq!(CARD_OVERLAY_OUT.duration, Duration::from_millis(180));
    }

    #[test]
    fn progress_holds_during_delay_then_reaches_one() {
        let r = CONTENT.resolved(false);
        assert_eq!(r.progress(Duration::from_millis(10)), 0.0);
        assert_eq!(r.progress(Duration::from_millis(30)), 0.0);
        let mid = r.progress(Duration::from_millis(120));
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(r.progress(r.total()), 1.0);
        assert_eq!(r.progress(r.total() * 3), 1.0);
    }
}
