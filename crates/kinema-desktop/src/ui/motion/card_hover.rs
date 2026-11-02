//! Poster-card hover choreography, expressed in motion presets.
//!
//! Enter: the card lifts ([`HOVER`]); after hover intent the scrim and four text
//! rows rise in ([`CARD_OVERLAY`]), each row one stagger later. Exit: everything
//! leaves together ([`CARD_OVERLAY_OUT`], [`HOVER`]).

use std::time::Duration;

use gpui::Pixels;

use super::{
    transition::{CARD_OVERLAY, CARD_OVERLAY_OUT, HOVER},
    tween::Tween,
};
use crate::ui::theme::motion::{delay, distance};

/// Number of staggered text rows (title, facts, genres, synopsis).
pub const ROWS: usize = 4;
/// Card lift while hovered; equals `HOVER`'s `Lift` distance.
pub const LIFT: Pixels = distance::SM;
/// Distance text rows travel upward while fading in; equals `CARD_OVERLAY`'s.
pub const ROW_RISE: Pixels = distance::MD;

fn ms(d: Duration) -> f32 {
    d.as_secs_f32() * 1000.
}

/// Current eased values for one poster card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardHoverValues {
    pub lift: f32,
    pub scrim: f32,
    pub rows: [f32; ROWS],
}

/// Per-card hover state: lift, scrim and four staggered text rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardHover {
    lift: Tween,
    scrim: Tween,
    rows: [Tween; ROWS],
}

impl Default for CardHover {
    fn default() -> Self {
        Self {
            lift: Tween::at(0.),
            scrim: Tween::at(0.),
            rows: [Tween::at(0.); ROWS],
        }
    }
}

impl CardHover {
    /// Point every channel at the hovered/idle state. Enter: lift now, overlay
    /// after the intent delay, rows staggered. Exit: all together, no delay.
    pub fn set_hovered(&mut self, now: f32, hovered: bool, reduced: bool) {
        let to = if hovered { 1. } else { 0. };
        if reduced {
            self.lift.snap(to);
            self.scrim.snap(to);
            self.rows.iter_mut().for_each(|r| r.snap(to));
            return;
        }
        let lift = ms(HOVER.duration);
        if hovered {
            let (intent, rise) = (ms(CARD_OVERLAY.delay), ms(CARD_OVERLAY.duration));
            self.lift.retarget(now, to, ms(HOVER.delay), lift);
            self.scrim.retarget(now, to, intent, rise);
            for (i, row) in self.rows.iter_mut().enumerate() {
                row.retarget(now, to, intent + ms(delay::STAGGER) * i as f32, rise);
            }
        } else {
            let out = ms(CARD_OVERLAY_OUT.duration);
            self.lift.retarget(now, to, 0., lift);
            self.scrim
                .retarget(now, to, ms(CARD_OVERLAY_OUT.delay), out);
            for row in &mut self.rows {
                row.retarget(now, to, ms(CARD_OVERLAY_OUT.delay), out);
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
    use crate::ui::motion::transition::Effect;

    #[test]
    fn distances_match_the_presets() {
        assert_eq!(HOVER.effect, Effect::Lift(LIFT));
        assert_eq!(CARD_OVERLAY.effect, Effect::Rise(ROW_RISE));
        assert_eq!(f32::from(LIFT), 4.);
        assert_eq!(f32::from(ROW_RISE), 8.);
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
