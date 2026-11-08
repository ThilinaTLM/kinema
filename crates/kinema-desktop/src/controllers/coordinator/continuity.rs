//! Episode continuity, coordinator side: the same-window source swap, the
//! preparation a window keeps, and the neighbour state its dock shows.
//!
//! # The swap
//!
//! [`AppCoordinator::swap_embedded_source`] takes an open embedded window and an
//! already-launched lease for another episode, and runs the sequence in
//! `docs/architecture/playback.md#episode-continuity`:
//!
//! 1. flush the old run, acknowledged (the save has committed);
//! 2. hold the new lease next to the old one;
//! 3. re-key the window under the new session and a new generation, tombstoning
//!    the old session;
//! 4. begin a run for the new episode, and give the window its target, title and
//!    progress (the OS media session follows from the window);
//! 5. `Player::load` with the paused-load and resume sequence;
//! 6. once the new media is ready, release the old lease, exactly once.
//!
//! Failure before step 6 releases the **new** lease and puts the old episode
//! back: the window is re-keyed to the old session and resumes it from the
//! position saved in step 1. External players are never swapped, since only
//! embedded windows exist here.
//!
//! The pieces that wait (the new episode's saved progress, the flush
//! acknowledgement) are small stages of [`PendingSwap`], advanced by the events
//! that complete them and by the poll tick, so nothing blocks the UI.
use super::{AppCoordinator, PlaybackIntent};
use crate::{
    app::{AppAction, AppOutcome},
    catalog::playback_progress::PlaybackProgressSnapshot,
    media::PlaybackSession,
    providers::PreparedTorrent,
    ui::views::player::{self, NeighbourDirection, NeighbourState, Neighbours, SwapSubject},
};
use gpui::*;
use kinema_core::{
    media::{PlaybackTitle, playback_title},
    sources::{DebridKind, PlaybackTarget, RemoteFile, TorrentSource},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

/// How long the old run's final save may take before the swap is abandoned. The
/// user keeps the episode they are watching; nothing is lost.
const FLUSH_DEADLINE: Duration = Duration::from_secs(3);

/// What neighbour resolution (`neighbours.rs`) is driven by. The coordinator
/// answers through [`AppCoordinator::set_neighbours`] and
/// [`AppCoordinator::neighbour_failed`], and acts through
/// [`AppCoordinator::play_kept_file`] or [`AppCoordinator::swap_embedded_source`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NeighbourEvent {
    /// A window now plays `target` (`None` when it is not catalog-tracked). Fired
    /// when a window opens and when a swap completes: publish its neighbours.
    SubjectChanged {
        session: String,
        target: Option<PlaybackTarget>,
    },
    /// The user asked for a neighbour. The direction already shows as busy.
    Requested {
        session: String,
        direction: NeighbourDirection,
    },
    /// The window is gone; drop anything held for it.
    Closed { session: String },
}
impl EventEmitter<NeighbourEvent> for AppCoordinator {}

/// A debrid preparation that belongs to an open window: the pack's file list
/// and provider, so another file can be unlocked and played later.
#[derive(Clone, Debug)]
pub(crate) struct KeptPreparation {
    /// The worker's preparation id (the `Prepare` command generation).
    pub id: u64,
    pub provider: DebridKind,
    pub source: TorrentSource,
    pub files: Arc<[RemoteFile]>,
}

impl KeptPreparation {
    fn of(id: u64, prepared: &PreparedTorrent) -> Self {
        Self {
            id,
            provider: prepared.provider,
            source: prepared.source.clone(),
            files: prepared.files.clone().into(),
        }
    }
}

/// The episode a swap moves to.
#[derive(Clone, Debug)]
pub(crate) struct EpisodeLaunch {
    pub title: String,
    pub episode_title: Option<String>,
    /// Must be an episode.
    pub target: PlaybackTarget,
    pub intent: PlaybackIntent,
    /// Which dock button asked, for the failure notice.
    pub direction: Option<NeighbourDirection>,
}

pub(crate) struct SwapRequest {
    /// Session of the open window (the old lease).
    pub window: String,
    /// The new lease, already launched. It must have a row in `sessions`.
    pub session: PlaybackSession,
    pub episode: EpisodeLaunch,
    /// The preparation the new lease was played from. The window keeps it from
    /// now on: the same one stays, a different one replaces and cancels the old.
    pub preparation: Option<u64>,
}

