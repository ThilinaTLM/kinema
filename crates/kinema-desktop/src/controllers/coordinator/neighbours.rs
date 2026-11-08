//! Episode continuity, the glue: carries out the [`resolver`](super::resolver)'s
//! effects against the catalog worker, the player registry and the app worker,
//! and feeds their answers back.
//!
//! - **Neighbour state**: a canonical listing is read off the UI thread
//!   (`LibraryOperation::Detail`) when a window opens, swaps, or the catalog
//!   changes; the resolver turns it into the dock's states.
//! - **Resolution**: the indexer query for the neighbour (its own worker lane,
//!   one at a time), the stream memory reads, then
//!   `choose_neighbour_stream`. The outcome plays a kept file, prepares a strong
//!   match, opens the ranked picker, or says why nothing could be found.
//! - **Record on play**: every playback start with an episode target remembers
//!   its stream ([`remember_for`]).
//!
//! Local reads and writes use their own request-id namespace (bit 62) so their
//! answers never reach the personal-state or progress handlers. Logs carry
//! categories and counts only: never hashes, release names or file names.
use super::{
    AppCoordinator, CoordinatorEvent, EpisodeLaunch, PlaybackIntent,
    resolver::{
        Decision, Effect, Kept, Listing, MemoryRead, Origin, Outcome, PickerSpec, Played, Resolver,
        Token,
    },
};
use crate::{
    app::{AppAction, AppOutcome},
    catalog::{
        library::{LibraryOperation, LibraryResult, LocalDetail, TitleRef},
        stream_memory::{ReleaseMemory, Remember, StreamMemory, StreamMemoryOperation},
    },
    catalog_worker::{
        MetadataAction, MetadataEvent, MetadataRequest, MetadataSnapshot, Mode, SourceLane, Submit,
        local::{LocalEvent, LocalRequest, LocalSubmit},
    },
    ui::views::player::{self, NeighbourDirection},
};
use gpui::*;
use kinema_core::{
    MediaKind,
    catalog::ImdbId,
    catalog_normalized::ExternalIdentifier,
    episode_continuity::{IndexerFailed, ReferenceRelease, SeasonListing},
    sources::{FileSelection, IndexerKind, PlaybackTarget, SourceCandidate},
    watch_progress::PlaybackObservation,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

/// Request ids of this module: bit 62, clear of the personal state (small
/// counters) and playback progress (bit 63).
const READ_NAMESPACE: u64 = 1 << 62;
/// An indexer query that never answers (a worker that dropped it) is given up
/// after this long, well past the adapter's own 15 s timeout.
const INDEXER_PATIENCE: Duration = Duration::from_secs(45);

/// What a local request answers.
enum Read {
    Listing(Token),
    Memory(Token),
    Remember,
}

/// What a preparation was asked for, remembered until its file is chosen.
struct Seed {
    target: PlaybackTarget,
    candidate: SourceCandidate,
}

/// The provider file a start unlocked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProviderFile {
    /// Basename only.
    pub name: String,
    pub size: u64,
}

/// What a playback start will remember: the stream and the file it resolved to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PlayPlan {
    pub target: PlaybackTarget,
    pub candidate: SourceCandidate,
    pub file: Option<ProviderFile>,
}

/// A strong match being prepared for a window. If its file turns out not to be
/// unique the ranked picker opens instead.
struct Preparing {
    session: String,
    direction: NeighbourDirection,
    launch: EpisodeLaunch,
    label: String,
    picker: PickerSpec,
}

/// The ranked stream list the main window shows for an episode a player window
/// asked for. A stream chosen there plays into that window.
#[derive(Clone, Debug)]
pub(crate) struct StreamPicker {
    /// The window's session when the picker opened.
    pub window: String,
    pub direction: NeighbourDirection,
    pub target: PlaybackTarget,
    pub spec: PickerSpec,
    /// Series title and episode name, for the stream list and the swap.
    pub title: String,
    pub episode_title: Option<String>,
}

/// What Detail does after asking to play an episode with its memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectPlay {
    /// Prepare this stream. `picker` is where to go if its file is not unique.
    Play {
        candidate: SourceCandidate,
        label: Option<String>,
        picker: Option<PickerSpec>,
    },
    /// The ranked stream list.
    Picker(PickerSpec),
    /// Nothing remembered: today's stream list.
    Fallback,
    /// The indexer failed or listed nothing; the text says which.
    Unavailable(String),
}

