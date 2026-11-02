//! Shared enter/exit lifecycle for overlays (popover, menu, sheet, tooltip).
//!
//! `Closed → Open → Closing { generation } → Closed`. Closing owns an exit
//! timer keyed by generation: reopening during the exit drops the timer and
//! bumps the generation, so a stale close can never hide the reopened overlay.
//! Hosts keep the overlay mounted while [`Presence::is_visible`] and render it
//! through [`Presence::animate`], which continues from the current progress
//! when an enter or exit is interrupted.

use std::{cell::Cell, rc::Rc};

use gpui::{prelude::*, *};

use super::{resolve, transition::Transition};

/// Lifecycle phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Open,
    Closing { generation: u64 },
}

/// Emitted once the exit finished and the overlay is no longer visible.
pub struct Closed;

pub struct Presence {
    phase: Phase,
    generation: u64,
    enter: Transition,
    exit: Transition,
    /// Last rendered visibility (0..=1), shared with the animation closure.
    progress: Rc<Cell<f32>>,
    /// Visibility the current enter/exit started from.
    from: f32,
    task: Option<Task<()>>,
}

impl EventEmitter<Closed> for Presence {}

impl Presence {
    /// Closed, with the given enter and exit presets.
    pub fn new(enter: Transition, exit: Transition) -> Self {
        Self {
            phase: Phase::Closed,
            generation: 0,
            enter,
            exit,
            progress: Rc::new(Cell::new(0.)),
            from: 0.,
            task: None,
        }
    }