/// Why a swap was refused. The new lease is released whenever one is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwapError {
    /// No open embedded window under that session.
    NoWindow,
    /// The window is already swapping.
    Busy,
    /// Only episodes swap; movies and untracked files do not.
    NotAnEpisode,
    /// The new lease is not a live session of this coordinator.
    UnknownLease,
    /// The request could not be queued (backpressure).
    Unavailable,
}

enum Stage {
    /// Waiting for the new episode's saved progress.
    ReadingProgress,
    /// Step 1 in flight: the old run's final save.
    Flushing { deadline: Instant, submitted: bool },
}

struct PendingSwap {
    request: SwapRequest,
    snapshot: Option<PlaybackProgressSnapshot>,
    stage: Stage,
}

/// Steps 3 to 5 are done; waiting for the window to report the new media.
struct LoadingSwap {
    /// The old session, whose lease is still held.
    from: String,
    direction: Option<NeighbourDirection>,
    /// What the window kept before, restored on failure and cancelled on success
    /// when the swap left that pack.
    kept_before: Option<KeptPreparation>,
    neighbours_before: Neighbours,
}

/// The six steps, recorded in tests to pin their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwapStep {
    FlushOld,
    NewLease,
    Rekey,
    NewRun,
    Load,
    ReleaseOld,
}

#[derive(Default)]
pub(super) struct ContinuityStore {
    /// File list of the latest preparation, claimed by the window that keeps it.
    prepared: Option<KeptPreparation>,
    /// `PlayFile` commands that asked to keep their preparation.
    keep_requests: HashMap<u64, u64>,
    /// The preparation each window keeps, by window session.
    kept: HashMap<String, KeptPreparation>,
    /// Launches whose result is a swap into an open window, not a new window.
    routes: HashMap<u64, (String, EpisodeLaunch)>,
    swaps: HashMap<String, PendingSwap>,
    /// By the new session.
    loading: HashMap<String, LoadingSwap>,
    /// What each window's dock was last told.
    neighbours: HashMap<String, Neighbours>,
    #[cfg(test)]
    pub(super) log: Vec<SwapStep>,
}

impl AppCoordinator {
    fn log_swap(&mut self, _step: SwapStep) {
        #[cfg(test)]
        self.continuity.log.push(_step);
    }
}

// ── kept preparation ──

impl AppCoordinator {
    /// The pack a window keeps, for resolving its neighbours.
    pub(crate) fn kept_preparation(&self, window: &str) -> Option<&KeptPreparation> {
        self.continuity.kept.get(window)
    }

    /// True while a window keeps this preparation; its play session then leaves
    /// it to the window instead of cancelling it.
    pub(crate) fn preparation_is_kept(&self, preparation: u64) -> bool {
        self.continuity
            .kept
            .values()
            .any(|kept| kept.id == preparation)
    }