struct Inflight {
    token: Token,
    target: PlaybackTarget,
    since: Instant,
}

/// One continuity indexer query at a time: the worker has one slot per lane and
/// a second request would replace the first.
#[derive(Default)]
struct IndexerQueue {
    queue: VecDeque<(Token, PlaybackTarget)>,
    inflight: Option<Inflight>,
    /// A saved listing waiting for the fresh one that follows it.
    held: HashMap<Token, Vec<SourceCandidate>>,
}

#[derive(Default)]
pub(super) struct FlowStore {
    resolver: Resolver,
    next_read: u64,
    reads: HashMap<u64, Read>,
    indexer: IndexerQueue,
    /// Preparations awaiting their file choice, by `Prepare` generation.
    seeds: HashMap<u64, Seed>,
    /// What each start (by command generation) will remember.
    plans: HashMap<u64, PlayPlan>,
    /// Strong matches being prepared for a window, by `Prepare` generation.
    preparing: HashMap<u64, Preparing>,
    /// Preparations started from the picker, to be played into its window.
    picker_preparations: HashSet<u64>,
    picker: Option<StreamPicker>,
    /// How the next episode was chosen, by the window it swaps out of.
    notes: HashMap<String, String>,
    ticket: u64,
    /// Everything sent to the workers, for tests (no worker is attached).
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(super) sent: Vec<Sent>,
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[derive(Clone, Debug)]
pub(super) enum Sent {
    Local(u64, LibraryOperation),
    Indexer(Token, PlaybackTarget),
}

// ── pure helpers ──

/// The canonical listing of a series, from the catalog's local detail.
pub(super) fn listing_of(detail: &LocalDetail) -> Listing {
    let mut seasons: BTreeMap<u32, SeasonListing> = BTreeMap::new();
    for season in &detail.seasons {
        let entry = seasons
            .entry(season.number)
            .or_insert_with(|| SeasonListing {
                number: season.number,
                ..SeasonListing::default()
            });
        entry.advertised = season
            .integer("episode_count")
            .and_then(|n| u32::try_from(n).ok());
    }
    let mut names = HashMap::new();
    for episode in &detail.episodes {
        let entry = seasons
            .entry(episode.season)
            .or_insert_with(|| SeasonListing {
                number: episode.season,
                ..SeasonListing::default()
            });
        entry.episodes.push(episode.metadata.number);
        if let Some(name) = episode.metadata.text("name") {
            names.insert((episode.season, episode.metadata.number), name.to_owned());
        }
    }
    Listing {
        seasons: seasons.into_values().collect(),
        names,
    }
}

/// Stored text must be non-empty, free of NUL and at most 1024 bytes.
fn clean(text: &str) -> Option<String> {
    const MAX: usize = 1024;
    if text.is_empty() || text.contains('\0') {
        return None;
    }
    let mut end = text.len().min(MAX);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(text[..end].to_owned())
}

fn basename(path: &str) -> Option<String> {
    path.rsplit(['/', '\\']).next().and_then(clean)
}

/// The record a start leaves behind: the stream with its resolved file, and the
/// season's release without the per-episode file hints. `None` when the plan has
/// no usable name or is not an episode. It holds only what the indexer and the
/// provider listing said: no URL, no debrid file id, no session id.
pub(super) fn remember_for(plan: &PlayPlan) -> Option<Remember> {
    let PlaybackTarget::Episode {
        series,
        season,
        episode,
    } = &plan.target
    else {
        return None;
    };
    let candidate = &plan.candidate;
    let release = ReleaseMemory {
        info_hash: candidate.torrent.hash.clone(),
        name: clean(&candidate.name)?,
        quality: candidate.quality.as_deref().and_then(clean),
        size: candidate.size.filter(|n| i64::try_from(*n).is_ok()),
        file_index: candidate.torrent.file_index,
        filename: candidate.torrent.filename.as_deref().and_then(clean),
    };
    let file = plan
        .file
        .as_ref()
        .filter(|file| i64::try_from(file.size).is_ok());
    Some(Remember {
        series: series_ref(series),
        season: *season,
        episode: *episode,
        stream: StreamMemory {
            release: release.clone(),
            file_name: file.and_then(|file| basename(&file.name)),
            file_size: file.map(|file| file.size),
        },
        season_release: ReleaseMemory {
            file_index: None,
            filename: None,
            ..release
        },
    })
}

fn series_ref(series: &ImdbId) -> TitleRef {
    TitleRef::External {
        kind: MediaKind::Series,
        identifier: ExternalIdentifier {
            namespace: "imdb".into(),
            external_id: series.as_str().into(),
        },
    }
}

/// What the window plays, as the next episode's reference.
fn played_of(plan: &PlayPlan) -> Played {
    Played {
        reference: ReferenceRelease {
            name: plan.candidate.name.clone(),
            file_name: plan
                .file
                .as_ref()
                .map(|file| file.name.clone())
                .or_else(|| plan.candidate.torrent.filename.clone()),
            size: plan.candidate.size,
        },
        hash: plan.candidate.torrent.hash.clone(),
    }
}

// ── effects ──

impl AppCoordinator {
    pub(super) fn apply_effects(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        let mut queue: VecDeque<Effect> = effects.into();
        while let Some(effect) = queue.pop_front() {
            match effect {
                Effect::Publish { session, dock } => self.set_neighbours(&session, dock, cx),
                Effect::ReadListing { token, series } => {
                    let operation = LibraryOperation::Detail(series_ref(&series));
                    if !self.submit_flow(operation, Read::Listing(token)) {
                        queue.extend(self.flow.resolver.listing_loaded(token, None));
                    }
                }
                Effect::ReadMemory { token, read } => {
                    let operation = LibraryOperation::StreamMemory(match read {
                        MemoryRead::Episode {
                            series,
                            season,
                            episode,
                        } => StreamMemoryOperation::Read {
                            series: series_ref(&series),
                            season,
                            episode,
                        },
                        MemoryRead::PreviousSeason { series, season } => {
                            StreamMemoryOperation::ReadPreviousSeason {
                                series: series_ref(&series),
                                season,
                            }
                        }
                    });
                    if !self.submit_flow(operation, Read::Memory(token)) {
                        queue.extend(self.flow.resolver.memory_result(token, None));
                    }
                }
                Effect::QueryIndexer {
                    token,
                    target,
                    interactive,
                } => {
                    if interactive {
                        self.flow.indexer.queue.push_front((token, target));
                    } else {
                        self.flow.indexer.queue.push_back((token, target));
                    }
                    queue.extend(self.pump_indexer());
                }
                Effect::CancelIndexer { token, target } => {
                    self.flow.indexer.held.remove(&token);
                    self.flow
                        .indexer
                        .queue
                        .retain(|(queued, _)| *queued != token);
                    if self
                        .flow
                        .indexer
                        .inflight
                        .as_ref()
                        .is_some_and(|inflight| inflight.token == token)
                    {
                        self.flow.indexer.inflight = None;
                        self.cancel_indexer(token, target);
                        queue.extend(self.pump_indexer());
                    }
                }
                Effect::Outcome(outcome) => self.act(outcome, cx),
            }
        }
    }

