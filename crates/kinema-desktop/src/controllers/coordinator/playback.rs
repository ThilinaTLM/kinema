//! Application-owned playback intent and durable progress coordination.
use super::{AppCoordinator, EpisodeRef, TitleRef};
use crate::ui::views::player_window::{self, PlayerCheckpoint};
use crate::{
    catalog::{
        library::{LibraryOperation, LibraryResult},
        playback_progress::{
            PlaybackProgressOperation, PlaybackProgressResult, PlaybackProgressSnapshot,
        },
    },
    catalog_worker::local::{LocalEvent, LocalRequest, LocalSubmit},
    player::PlayerChoice,
};
use gpui::*;
use kinema_core::{
    MediaKind, catalog_normalized::ExternalIdentifier, media::PlaybackTitle,
    sources::PlaybackTarget,
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackIntent {
    #[default]
    Normal,
    Resume,
    StartOver,
}

struct PendingOpen {
    session: String,
    mode: PlayerChoice,
    url: String,
    title: PlaybackTitle,
    intent: PlaybackIntent,
}

enum PendingProgress {
    Open(PendingOpen),
    Begin {
        session: String,
        generation: u64,
        epoch: i64,
    },
    Save {
        session: String,
        sample: PlayerCheckpoint,
    },
}

struct Run {
    snapshot: PlaybackProgressSnapshot,
    generation: u64,
    sequence: u64,
    in_flight: bool,
    latest: Option<PlayerCheckpoint>,
    retries: u8,
    retry_at: Option<Instant>,
    ready: Option<(u64, i64, bool)>,
}

#[derive(Default)]
pub(super) struct PlaybackStore {
    pub(super) intents: HashMap<u64, PlaybackIntent>,
    next: u64,
    pending: HashMap<u64, PendingProgress>,
    runs: HashMap<String, Run>,
}

impl AppCoordinator {
    fn submit_progress(
        &mut self,
        operation: LibraryOperation,
        pending: PendingProgress,
    ) -> Result<(), String> {
        // A disjoint namespace from personal UI mutations; all requests still use
        // the single durable catalog FIFO and its transactional revision ordering.
        self.playback.next = self
            .playback
            .next
            .checked_add(1)
            .filter(|n| *n < (1 << 63))
            .ok_or("Playback request limit reached.")?;
        let request_id = (1 << 63) | self.playback.next;
        let worker = self.worker.as_ref().map_err(Clone::clone)?;
        match worker.submit_local(LocalRequest {
            request_id,
            generation: 0,
            operation,
        }) {
            LocalSubmit::Queued => {
                self.playback.pending.insert(request_id, pending);
                Ok(())
            }
            LocalSubmit::Full => Err("Progress queue is busy. Retry playback.".into()),
            LocalSubmit::Closed => Err("Progress storage is unavailable. Retry playback.".into()),
        }
    }

    pub(super) fn open_with_progress(
        &mut self,
        session: String,
        mode: PlayerChoice,
        url: String,
        title: PlaybackTitle,
        intent: PlaybackIntent,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .sessions
            .iter()
            .find(|s| s.session.id == session)
            .and_then(|s| s.target.clone());
        let pending = PendingOpen {
            session,
            mode,
            url,
            title,
            intent,
        };
        let Some(target) = target else {
            self.finish_progress_open(pending, None, cx);
            return;
        };
        let reference = TitleRef::External {
            kind: match target {
                PlaybackTarget::Movie(_) => MediaKind::Movie,
                PlaybackTarget::Episode { .. } => MediaKind::Series,
            },
            identifier: ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: target.imdb_id().as_str().into(),
            },
        };
        let operation = match target {
            PlaybackTarget::Movie(_) => LibraryOperation::ReadTitles(vec![reference]),
            PlaybackTarget::Episode {
                season, episode, ..
            } => LibraryOperation::ReadEpisodes(vec![EpisodeRef::Coordinates {
                title: reference,
                season,
                episode,
            }]),
        };
        if let Err(error) = self.submit_progress(operation, PendingProgress::Open(pending)) {
            self.notice = Some(error);
        }
    }

    fn finish_progress_open(
        &mut self,
        pending: PendingOpen,
        snapshot: Option<PlaybackProgressSnapshot>,
        cx: &mut Context<Self>,
    ) {
        if self.closed_embedded_sessions.contains_key(&pending.session)
            || !self
                .sessions
                .iter()
                .any(|s| s.session.id == pending.session)
        {
            return;
        }
        if let Some(snapshot) = snapshot {
            self.playback
                .runs
                .entry(pending.session.clone())
                .or_insert(Run {
                    snapshot,
                    generation: 0,
                    sequence: 0,
                    in_flight: false,
                    latest: None,
                    retries: 0,
                    retry_at: None,
                    ready: None,
                });
        }
        let launch = snapshot.map(|s| crate::ui::views::player_window::PlaybackLaunch {
            subject: s.target,
            epoch: s.epoch,
            run: 0,
            checkpoint: s.checkpoint.map(|c| c.observation),
            watched: s.watch.is_some_and(|w| w.watched),
            intent: pending.intent,
        });
        if crate::ui::views::player_window::open(
            pending.session.clone(),
            pending.mode,
            pending.url,
            pending.title,
            launch,
            cx.entity(),
            cx,
        )
        .is_err()
        {
            self.notice = Some("Embedded player could not be opened.".into());
            self.queue_embedded_release(pending.session, cx);
        }
    }

    pub(super) fn apply_playback_event(
        &mut self,
        event: &LocalEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.playback.pending.remove(&event.request.request_id) else {
            return false;
        };
        match pending {
            PendingProgress::Begin {
                session,
                generation,
                epoch,
            } => match &event.result {
                Ok(LibraryResult::PlaybackProgress(PlaybackProgressResult::Applied(snapshot))) => {
                    if let Some(run) = self.playback.runs.get_mut(&session) {
                        if run.generation != generation {
                            return true;
                        }
                        let presence_changed = resumable(&run.snapshot) != resumable(snapshot);
                        run.snapshot = *snapshot;
                        run.in_flight = false;
                        run.sequence = 0;
                        run.latest = None;
                        run.retry_at = None;
                        run.retries = 0;
                        player_window::authorize_progress(
                            &session,
                            generation,
                            epoch,
                            snapshot.epoch,
                            snapshot.active_run,
                            cx,
                        );
                        self.revoke_other_writers(&session, *snapshot, cx);
                        self.apply_playback_snapshot(*snapshot);
                        if presence_changed {
                            self.refresh_personal_after_catalog(cx);
                        }
                    }
                }
                _ => {
                    if let Some(run) = self.playback.runs.get_mut(&session) {
                        run.in_flight = false;
                    }
                    self.notice = Some("Progress could not be initialized. Retry in the player or play without saving.".into());
                    player_window::progress_notice(&session, generation, true, cx);
                }
            },
            PendingProgress::Save { session, sample } => {
                let Some(run) = self.playback.runs.get_mut(&session) else {
                    return true;
                };
                if run.generation != sample.generation {
                    return true;
                }
                run.in_flight = false;
                match &event.result {
                    Ok(LibraryResult::PlaybackProgress(result)) => {
                        let (PlaybackProgressResult::Applied(snapshot)
                        | PlaybackProgressResult::Stale(snapshot)) = result;
                        let changed_watch = snapshot.watch != run.snapshot.watch
                            || resumable(snapshot) != resumable(&run.snapshot);
                        let stale = matches!(result, PlaybackProgressResult::Stale(_));
                        run.snapshot = *snapshot;
                        run.retries = 0;
                        run.retry_at = None;
                        if stale {
                            run.latest = None;
                            player_window::invalidate_progress(
                                &session,
                                sample.generation,
                                sample.epoch,
                                cx,
                            );
                            if snapshot.active_run == sample.run && run.ready.is_none() {
                                // A manual toggle/reset retains run ownership but advances
                                // its epoch. Discard all pre-reset samples, including retries.
                                player_window::authorize_progress(
                                    &session,
                                    sample.generation,
                                    sample.epoch,
                                    snapshot.epoch,
                                    snapshot.active_run,
                                    cx,
                                );
                            }
                        } else {
                            player_window::progress_notice(&session, sample.generation, false, cx);
                        }
                        self.apply_playback_snapshot(*snapshot);
                        if changed_watch {
                            self.refresh_personal_after_catalog(cx);
                        }
                        if !self.send_pending_ready(&session, cx) {
                            self.send_latest_checkpoint(&session, cx);
                        }
                    }
                    _ => {
                        if run.latest.is_none() {
                            run.latest = Some(sample.clone());
                        }
                        run.retries = run.retries.saturating_add(1);
                        run.retry_at = (run.retries <= 3)
                            .then(|| Instant::now() + Duration::from_secs(1 << (run.retries - 1)));
                        self.notice =
                            Some("Progress could not be saved. Playback can continue.".into());
                        player_window::progress_notice(&session, sample.generation, true, cx);
                        self.send_pending_ready(&session, cx);
                    }
                }
            }
            PendingProgress::Open(pending) => {
                let result = match &event.result {
                    Ok(LibraryResult::Titles(states)) => states
                        .first()
                        .ok_or_else(|| "Movie progress was not returned.".to_owned())
                        .and_then(|s| {
                            s.as_ref().map(|s| s.playback_progress).map_err(|_| {
                                "Movie identity could not be resolved for progress.".to_owned()
                            })
                        }),
                    Ok(LibraryResult::Episodes(states)) => states
                        .first()
                        .ok_or_else(|| "Episode progress was not returned.".to_owned())
                        .and_then(|s| {
                            s.as_ref().map(|s| Some(s.playback_progress)).map_err(|_| {
                                "Episode identity could not be resolved for progress.".to_owned()
                            })
                        }),
                    Err(_) => Err(
                        "Saved progress could not be read. Retry playback from Transfers.".into(),
                    ),
                    _ => Err("Saved progress returned an unexpected response.".into()),
                };
                match result {
                    Ok(snapshot) => self.finish_progress_open(pending, snapshot, cx),
                    Err(error) => {
                        self.notice = Some(error.clone());
                        self.session_error(&pending.session, &error);
                    }
                }
            }
        }
        true
    }
}