    /// Whether a play of this preparation should be kept by its window: an
    /// episode that opens in the embedded player.
    pub(crate) fn wants_kept_preparation(&self, preparation: u64) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|s| s.player.is_managed())
            && self
                .session_identity
                .get(&preparation)
                .is_some_and(|(_, target, _)| matches!(target, PlaybackTarget::Episode { .. }))
    }

    /// Remembers a freshly prepared pack's files until a window claims them.
    pub(super) fn note_prepared(&mut self, id: u64, prepared: &PreparedTorrent) {
        self.continuity.prepared = Some(KeptPreparation::of(id, prepared));
    }

    pub(super) fn continuity_keep_request(&mut self, generation: u64, preparation: u64) {
        self.continuity
            .keep_requests
            .insert(generation, preparation);
    }

    pub(super) fn forget_prepared(&mut self, preparation: u64) {
        if self
            .continuity
            .prepared
            .as_ref()
            .is_some_and(|p| p.id == preparation)
        {
            self.continuity.prepared = None;
        }
    }

    /// A start that will not be used (stale, or its window already closed): drop
    /// the route and cancel the preparation it asked to keep.
    pub(super) fn discard_route(
        &mut self,
        generation: u64,
        keep: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        self.continuity.routes.remove(&generation);
        if let Some(id) = keep
            && !self.preparation_is_kept(id)
        {
            self.cancel_preparation(id, cx);
        }
    }

    /// Cancels the worker's copy and forgets ours. Idempotent.
    fn release_kept(&mut self, kept: KeptPreparation, cx: &mut Context<Self>) {
        if self
            .continuity
            .prepared
            .as_ref()
            .is_some_and(|p| p.id == kept.id)
        {
            self.continuity.prepared = None;
        }
        self.cancel_preparation(kept.id, cx);
    }

    pub(super) fn claim_preparation(&self, id: u64) -> Option<KeptPreparation> {
        self.continuity
            .prepared
            .as_ref()
            .filter(|prepared| prepared.id == id)
            .cloned()
            .or_else(|| {
                self.continuity
                    .kept
                    .values()
                    .find(|kept| kept.id == id)
                    .cloned()
            })
    }

    /// The window opened with a `PlayFile` that asked to keep its preparation.
    pub(super) fn keep_for_window(
        &mut self,
        window: &str,
        preparation: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = preparation else {
            return;
        };
        match self.claim_preparation(id) {
            Some(kept) => {
                self.continuity.kept.insert(window.to_owned(), kept);
            }
            // The file list never arrived; the worker's copy is useless to us.
            None => {
                self.cancel_preparation(id, cx);
            }
        }
    }

    /// Everything a window holds that is not a lease: its pack, its dock state,
    /// and a swap still in progress. Called when the window goes away.
    pub(super) fn drop_window_state(&mut self, session: &str, cx: &mut Context<Self>) {
        // Closing mid-swap: the window is under the new session; the old lease
        // is still held and must go too.
        if let Some(loading) = self.continuity.loading.remove(session) {
            let LoadingSwap {
                from, kept_before, ..
            } = loading;
            self.queue_embedded_release(from.clone(), cx);
            self.drop_run(&from);
            self.flow_closed(&from, cx);
            if let Some(before) = kept_before
                && self.continuity.kept.get(session).map(|k| k.id) != Some(before.id)
            {
                self.release_kept(before, cx);
            }
        }
        self.flow_closed(session, cx);
        if let Some(pending) = self.continuity.swaps.remove(session) {
            self.queue_embedded_release(pending.request.session.id, cx);
        }
        if let Some(kept) = self.continuity.kept.remove(session) {
            self.release_kept(kept, cx);
        }
        if self.continuity.neighbours.remove(session).is_some() {
            cx.emit(NeighbourEvent::Closed {
                session: session.to_owned(),
            });
        }
    }

    /// Account invalidation: the worker already dropped every preparation and
    /// released its leases. Forget ours.
    pub(super) fn invalidate_continuity(&mut self) {
        self.continuity.prepared = None;
        self.continuity.kept.clear();
        self.continuity.keep_requests.clear();
    }
}

// ── neighbour state ──

impl AppCoordinator {
    /// Publishes what the dock offers for `session`.
    pub(crate) fn set_neighbours(
        &mut self,
        session: &str,
        neighbours: Neighbours,
        cx: &mut Context<Self>,
    ) {
        if !self.embedded_sessions.contains(session) {
            return;
        }
        self.continuity
            .neighbours
            .insert(session.to_owned(), neighbours);
        player::set_neighbours(session, neighbours, cx);
    }

    /// A window asked for a neighbour. Only an enabled direction goes out; the
    /// window's own optimistic state is corrected otherwise.
    pub(crate) fn request_neighbour(
        &mut self,
        session: &str,
        _generation: u64,
        direction: NeighbourDirection,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .continuity
            .neighbours
            .get(session)
            .copied()
            .unwrap_or_default();
        if !self.embedded_sessions.contains(session)
            || current.get(direction) != NeighbourState::Enabled
            || self.swap_in_progress(session)
        {
            player::set_neighbours(session, current, cx);
            return;
        }
        let mut busy = current;
        busy.set(direction, NeighbourState::Busy);
        self.set_neighbours(session, busy, cx);
        cx.emit(NeighbourEvent::Requested {
            session: session.to_owned(),
            direction,
        });
        self.flow_requested(session, direction, cx);
    }

    /// Resolution failed or was cancelled: the control is usable again and the
    /// dock says so. The current episode keeps playing.
    pub(crate) fn neighbour_failed(
        &mut self,
        session: &str,
        direction: NeighbourDirection,
        cx: &mut Context<Self>,
    ) {
        self.reset_busy(session, cx);
        if self.embedded_sessions.contains(session) {
            player::neighbour_failed(session, Some(direction), cx);
        }
    }

    pub(super) fn reset_busy(&mut self, session: &str, cx: &mut Context<Self>) {
        // The resolver knows the real neighbours; without a window of its own
        // the dock's last state is all there is.
        if self.flow_settle(session, cx) {
            return;
        }
        let mut neighbours = self
            .continuity
            .neighbours
            .get(session)
            .copied()
            .unwrap_or_default();
        for direction in [NeighbourDirection::Next, NeighbourDirection::Previous] {
            if neighbours.get(direction) == NeighbourState::Busy {
                neighbours.set(direction, NeighbourState::Enabled);
            }
        }
        self.set_neighbours(session, neighbours, cx);
    }