    /// Submits a local read or write in this module's id namespace.
    fn submit_flow(&mut self, operation: LibraryOperation, read: Read) -> bool {
        let Some(id) = self
            .flow
            .next_read
            .checked_add(1)
            .filter(|n| *n < READ_NAMESPACE)
        else {
            return false;
        };
        let request_id = READ_NAMESPACE | id;
        #[cfg(all(test, feature = "ui-probe-test"))]
        if self.worker.is_err() {
            self.flow.next_read = id;
            self.flow.reads.insert(request_id, read);
            self.flow.sent.push(Sent::Local(request_id, operation));
            return true;
        }
        let Ok(worker) = &self.worker else {
            return false;
        };
        match worker.submit_local(LocalRequest {
            request_id,
            generation: 0,
            operation,
        }) {
            LocalSubmit::Queued => {
                self.flow.next_read = id;
                self.flow.reads.insert(request_id, read);
                true
            }
            LocalSubmit::Full => {
                tracing::debug!(category = "queue_full", "Continuity read not queued");
                false
            }
            LocalSubmit::Closed => false,
        }
    }

    /// Starts the next queued indexer query if none is running. Queries that
    /// cannot even be submitted answer as a failure.
    fn pump_indexer(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        while self.flow.indexer.inflight.is_none() {
            let Some((token, target)) = self.flow.indexer.queue.pop_front() else {
                break;
            };
            if self.submit_indexer(token, &target) {
                self.flow.indexer.inflight = Some(Inflight {
                    token,
                    target,
                    since: Instant::now(),
                });
            } else {
                tracing::debug!(
                    category = "indexer_unsubmitted",
                    "Continuity query not sent"
                );
                effects.extend(self.flow.resolver.indexer_result(token, Err(IndexerFailed)));
            }
        }
        effects
    }

