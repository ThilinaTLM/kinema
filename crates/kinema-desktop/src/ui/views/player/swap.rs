//! The window half of a same-window source swap (episode continuity).
//!
//! The coordinator runs the sequence ([`controllers::coordinator::continuity`]);
//! this module is the window's three steps, each its own call so the
//! coordinator keeps the order and can undo:
//!
//! 3. [`rekey_window`]: the registry entry moves to the new session under a new
//!    generation. Anything addressed to the old session or generation no longer
//!    finds the window.
//! 4. [`set_subject`]: title, window title, subtitle target and a fresh
//!    [`Progress`] for the new episode. The OS media session follows, because it
//!    reads the same snapshot.
//! 5. [`load_source`]: `Player::load` with the paused-load and resume sequence of
//!    a first start, and audio/subtitle carry-over once the new tracks are known.
//!
//! After step 5 the window watches the new media. When it is ready, or failed,
//! it reports once ([`PlayerWindow::observe_swap`]) and the coordinator decides
//! which lease to release.
use super::*;
use kinema_core::tracks::{CarryTrack, TrackCarry, TrackPlan, TrackRole};

/// Why a swap step was not taken. Nothing has changed when a step is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwapRefusal {
    /// No live window for that session.
    NoWindow,
    /// The window cannot take a swap now (no backend, failed, not embedded).
    NotReady,
    /// A swap is already in progress, or the target session is already a window.
    Busy,
}

/// Step 4 payload: everything the window shows or saves against the new episode.
pub(crate) struct SwapSubject {
    pub title: PlaybackTitle,
    pub subtitle_target: Option<PlaybackTarget>,
    pub progress: Option<PlaybackLaunch>,
}

/// The new media is loading. `media_generation` is the outgoing file's, so only
/// a newer lifecycle can count as the new episode.
pub(super) struct WindowSwap {
    media_generation: u64,
}

/// Selections to restore once the new file's tracks are observed.
pub(super) struct PendingCarry {
    tracks: TrackCarry,
    after_generation: u64,
}

/// In-memory stand-in for the player so swap behavior can be exercised without
/// libmpv: loads and track selections are recorded instead of sent.
#[cfg(all(test, feature = "ui-probe-test"))]
#[derive(Default)]
pub(super) struct FixtureBackend {
    pub loads: Vec<(String, bool)>,
    pub tracks: Vec<(TrackKind, i64)>,
}

fn with_window<R>(
    session: &str,
    cx: &mut App,
    f: impl FnOnce(&mut PlayerWindow, &mut Window, &mut Context<PlayerWindow>) -> R,
) -> Result<R, SwapRefusal> {
    let handle = cx
        .try_global::<PlayerWindows>()
        .and_then(|registry| registry.entries.get(session))
        .map(|entry| entry.handle)
        .ok_or(SwapRefusal::NoWindow)?;
    handle
        .update(cx, |view, window, cx| f(view, window, cx))
        .map_err(|_| SwapRefusal::NoWindow)
}

/// Step 3. Returns the window's new generation.
pub(crate) fn rekey_window(from: &str, to: &str, cx: &mut App) -> Result<u64, SwapRefusal> {
    let registry = cx
        .try_global::<PlayerWindows>()
        .ok_or(SwapRefusal::NoWindow)?;
    if registry.entries.contains_key(to) {
        return Err(SwapRefusal::Busy);
    }
    let old_generation = registry
        .entries
        .get(from)
        .map(|entry| entry.generation)
        .ok_or(SwapRefusal::NoWindow)?;
    with_window(from, cx, |view, _, _| {
        if view.lease.as_ref() != Some(&(from.to_owned(), old_generation)) {
            return Err(SwapRefusal::NoWindow);
        }
        if view.swap.is_some() {
            return Err(SwapRefusal::Busy);
        }
        if view.mode != PlayerChoice::Embedded || !view.backend_available() {
            return Err(SwapRefusal::NotReady);
        }
        Ok(())
    })??;
    let registry = cx.global_mut::<PlayerWindows>();
    registry.next += 1;
    let generation = registry.next;
    let Some(mut entry) = registry.entries.remove(from) else {
        return Err(SwapRefusal::NoWindow);
    };
    entry.generation = generation;
    let handle = entry.handle;
    registry.entries.insert(to.to_owned(), entry);
    registry
        .focus_events
        .retain(|event| *event != desktop_target(old_generation));
    let session = to.to_owned();
    let active = handle
        .update(cx, move |view, window, _| {
            view.lease = Some((session, generation));
            window.is_window_active()
        })
        .unwrap_or(false);
    if active {
        record_player_focus(generation, cx);
    }
    Ok(generation)
}

