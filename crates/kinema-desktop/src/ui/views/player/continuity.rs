//! Episode continuity in the player window: the Next/Previous controls and the
//! **Up next** card.
//!
//! The window never resolves anything. It shows the [`Neighbours`] it is given,
//! reports a user's intent through [`NeighbourRequests`] and shows a dock notice
//! when the coordinator says a request failed. Resolution, leases and the source
//! swap belong to the coordinator (see `controllers::coordinator::continuity`).
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NeighbourDirection {
    Next,
    Previous,
}

impl NeighbourDirection {
    /// Dock tooltip and notice noun.
    pub(crate) fn episode(self) -> &'static str {
        match self {
            Self::Next => "next episode",
            Self::Previous => "previous episode",
        }
    }
}

/// One direction of the dock, as the coordinator describes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NeighbourState {
    /// Not an episode, not tracked, or an external player: no control at all.
    #[default]
    Hidden,
    /// An episode, but there is no neighbour that way (first or last episode).
    // Published by Phase 2's neighbour resolution; tests and the lab build it today.
    #[allow(dead_code)]
    Disabled,
    /// A neighbour exists and can be requested.
    Enabled,
    /// A request is being resolved. The control shows busy and ignores input.
    Busy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Neighbours {
    pub previous: NeighbourState,
    pub next: NeighbourState,
}

impl Neighbours {
    pub fn get(self, direction: NeighbourDirection) -> NeighbourState {
        match direction {
            NeighbourDirection::Next => self.next,
            NeighbourDirection::Previous => self.previous,
        }
    }
    pub fn set(&mut self, direction: NeighbourDirection, state: NeighbourState) {
        match direction {
            NeighbourDirection::Next => self.next = state,
            NeighbourDirection::Previous => self.previous = state,
        }
    }
    /// The OS media controls advertise a direction only while the dock offers it.
    pub(crate) fn advertised(self, direction: NeighbourDirection) -> bool {
        self.get(direction) == NeighbourState::Enabled
    }
}

/// The window's outlet for "play the next/previous episode". Injected so the
/// window can be driven without a coordinator; production uses
/// `Entity<AppCoordinator>`.
pub(crate) trait NeighbourRequests {
    /// `session` and `generation` identify the window's current lease. The
    /// answer comes back as `set_neighbours` / `neighbour_failed` on the registry.
    fn request(&self, session: &str, generation: u64, direction: NeighbourDirection, cx: &mut App);
}

impl NeighbourRequests for Entity<AppCoordinator> {
    fn request(&self, session: &str, generation: u64, direction: NeighbourDirection, cx: &mut App) {
        let (owner, session) = (self.clone(), session.to_owned());
        // Deferred: the window may be updating inside the coordinator's effects.
        cx.defer(move |cx| {
            owner.update(cx, |coordinator, cx| {
                coordinator.request_neighbour(&session, generation, direction, cx)
            })
        });
    }
}

/// Whether the **Up next** countdown should start now.
pub(super) fn up_next_due(
    ended: bool,
    next: NeighbourState,
    dismissed: bool,
    shown: bool,
    swapping: bool,
) -> bool {
    ended && next == NeighbourState::Enabled && !dismissed && !shown && !swapping
}

/// Where the card is in its life.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum UpNextPhase {
    /// Shown ahead of the end (credits chapter or the last 30 s). Playback
    /// continues; only Cancel, Escape or Play now end it. `trigger` is the
    /// position, in seconds, where it appeared.
    Early { trigger: f64 },
    /// The file ended: a 5 s countdown, and any input cancels it.
    Countdown,
}

/// The Up next card. Its countdown task is owned here: dropping the card, on
/// input, close or swap, cancels it. The early phase has no task; the sample tick
/// drives it.
pub(super) struct UpNext {
    pub(super) phase: UpNextPhase,
    pub(super) remaining: u32,
    /// Steps this countdown started with; its bar fills over them, so a
    /// countdown that continues a full early bar starts empty, never part-filled.
    steps: u32,
    /// When the card first appeared, on the executor's clock.
    shown_at: Instant,
    _countdown: Task<()>,
}

impl UpNext {
    /// A countdown card frozen at `remaining` seconds, for the design lab.
    #[cfg(feature = "ui-probe")]
    pub(super) fn fixture(remaining: u32) -> Self {
        Self {
            phase: UpNextPhase::Countdown,
            remaining,
            steps: countdown_steps(),
            shown_at: Instant::now(),
            _countdown: Task::ready(()),
        }
    }