    fn indexer_action(&self, target: &PlaybackTarget) -> MetadataAction {
        MetadataAction::Sources {
            indexer: self
                .settings
                .as_ref()
                .map(|settings| settings.indexer)
                .unwrap_or(IndexerKind::Torrentio),
            target: target.clone(),
            lane: SourceLane::Continuity,
        }
    }

    fn submit_indexer(&mut self, token: Token, target: &PlaybackTarget) -> bool {
        let request = MetadataRequest {
            generation: token,
            action: self.indexer_action(target),
            mode: Mode::Load,
        };
        #[cfg(all(test, feature = "ui-probe-test"))]
        if self.worker.is_err() {
            self.flow.sent.push(Sent::Indexer(token, target.clone()));
            return true;
        }
        self.worker
            .as_ref()
            .is_ok_and(|worker| worker.submit_metadata(request) != Submit::Closed)
    }

    fn cancel_indexer(&self, token: Token, target: PlaybackTarget) {
        if let Ok(worker) = &self.worker {
            worker.cancel_metadata(&self.indexer_action(&target), token);
        }
    }

    /// The worker's answer to a continuity indexer query.
    pub(super) fn flow_indexer_event(&mut self, event: MetadataEvent, cx: &mut Context<Self>) {
        let token = event.generation;
        if self
            .flow
            .indexer
            .inflight
            .as_ref()
            .is_none_or(|inflight| inflight.token != token)
        {
            tracing::debug!(
                outcome = "rejected_stale",
                "Continuity indexer result is no longer awaited"
            );
            return;
        }
        let result = match event.result {
            Ok(MetadataSnapshot::Sources(found)) => {
                if event.saved && event.refreshing {
                    // A saved listing; the fresh one follows. If it fails, this
                    // one is better than nothing.
                    self.flow.indexer.held.insert(token, found.candidates);
                    return;
                }
                Ok(found.candidates)
            }
            Ok(_) | Err(_) => match self.flow.indexer.held.remove(&token) {
                Some(saved) => Ok(saved),
                None => {
                    tracing::debug!(category = "indexer_failed", "Continuity query failed");
                    Err(IndexerFailed)
                }
            },
        };
        self.flow.indexer.held.remove(&token);
        self.flow.indexer.inflight = None;
        let mut effects = self.flow.resolver.indexer_result(token, result);
        effects.extend(self.pump_indexer());
        self.apply_effects(effects, cx);
    }

    /// Poll tick: gives up on an indexer query that never answers.
    pub(super) fn flow_tick(&mut self, cx: &mut Context<Self>) {
        let Some(inflight) = &self.flow.indexer.inflight else {
            return;
        };
        if inflight.since.elapsed() < INDEXER_PATIENCE {
            return;
        }
        let (token, target) = (inflight.token, inflight.target.clone());
        tracing::debug!(category = "indexer_timeout", "Continuity query gave up");
        self.flow.indexer.inflight = None;
        self.flow.indexer.held.remove(&token);
        self.cancel_indexer(token, target);
        let mut effects = self.flow.resolver.indexer_result(token, Err(IndexerFailed));
        effects.extend(self.pump_indexer());
        self.apply_effects(effects, cx);
    }