/// Step 4, window half.
pub(crate) fn set_subject(
    session: &str,
    subject: SwapSubject,
    cx: &mut App,
) -> Result<(), SwapRefusal> {
    with_window(session, cx, |view, window, cx| {
        view.apply_subject(subject, window, cx)
    })
}

/// Step 5. With `watch`, the window then reports readiness or failure and
/// carries audio and subtitle choices over; a rollback passes `false`.
pub(crate) fn load_source(
    session: &str,
    url: &str,
    watch: bool,
    cx: &mut App,
) -> Result<(), SwapRefusal> {
    with_window(session, cx, |view, _, cx| view.begin_load(url, watch, cx))?
}

impl PlayerWindow {
    pub(super) fn apply_subject(
        &mut self,
        subject: SwapSubject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.set_window_title(&subject.title.full);
        self.title = subject.title;
        self.subtitle_target = subject.subtitle_target;
        self.progress = subject.progress.map(Progress::new);
        // Everything below belonged to the outgoing episode.
        self.menu = None;
        self.search = None;
        self.subtitle_pending = None;
        self.subtitle_notice = None;
        self.control_notice = false;
        self.neighbours = Neighbours::default();
        self.neighbour_notice = None;
        self.neighbour_note = None;
        self.up_next = None;
        self.up_next_dismissed = false;
        self.subtitle_position = None;
        self.delay_requested = None;
        self.speed_requested = None;
        self.seek_requested = None;
        self.scrub = None;
        self.hover_ratio = None;
        cx.notify();
    }