    /// An early card, for the design lab.
    #[cfg(feature = "ui-probe")]
    pub(super) fn fixture_early(trigger: f64) -> Self {
        Self {
            phase: UpNextPhase::Early { trigger },
            remaining: countdown_steps(),
            steps: countdown_steps(),
            shown_at: Instant::now(),
            _countdown: Task::ready(()),
        }
    }

    pub(super) fn is_early(&self) -> bool {
        matches!(self.phase, UpNextPhase::Early { .. })
    }
}

/// Steps left for a card that has been up for `shown`, at least one.
fn remaining_after(shown: Duration) -> u32 {
    let left = timing::UP_NEXT_COUNTDOWN.saturating_sub(shown);
    let tick = timing::UP_NEXT_TICK.as_millis().max(1);
    (left.as_millis().div_ceil(tick) as u32).clamp(1, countdown_steps())
}

/// Fill of a countdown bar that started at `steps`: forward, from empty to full.
pub(super) fn countdown_fill(remaining: u32, steps: u32) -> f32 {
    let total = steps.max(1);
    (total.saturating_sub(remaining) as f32 / total as f32).clamp(0., 1.)
}

/// Whole seconds in the countdown, from the token durations.
pub(super) fn countdown_steps() -> u32 {
    (timing::UP_NEXT_COUNTDOWN.as_millis() / timing::UP_NEXT_TICK.as_millis().max(1)) as u32
}

impl PlayerWindow {
    pub(super) fn neighbour_state(&self, direction: NeighbourDirection) -> NeighbourState {
        self.neighbours.get(direction)
    }

    /// The user (or the OS media controls, or the countdown) asked for a
    /// neighbouring episode. Returns whether a request went out.
    pub(super) fn request_neighbour(
        &mut self,
        direction: NeighbourDirection,
        cx: &mut Context<Self>,
    ) -> bool {
        self.activity(cx);
        if self.neighbour_state(direction) != NeighbourState::Enabled
            || self.swap.is_some()
            || !self.backend_available()
        {
            return false;
        }
        let (Some((session, generation)), Some(requests)) =
            (self.lease.clone(), self.neighbour_requests.clone())
        else {
            return false;
        };
        self.cancel_up_next();
        self.neighbour_notice = None;
        self.neighbour_note = None;
        // Busy at once, so a second press cannot queue a second request; the
        // coordinator's own state follows.
        self.neighbours.set(direction, NeighbourState::Busy);
        requests.request(&session, generation, direction, cx);
        cx.notify();
        true
    }

    pub(super) fn apply_neighbours(&mut self, neighbours: Neighbours, cx: &mut Context<Self>) {
        if self.neighbours == neighbours {
            return;
        }
        self.neighbours = neighbours;
        if self.neighbours.next != NeighbourState::Enabled {
            self.cancel_up_next();
        }
        cx.notify();
    }

    /// A request failed: the control returns to its prior state (the coordinator
    /// republishes it) and the dock says so.
    pub(super) fn show_neighbour_failure(
        &mut self,
        direction: Option<NeighbourDirection>,
        cx: &mut Context<Self>,
    ) {
        self.neighbour_notice = Some((direction, Instant::now()));
        self.neighbour_note = None;
        self.last_activity = Instant::now();
        cx.notify();
    }

    /// Shows `text` in the dock status: why a request failed, or how an episode
    /// was chosen. Replaces the generic failure wording.
    pub(super) fn show_neighbour_note(
        &mut self,
        text: String,
        warning: bool,
        cx: &mut Context<Self>,
    ) {
        self.neighbour_notice = Some((None, Instant::now()));
        self.neighbour_note = Some((text, warning));
        self.last_activity = Instant::now();
        cx.notify();
    }

    /// The dock note and whether it is a warning, while it is showing.
    pub(super) fn neighbour_note_text(&self) -> Option<(&str, bool)> {
        self.neighbour_notice?;
        self.neighbour_note
            .as_ref()
            .map(|(text, warning)| (text.as_str(), *warning))
    }

    /// Text for the dock status; `None` once it has run its course.
    pub(super) fn neighbour_notice_text(&self) -> Option<String> {
        let (direction, _) = self.neighbour_notice?;
        Some(match direction {
            Some(direction) => format!("Couldn’t open the {}", direction.episode()),
            None => "Couldn’t switch episode".to_owned(),
        })
    }

    /// Expires the dock notice. Returns whether the view changed.
    pub(super) fn observe_neighbour_notice(&mut self, now: Instant) -> bool {
        if self
            .neighbour_notice
            .is_some_and(|(_, since)| now.saturating_duration_since(since) >= timing::PLAYER_NOTICE)
        {
            self.neighbour_notice = None;
            self.neighbour_note = None;
            return true;
        }
        false
    }

    // ── Up next ──