    pub(super) fn swap_in_progress(&self, session: &str) -> bool {
        self.continuity.swaps.contains_key(session)
            || self.continuity.loading.contains_key(session)
            || self
                .continuity
                .routes
                .values()
                .any(|(window, _)| window == session)
    }

    /// A window opened (or a swap completed): tell whoever resolves neighbours.
    pub(super) fn announce_subject(
        &mut self,
        session: &str,
        target: Option<PlaybackTarget>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(NeighbourEvent::SubjectChanged {
            session: session.to_owned(),
            target: target.clone(),
        });
        self.flow_subject_changed(session, target, cx);
    }
}

// ── routing a launch into a window ──

impl AppCoordinator {
    /// Unlocks `file_id` of the window's kept pack and plays it into the same
    /// window: a `PlayKeptFile` whose result swaps the source instead of opening
    /// a window. Returns the command generation.
    pub(crate) fn play_kept_file(
        &mut self,
        window: &str,
        file_id: String,
        episode: EpisodeLaunch,
        cx: &mut Context<Self>,
    ) -> Result<u64, SwapError> {
        if !self.embedded_sessions.contains(window)
            || self.closed_embedded_sessions.contains_key(window)
        {
            return Err(SwapError::NoWindow);
        }
        if !matches!(episode.target, PlaybackTarget::Episode { .. }) {
            return Err(SwapError::NotAnEpisode);
        }
        if self.swap_in_progress(window) {
            return Err(SwapError::Busy);
        }
        let Some(kept) = self.continuity.kept.get(window) else {
            return Err(SwapError::NoWindow);
        };
        if !kept.files.iter().any(|file| file.id == file_id) {
            return Err(SwapError::UnknownLease);
        }
        let preparation = kept.id;
        let generation = self
            .command(
                AppAction::PlayKeptFile {
                    preparation,
                    file_id,
                },
                cx,
            )
            .ok_or(SwapError::Unavailable)?;
        self.session_identity.insert(
            generation,
            (
                episode.title.clone(),
                episode.target.clone(),
                episode.episode_title.clone(),
            ),
        );
        self.playback.intents.insert(generation, episode.intent);
        self.continuity
            .keep_requests
            .insert(generation, preparation);
        self.continuity
            .routes
            .insert(generation, (window.to_owned(), episode));
        Ok(generation)
    }

    /// Makes the launch `generation` (a `PlayFile` that kept its preparation, from
    /// a stream the user picked for the next episode) swap into `window` when it
    /// starts, instead of opening a new window.
    pub(crate) fn route_launch_to_window(
        &mut self,
        generation: u64,
        window: &str,
        episode: EpisodeLaunch,
    ) {
        self.continuity
            .routes
            .insert(generation, (window.to_owned(), episode));
    }

    /// A routed launch ended without a lease: the window plays on.
    pub(super) fn route_failed(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.continuity.keep_requests.remove(&generation);
        if let Some((window, episode)) = self.continuity.routes.remove(&generation) {
            self.reset_busy(&window, cx);
            if self.embedded_sessions.contains(&window) {
                player::neighbour_failed(&window, episode.direction, cx);
            }
        }
    }

    /// Whether the launch `generation` is routed into a window.
    pub(super) fn is_routed(&self, generation: u64) -> bool {
        self.continuity.routes.contains_key(&generation)
    }

    /// A routed launch started a lease: hand it to the swap, which owns it from
    /// here (and releases it on any failure).
    pub(super) fn route_started(
        &mut self,
        generation: u64,
        session: &PlaybackSession,
        keep: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some((window, episode)) = self.continuity.routes.remove(&generation) else {
            return;
        };
        let _ = self.swap_embedded_source(
            SwapRequest {
                window,
                session: session.clone(),
                episode,
                preparation: keep,
            },
            cx,
        );
    }

    /// The preparation a started launch asked to keep, if any.
    pub(super) fn take_keep_request(&mut self, generation: u64) -> Option<u64> {
        self.continuity.keep_requests.remove(&generation)
    }
}

// ── the swap ──