    pub(super) fn begin_load(
        &mut self,
        url: &str,
        watch: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), SwapRefusal> {
        if self.swap.is_some() {
            return Err(SwapRefusal::Busy);
        }
        if let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref()) {
            self.state = player.state();
        }
        let carry = TrackCarry::capture(&carry_tracks(&self.state.tracks));
        let generation = self.state.media_generation;
        // A tracked episode loads paused and plays when progress is ready; an
        // untracked one just plays.
        let paused = self.progress.is_some();
        // Discard the previous GPU generation before the load; the backend
        // rejects late frames from the obsolete source.
        #[cfg(feature = "gpu-surface")]
        {
            self.gpu_frame.take();
        }
        self.issue_load(url, paused)?;
        self.source_url = Some(url.to_owned());
        self.swap = watch.then_some(WindowSwap {
            media_generation: generation,
        });
        self.carry = (watch && !carry.is_empty()).then_some(PendingCarry {
            tracks: carry,
            after_generation: generation,
        });
        cx.notify();
        Ok(())
    }

    fn issue_load(&mut self, url: &str, paused: bool) -> Result<(), SwapRefusal> {
        #[cfg(all(test, feature = "ui-probe-test"))]
        if let Some(fixture) = self.fixture_backend.as_mut() {
            fixture.loads.push((url.to_owned(), paused));
            return Ok(());
        }
        let player = self
            .player
            .as_ref()
            .and_then(|owner| owner.player.as_ref())
            .ok_or(SwapRefusal::NotReady)?;
        player
            .set_paused(paused)
            .and_then(|()| player.load(url))
            .map_err(|_| SwapRefusal::NotReady)
    }

    /// Terminal failure of this window, unless it is a swap's failed media that
    /// the coordinator is about to roll back.
    pub(super) fn failed(&self) -> bool {
        !self.failure_masked() && terminal_failure(self.error.is_some(), &self.state)
    }

    /// Whether a failure on the media in flight belongs to a swap that the
    /// coordinator is about to roll back. Such a failure must not close the window.
    pub(super) fn failure_masked(&self) -> bool {
        self.rollback_after
            .is_some_and(|failed| self.state.media_generation <= failed)
            && self.state.fatal_error.is_none()
            && !self.state.stopped
    }

    /// Watches the incoming media and reports once: ready, or failed.
    pub(super) fn observe_swap(&mut self, cx: &mut Context<Self>) {
        if let Some(failed) = self.rollback_after
            && self.state.media_generation > failed
        {
            self.rollback_after = None;
        }
        let Some(swap) = &self.swap else {
            return;
        };
        let state = &self.state;
        let newer = state.media_generation > swap.media_generation;
        let progress_failed = self
            .progress
            .as_ref()
            .is_some_and(|p| p.phase == ProgressPhase::Unavailable);
        let media_failed = newer
            && state.media_stopped
            && (state.error.is_some() || state.failure_reason.is_some())
            && !state.stopped
            && state.fatal_error.is_none();
        if media_failed || progress_failed {
            self.rollback_after = Some(state.media_generation);
            self.swap = None;
            self.carry = None;
            self.report_swap(false, cx);
            return;
        }
        let progress_ready = self.progress.as_ref().is_none_or(|p| {
            matches!(
                p.phase,
                ProgressPhase::Ready
                    | ProgressPhase::Reset
                    | ProgressPhase::Active
                    | ProgressPhase::Choice
                    | ProgressPhase::Mismatch
            )
        });
        if newer && state.file_loaded && !state.loading && !state.media_stopped && progress_ready {
            self.swap = None;
            self.report_swap(true, cx);
        }
    }

    fn report_swap(&self, ready: bool, cx: &mut Context<Self>) {
        let (Some((session, generation)), Some(owner)) = (&self.lease, &self.progress_owner) else {
            return;
        };
        let (session, generation, owner) = (session.clone(), *generation, owner.clone());
        cx.defer(move |cx| {
            owner.update(cx, |owner, cx| {
                owner.swap_window_report(&session, generation, ready, cx)
            })
        });
    }

    /// Selects the previous language and flags once the new tracks are observed.
    pub(super) fn observe_carry(&mut self, cx: &mut Context<Self>) {
        let Some(carry) = &self.carry else {
            return;
        };
        if !(self.state.tracks_available
            && self.state.file_loaded
            && !self.state.loading
            && self.state.media_generation > carry.after_generation)
        {
            return;
        }
        let plan = carry.tracks.plan(&carry_tracks(&self.state.tracks));
        self.carry = None;
        self.apply_track_plan(plan, cx);
    }

    pub(super) fn apply_track_plan(&mut self, plan: TrackPlan, cx: &mut Context<Self>) {
        if let Some(id) = plan.audio {
            self.select_carried(TrackKind::Audio, id, cx);
        }
        if let Some(id) = plan.subtitle {
            self.select_carried(TrackKind::Subtitle, id, cx);
        }
    }

    fn select_carried(&mut self, kind: TrackKind, id: i64, cx: &mut Context<Self>) {
        #[cfg(all(test, feature = "ui-probe-test"))]
        if let Some(fixture) = self.fixture_backend.as_mut() {
            fixture.tracks.push((kind, id));
            return;
        }
        self.presentation_command(
            |player| match kind {
                TrackKind::Subtitle => player.select_subtitle(Some(id)),
                _ => player.select_audio(Some(id)),
            },
            cx,
        );
    }
}

/// The audio and subtitle tracks of the player state, for the carry-over policy
/// in `kinema_core::tracks`. Video tracks are never carried.
fn carry_tracks(tracks: &[kinema_mpv::Track]) -> Vec<CarryTrack<'_>> {
    tracks
        .iter()
        .filter_map(|track| {
            Some(CarryTrack {
                id: track.id,
                role: match track.kind {
                    TrackKind::Audio => TrackRole::Audio,
                    TrackKind::Subtitle => TrackRole::Subtitle,
                    _ => return None,
                },
                lang: track.lang.as_deref(),
                selected: track.selected,
                external: track.external,
                forced: track.forced,
                hearing_impaired: track.hearing_impaired,
            })
        })
        .collect()
}