    /// A local answer in this module's namespace. Returns whether it was ours.
    pub(super) fn apply_flow_event(&mut self, event: &LocalEvent, cx: &mut Context<Self>) -> bool {
        let Some(read) = self.flow.reads.remove(&event.request.request_id) else {
            return false;
        };
        let effects = match (read, &event.result) {
            (Read::Listing(token), Ok(LibraryResult::Detail(detail))) => self
                .flow
                .resolver
                .listing_loaded(token, Some(listing_of(detail))),
            (Read::Listing(token), _) => {
                tracing::debug!(category = "listing_unavailable", "Episode listing not read");
                self.flow.resolver.listing_loaded(token, None)
            }
            (Read::Memory(token), Ok(LibraryResult::StreamMemory(found))) => {
                self.flow.resolver.memory_result(token, Some(found.clone()))
            }
            (Read::Memory(token), _) => {
                tracing::debug!(category = "memory_unavailable", "Stream memory not read");
                self.flow.resolver.memory_result(token, None)
            }
            (Read::Remember, Ok(_)) => {
                tracing::debug!(category = "stream_remembered", "Stream memory written");
                Vec::new()
            }
            (Read::Remember, Err(error)) => {
                // A title the catalog has no episode row for is expected and
                // quiet: nothing to remember it against.
                let unmapped =
                    *error == crate::catalog::CatalogError::MissingIdentityMapping.to_string();
                tracing::debug!(
                    category = if unmapped {
                        "stream_memory_unmapped"
                    } else {
                        "stream_memory_failed"
                    },
                    "Stream memory not written"
                );
                Vec::new()
            }
        };
        self.apply_effects(effects, cx);
        true
    }
}

// ── windows ──

impl AppCoordinator {
    fn kept_snapshot(&self, session: &str) -> Option<Kept> {
        self.kept_preparation(session).map(|kept| Kept {
            provider: kept.provider,
            files: kept.files.clone(),
            hash: kept.source.hash.clone(),
        })
    }

    /// A window opened or swapped: publish its neighbours and read its listing.
    pub(super) fn flow_subject_changed(
        &mut self,
        session: &str,
        target: Option<PlaybackTarget>,
        cx: &mut Context<Self>,
    ) {
        // The picked episode is playing: the picker has done its job.
        if self
            .flow
            .picker
            .as_ref()
            .is_some_and(|picker| Some(&picker.target) == target.as_ref())
        {
            self.flow.picker = None;
        }
        let effects = self.flow.resolver.subject_changed(session, target.as_ref());
        self.apply_effects(effects, cx);
    }

    /// The user (or Up next) asked for a neighbour.
    pub(super) fn flow_requested(
        &mut self,
        session: &str,
        direction: NeighbourDirection,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .sessions
            .iter()
            .find(|row| row.session.id == session)
            .and_then(|row| row.target.clone());
        let Some(target) = target else {
            self.reset_busy(session, cx);
            return;
        };
        let kept = self.kept_snapshot(session);
        let effects = self
            .flow
            .resolver
            .request(session, &target, direction, kept);
        self.apply_effects(effects, cx);
    }

    /// Playback progress: prefetch the next episode once 80% is reached.
    pub(super) fn flow_progress(
        &mut self,
        session: &str,
        observation: PlaybackObservation,
        cx: &mut Context<Self>,
    ) {
        let kept = self.kept_snapshot(session);
        let effects = self.flow.resolver.progress(session, observation, kept);
        if !effects.is_empty() {
            self.apply_effects(effects, cx);
        }
    }

    /// The catalog may hold new or changed episode rows: read the listings of
    /// the series that open windows play again.
    pub(super) fn flow_catalog_changed(&mut self, cx: &mut Context<Self>) {
        let mut effects = Vec::new();
        for series in self.flow.resolver.series_in_use() {
            effects.extend(self.flow.resolver.listing_changed(&series));
        }
        if !effects.is_empty() {
            self.apply_effects(effects, cx);
        }
    }

    /// The window is gone: cancel everything held for it.
    pub(super) fn flow_closed(&mut self, session: &str, cx: &mut Context<Self>) {
        self.flow.notes.remove(session);
        if self
            .flow
            .picker
            .as_ref()
            .is_some_and(|picker| picker.window == session)
        {
            self.flow.picker = None;
        }
        let doomed: Vec<u64> = self
            .flow
            .preparing
            .iter()
            .filter(|(_, p)| p.session == session)
            .map(|(generation, _)| *generation)
            .collect();
        for generation in doomed {
            self.flow.preparing.remove(&generation);
            self.cancel_preparation(generation, cx);
        }
        let effects = self.flow.resolver.close(session);
        self.apply_effects(effects, cx);
    }

    /// Drops resolver state of sessions that are no longer windows (the old
    /// session of a completed swap).
    pub(super) fn flow_collect(&mut self, cx: &mut Context<Self>) {
        let live: HashSet<String> = self.embedded_sessions.iter().cloned().collect();
        let effects = self
            .flow
            .resolver
            .retain_live(|session| live.contains(session));
        self.apply_effects(effects, cx);
    }