impl AppCoordinator {
    /// Swaps the source of `request.window` to an already-launched lease. See the
    /// module docs for the six steps. Any refusal releases the new lease, so the
    /// caller never has to.
    pub(crate) fn swap_embedded_source(
        &mut self,
        request: SwapRequest,
        cx: &mut Context<Self>,
    ) -> Result<(), SwapError> {
        if let Err(error) = self.check_swap(&request) {
            self.fail_swap(request, cx);
            return Err(error);
        }
        let window = request.window.clone();
        if self
            .read_progress_for_swap(&window, &request.episode.target)
            .is_err()
        {
            self.fail_swap(request, cx);
            return Err(SwapError::Unavailable);
        }
        self.continuity.swaps.insert(
            window,
            PendingSwap {
                request,
                snapshot: None,
                stage: Stage::ReadingProgress,
            },
        );
        Ok(())
    }

    fn check_swap(&self, request: &SwapRequest) -> Result<(), SwapError> {
        let window = &request.window;
        if !self.embedded_sessions.contains(window)
            || self.closed_embedded_sessions.contains_key(window)
        {
            return Err(SwapError::NoWindow);
        }
        if self.swap_in_progress(window) {
            return Err(SwapError::Busy);
        }
        if !matches!(request.episode.target, PlaybackTarget::Episode { .. }) {
            return Err(SwapError::NotAnEpisode);
        }
        let to = &request.session.id;
        if to == window
            || self.closed_embedded_sessions.contains_key(to)
            || !self.sessions.iter().any(|s| &s.session.id == to)
        {
            return Err(SwapError::UnknownLease);
        }
        Ok(())
    }