impl AppCoordinator {
    pub(crate) fn player_progress_ready(
        &mut self,
        session: String,
        generation: u64,
        epoch: i64,
        start_over: bool,
        cx: &mut Context<Self>,
    ) {
        if self.closed_embedded_sessions.contains_key(&session) {
            return;
        }
        let Some(run) = self.playback.runs.get_mut(&session) else {
            return;
        };
        // Ready is an explicit new run (including replay/start-over). A
        // manual mutation may have advanced the durable epoch while it waited
        // behind the previous save; preserve the player's old epoch for its
        // authorization callback, but begin against current transactional state.
        if epoch > run.snapshot.epoch || generation < run.generation {
            return;
        }
        if run.in_flight {
            run.ready = Some((generation, epoch, start_over));
            return;
        }
        run.generation = generation;
        run.in_flight = true;
        let target = run.snapshot.target;
        if let Err(error) = self.submit_progress(
            LibraryOperation::PlaybackProgress(PlaybackProgressOperation::BeginRun {
                target,
                start_over,
            }),
            PendingProgress::Begin {
                session: session.clone(),
                generation,
                epoch,
            },
        ) {
            if let Some(run) = self.playback.runs.get_mut(&session) {
                run.in_flight = false;
            }
            self.notice = Some(error);
            cx.defer(move |cx| player_window::progress_notice(&session, generation, true, cx));
        }
    }
    fn revoke_other_writers(
        &mut self,
        owner: &str,
        snapshot: PlaybackProgressSnapshot,
        cx: &mut Context<Self>,
    ) {
        let mut duplicate = false;
        for (session, run) in &mut self.playback.runs {
            if session != owner
                && run.snapshot.target == snapshot.target
                && run.snapshot.active_run != 0
                && run.snapshot.active_run != snapshot.active_run
            {
                run.latest = None;
                run.retry_at = None;
                player_window::invalidate_progress(session, run.generation, run.snapshot.epoch, cx);
                duplicate = true;
            }
        }
        if duplicate {
            self.notice = Some("The newest player now owns saved progress for this title. Older playback continues without updating its resume point.".into());
        }
    }