    /// The account changed: kept packs and everything resolved from them are gone.
    pub(super) fn flow_invalidated(&mut self, cx: &mut Context<Self>) {
        self.flow.preparing.clear();
        self.flow.picker_preparations.clear();
        self.flow.picker = None;
        self.flow.seeds.clear();
        let effects = self.flow.resolver.invalidate();
        self.apply_effects(effects, cx);
    }

    /// A request finished without playing (failed, cancelled, dismissed): the
    /// dock is Enabled again. Returns whether the resolver republished it.
    pub(super) fn flow_settle(&mut self, session: &str, cx: &mut Context<Self>) -> bool {
        let effects = self.flow.resolver.settle(session);
        let published = effects
            .iter()
            .any(|effect| matches!(effect, Effect::Publish { .. }));
        self.apply_effects(effects, cx);
        published
    }

    /// Resolution found nothing to play: say why in the dock.
    pub(crate) fn neighbour_unavailable(
        &mut self,
        session: &str,
        reason: &str,
        cx: &mut Context<Self>,
    ) {
        self.reset_busy(session, cx);
        if self.embedded_sessions.contains(session) {
            player::neighbour_note(session, reason, true, cx);
        }
    }

    /// How the episode that just started was chosen, shown once its swap is ready.
    pub(super) fn flow_swap_ready(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        if let Some(note) = self.flow.notes.remove(from) {
            player::neighbour_note(to, &note, false, cx);
        }
        self.flow_collect(cx);
    }
}

// ── acting on an outcome ──

impl AppCoordinator {
    fn act(&mut self, outcome: Outcome, cx: &mut Context<Self>) {
        let Outcome {
            origin,
            target,
            decision,
        } = outcome;
        match origin {
            Origin::Detail { ticket } => {
                let play = match decision {
                    Decision::Direct { candidate } => DirectPlay::Play {
                        candidate,
                        label: None,
                        picker: None,
                    },
                    Decision::Prepare {
                        candidate,
                        label,
                        picker,
                    } => DirectPlay::Play {
                        candidate,
                        label: Some(label),
                        picker: Some(picker),
                    },
                    Decision::Picker(spec) => DirectPlay::Picker(spec),
                    Decision::Unavailable { label } => DirectPlay::Unavailable(label),
                    Decision::PlayKept { .. } | Decision::Fallback => DirectPlay::Fallback,
                };
                cx.emit(CoordinatorEvent::DirectPlay { ticket, play });
            }
            Origin::Window {
                session, direction, ..
            } => self.act_on_window(session, direction, target, decision, cx),
        }
    }

    fn act_on_window(
        &mut self,
        session: String,
        direction: NeighbourDirection,
        target: PlaybackTarget,
        decision: Decision,
        cx: &mut Context<Self>,
    ) {
        if !self.embedded_sessions.contains(&session) {
            return;
        }
        let PlaybackTarget::Episode {
            series,
            season,
            episode,
        } = &target
        else {
            return;
        };
        let title = self
            .sessions
            .iter()
            .find(|row| row.session.id == session)
            .and_then(|row| row.title.clone());
        let Some(title) = title else {
            return self.neighbour_failed(&session, direction, cx);
        };
        let launch = EpisodeLaunch {
            title: title.clone(),
            episode_title: self.flow.resolver.episode_name(series, *season, *episode),
            target: target.clone(),
            // A neighbour with saved progress resumes through the normal
            // sequence; one without starts at the beginning.
            intent: PlaybackIntent::Resume,
            direction: Some(direction),
        };
        match decision {
            Decision::PlayKept {
                file_index,
                candidate,
                label,
            } => {
                let file = self
                    .kept_preparation(&session)
                    .and_then(|kept| kept.files.get(file_index).cloned());
                let Some(file) = file else {
                    return self.neighbour_failed(&session, direction, cx);
                };
                match self.play_kept_file(&session, file.id.clone(), launch, cx) {
                    Ok(generation) => {
                        self.flow.plans.insert(
                            generation,
                            PlayPlan {
                                target,
                                candidate,
                                file: provider_file(&file),
                            },
                        );
                        self.flow.notes.insert(session, label);
                    }
                    Err(_) => self.neighbour_failed(&session, direction, cx),
                }
            }
            Decision::Prepare {
                candidate,
                label,
                picker,
            } => {
                let prepared = self.prepare(
                    title,
                    launch.episode_title.clone(),
                    target,
                    candidate,
                    launch.intent,
                    cx,
                );
                match prepared {
                    Some(generation) => {
                        self.flow.preparing.insert(
                            generation,
                            Preparing {
                                session,
                                direction,
                                launch,
                                label,
                                picker,
                            },
                        );
                    }
                    None => self.neighbour_failed(&session, direction, cx),
                }
            }
            Decision::Picker(spec) => {
                self.open_picker(session, direction, target, spec, launch, cx)
            }
            Decision::Unavailable { label } => self.neighbour_unavailable(&session, &label, cx),
            Decision::Direct { .. } | Decision::Fallback => {
                self.neighbour_failed(&session, direction, cx)
            }
        }
    }

