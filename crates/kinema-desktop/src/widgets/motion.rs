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

/// Poster-card hover timings (spec §6). Times are milliseconds.
pub mod card_hover {
    /// Lift + shadow, entering / leaving.
    pub const LIFT_IN_MS: f32 = 220.;
    pub const LIFT_OUT_MS: f32 = 200.;
    /// Scrim/panel fade, entering / leaving.
    pub const SCRIM_IN_MS: f32 = 260.;
    pub const SCRIM_OUT_MS: f32 = 200.;
    /// Text rows (title, facts, genres, synopsis) rise + fade.
    pub const ROW_IN_MS: f32 = 260.;
    pub const ROW_OUT_MS: f32 = 200.;
    /// Hover intent: the overlay starts this long after the lift.
    pub const INTENT_MS: f32 = 60.;
    /// Extra start delay per row on enter only.
    pub const STAGGER_MS: f32 = 30.;
    /// Rows travel this far (px) upward while fading in.
    pub const ROW_RISE_PX: f32 = 10.;
    pub const LIFT_PX: f32 = 3.;
    pub const ROWS: usize = 4;
}

/// One interruptible eased transition. Retargeting mid-flight starts from the
/// current value, so reversing never snaps. Times are caller-supplied ms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start: f32,
    delay: f32,
    duration: f32,
}

impl Tween {
    pub const fn at(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: 0.,
            delay: 0.,
            duration: 0.,
        }
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn value(&self, now: f32) -> f32 {
        let t = now - self.start - self.delay;
        if self.duration <= 0. || t >= self.duration {
            self.to
        } else if t <= 0. {
            self.from
        } else {
            lerp(self.from, self.to, ease_out_cubic(t / self.duration))
        }
    }

    pub fn settled(&self, now: f32) -> bool {
        now - self.start - self.delay >= self.duration
    }

    /// Head for `to`; a no-op when already heading there.
    pub fn retarget(&mut self, now: f32, to: f32, delay: f32, duration: f32) {
        if (to - self.to).abs() < f32::EPSILON {
            return;
        }
        *self = Self {
            from: self.value(now),
            to,
            start: now,
            delay,
            duration,
        };
    }

    /// Jump to `to` (reduced motion).
    pub fn snap(&mut self, to: f32) {
        *self = Self::at(to);
    }
}

/// Current eased values for one poster card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardHoverValues {
    pub lift: f32,
    pub scrim: f32,
    pub rows: [f32; card_hover::ROWS],
}

/// Per-card hover state: lift, scrim and four staggered text rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardHover {
    lift: Tween,
    scrim: Tween,
    rows: [Tween; card_hover::ROWS],
}

impl Default for CardHover {
    fn default() -> Self {
        Self {
            lift: Tween::at(0.),
            scrim: Tween::at(0.),
            rows: [Tween::at(0.); card_hover::ROWS],
        }
    }
}

impl CardHover {
    /// Point every channel at the hovered/idle state. Enter: lift now, overlay
    /// after the intent delay, rows staggered. Exit: all together, no delay.
    pub fn set_hovered(&mut self, now: f32, hovered: bool, reduced: bool) {
        use card_hover::*;
        let to = if hovered { 1. } else { 0. };
        if reduced {
            self.lift.snap(to);
            self.scrim.snap(to);
            self.rows.iter_mut().for_each(|r| r.snap(to));
            return;
        }
        if hovered {
            self.lift.retarget(now, to, 0., LIFT_IN_MS);
            self.scrim.retarget(now, to, INTENT_MS, SCRIM_IN_MS);
            for (i, row) in self.rows.iter_mut().enumerate() {
                row.retarget(now, to, INTENT_MS + STAGGER_MS * i as f32, ROW_IN_MS);
            }
        } else {
            self.lift.retarget(now, to, 0., LIFT_OUT_MS);
            self.scrim.retarget(now, to, 0., SCRIM_OUT_MS);
            for row in &mut self.rows {
                row.retarget(now, to, 0., ROW_OUT_MS);
            }
        }
    }

    pub fn values(&self, now: f32) -> CardHoverValues {
        CardHoverValues {
            lift: self.lift.value(now),
            scrim: self.scrim.value(now),
            rows: self.rows.map(|r| r.value(now)),
        }
    }

    pub fn settled(&self, now: f32) -> bool {
        self.lift.settled(now)
            && self.scrim.settled(now)
            && self.rows.iter().all(|r| r.settled(now))
    }
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

    #[test]
    fn tween_reversal_continues_from_current_value() {
        let mut t = Tween::at(0.);
        t.retarget(0., 1., 0., 200.);
        let mid = t.value(80.);
        assert!(mid > 0. && mid < 1.);
        t.retarget(80., 0., 0., 200.);
        assert!((t.value(80.) - mid).abs() < 1e-6, "no snap on reversal");
        assert!(t.value(120.) < mid);
        assert_eq!(t.value(400.), 0.);
        assert!(t.settled(400.));
    }

    #[test]
    fn tween_holds_start_value_during_delay_and_ignores_same_target() {
        let mut t = Tween::at(0.);
        t.retarget(10., 1., 60., 100.);
        assert_eq!(t.value(60.), 0.);
        assert!(t.value(100.) > 0.);
        assert!(!t.settled(100.));
        let before = t;
        t.retarget(50., 1., 0., 10.);
        assert_eq!(t, before);
    }

    #[test]
    fn card_hover_enter_staggers_and_exit_is_together() {
        let mut h = CardHover::default();
        h.set_hovered(0., true, false);
        let early = h.values(30.);
        assert!(early.lift > 0.);
        assert_eq!(early.scrim, 0., "overlay waits for hover intent");
        let v = h.values(130.);
        assert!(v.rows[0] > v.rows[1] && v.rows[1] > v.rows[2] && v.rows[2] > v.rows[3]);
        assert!(h.settled(1000.));
        assert_eq!(h.values(1000.).rows, [1.; 4]);
        h.set_hovered(1000., false, false);
        let v = h.values(1100.);
        assert!(v.rows.iter().all(|r| (r - v.rows[0]).abs() < 1e-6));
        assert_eq!(
            h.values(1200.),
            CardHoverValues {
                lift: 0.,
                scrim: 0.,
                rows: [0.; 4]
            }
        );
    }

    #[test]
    fn card_hover_brief_pass_never_flashes_overlay() {
        let mut h = CardHover::default();
        h.set_hovered(0., true, false);
        h.set_hovered(40., false, false);
        for now in [40., 80., 120., 300.] {
            assert_eq!(h.values(now).scrim, 0.);
        }
    }

    #[test]
    fn card_hover_reduced_motion_is_instant() {
        let mut h = CardHover::default();
        h.set_hovered(0., true, true);
        assert!(h.settled(0.));
        assert_eq!(
            h.values(0.),
            CardHoverValues {
                lift: 1.,
                scrim: 1.,
                rows: [1.; 4]
            }
        );
        h.set_hovered(1., false, true);
        assert_eq!(h.values(1.).lift, 0.);
    }
}
