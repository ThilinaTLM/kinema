//! Smooth mouse-wheel scrolling.
//!
//! GPUI applies every wheel event to the scroll offset at once, so a notched
//! wheel (`ScrollDelta::Lines`) jumps several lines per click. [`SmoothWheel`]
//! turns each notch into a short [`WHEEL`] glide instead. Attach its
//! [`SmoothWheel::listener`] with `on_scroll_wheel` on the same element that
//! tracks the handle, and paint its [`SmoothWheel::probe`] in the same frame.
//! The probe records the offset before GPUI's own scroll handler applies the
//! event; the listener runs right after that handler, reverts the jump it
//! made, and glides to the destination instead. Measuring the applied jump,
//! rather than recomputing it, keeps GPUI's line height and axis mapping.
//!
//! Precise (touchpad/pixel) deltas already arrive per frame and pass through
//! untouched, as do keyboard, scrollbar and programmatic scrolling. Any of
//! them, or a layout clamp, moving the offset mid-glide makes the glide yield.
//! Reduced motion keeps GPUI's instant jump.

use std::{cell::RefCell, rc::Rc, time::Instant};

use gpui::{
    App, DispatchPhase, EntityId, IntoElement, Pixels, Point, ScrollHandle, ScrollWheelEvent,
    Styled, Window, canvas, px,
};

use super::{Tween, resolve, transition::Axis, transition::WHEEL};
use crate::ui::theme::size;

/// Pure glide state along one axis. Offsets are GPUI scroll offsets: `0` at
/// the start, `-max` at the end. Times are milliseconds on a caller clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Glide {
    tween: Tween,
    /// Offset written by the last frame; `None` when idle.
    last: Option<f32>,
}

impl Default for Glide {
    fn default() -> Self {
        Self {
            tween: Tween::at(0.),
            last: None,
        }
    }
}

impl Glide {
    pub(crate) fn active(&self) -> bool {
        self.last.is_some()
    }

    /// Add one wheel step of `delta` from the displayed `current` offset.
    /// Steps in the same direction stack onto the pending destination; a
    /// reversal starts from where the content is now.
    pub(crate) fn wheel(&mut self, now: f32, current: f32, delta: f32, max: f32, duration: f32) {
        let pending = self.tween.target() - current;
        let base = if self.continues(current) && pending * delta > 0. {
            self.tween.target()
        } else {
            current
        };
        let target = (base + delta).clamp(-max.max(0.), 0.);
        self.tween = Tween::at(current);
        self.tween.retarget(now, target, 0., duration);
        self.last = Some(current);
    }

    /// Advance to `now`. `observed` is the handle's offset: if something else
    /// moved it since the last frame, the glide yields and returns `None`.
    pub(crate) fn tick(&mut self, now: f32, observed: f32) -> Option<f32> {
        if !self.continues(observed) {
            self.last = None;
            return None;
        }
        let value = self.tween.value(now);
        self.last = (!self.tween.settled(now)).then_some(value);
        Some(value)
    }

    fn continues(&self, observed: f32) -> bool {
        self.last
            .is_some_and(|last| (observed - last).abs() <= f32::from(size::SUBPIXEL))
    }
}

fn get(offset: Point<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::X => offset.x,
        Axis::Y => offset.y,
    }
}

fn set(handle: &ScrollHandle, axis: Axis, value: Pixels) {
    let mut offset = handle.offset();
    match axis {
        Axis::X => offset.x = value,
        Axis::Y => offset.y = value,
    }
    handle.set_offset(offset);
}

struct State {
    glide: Glide,
    /// Handle offset when the current wheel event entered capture phase.
    before: Option<Point<Pixels>>,
    epoch: Instant,
    ticking: bool,
}

/// Shared smooth-wheel state for one scroller. Clones share state; keep one
/// per scroll handle (a view field or keyed element state).
#[derive(Clone)]
pub struct SmoothWheel(Rc<RefCell<State>>);

impl Default for SmoothWheel {
    fn default() -> Self {
        Self::new()
    }
}