    fn open_picker(
        &mut self,
        window: String,
        direction: NeighbourDirection,
        target: PlaybackTarget,
        spec: PickerSpec,
        launch: EpisodeLaunch,
        cx: &mut Context<Self>,
    ) {
        player::neighbour_note(&window, &spec.label, true, cx);
        self.flow.picker = Some(StreamPicker {
            window,
            direction,
            target,
            spec,
            title: launch.title,
            episode_title: launch.episode_title,
        });
        cx.emit(CoordinatorEvent::OpenStreamPicker);
        cx.notify();
    }

    /// The picker open for a player window, if any.
    pub(crate) fn stream_picker(&self) -> Option<&StreamPicker> {
        self.flow.picker.as_ref()
    }

    /// The Streams page showing the picker was dismissed (Back, Esc, another
    /// title) or has no more use: the player's control is usable again.
    pub(crate) fn close_stream_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.flow.picker.take() else {
            return;
        };
        self.flow.picker_preparations.clear();
        if self.embedded_sessions.contains(&picker.window) && !self.swap_in_progress(&picker.window)
        {
            self.flow_settle(&picker.window, cx);
        }
        cx.notify();
    }

    /// A preparation of ours produced its result.
    pub(super) fn flow_prepare_outcome(
        &mut self,
        generation: u64,
        outcome: &AppOutcome,
        cx: &mut Context<Self>,
    ) {
        let Some(preparing) = self.flow.preparing.remove(&generation) else {
            return;
        };
        let Preparing {
            session,
            direction,
            launch,
            label,
            picker,
        } = preparing;
        let live = self.embedded_sessions.contains(&session)
            && !self.closed_embedded_sessions.contains_key(&session);
        if !live {
            self.cancel_preparation(generation, cx);
            return;
        }
        let AppOutcome::Prepared {
            prepared,
            selection,
            ..
        } = outcome
        else {
            // The service could not prepare it. The current episode plays on.
            return self.neighbour_failed(&session, direction, cx);
        };
        let file = match selection {
            FileSelection::Unique(index) => prepared.files.get(*index),
            _ => None,
        };
        let Some(file) = file else {
            // The torrent's file for this episode is not unique: never guess.
            self.cancel_preparation(generation, cx);
            let target = launch.target.clone();
            return self.open_picker(session, direction, target, picker, launch, cx);
        };
        let keep = self.wants_kept_preparation(generation);
        let Some(play) = self.command(
            AppAction::PlayFile {
                preparation: generation,
                file_id: file.id.clone(),
                keep,
            },
            cx,
        ) else {
            self.cancel_preparation(generation, cx);
            return self.neighbour_failed(&session, direction, cx);
        };
        self.flow.notes.insert(session.clone(), label);
        self.route_launch_to_window(play, &session, launch);
    }
}

fn provider_file(file: &kinema_core::sources::RemoteFile) -> Option<ProviderFile> {
    Some(ProviderFile {
        name: basename(&file.path)?,
        size: file.size,
    })
}

/// Test seam: how the next episode was chosen, as if resolved for `window`.
#[cfg(all(test, feature = "ui-probe-test"))]
impl AppCoordinator {
    pub(crate) fn fixture_transition_note(&mut self, window: &str, note: &str) {
        self.flow.notes.insert(window.to_owned(), note.to_owned());
    }

    /// Test seam: a picker a player window is waiting on.
    pub(crate) fn fixture_stream_picker(&mut self, picker: StreamPicker) {
        self.flow.picker = Some(picker);
    }
}