    /// The new episode's saved progress arrived (or failed to).
    pub(super) fn swap_progress_read(
        &mut self,
        window: &str,
        result: Result<Option<PlaybackProgressSnapshot>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut pending) = self.continuity.swaps.remove(window) else {
            return;
        };
        match result {
            Ok(Some(snapshot)) => {
                pending.snapshot = Some(snapshot);
                pending.stage = Stage::Flushing {
                    deadline: Instant::now() + FLUSH_DEADLINE,
                    submitted: false,
                };
                self.continuity.swaps.insert(window.to_owned(), pending);
                self.advance_swaps(cx);
            }
            Ok(None) | Err(_) => self.fail_swap(pending.request, cx),
        }
    }

    /// Moves every swap waiting on the old run's save forward. Cheap when none.
    pub(super) fn advance_swaps(&mut self, cx: &mut Context<Self>) {
        if self.continuity.swaps.is_empty() {
            return;
        }
        let windows: Vec<_> = self.continuity.swaps.keys().cloned().collect();
        for window in windows {
            self.advance_swap(&window, cx);
        }
    }

    fn advance_swap(&mut self, window: &str, cx: &mut Context<Self>) {
        let Some(pending) = self.continuity.swaps.get(window) else {
            return;
        };
        let to = pending.request.session.id.clone();
        if !matches!(pending.stage, Stage::Flushing { .. }) {
            return;
        }
        // The window closed, or an account change released a lease, meanwhile.
        if !self.embedded_sessions.contains(window)
            || self.closed_embedded_sessions.contains_key(window)
            || !self.sessions.iter().any(|s| s.session.id == to)
        {
            if let Some(pending) = self.continuity.swaps.remove(window) {
                self.queue_embedded_release(pending.request.session.id, cx);
            }
            return;
        }
        // Step 1: hand over the old run's last observation, once.
        let submit = match self.continuity.swaps.get_mut(window) {
            Some(PendingSwap {
                stage: Stage::Flushing { submitted, .. },
                ..
            }) if !*submitted => {
                *submitted = true;
                true
            }
            _ => false,
        };
        if submit {
            self.log_swap(SwapStep::FlushOld);
            if let Some(sample) = player::final_checkpoint(window, cx) {
                self.submit_player_checkpoint(window.to_owned(), sample, cx);
            }
        }
        if self.run_drained(window) {
            self.perform_swap(window, cx);
            return;
        }
        let expired = matches!(
            self.continuity.swaps.get(window),
            Some(PendingSwap { stage: Stage::Flushing { deadline, .. }, .. })
                if Instant::now() >= *deadline
        );
        if expired && let Some(pending) = self.continuity.swaps.remove(window) {
            tracing::warn!(
                category = "swap_flush_timeout",
                "Progress could not be saved in time; staying on the current episode"
            );
            self.fail_swap(pending.request, cx);
        }
    }

    /// Steps 2 to 5. The old run is saved and nothing is in flight.
    fn perform_swap(&mut self, window: &str, cx: &mut Context<Self>) {
        let Some(PendingSwap {
            request,
            snapshot: Some(snapshot),
            ..
        }) = self.continuity.swaps.remove(window)
        else {
            return;
        };
        let to = request.session.id.clone();
        let PlaybackTarget::Episode {
            season, episode, ..
        } = request.episode.target
        else {
            return self.fail_swap(request, cx);
        };
        let title: PlaybackTitle = playback_title(
            &request.episode.title,
            Some((season, episode, request.episode.episode_title.as_deref())),
        );

        // Step 2: both leases are held; the new one belongs to the window.
        self.log_swap(SwapStep::NewLease);
        self.embedded_sessions.insert(to.clone());
        self.player_titles.insert(to.clone(), title.clone());
        if let Some(row) = self.sessions.iter_mut().find(|s| s.session.id == to) {
            row.title = Some(request.episode.title.clone());
            row.target = Some(request.episode.target.clone());
        }

        // Step 3: the window moves to the new session; the old one is tombstoned
        // so a late event from it cannot reopen or update a window.
        self.log_swap(SwapStep::Rekey);
        if player::rekey_window(window, &to, cx).is_err() {
            self.embedded_sessions.remove(&to);
            return self.fail_swap(request, cx);
        }
        self.embedded_sessions.remove(window);
        self.closed_embedded_sessions
            .insert(window.to_owned(), false);
        let kept_before = self.continuity.kept.remove(window);
        match request.preparation {
            Some(id) if kept_before.as_ref().is_some_and(|k| k.id == id) => {
                if let Some(kept) = kept_before.clone() {
                    self.continuity.kept.insert(to.clone(), kept);
                }
            }
            Some(id) => {
                if let Some(kept) = self.claim_preparation(id) {
                    self.continuity.kept.insert(to.clone(), kept);
                }
            }
            None => {}
        }
        let neighbours_before = self
            .continuity
            .neighbours
            .remove(window)
            .unwrap_or_default();

        // Step 4: a run for the new subject; the old run takes no more samples.
        self.log_swap(SwapStep::NewRun);
        self.seal_run(window);
        self.install_run(&to, snapshot);
        let subject = SwapSubject {
            title,
            subtitle_target: Some(request.episode.target.clone()),
            progress: Some(super::playback::launch_for(
                &snapshot,
                request.episode.intent,
            )),
        };
        let loading = LoadingSwap {
            from: window.to_owned(),
            direction: request.episode.direction,
            kept_before,
            neighbours_before,
        };
        if player::set_subject(&to, subject, cx).is_err() {
            return self.restore_window(loading, &to, cx);
        }

        // Step 5: load. Readiness (or failure) comes back from the window.
        self.log_swap(SwapStep::Load);
        if player::load_source(&to, &request.session.url, true, cx).is_err() {
            return self.restore_window(loading, &to, cx);
        }
        self.continuity.loading.insert(to.clone(), loading);
        self.announce_subject(&to, Some(request.episode.target), cx);
        cx.notify();
    }

    /// The window's report on the incoming media (`ready` or failed).
    pub(crate) fn swap_window_report(
        &mut self,
        session: &str,
        _generation: u64,
        ready: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(loading) = self.continuity.loading.remove(session) else {
            return;
        };
        if !ready {
            return self.restore_window(loading, session, cx);
        }
        // Step 6: the new episode is ready; the old lease is released, once.
        self.log_swap(SwapStep::ReleaseOld);
        let LoadingSwap {
            from, kept_before, ..
        } = loading;
        self.queue_embedded_release(from.clone(), cx);
        self.drop_run(&from);
        if let Some(before) = kept_before
            && self.continuity.kept.get(session).map(|k| k.id) != Some(before.id)
        {
            // The swap left the pack.
            self.release_kept(before, cx);
        }
        self.flow_swap_ready(&from, session, cx);
        cx.notify();
    }

    /// Undoes steps 3 to 5 after the new media could not be started: release the
    /// **new** lease and put the old episode back, resuming from its saved
    /// position. If the window cannot be restored it is closed and both leases go.
    fn restore_window(&mut self, loading: LoadingSwap, to: &str, cx: &mut Context<Self>) {
        let from = loading.from.clone();
        self.embedded_sessions.remove(to);
        self.drop_run(to);
        self.player_titles.remove(to);
        if let Some(kept) = self.continuity.kept.remove(to)
            && loading.kept_before.as_ref().map(|k| k.id) != Some(kept.id)
        {
            self.release_kept(kept, cx);
        }
        if let Some(before) = loading.kept_before.clone() {
            self.continuity.kept.insert(from.clone(), before);
        }
        // The new lease is released exactly once, whatever happens next.
        self.queue_embedded_release(to.to_owned(), cx);

        let restored = self.put_old_episode_back(&from, to, cx);
        if restored {
            self.closed_embedded_sessions.remove(&from);
            self.embedded_sessions.insert(from.clone());
            self.unseal_run(&from);
            self.continuity
                .neighbours
                .insert(from.clone(), loading.neighbours_before);
            player::set_neighbours(&from, loading.neighbours_before, cx);
            // The request that led here is over.
            self.reset_busy(&from, cx);
            player::neighbour_failed(&from, loading.direction, cx);
        } else {
            // Nothing to restore into: close the window and give everything up.
            player::close_session(to, cx);
            self.queue_embedded_release(from.clone(), cx);
            self.drop_run(&from);
            if let Some(kept) = self.continuity.kept.remove(&from) {
                self.release_kept(kept, cx);
            }
            self.set_notice(
                super::NoticeTopic::Playback,
                "The next episode could not be started, and the player could not go back.",
            );
        }
        cx.notify();
    }

    fn put_old_episode_back(&mut self, from: &str, to: &str, cx: &mut Context<Self>) -> bool {
        let Some(row) = self.sessions.iter().find(|s| s.session.id == from) else {
            return false;
        };
        let url = row.session.url.clone();
        let target = row.target.clone();
        let Some(title) = self.player_titles.get(from).cloned() else {
            return false;
        };
        let progress = self
            .sealed_snapshot(from)
            .map(|snapshot| super::playback::launch_for(&snapshot, PlaybackIntent::Resume));
        if player::rekey_window(to, from, cx).is_err() {
            return false;
        }
        let subject = SwapSubject {
            title,
            subtitle_target: target,
            progress,
        };
        player::set_subject(from, subject, cx).is_ok()
            && player::load_source(from, &url, false, cx).is_ok()
    }

    /// A swap that never reached the window: release the new lease, keep the
    /// old episode, say so in the dock.
    fn fail_swap(&mut self, request: SwapRequest, cx: &mut Context<Self>) {
        self.queue_embedded_release(request.session.id.clone(), cx);
        self.reset_busy(&request.window, cx);
        if self.embedded_sessions.contains(&request.window) {
            player::neighbour_failed(&request.window, request.episode.direction, cx);
        }
        if let Some(id) = request.preparation
            && !self.preparation_is_kept(id)
        {
            self.cancel_preparation(id, cx);
        }
    }

    /// Account change: the worker released every lease; forget swaps in flight.
    pub(super) fn invalidate_swaps(&mut self, released: &[String]) {
        self.continuity.swaps.retain(|window, pending| {
            !released.contains(window) && !released.contains(&pending.request.session.id)
        });
        self.continuity
            .loading
            .retain(|to, loading| !released.contains(to) && !released.contains(&loading.from));
    }

    /// For a result that is not a start: drop a route that would never resolve.
    pub(super) fn route_outcome_failed(
        &mut self,
        generation: u64,
        outcome: &AppOutcome,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            outcome,
            AppOutcome::Failed { .. }
                | AppOutcome::CacheFull { .. }
                | AppOutcome::CacheLimitNeeded { .. }
                | AppOutcome::Busy { .. }
        ) {
            self.route_failed(generation, cx);
        }
    }
}