    /// The position from which the card shows before the end, for this file.
    pub(super) fn up_next_trigger(&self) -> Option<f64> {
        let chapters: Vec<(f64, Option<&str>)> = self
            .state
            .chapters
            .iter()
            .map(|chapter| (chapter.time, chapter.title.as_deref()))
            .collect();
        kinema_core::episode_continuity::up_next_trigger(self.state.duration, &chapters)
    }

    /// Whether the card stops the chrome from auto-hiding: only the countdown.
    pub(super) fn up_next_pins_chrome(&self) -> bool {
        self.up_next.as_ref().is_some_and(|card| !card.is_early())
    }

    /// Drives the card from the sample tick: the early card at the trigger, the
    /// countdown at the end of the file. A no-op when already in the right phase.
    pub(super) fn observe_up_next(&mut self, cx: &mut Context<Self>) {
        if self.state.ended {
            self.up_next_saw_end = true;
            match self
                .up_next
                .as_ref()
                .map(|card| (card.phase, card.shown_at))
            {
                Some((UpNextPhase::Early { .. }, shown_at)) => {
                    let shown = cx
                        .background_executor()
                        .now()
                        .saturating_duration_since(shown_at);
                    if shown >= timing::UP_NEXT_COUNTDOWN {
                        self.play_up_next(cx);
                    } else {
                        self.start_countdown(remaining_after(shown), cx);
                    }
                }
                Some((UpNextPhase::Countdown, _)) => {}
                None => {
                    if up_next_due(
                        true,
                        self.neighbours.next,
                        self.up_next_dismissed,
                        false,
                        self.swap.is_some(),
                    ) {
                        self.start_countdown(countdown_steps(), cx);
                    }
                }
            }
            return;
        }
        if std::mem::take(&mut self.up_next_saw_end) {
            // Replay or seek away from the end: the next ending may offer it again.
            self.up_next_dismissed = false;
        }
        let trigger = self
            .up_next_trigger()
            .filter(|trigger| self.state.position >= *trigger);
        let Some(trigger) = trigger else {
            // Before the trigger (or none): seeking back re-arms the card.
            self.up_next_dismissed = false;
            if self.up_next.is_some() {
                self.cancel_up_next();
                self.up_next_dismissed = false;
                cx.notify();
            }
            return;
        };
        if self.up_next.is_none()
            && self.neighbours.next == NeighbourState::Enabled
            && !self.up_next_dismissed
            && self.swap.is_none()
        {
            self.show_early_up_next(trigger, cx);
        }
    }

    fn show_early_up_next(&mut self, trigger: f64, cx: &mut Context<Self>) {
        self.up_next = Some(UpNext {
            phase: UpNextPhase::Early { trigger },
            remaining: countdown_steps(),
            steps: countdown_steps(),
            shown_at: cx.background_executor().now(),
            _countdown: Task::ready(()),
        });
        cx.notify();
    }