    /// Already open (overlay created on demand).
    pub fn opened(enter: Transition, exit: Transition) -> Self {
        let mut presence = Self::new(enter, exit);
        presence.begin_open();
        presence
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Mounted: open or still playing its exit.
    pub fn is_visible(&self) -> bool {
        self.phase != Phase::Closed
    }

    /// Open and accepting interaction (not closing).
    pub fn is_open(&self) -> bool {
        self.phase == Phase::Open
    }

    fn begin_open(&mut self) -> bool {
        if self.phase == Phase::Open {
            return false;
        }
        // Dropping the task cancels a pending exit timer.
        self.task = None;
        self.from = if self.is_visible() {
            self.progress.get()
        } else {
            0.
        };
        self.generation = self.generation.wrapping_add(1);
        self.phase = Phase::Open;
        true
    }

    /// Returns the exit generation when a close started.
    fn begin_close(&mut self) -> Option<u64> {
        if self.phase != Phase::Open {
            return None;
        }
        self.from = self.progress.get();
        self.generation = self.generation.wrapping_add(1);
        self.phase = Phase::Closing {
            generation: self.generation,
        };
        Some(self.generation)
    }

    /// Complete the exit of `generation`; stale generations are ignored.
    fn finish(&mut self, generation: u64) -> bool {
        if self.phase != (Phase::Closing { generation }) {
            return false;
        }
        self.task = None;
        self.phase = Phase::Closed;
        self.progress.set(0.);
        self.from = 0.;
        true
    }

    /// Open, or reopen during an exit (continuing from its current progress).
    pub fn open(&mut self, cx: &mut Context<Self>) {
        if self.begin_open() {
            cx.notify();
        }
    }

    /// Start the exit; [`Closed`] is emitted when it has played (immediately
    /// under reduced motion). A later [`Self::open`] cancels it.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        let Some(generation) = self.begin_close() else {
            return;
        };
        let wait = resolve(self.exit, cx).duration;
        if wait.is_zero() {
            self.complete(generation, cx);
        } else {
            self.task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |this, cx| this.complete(generation, cx));
            }));
        }
        cx.notify();
    }

    fn complete(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.finish(generation) {
            cx.emit(Closed);
            cx.notify();
        }
    }

    /// Render `el` with the enter or exit preset for the current phase.
    /// `id` must be stable per overlay; the generation is added so every
    /// open/close restarts the animation.
    pub fn animate<E: Styled + IntoElement + 'static>(
        &self,
        el: E,
        id: impl Into<SharedString>,
        cx: &App,
    ) -> AnyElement {
        let closing = matches!(self.phase, Phase::Closing { .. });
        let preset = if closing { self.exit } else { self.enter };
        let r = resolve(preset, cx);
        let progress = self.progress.clone();
        if r.is_instant() {
            let shown = if closing { 0. } else { 1. };
            progress.set(shown);
            return r.effect.apply(el, shown).into_any_element();
        }
        let from = self.from;
        let id = ElementId::NamedInteger(id.into(), self.generation);
        el.with_animation(id, Animation::new(r.total()), move |el, t| {
            let t = r.progress_at(t);
            let shown = if closing {
                from * (1. - t)
            } else {
                from + (1. - from) * t
            };
            progress.set(shown);
            r.effect.apply(el, shown)
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::motion::transition::{POPUP_IN, POPUP_OUT};
    use ::core::prelude::v1::test;

    fn presence() -> Presence {
        Presence::new(POPUP_IN, POPUP_OUT)
    }

    #[test]
    fn open_close_follow_the_phases() {
        let mut p = presence();
        assert_eq!(p.phase(), Phase::Closed);
        assert!(!p.is_visible());
        assert!(p.begin_open());
        assert!(p.is_open() && p.is_visible());
        assert!(!p.begin_open(), "already open");
        let g = p.begin_close();
        assert_eq!(
            p.phase(),
            Phase::Closing {
                generation: g.unwrap_or(0)
            }
        );
        assert!(p.is_visible() && !p.is_open());
        assert!(p.begin_close().is_none(), "already closing");
    }

    #[test]
    fn close_then_settle_hides_the_overlay() {
        let mut p = presence();
        p.begin_open();
        let g = p.begin_close().unwrap_or(0);
        assert!(p.finish(g));
        assert_eq!(p.phase(), Phase::Closed);
        assert!(!p.finish(g), "settling twice is a no-op");
    }

    #[test]
    fn reopen_during_exit_cancels_the_stale_close() {
        let mut p = presence();
        p.begin_open();
        p.progress.set(0.5);
        let stale = p.begin_close().unwrap_or(0);
        assert_eq!(p.from, 0.5);
        p.progress.set(0.25);
        assert!(p.begin_open());
        assert_eq!(p.from, 0.25, "reopen continues from the visible progress");
        assert!(!p.finish(stale), "stale timer must not close");
        assert!(p.is_open());
        // A second close gets a fresh generation; the first stays stale.
        let fresh = p.begin_close().unwrap_or(0);
        assert_ne!(fresh, stale);
        assert!(!p.finish(stale));
        assert!(p.finish(fresh));
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
mod timer_tests {
    use super::{Closed, Phase, Presence};
    use crate::ui::motion::transition::{POPUP_IN, POPUP_OUT};
    use gpui::{AppContext, Entity, TestAppContext};
    use std::{cell::Cell, rc::Rc, time::Duration};

    fn setup(cx: &mut TestAppContext, reduced: bool) -> (Entity<Presence>, Rc<Cell<usize>>) {
        cx.update(|cx| {
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(reduced, cx);
        });
        let closed = Rc::new(Cell::new(0));
        let presence = cx.new(|_| Presence::opened(POPUP_IN, POPUP_OUT));
        let count = closed.clone();
        cx.update(|cx| {
            cx.subscribe(&presence, move |_, _: &Closed, _| {
                count.set(count.get() + 1)
            })
            .detach();
        });
        (presence, closed)
    }

    #[gpui::test]
    fn close_settles_after_the_exit_duration(cx: &mut TestAppContext) {
        let (presence, closed) = setup(cx, false);
        presence.update(cx, |p, cx| p.close(cx));
        cx.run_until_parked();
        assert!(
            cx.read(|cx| presence.read(cx).is_visible()),
            "still exiting"
        );
        cx.executor()
            .advance_clock(POPUP_OUT.duration + Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(cx.read(|cx| presence.read(cx).phase()), Phase::Closed);
        assert_eq!(closed.get(), 1);
    }

    #[gpui::test]
    fn reopen_during_exit_survives_the_stale_timer(cx: &mut TestAppContext) {
        let (presence, closed) = setup(cx, false);
        presence.update(cx, |p, cx| p.close(cx));
        cx.executor().advance_clock(Duration::from_millis(10));
        presence.update(cx, |p, cx| p.open(cx));
        cx.executor().advance_clock(POPUP_OUT.duration * 3);
        cx.run_until_parked();
        assert_eq!(cx.read(|cx| presence.read(cx).phase()), Phase::Open);
        assert_eq!(closed.get(), 0);
    }

    #[gpui::test]
    fn reduced_motion_closes_immediately(cx: &mut TestAppContext) {
        let (presence, closed) = setup(cx, true);
        presence.update(cx, |p, cx| p.close(cx));
        cx.run_until_parked();
        assert_eq!(cx.read(|cx| presence.read(cx).phase()), Phase::Closed);
        assert_eq!(closed.get(), 1);
    }
}
