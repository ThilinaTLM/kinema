//! One interruptible eased transition for frame-driven values.

use crate::ui::theme::motion::Ease;

/// Retargeting mid-flight starts from the current value, so reversing never
/// snaps. Times are caller-supplied milliseconds on a shared clock.
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
            super::lerp(self.from, self.to, Ease::Out.apply(t / self.duration))
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