impl SmoothWheel {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(State {
            glide: Glide::default(),
            before: None,
            epoch: Instant::now(),
            ticking: false,
        })))
    }

    /// Zero-size element that records the offset of `handle` before GPUI
    /// applies a wheel event. Paint it anywhere in the scroller's frame.
    pub fn probe(&self, handle: &ScrollHandle) -> impl IntoElement + use<> {
        let state = self.0.clone();
        let handle = handle.clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let (state, handle) = (state.clone(), handle.clone());
                window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, _| {
                    if phase == DispatchPhase::Capture {
                        state.borrow_mut().before = Some(handle.offset());
                    }
                });
            },
        )
        .absolute()
        .size(Pixels::ZERO)
    }

    /// Wheel listener for the element that tracks `handle` and scrolls along
    /// `axis`. Call during render so the enclosing view is notified per frame.
    pub fn listener(
        &self,
        handle: &ScrollHandle,
        axis: Axis,
        window: &Window,
        cx: &App,
    ) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
        let state = self.0.clone();
        let handle = handle.clone();
        let view = window.current_view();
        let duration = resolve(WHEEL, cx).duration.as_secs_f32() * 1000.;
        move |event, window, _| {
            let Some(before) = state.borrow_mut().before.take() else {
                return;
            };
            if event.delta.precise() || duration <= 0. {
                return;
            }
            // GPUI's own handler already jumped; undo it and glide instead.
            let current = get(before, axis);
            let delta = get(handle.offset(), axis) - current;
            if delta == Pixels::ZERO {
                return;
            }
            let max = get(handle.max_offset(), axis);
            {
                let mut s = state.borrow_mut();
                let now = s.epoch.elapsed().as_secs_f32() * 1000.;
                s.glide.wheel(
                    now,
                    f32::from(current),
                    f32::from(delta),
                    f32::from(max),
                    duration,
                );
            }
            set(&handle, axis, current);
            schedule(state.clone(), handle.clone(), axis, view, window);
        }
    }
}

fn schedule(
    state: Rc<RefCell<State>>,
    handle: ScrollHandle,
    axis: Axis,
    view: EntityId,
    window: &Window,
) {
    if std::mem::replace(&mut state.borrow_mut().ticking, true) {
        return;
    }
    window.on_next_frame(move |window, cx| frame(state, handle, axis, view, window, cx));
}

fn frame(
    state: Rc<RefCell<State>>,
    handle: ScrollHandle,
    axis: Axis,
    view: EntityId,
    window: &mut Window,
    cx: &mut App,
) {
    let (value, active) = {
        let mut s = state.borrow_mut();
        s.ticking = false;
        let now = s.epoch.elapsed().as_secs_f32() * 1000.;
        let value = s.glide.tick(now, f32::from(get(handle.offset(), axis)));
        (value, s.glide.active())
    };
    if let Some(value) = value {
        set(&handle, axis, px(value));
        cx.notify(view);
    }
    if active {
        schedule(state, handle, axis, view, window);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const D: f32 = 180.;

    #[test]
    fn a_notch_glides_from_the_current_offset_to_its_destination() {
        let mut g = Glide::default();
        g.wheel(0., -100., -60., 1000., D);
        assert!(g.active());
        assert_eq!(g.tick(0., -100.), Some(-100.));
        let mid = g.tick(60., -100.).unwrap();
        assert!(mid < -100. && mid > -160., "{mid}");
        assert_eq!(g.tick(D, mid), Some(-160.));
        assert!(!g.active());
        assert_eq!(g.tick(D + 16., -160.), None);
    }

    #[test]
    fn notches_in_one_direction_stack_onto_the_destination() {
        let mut g = Glide::default();
        g.wheel(0., 0., -60., 1000., D);
        let at = g.tick(30., 0.).unwrap();
        g.wheel(30., at, -60., 1000., D);
        assert_eq!(g.tick(30. + D, at), Some(-120.));
    }

    #[test]
    fn reversing_starts_from_the_current_offset() {
        let mut g = Glide::default();
        g.wheel(0., 0., -60., 1000., D);
        let at = g.tick(30., 0.).unwrap();
        g.wheel(30., at, 60., 1000., D);
        // Clamped at the top: the pending -60 is discarded, not netted.
        assert_eq!(g.tick(30. + D, at), Some(0.));
        let mut g = Glide::default();
        g.wheel(0., -500., -60., 1000., D);
        let at = g.tick(30., -500.).unwrap();
        g.wheel(30., at, 60., 1000., D);
        assert!((g.tick(30. + D, at).unwrap() - (at + 60.)).abs() < 1e-3);
    }

    #[test]
    fn destinations_clamp_to_the_scroll_range() {
        let mut g = Glide::default();
        g.wheel(0., -980., -60., 1000., D);
        assert_eq!(g.tick(D, -980.), Some(-1000.));
        let mut g = Glide::default();
        g.wheel(0., 0., 60., 1000., D);
        assert_eq!(g.tick(D, 0.), Some(0.));
        // Nothing to scroll.
        let mut g = Glide::default();
        g.wheel(0., 0., -60., 0., D);
        assert_eq!(g.tick(D, 0.), Some(0.));
    }

    #[test]
    fn an_outside_scroll_makes_the_glide_yield() {
        let mut g = Glide::default();
        g.wheel(0., 0., -60., 1000., D);
        let at = g.tick(30., 0.).unwrap();
        // Keyboard, scrollbar or touchpad moved the content meanwhile.
        assert_eq!(g.tick(60., at - 200.), None);
        assert!(!g.active());
        // A stale pending destination does not leak into the next notch.
        g.wheel(90., -300., -60., 1000., D);
        assert_eq!(g.tick(90. + D, -300.), Some(-360.));
    }
}