// ── the picker's preparations and the plans of every start ──

impl AppCoordinator {
    /// A preparation was admitted (`prepare`): remember what it was for.
    pub(super) fn flow_prepared_for(
        &mut self,
        generation: u64,
        target: &PlaybackTarget,
        candidate: &SourceCandidate,
    ) {
        if !matches!(target, PlaybackTarget::Episode { .. }) {
            return;
        }
        // Like `session_identity`: a newer preparation supersedes the older ones.
        let pending: Vec<u64> = self
            .flow
            .seeds
            .keys()
            .copied()
            .filter(|g| self.tracker.is_pending(*g) || self.preparation_is_kept(*g))
            .collect();
        self.flow.seeds.retain(|g, _| pending.contains(g));
        self.flow.seeds.insert(
            generation,
            Seed {
                target: target.clone(),
                candidate: candidate.clone(),
            },
        );
        // Played into the window that asked for the picker, if it still plays
        // the episode before the one picked.
        if let Some(picker) = &self.flow.picker
            && &picker.target == target
            && self.embedded_sessions.contains(&picker.window)
            && !self.closed_embedded_sessions.contains_key(&picker.window)
        {
            self.flow.picker_preparations.insert(generation);
        }
    }

    /// A `PlayFile` command is being issued: link its start to the stream and
    /// the file it unlocks, and route it into the picker's window.
    pub(super) fn flow_play_file(
        &mut self,
        generation: u64,
        preparation: u64,
        file_id: &str,
        keep: bool,
    ) {
        if let Some(seed) = self.flow.seeds.get(&preparation) {
            let file = self
                .claim_preparation(preparation)
                .and_then(|kept| kept.files.iter().find(|file| file.id == file_id).cloned())
                .and_then(|file| provider_file(&file));
            self.flow.plans.insert(
                generation,
                PlayPlan {
                    target: seed.target.clone(),
                    candidate: seed.candidate.clone(),
                    file,
                },
            );
        }
        if keep
            && self.flow.picker_preparations.contains(&preparation)
            && let Some(picker) = self.flow.picker.clone()
        {
            let launch = EpisodeLaunch {
                title: picker.title,
                episode_title: picker.episode_title,
                target: picker.target,
                intent: PlaybackIntent::Resume,
                direction: Some(picker.direction),
            };
            self.route_launch_to_window(generation, &picker.window, launch);
        }
    }

    /// A start succeeded: remember the stream, and make it the window's
    /// reference for its own neighbours.
    pub(super) fn flow_started(&mut self, generation: u64, session: &str, embedded: bool) {
        let Some(plan) = self.flow.plans.remove(&generation) else {
            return;
        };
        if embedded {
            self.flow.resolver.set_played(session, played_of(&plan));
        }
        if let Some(remember) = remember_for(&plan) {
            let operation =
                LibraryOperation::StreamMemory(StreamMemoryOperation::Remember(Box::new(remember)));
            if !self.submit_flow(operation, Read::Remember) {
                tracing::debug!(
                    category = "stream_memory_unqueued",
                    "Stream memory not queued"
                );
            }
        }
    }

    /// Forgets the plan of a command that ended without a start.
    pub(super) fn flow_plan_dropped(&mut self, generation: u64) {
        self.flow.plans.remove(&generation);
    }
}

// ── Detail: play or resume with memory ──

impl AppCoordinator {
    /// Starts looking for what to play for `target` (an episode) from the
    /// stream memory. The answer arrives as [`CoordinatorEvent::DirectPlay`]
    /// with the returned ticket.
    pub(crate) fn play_episode_direct(
        &mut self,
        target: PlaybackTarget,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.flow.ticket += 1;
        let ticket = self.flow.ticket;
        let mut effects = self.flow.resolver.cancel_detail(ticket - 1);
        effects.extend(self.flow.resolver.detail(ticket, &target));
        self.apply_effects(effects, cx);
        ticket
    }

    /// Detail is no longer waiting for `ticket`.
    pub(crate) fn cancel_episode_direct(&mut self, ticket: u64, cx: &mut Context<Self>) {
        let effects = self.flow.resolver.cancel_detail(ticket);
        self.apply_effects(effects, cx);
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[path = "neighbours_tests.rs"]
mod tests;