    fn send_pending_ready(&mut self, session: &str, cx: &mut Context<Self>) -> bool {
        let ready = self
            .playback
            .runs
            .get_mut(session)
            .and_then(|run| run.ready.take());
        if let Some((generation, epoch, start_over)) = ready {
            self.player_progress_ready(session.into(), generation, epoch, start_over, cx);
            true
        } else {
            false
        }
    }
    pub(crate) fn submit_player_checkpoint(
        &mut self,
        session: String,
        sample: PlayerCheckpoint,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.playback.runs.get_mut(&session) else {
            return;
        };
        if sample.generation != run.generation
            || sample.subject != run.snapshot.target
            || sample.run != run.snapshot.active_run
            || sample.epoch != run.snapshot.epoch
            || sample.sequence <= run.sequence
        {
            return;
        }
        run.sequence = sample.sequence;
        if sample.force {
            run.retry_at = None;
        }
        run.latest = Some(sample);
        if run.retries > 3 {
            run.retries = 0;
            run.retry_at = None;
        }
        self.send_latest_checkpoint(&session, cx);
    }
    fn send_latest_checkpoint(&mut self, session: &str, cx: &mut Context<Self>) {
        let Some(run) = self.playback.runs.get_mut(session) else {
            return;
        };
        if run.in_flight || run.retry_at.is_some_and(|at| at > Instant::now()) {
            return;
        }
        let Some(sample) = run.latest.take() else {
            return;
        };
        run.in_flight = true;
        let operation = PlaybackProgressOperation::Save {
            target: sample.subject,
            run: sample.run,
            epoch: sample.epoch,
            observation: sample.observation,
        };
        if let Err(error) = self.submit_progress(
            LibraryOperation::PlaybackProgress(operation),
            PendingProgress::Save {
                session: session.into(),
                sample: sample.clone(),
            },
        ) {
            if let Some(run) = self.playback.runs.get_mut(session) {
                run.in_flight = false;
                run.latest = Some(sample.clone());
                run.retries = run.retries.saturating_add(1);
                run.retry_at = (run.retries <= 3)
                    .then(|| Instant::now() + Duration::from_secs(1 << (run.retries - 1)));
            }
            self.notice = Some(error);
            let session = session.to_owned();
            cx.defer(move |cx| {
                player_window::progress_notice(&session, sample.generation, true, cx)
            });
        }
    }
    pub(super) fn retry_playback_progress(&mut self, cx: &mut Context<Self>) {
        let due = self
            .playback
            .runs
            .iter()
            .filter(|(_, run)| run.retry_at.is_some_and(|at| at <= Instant::now()))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for session in due {
            self.send_latest_checkpoint(&session, cx);
        }
        self.playback.runs.retain(|id, run| {
            self.sessions.iter().any(|s| &s.session.id == id)
                || run.in_flight
                || (run.latest.is_some() && run.retries <= 3)
        });
    }
    pub(crate) fn flush_playback_on_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.update(player_window::checkpoint_all);
            // Player callbacks are deferred so shutdown never reenters a borrowed
            // coordinator/player. Allow those captured final samples to arrive.
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
            let deadline = Instant::now() + Duration::from_millis(1800);
            loop {
                let done = this
                    .update(cx, |c, cx| {
                        c.drain_personal(cx);
                        c.retry_playback_progress(cx);
                        c.playback.pending.is_empty()
                            && c.playback
                                .runs
                                .values()
                                .all(|r| !r.in_flight && r.latest.is_none())
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
                if Instant::now() >= deadline {
                    tracing::warn!(
                        category = "progress_flush_incomplete",
                        "Some playback progress could not be saved before shutdown"
                    );
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
        })
    }
}

fn resumable(s: &PlaybackProgressSnapshot) -> bool {
    !s.watch.is_some_and(|w| w.watched)
        && s.checkpoint
            .is_some_and(|c| c.observation.position_ms() > 0)
}

#[cfg(test)]
#[path = "playback_tests.rs"]
mod tests;