    /// Starts (or continues, from the early card) the countdown at `remaining`.
    fn start_countdown(&mut self, remaining: u32, cx: &mut Context<Self>) {
        let countdown = cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(timing::UP_NEXT_TICK).await;
                match view.update(cx, |view, cx| view.up_next_tick(cx)) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        });
        let shown_at = self
            .up_next
            .as_ref()
            .map_or_else(|| cx.background_executor().now(), |card| card.shown_at);
        self.up_next = Some(UpNext {
            phase: UpNextPhase::Countdown,
            remaining,
            steps: remaining,
            shown_at,
            _countdown: countdown,
        });
        cx.notify();
    }

    /// One countdown step. Returns whether the countdown continues.
    fn up_next_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(card) = self.up_next.as_mut() else {
            return false;
        };
        card.remaining = card.remaining.saturating_sub(1);
        if card.remaining > 0 {
            cx.notify();
            return true;
        }
        self.play_up_next(cx);
        false
    }

    /// **Play now**, or the countdown reaching zero.
    pub(super) fn play_up_next(&mut self, cx: &mut Context<Self>) {
        // Taking the card drops (cancels) its task, even when called from it.
        let card = self.up_next.take();
        self.up_next_dismissed = true;
        if card.is_some() {
            self.request_neighbour(NeighbourDirection::Next, cx);
        }
        cx.notify();
    }

    /// Cancels the countdown for good until the next ending. Returns whether a
    /// card was showing.
    pub(super) fn cancel_up_next(&mut self) -> bool {
        self.up_next_dismissed = true;
        self.up_next.take().is_some()
    }

    /// Any deliberate input (key, click, wheel) ends the countdown. Pointer
    /// movement alone does not: it only reveals the chrome.
    /// The early card ignores it: only Cancel and Escape dismiss that one.
    pub(super) fn user_input(&mut self, cx: &mut Context<Self>) {
        if self.up_next.as_ref().is_some_and(|card| !card.is_early()) && self.cancel_up_next() {
            cx.notify();
        }
    }

    pub(super) fn up_next_card(
        &self,
        chrome_visible: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = self.up_next.as_ref()?;
        let p = Palette::cinema();
        // Above the dock while it shows; at the bottom edge when it is hidden.
        let dock = if !chrome_visible {
            Pixels::default()
        } else if self.dock_height > Pixels::default() {
            self.dock_height
        } else {
            DOCK_ESTIMATE
        };
        let (remaining, steps) = (card.remaining, card.steps);
        let (label, line, fill) = match card.phase {
            UpNextPhase::Early { trigger } => (
                "Up next. The next episode plays when this one ends.".to_owned(),
                "Next episode when this one ends".to_owned(),
                kinema_core::episode_continuity::up_next_fill(
                    self.state.position,
                    trigger,
                    self.state.duration.unwrap_or(0.),
                ),
            ),
            UpNextPhase::Countdown => (
                format!("Up next. Playing the next episode in {remaining} seconds."),
                format!("Next episode in {remaining}…"),
                countdown_fill(remaining, steps),
            ),
        };
        // Position on a wrapper: the entry preset offsets `top`, which on an
        // element pinned by `bottom` would stretch it to the window's top edge.
        let card = div()
            .id("player-up-next")
            .w(player::UP_NEXT_WIDTH)
            .flex()
            .flex_col()
            .gap(s::S8)
            .p(s::S12)
            .occlude()
            .rounded(radius::LG)
            .bg(p.scrim)
            .border_1()
            .border_color(p.border)
            .aria_label(label)
            // A press inside the card is a choice, not "other input".
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text(TextSize::Sm)
                    .weight(Weight::Semibold)
                    .text_color(p.foreground)
                    .child("Up next"),
            )
            .child(
                div()
                    .text(TextSize::Sm)
                    .text_color(p.muted_foreground)
                    .child(line),
            )
            .child(
                div()
                    .w_full()
                    .h(s::S4)
                    .rounded(radius::FULL)
                    .bg(p.input.opacity(opacity::TRACK))
                    .child(
                        div()
                            .h_full()
                            .w(relative(fill))
                            .rounded(radius::FULL)
                            .bg(p.primary),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(s::S8)
                    .child(
                        Button::new("player-up-next-play", "Play now")
                            .variant(ButtonVariant::Primary)
                            .over_art(true)
                            .focus_handle(self.controls[UP_NEXT_PLAY].clone())
                            .on_click(cx.listener(|view, _, _, cx| view.play_up_next(cx))),
                    )
                    .child(
                        Button::new("player-up-next-cancel", "Cancel")
                            .variant(ButtonVariant::Ghost)
                            .over_art(true)
                            .focus_handle(self.controls[UP_NEXT_CANCEL].clone())
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.cancel_up_next();
                                cx.notify();
                            })),
                    ),
            )
            .animate_enter("player-up-next-enter", POPUP_IN, cx);
        Some(
            div()
                .absolute()
                .right(DOCK_SIDE_INSET)
                .bottom(dock + DOCK_BOTTOM_INSET + s::S16)
                .child(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn countdown_is_five_whole_seconds() {
        assert_eq!(countdown_steps(), 5);
    }

    #[test]
    fn up_next_starts_only_for_an_ended_file_with_a_ready_next_episode() {
        use NeighbourState::*;
        assert!(up_next_due(true, Enabled, false, false, false));
        for next in [Hidden, Disabled, Busy] {
            assert!(!up_next_due(true, next, false, false, false), "{next:?}");
        }
        assert!(
            !up_next_due(false, Enabled, false, false, false),
            "not ended"
        );
        assert!(!up_next_due(true, Enabled, true, false, false), "dismissed");
        assert!(
            !up_next_due(true, Enabled, false, true, false),
            "already shown"
        );
        assert!(!up_next_due(true, Enabled, false, false, true), "mid-swap");
    }

    #[test]
    fn only_an_enabled_direction_is_advertised() {
        use NeighbourState::*;
        let mut n = Neighbours::default();
        for state in [Hidden, Disabled, Busy] {
            n.set(NeighbourDirection::Next, state);
            assert!(!n.advertised(NeighbourDirection::Next), "{state:?}");
        }
        n.set(NeighbourDirection::Next, Enabled);
        assert!(n.advertised(NeighbourDirection::Next));
        assert!(!n.advertised(NeighbourDirection::Previous));
    }
}