/// Offline seams so window-level tests can drive the real swap without a
/// catalog worker or a media service.
#[cfg(all(test, feature = "ui-probe-test"))]
impl AppCoordinator {
    /// Books an open window's coordinator side: lease row, embedded flag, run and title.
    pub(crate) fn fixture_window(
        &mut self,
        session: &str,
        url: &str,
        title: &str,
        target: PlaybackTarget,
        snapshot: PlaybackProgressSnapshot,
    ) {
        self.fixture_lease(session, url);
        self.embedded_sessions.insert(session.to_owned());
        self.install_run(session, snapshot);
        if let Some(row) = self.sessions.iter_mut().find(|s| s.session.id == session) {
            row.title = Some(title.to_owned());
            row.target = Some(target.clone());
        }
        let episode = match target {
            PlaybackTarget::Episode {
                season, episode, ..
            } => Some((season, episode, None)),
            PlaybackTarget::Movie(_) => None,
        };
        self.player_titles
            .insert(session.to_owned(), playback_title(title, episode));
        self.tracker.session_started(session.to_owned(), true);
    }

    /// A launched lease that no window owns yet.
    pub(crate) fn fixture_lease(&mut self, session: &str, url: &str) {
        self.sessions.push(super::SessionSnapshot {
            session: PlaybackSession {
                id: session.into(),
                url: url.into(),
            },
            file_name: format!("{session}.mkv"),
            origin: crate::media::SessionOrigin::Remote,
            title: None,
            target: None,
            progress: None,
            error: None,
            can_reveal: false,
        });
        self.tracker.session_started(session.to_owned(), true);
    }

    /// `swap_embedded_source` with the catalog read already answered.
    pub(crate) fn fixture_swap(
        &mut self,
        request: SwapRequest,
        snapshot: PlaybackProgressSnapshot,
        cx: &mut Context<Self>,
    ) -> Result<(), SwapError> {
        if let Err(error) = self.check_swap(&request) {
            self.fail_swap(request, cx);
            return Err(error);
        }
        let window = request.window.clone();
        self.continuity.swaps.insert(
            window.clone(),
            PendingSwap {
                request,
                snapshot: None,
                stage: Stage::ReadingProgress,
            },
        );
        self.swap_progress_read(&window, Ok(Some(snapshot)), cx);
        Ok(())
    }

    /// The old run's save finished: nothing in flight, nothing waiting.
    pub(crate) fn fixture_acknowledge_save(&mut self, session: &str, cx: &mut Context<Self>) {
        self.fixture_save_in_flight(session, false);
        self.fixture_clear_latest(session);
        self.advance_swaps(cx);
    }

    /// Makes the flush deadline of a waiting swap pass.
    pub(crate) fn fixture_expire_flush(&mut self, window: &str, cx: &mut Context<Self>) {
        if let Some(PendingSwap {
            stage: Stage::Flushing { deadline, .. },
            ..
        }) = self.continuity.swaps.get_mut(window)
        {
            *deadline = Instant::now() - Duration::from_secs(1);
        }
        self.advance_swaps(cx);
    }

    /// Registers a kept pack whose cancellation can be observed.
    pub(crate) fn fixture_preparation_cancelled(&self, id: u64) -> bool {
        !self.session_identity.contains_key(&id)
    }

    pub(crate) fn fixture_swap_log(&self) -> Vec<SwapStep> {
        self.continuity.log.clone()
    }

    pub(crate) fn fixture_keep(&mut self, session: &str, id: u64) {
        // Cancelling a preparation forgets its identity: that is what tests observe.
        self.session_identity.insert(
            id,
            (
                "pack".into(),
                PlaybackTarget::Movie(
                    kinema_core::catalog::ImdbId::new("tt1234567")
                        .unwrap_or_else(|e| panic!("fixture id: {e}")),
                ),
                None,
            ),
        );
        self.continuity.kept.insert(
            session.to_owned(),
            KeptPreparation {
                id,
                provider: DebridKind::RealDebrid,
                source: TorrentSource {
                    hash: kinema_core::sources::InfoHash::new(
                        "0123456789012345678901234567890123456789",
                    )
                    .unwrap_or_else(|e| panic!("fixture hash: {e}")),
                    file_index: None,
                    filename: None,
                },
                files: Arc::from(vec![RemoteFile {
                    id: "f1".into(),
                    path: "Show.S01E01.mkv".into(),
                    size: 1,
                }]),
            },
        );
    }

    /// A window's kept pack, with the files and hash a test chooses.
    pub(crate) fn fixture_kept_pack(&mut self, session: &str, kept: KeptPreparation) {
        self.continuity.kept.insert(session.to_owned(), kept);
    }

    pub(crate) fn fixture_probe(&self, session: &str) -> SessionProbe {
        SessionProbe {
            embedded: self.embedded_sessions.contains(session),
            tombstoned: self.closed_embedded_sessions.contains_key(session),
            kept: self.continuity.kept.get(session).map(|k| k.id),
            run_sealed: self.run_sealed(session),
            release_pending: self.pending_embedded_releases.contains(session),
            swapping: self.swap_in_progress(session),
            neighbours: self.continuity.neighbours.get(session).copied(),
        }
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[derive(Debug, PartialEq)]
pub(crate) struct SessionProbe {
    pub embedded: bool,
    pub tombstoned: bool,
    pub kept: Option<u64>,
    pub run_sealed: Option<bool>,
    pub release_pending: bool,
    pub swapping: bool,
    pub neighbours: Option<Neighbours>,
}
