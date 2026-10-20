//! One SQLite owner with a bounded, four-thread blocking HTTP fetch pool.
//!
//! Two search slots and sixteen metadata slots coalesce/replace queued work.
//! Preparation and persistence run only on `kinema-catalog`; Send network/parse
//! jobs run on `kinema-fetch-N`. Completed jobs are persisted before new work is
//! prepared. At most four jobs (including unpersisted completions) are admitted.
//! Replacement cancels the old result, not its blocking HTTP call. Another free
//! fetch thread can start the replacement immediately; saturation still waits
//! for a bounded HTTP timeout. Drop closes mailboxes without joining threads.

use std::{
    any::Any,
    collections::VecDeque,
    io,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, ImdbId, SearchQuery},
    sources::{IndexerKind, PlaybackTarget},
};

use crate::{
    catalog::{CachedDetails, CachedSearch, CachedSources, CachedTmdb, CatalogService},
    tmdb::TmdbQuery,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// Show saved results; fetch on a miss or unacquired detail facts.
    Load,
    /// Show saved results first, then always fetch remotely.
    Refresh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Request {
    pub generation: u64,
    pub query: SearchQuery,
    pub mode: Mode,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MetadataAction {
    Browse {
        kind: MediaKind,
        offset: u32,
    },
    Details {
        kind: MediaKind,
        id: ImdbId,
    },
    Sources {
        indexer: IndexerKind,
        target: PlaybackTarget,
    },
    Tmdb(TmdbQuery),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataRequest {
    pub generation: u64,
    pub action: MetadataAction,
    pub mode: Mode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MetadataSnapshot {
    Browse(CachedSearch),
    Details(CachedDetails),
    Sources(CachedSources),
    Tmdb(CachedTmdb),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataEvent {
    pub generation: u64,
    pub action: MetadataAction,
    pub result: Result<MetadataSnapshot, String>,
    pub saved: bool,
    pub refreshing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Work {
    Search(Request),
    Metadata(MetadataRequest),
}

// Never Debug a Work/MetadataAction: searches may contain private text or URLs.
impl Work {
    pub(crate) fn diagnostic_span(&self) -> tracing::Span {
        let (generation, mode) = match self {
            Self::Search(r) => (r.generation, r.mode),
            Self::Metadata(r) => (r.generation, r.mode),
        };
        let operation = match self {
            Self::Search(_) => "search",
            Self::Metadata(r) => match &r.action {
                MetadataAction::Browse { .. } => "browse",
                MetadataAction::Details { .. } => "details",
                MetadataAction::Sources { .. } => "sources",
                MetadataAction::Tmdb(q) => q.operation(),
            },
        };
        let provider = match self {
            Self::Metadata(MetadataRequest {
                action: MetadataAction::Tmdb(_),
                ..
            }) => "tmdb",
            Self::Metadata(MetadataRequest {
                action: MetadataAction::Sources { indexer, .. },
                ..
            }) => match indexer {
                IndexerKind::Torrentio => "torrentio",
                IndexerKind::Peerflix => "peerflix",
            },
            _ => "cinemeta",
        };
        // Worker, fetch, persistence and terminal paths may establish this
        // context independently. Each is a request root: inheriting an already
        // entered catalog_request would render the same identity twice.
        let span = tracing::info_span!(
            parent: None,
            "catalog_request",
            operation,
            generation,
            ?mode,
            provider,
            media_kind = tracing::field::Empty,
            imdb_id = tracing::field::Empty,
            tmdb_id = tracing::field::Empty,
            page = tracing::field::Empty,
            season = tracing::field::Empty,
            episode = tracing::field::Empty,
            offset = tracing::field::Empty
        );
        match self {
            Self::Search(r) => {
                span.record("media_kind", tracing::field::debug(r.query.kind()));
            }
            Self::Metadata(r) => match &r.action {
                MetadataAction::Browse { kind, offset } => {
                    span.record("media_kind", tracing::field::debug(kind));
                    span.record("offset", offset);
                }
                MetadataAction::Details { kind, id } => {
                    span.record("media_kind", tracing::field::debug(kind));
                    span.record("imdb_id", id.as_str());
                }
                MetadataAction::Sources { target, .. } => {
                    span.record("media_kind", tracing::field::debug(target.kind()));
                    span.record("imdb_id", target.imdb_id().as_str());
                    if let PlaybackTarget::Episode {
                        season, episode, ..
                    } = target
                    {
                        span.record("season", season);
                        span.record("episode", episode);
                    }
                }
                MetadataAction::Tmdb(q) => {
                    q.record_diagnostics(&span);
                }
            },
        }
        span
    }
}

fn refreshed_count(value: &Refreshed) -> usize {
    match value {
        Refreshed::Search(s) => s.titles.len(),
        Refreshed::Metadata(s) => match s.as_ref() {
            MetadataSnapshot::Browse(s) => s.titles.len(),
            MetadataSnapshot::Details(_) => 1,
            MetadataSnapshot::Sources(s) => s.candidates.len(),
            MetadataSnapshot::Tmdb(s) => crate::tmdb::data_count(&s.data),
        },
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Snapshot {
    pub titles: Vec<CatalogTitle>,
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    /// A saved snapshot (possibly empty). A remote fetch follows when requested.
    CacheHit {
        snapshot: Snapshot,
        remote_follows: bool,
    },
    CacheMiss,
    Fetched(Snapshot),
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Event {
    pub kind: MediaKind,
    pub generation: u64,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Submit {
    Queued,
    Replaced,
    Coalesced,
    Closed,
}

#[derive(Default)]
struct MailState {
    slots: [Option<Request>; 2],
    search_enqueued: [Option<Instant>; 2],
    metadata_enqueued: [Option<Instant>; METADATA_SLOTS],
    metadata: [Option<MetadataRequest>; METADATA_SLOTS],
    active: [Option<ActiveMetadata>; METADATA_SLOTS],
    active_search: [Option<ActiveSearch>; 2],
    /// Kind served first next, so one kind cannot starve the other.
    next: usize,
    closed: bool,
    completed: VecDeque<Box<dyn Any + Send>>,
}

struct ActiveSearch {
    query: SearchQuery,
    generation: u64,
    cancel: Arc<AtomicBool>,
}

struct ActiveMetadata {
    action: MetadataAction,
    generation: u64,
    cancel: Arc<AtomicBool>,
}

fn slot(kind: MediaKind) -> usize {
    match kind {
        MediaKind::Movie => 0,
        MediaKind::Series => 1,
    }
}

// Separate kind/indexer operation slots avoid one page starving another while
// keeping pending metadata bounded to sixteen operations, regardless of IDs/pages.
const METADATA_SLOTS: usize = 16;

fn metadata_slot(action: &MetadataAction) -> usize {
    match action {
        MetadataAction::Browse { kind, .. } => slot(*kind),
        MetadataAction::Details { kind, .. } => 2 + slot(*kind),
        MetadataAction::Sources { indexer, target } => {
            4 + slot(target.kind()) * 2
                + match indexer {
                    IndexerKind::Torrentio => 0,
                    IndexerKind::Peerflix => 1,
                }
        }
        MetadataAction::Tmdb(query) => {
            8 + match query {
                TmdbQuery::Browse { kind, .. } => slot(*kind),
                TmdbQuery::Search { query, .. } => 2 + slot(query.kind()),
                TmdbQuery::Details { kind, .. } => 4 + slot(*kind),
                TmdbQuery::Season { .. } => 6,
                TmdbQuery::Similar { .. } => 7,
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct Mailbox {
    state: Mutex<MailState>,
    wake: Condvar,
}

impl Mailbox {
    fn lock(&self) -> MutexGuard<'_, MailState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn submit(&self, request: Request) -> Submit {
        let mut state = self.lock();
        if state.closed {
            return Submit::Closed;
        }
        let _span = Work::Search(request.clone()).diagnostic_span().entered();
        let index = slot(request.query.kind());
        state.search_enqueued[index] = Some(Instant::now());
        if let Some(active) = &state.active_search[index]
            && (active.generation != request.generation || active.query != request.query)
        {
            tracing::debug!(
                outcome = "superseded",
                "Active catalog request canceled by replacement"
            );
            active.cancel.store(true, Ordering::Release);
        }
        let outcome = match &mut state.slots[index] {
            Some(pending) if pending.query == request.query => {
                pending.generation = request.generation;
                if request.mode == Mode::Refresh {
                    pending.mode = Mode::Refresh;
                }
                Submit::Coalesced
            }
            Some(pending) => {
                *pending = request;
                Submit::Replaced
            }
            empty => {
                *empty = Some(request);
                Submit::Queued
            }
        };
        tracing::debug!(outcome = ?outcome, "Catalog request submitted");
        self.wake.notify_one();
        outcome
    }

    fn submit_metadata(&self, request: MetadataRequest) -> Submit {
        let mut state = self.lock();
        if state.closed {
            return Submit::Closed;
        }
        let _span = Work::Metadata(request.clone()).diagnostic_span().entered();
        let index = metadata_slot(&request.action);
        state.metadata_enqueued[index] = Some(Instant::now());
        if let Some(active) = &state.active[index]
            && (active.generation != request.generation || active.action != request.action)
        {
            tracing::debug!(
                outcome = "superseded",
                "Active catalog request canceled by replacement"
            );
            active.cancel.store(true, Ordering::Release);
        }
        let result = match &mut state.metadata[index] {
            Some(pending) if pending.action == request.action => {
                pending.generation = request.generation;
                if request.mode == Mode::Refresh {
                    pending.mode = Mode::Refresh;
                }
                Submit::Coalesced
            }
            Some(pending) => {
                *pending = request;
                Submit::Replaced
            }
            empty => {
                *empty = Some(request);
                Submit::Queued
            }
        };
        tracing::debug!(outcome = ?result, "Catalog request submitted");
        self.wake.notify_one();
        result
    }

    /// Blocks until work is available; `None` once closed.
    #[cfg(test)]
    fn take_work(&self) -> Option<Work> {
        let mut state = self.lock();
        loop {
            if state.closed {
                return None;
            }
            let slots = state.slots.len() + state.metadata.len();
            for offset in 0..slots {
                let index = (state.next + offset) % slots;
                let work = if index < state.slots.len() {
                    state.slots[index].take().map(|request| {
                        state.active_search[index] = Some(ActiveSearch {
                            query: request.query.clone(),
                            generation: request.generation,
                            cancel: Arc::new(AtomicBool::new(false)),
                        });
                        Work::Search(request)
                    })
                } else {
                    let metadata_index = index - state.slots.len();
                    state.metadata[metadata_index].take().map(|request| {
                        state.active[metadata_index] = Some(ActiveMetadata {
                            action: request.action.clone(),
                            generation: request.generation,
                            cancel: Arc::new(AtomicBool::new(false)),
                        });
                        Work::Metadata(request)
                    })
                };
                if let Some(work) = work {
                    state.next = (index + 1) % slots;
                    return Some(work);
                }
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn next<B: Backend>(&self, in_flight: usize) -> Option<Ready<B>> {
        let mut state = self.lock();
        loop {
            if state.closed {
                return None;
            }
            if let Some(completed) = state.completed.pop_front() {
                return completed
                    .downcast::<Completed<B>>()
                    .ok()
                    .map(|value| Ready::Completed(*value));
            }
            if in_flight < FETCH_THREADS {
                let slots = state.slots.len() + state.metadata.len();
                for offset in 0..slots {
                    let index = (state.next + offset) % slots;
                    let work = if index < state.slots.len() {
                        state.slots[index].take().map(|request| {
                            state.active_search[index] = Some(ActiveSearch {
                                query: request.query.clone(),
                                generation: request.generation,
                                cancel: Arc::new(AtomicBool::new(false)),
                            });
                            Work::Search(request)
                        })
                    } else {
                        let metadata_index = index - state.slots.len();
                        state.metadata[metadata_index].take().map(|request| {
                            state.active[metadata_index] = Some(ActiveMetadata {
                                action: request.action.clone(),
                                generation: request.generation,
                                cancel: Arc::new(AtomicBool::new(false)),
                            });
                            Work::Metadata(request)
                        })
                    };
                    if let Some(work) = work {
                        state.next = (index + 1) % slots;
                        let enqueued = if index < state.slots.len() {
                            state.search_enqueued[index].take()
                        } else {
                            let metadata_index = index - state.slots.len();
                            state.metadata_enqueued[metadata_index].take()
                        };
                        return Some(Ready::Work(
                            work,
                            enqueued.map_or(Duration::ZERO, |time| time.elapsed()),
                        ));
                    }
                }
            }
            let queued =
                state.slots.iter().flatten().count() + state.metadata.iter().flatten().count();
            if in_flight >= FETCH_THREADS && queued > 0 {
                tracing::debug!(
                    in_flight,
                    queued,
                    "Catalog fetch pool saturated; queued work waiting"
                );
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    #[cfg(test)]
    fn take(&self) -> Option<Request> {
        match self.take_work()? {
            Work::Search(request) => Some(request),
            Work::Metadata(_) => None,
        }
    }

    #[cfg(test)]
    fn metadata_superseded(&self, action: &MetadataAction) -> bool {
        let state = self.lock();
        let index = metadata_slot(action);
        state.closed
            || state.metadata[index].is_some()
            || state.active[index]
                .as_ref()
                .is_some_and(|active| active.cancel.load(Ordering::Acquire))
    }

    fn metadata_cancellation(&self, request: &MetadataRequest) -> Arc<AtomicBool> {
        let mut state = self.lock();
        let index = metadata_slot(&request.action);
        if let Some(active) = &state.active[index]
            && active.action == request.action
            && active.generation == request.generation
        {
            return Arc::clone(&active.cancel);
        }
        let cancel = Arc::new(AtomicBool::new(state.closed));
        state.active[index] = Some(ActiveMetadata {
            action: request.action.clone(),
            generation: request.generation,
            cancel: Arc::clone(&cancel),
        });
        cancel
    }

    #[cfg(test)]
    fn finish_metadata(&self, request: &MetadataRequest) {
        let mut state = self.lock();
        let index = metadata_slot(&request.action);
        if state.active[index].as_ref().is_some_and(|active| {
            active.action == request.action && active.generation == request.generation
        }) {
            state.active[index] = None;
        }
    }

    fn cancel_metadata(&self, action: &MetadataAction, generation: u64) {
        tracing::debug!(
            slot = metadata_slot(action),
            generation,
            outcome = "cancelled",
            "Catalog metadata cancellation requested"
        );
        let mut state = self.lock();
        let index = metadata_slot(action);
        if state.metadata[index]
            .as_ref()
            .is_some_and(|request| request.action == *action && request.generation == generation)
        {
            state.metadata[index] = None;
        }
        if let Some(active) = &state.active[index]
            && active.action == *action
            && active.generation == generation
        {
            active.cancel.store(true, Ordering::Release);
        }
    }

    fn cancel_search(&self, kind: MediaKind, generation: u64) {
        tracing::debug!(
            ?kind,
            generation,
            outcome = "cancelled",
            "Catalog search cancellation requested"
        );
        let mut state = self.lock();
        let index = slot(kind);
        if state.slots[index]
            .as_ref()
            .is_some_and(|request| request.generation == generation)
        {
            state.slots[index] = None;
        }
        if let Some(active) = &state.active_search[index]
            && active.generation == generation
        {
            active.cancel.store(true, Ordering::Release);
        }
    }

    fn is_closed(&self) -> bool {
        self.lock().closed
    }

    fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        state.slots = Default::default();
        state.metadata = Default::default();
        state.completed.clear();
        for active in state.active.iter().flatten() {
            active.cancel.store(true, Ordering::Release);
        }
        for active in state.active_search.iter().flatten() {
            active.cancel.store(true, Ordering::Release);
        }
        self.wake.notify_all();
    }
}

/// Most events one request can produce: a cache outcome, then a terminal one.
const EVENTS_PER_KIND: usize = 2;

/// Bounded worker-to-UI mailbox. Pushing never blocks: an event for a newer
/// generation discards the kind's queued older events, so at most
/// [`EVENTS_PER_KIND`] per kind are held and the latest terminal event is
/// always preserved.
#[derive(Default)]
struct EventBox {
    queues: Mutex<[VecDeque<Event>; 2]>,
}

impl EventBox {
    fn lock(&self) -> MutexGuard<'_, [VecDeque<Event>; 2]> {
        self.queues.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, event: Event) {
        let mut queues = self.lock();
        let queue = &mut queues[slot(event.kind)];
        if queue
            .back()
            .is_some_and(|last| last.generation != event.generation)
        {
            queue.clear();
        }
        if queue.len() == EVENTS_PER_KIND {
            // Cannot happen for one request; keep the newest if it does.
            queue.pop_front();
        }
        queue.push_back(event);
    }

    fn pop(&self) -> Option<Event> {
        self.lock().iter_mut().find_map(VecDeque::pop_front)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().iter().map(VecDeque::len).sum()
    }
}

#[derive(Default)]
struct MetadataEventBox {
    queues: Mutex<[VecDeque<MetadataEvent>; METADATA_SLOTS]>,
}

impl MetadataEventBox {
    fn push(&self, event: MetadataEvent) {
        let mut queues = self.queues.lock().unwrap_or_else(PoisonError::into_inner);
        let queue = &mut queues[metadata_slot(&event.action)];
        if queue
            .back()
            .is_some_and(|last| last.generation != event.generation || last.action != event.action)
        {
            queue.clear();
        }
        if queue.len() == EVENTS_PER_KIND {
            queue.pop_front();
        }
        queue.push_back(event);
    }

    fn cancel(&self, action: &MetadataAction, generation: u64) {
        let mut queues = self.queues.lock().unwrap_or_else(PoisonError::into_inner);
        queues[metadata_slot(action)]
            .retain(|event| event.action != *action || event.generation != generation);
    }

    fn pop(&self) -> Option<MetadataEvent> {
        self.queues
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
            .find_map(VecDeque::pop_front)
    }
}

pub(crate) enum Refreshed {
    Search(Snapshot),
    Metadata(Box<MetadataSnapshot>),
}
impl Refreshed {
    pub(crate) fn metadata(snapshot: MetadataSnapshot) -> Self {
        Self::Metadata(Box::new(snapshot))
    }
}
pub(crate) type Persist<B> = Box<dyn FnOnce(&mut B) -> Result<Refreshed, String> + Send>;
pub(crate) type Fetch<B> = Box<dyn FnOnce() -> Persist<B> + Send>;

/// Blocking catalog operations; errors are already user-safe messages.
pub(crate) trait Backend: Sized + 'static {
    fn prepare(
        &mut self,
        _work: &Work,
        _cancel: &Arc<AtomicBool>,
    ) -> Option<Result<Fetch<Self>, String>> {
        None
    }
    fn set_cancellation(&mut self, _cancel: Option<Arc<AtomicBool>>) {}

    fn read(&mut self, query: &SearchQuery) -> Result<Option<Snapshot>, String>;
    fn refresh(&mut self, query: &SearchQuery) -> Result<Snapshot, String>;
    fn refresh_search(
        &mut self,
        query: &SearchQuery,
        _cancel: &Arc<AtomicBool>,
    ) -> Result<Snapshot, String> {
        self.refresh(query)
    }
    fn read_metadata(
        &mut self,
        _action: &MetadataAction,
    ) -> Result<Option<MetadataSnapshot>, String> {
        Err("Metadata is unavailable from this backend.".into())
    }
    /// Called only after a successful Load-mode metadata cache hit. Unknown
    /// facts are not necessarily unacquired; storage owns that distinction.
    fn metadata_needs_refresh(&mut self, _action: &MetadataAction) -> Result<bool, String> {
        Ok(false)
    }
    fn refresh_metadata(
        &mut self,
        _action: &MetadataAction,
        _cancel: &Arc<AtomicBool>,
    ) -> Result<MetadataSnapshot, String> {
        Err("Metadata is unavailable from this backend.".into())
    }
}

pub(crate) fn snapshot(search: CachedSearch) -> Snapshot {
    Snapshot {
        titles: search.titles,
        fetched_at: search.fetched_at,
    }
}

impl Backend for CatalogService {
    fn prepare(
        &mut self,
        work: &Work,
        cancel: &Arc<AtomicBool>,
    ) -> Option<Result<Fetch<Self>, String>> {
        Some(
            self.prepare_remote(work.clone(), Arc::clone(cancel))
                .map_err(crate::catalog::diagnostic_error),
        )
    }
    fn set_cancellation(&mut self, cancel: Option<Arc<AtomicBool>>) {
        self.cancellation = cancel;
    }
    fn read(&mut self, query: &SearchQuery) -> Result<Option<Snapshot>, String> {
        CatalogService::read(self, query)
            .map(|found| found.map(snapshot))
            .map_err(crate::catalog::diagnostic_error)
    }

    fn refresh(&mut self, query: &SearchQuery) -> Result<Snapshot, String> {
        CatalogService::refresh(self, query)
            .map(snapshot)
            .map_err(crate::catalog::diagnostic_error)
    }

    fn refresh_search(
        &mut self,
        query: &SearchQuery,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Snapshot, String> {
        self.cancellation = Some(Arc::clone(cancel));
        let result = Backend::refresh(self, query);
        self.cancellation = None;
        result
    }

    fn read_metadata(
        &mut self,
        action: &MetadataAction,
    ) -> Result<Option<MetadataSnapshot>, String> {
        match action {
            MetadataAction::Browse { kind, offset } => self
                .read_browse(*kind, *offset)
                .map(|value| value.map(MetadataSnapshot::Browse)),
            MetadataAction::Details { kind, id } => self
                .read_details(*kind, id)
                .map(|value| value.map(MetadataSnapshot::Details)),
            MetadataAction::Sources { indexer, target } => self
                .read_sources(*indexer, target)
                .map(|value| value.map(MetadataSnapshot::Sources)),
            MetadataAction::Tmdb(query) => self
                .read_tmdb(query)
                .map(|value| value.map(MetadataSnapshot::Tmdb)),
        }
        .map_err(crate::catalog::diagnostic_error)
    }

    fn metadata_needs_refresh(&mut self, action: &MetadataAction) -> Result<bool, String> {
        CatalogService::metadata_needs_refresh(self, action).map_err(|error| {
            tracing::debug!(category = ?error, "Detail facts acquisition check failed");
            error.to_string()
        })
    }

    fn refresh_metadata(
        &mut self,
        action: &MetadataAction,
        cancel: &Arc<AtomicBool>,
    ) -> Result<MetadataSnapshot, String> {
        self.cancellation = Some(Arc::clone(cancel));
        let result = match action {
            MetadataAction::Browse { kind, offset } => self
                .refresh_browse(*kind, *offset)
                .map(MetadataSnapshot::Browse),
            MetadataAction::Details { kind, id } => self
                .refresh_details(*kind, id)
                .map(MetadataSnapshot::Details),
            MetadataAction::Sources { indexer, target } => self
                .refresh_sources(*indexer, target)
                .map(MetadataSnapshot::Sources),
            MetadataAction::Tmdb(query) => self.refresh_tmdb(query).map(MetadataSnapshot::Tmdb),
        };
        self.cancellation = None;
        result.map_err(crate::catalog::diagnostic_error)
    }
}

/// Result of a non-blocking look at the worker's event mailbox.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Poll {
    Event(Event),
    /// Nothing right now; the worker is still running.
    Empty,
    /// The worker has exited (or was shut down) and no events remain.
    Disconnected,
}

/// Closes the mailbox however the worker thread ends, including a panic.
struct CloseOnExit(Arc<Mailbox>);

impl Drop for CloseOnExit {
    fn drop(&mut self) {
        tracing::info!("Catalog worker stopped");
        self.0.close();
    }
}

pub(crate) struct CatalogWorker {
    mailbox: Arc<Mailbox>,
    events: Arc<EventBox>,
    metadata_events: Arc<MetadataEventBox>,
    // Held for ownership; intentionally not joined on drop (see module docs).
    _thread: JoinHandle<()>,
}

impl CatalogWorker {
    /// Starts the production worker; the service is opened on the worker.
    pub(crate) fn spawn_default() -> io::Result<Self> {
        Self::spawn(|| CatalogService::open_default().map_err(crate::catalog::diagnostic_error))
    }

    pub(crate) fn spawn<B: Backend + 'static>(
        open: impl FnMut() -> Result<B, String> + Send + 'static,
    ) -> io::Result<Self> {
        let mailbox = Arc::new(Mailbox::default());
        let events = Arc::new(EventBox::default());
        let metadata_events = Arc::new(MetadataEventBox::default());
        let worker_metadata_events = Arc::clone(&metadata_events);
        let worker_mailbox = Arc::clone(&mailbox);
        let worker_events = Arc::clone(&events);
        let thread = std::thread::Builder::new()
            .name("kinema-catalog".into())
            .spawn(move || {
                let _close = CloseOnExit(Arc::clone(&worker_mailbox));
                run(
                    &worker_mailbox,
                    &worker_events,
                    &worker_metadata_events,
                    open,
                );
            })?;
        Ok(Self {
            mailbox,
            events,
            metadata_events,
            _thread: thread,
        })
    }

    pub(crate) fn submit(&self, request: Request) -> Submit {
        self.mailbox.submit(request)
    }

    pub(crate) fn submit_metadata(&self, request: MetadataRequest) -> Submit {
        self.mailbox.submit_metadata(request)
    }

    /// Exact-generation cancellation for clear-search/provider-change owners.
    pub(crate) fn cancel_search(&self, kind: MediaKind, generation: u64) {
        self.mailbox.cancel_search(kind, generation);
        self.events.lock()[slot(kind)].retain(|event| event.generation != generation);
    }

    pub(crate) fn cancel_metadata(&self, action: &MetadataAction, generation: u64) {
        self.mailbox.cancel_metadata(action, generation);
        self.metadata_events.cancel(action, generation);
    }

    /// Non-blocking; the UI checks the action and generation before applying.
    pub(crate) fn poll_metadata(&self) -> Option<MetadataEvent> {
        self.metadata_events.pop()
    }

    /// Non-blocking.
    pub(crate) fn poll(&self) -> Poll {
        if let Some(event) = self.events.pop() {
            Poll::Event(event)
        } else if self.mailbox.is_closed() {
            Poll::Disconnected
        } else {
            Poll::Empty
        }
    }
}

impl Drop for CatalogWorker {
    fn drop(&mut self) {
        let searches = {
            let state = self.mailbox.lock();
            state
                .slots
                .iter()
                .flatten()
                .map(|request| (request.query.kind(), request.generation))
                .chain(
                    state
                        .active_search
                        .iter()
                        .flatten()
                        .map(|active| (active.query.kind(), active.generation)),
                )
                .collect::<Vec<_>>()
        };
        for (kind, generation) in searches {
            self.cancel_search(kind, generation);
        }
        tracing::debug!("Catalog worker stop requested");
        self.mailbox.close();
    }
}

/// `open` is retried on the next request after a failure, so a transient
/// storage or client error recovers without restarting the application.
const FETCH_THREADS: usize = 4;
struct Completed<B> {
    work: Work,
    cancel: Arc<AtomicBool>,
    persist: Persist<B>,
}
enum Ready<B> {
    Work(Work, Duration),
    Completed(Completed<B>),
}
type FetchJob = Box<dyn FnOnce() + Send>;

fn run<B: Backend>(
    mailbox: &Arc<Mailbox>,
    events: &EventBox,
    metadata_events: &MetadataEventBox,
    mut open: impl FnMut() -> Result<B, String>,
) {
    tracing::info!(fetch_threads = FETCH_THREADS, "Catalog worker started");
    // At most four submitted jobs, including completed-but-not-persisted jobs.
    // The SQLite owner never waits for a busy fetch thread or joins on shutdown.
    let (jobs, receiver) = std::sync::mpsc::sync_channel::<FetchJob>(FETCH_THREADS);
    let receiver = Arc::new(Mutex::new(receiver));
    for index in 0..FETCH_THREADS {
        let receiver = Arc::clone(&receiver);
        let close = Arc::clone(mailbox);
        if std::thread::Builder::new()
            .name(format!("kinema-fetch-{index}"))
            .spawn(move || {
                loop {
                    let job = receiver
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .recv();
                    let Ok(job) = job else {
                        break;
                    };
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                        tracing::error!(category = "fetch_panic", "Catalog fetch worker panicked");
                        close.close();
                        break;
                    }
                }
            })
            .inspect_err(|error| tracing::error!(io_kind = ?error.kind(), "Catalog fetch worker spawn failed"))
            .is_err()
        {
            return;
        }
    }
    let mut backend: Option<B> = None;
    let mut in_flight = 0;
    while let Some(ready) = mailbox.next::<B>(in_flight) {
        match ready {
            Ready::Completed(completed) => {
                in_flight -= 1;
                let Completed {
                    work,
                    cancel,
                    persist,
                } = completed;
                let _span = work.diagnostic_span().entered();
                if !cancel.load(Ordering::Acquire)
                    && !mailbox.is_closed()
                    && let Some(backend) = backend.as_mut()
                {
                    backend.set_cancellation(Some(Arc::clone(&cancel)));
                    let started = Instant::now();
                    let result = persist(backend);
                    tracing::debug!(
                        persist_ms = started.elapsed().as_millis() as u64,
                        "Catalog persistence completed"
                    );
                    backend.set_cancellation(None);
                    send_terminal(mailbox, events, metadata_events, &work, &cancel, result);
                } else {
                    tracing::debug!(
                        outcome = "discarded_stale",
                        "Catalog completion discarded before persistence"
                    );
                }
                finish_work(mailbox, &work, &cancel);
            }
            Ready::Work(work, queue_wait) => {
                let _span = work.diagnostic_span().entered();
                tracing::debug!(
                    queue_wait_ms = queue_wait.as_millis() as u64,
                    in_flight,
                    "Catalog request dequeued"
                );
                let cancel = match &work {
                    Work::Search(request) => mailbox.lock().active_search
                        [slot(request.query.kind())]
                    .as_ref()
                    .map(|active| Arc::clone(&active.cancel))
                    .unwrap_or_else(|| Arc::new(AtomicBool::new(true))),
                    Work::Metadata(request) => mailbox.metadata_cancellation(request),
                };
                if backend.is_none() {
                    match open() {
                        Ok(opened) => {
                            tracing::info!("Catalog backend opened");
                            backend = Some(opened);
                        }
                        Err(message) => {
                            tracing::warn!(
                                category = "open_failed",
                                "Catalog worker could not open backend; next request may retry"
                            );
                            send_terminal(
                                mailbox,
                                events,
                                metadata_events,
                                &work,
                                &cancel,
                                Err(message),
                            );
                            finish_work(mailbox, &work, &cancel);
                            continue;
                        }
                    }
                }
                let Some(backend) = backend.as_mut() else {
                    continue;
                };
                if !read_before_fetch(mailbox, events, metadata_events, backend, &work, &cancel) {
                    finish_work(mailbox, &work, &cancel);
                    continue;
                }
                backend.set_cancellation(Some(Arc::clone(&cancel)));
                let prepared = backend.prepare(&work, &cancel);
                backend.set_cancellation(None);
                let fetch = match prepared {
                    Some(Ok(fetch)) => fetch,
                    Some(Err(message)) => {
                        send_terminal(
                            mailbox,
                            events,
                            metadata_events,
                            &work,
                            &cancel,
                            Err(message),
                        );
                        finish_work(mailbox, &work, &cancel);
                        continue;
                    }
                    None => {
                        // Small synchronous test backends can retain the old interface.
                        let result = match &work {
                            Work::Search(request) => backend
                                .refresh_search(&request.query, &cancel)
                                .map(Refreshed::Search),
                            Work::Metadata(request) => backend
                                .refresh_metadata(&request.action, &cancel)
                                .map(Refreshed::metadata),
                        };
                        send_terminal(mailbox, events, metadata_events, &work, &cancel, result);
                        finish_work(mailbox, &work, &cancel);
                        continue;
                    }
                };
                let mailbox = Arc::clone(mailbox);
                in_flight += 1;
                if jobs
                    .send(Box::new(move || {
                        let _span = work.diagnostic_span().entered();
                        let started = Instant::now();
                        let persist = fetch();
                        tracing::debug!(
                            fetch_ms = started.elapsed().as_millis() as u64,
                            "Catalog fetch completed"
                        );
                        let mut state = mailbox.lock();
                        if !state.closed {
                            state.completed.push_back(Box::new(Completed {
                                work,
                                cancel,
                                persist,
                            }));
                            mailbox.wake.notify_one();
                        } else {
                            tracing::debug!(
                                outcome = "discarded_stale",
                                "Catalog fetch finished after shutdown"
                            );
                        }
                    }))
                    .is_err()
                {
                    tracing::error!(
                        category = "fetch_queue_closed",
                        "Catalog fetch pool disconnected"
                    );
                    return;
                }
            }
        }
    }
}

fn finish_work(mailbox: &Mailbox, work: &Work, cancel: &Arc<AtomicBool>) {
    match work {
        Work::Metadata(request) => {
            let mut state = mailbox.lock();
            let active = &mut state.active[metadata_slot(&request.action)];
            if active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(&active.cancel, cancel))
            {
                *active = None;
            }
        }
        Work::Search(request) => {
            let mut state = mailbox.lock();
            let active = &mut state.active_search[slot(request.query.kind())];
            if active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(&active.cancel, cancel))
            {
                *active = None;
            }
        }
    }
}

fn send_terminal(
    mailbox: &Mailbox,
    events: &EventBox,
    metadata_events: &MetadataEventBox,
    work: &Work,
    cancel: &AtomicBool,
    result: Result<Refreshed, String>,
) {
    let state = mailbox.lock();
    let _span = work.diagnostic_span().entered();
    if state.closed || cancel.load(Ordering::Acquire) {
        tracing::debug!(
            outcome = "discarded_stale",
            "Catalog terminal result discarded"
        );
        return;
    }
    match &result {
        Ok(value) => tracing::debug!(
            outcome = "fetched",
            count = refreshed_count(value),
            empty = refreshed_count(value) == 0,
            "Catalog request finished"
        ),
        Err(_) => tracing::warn!(category = "backend_failed", "Catalog request failed"),
    }
    match work {
        Work::Search(request) => {
            let outcome = match result {
                Ok(Refreshed::Search(snapshot)) => Outcome::Fetched(snapshot),
                Err(message) => Outcome::Failed(message),
                _ => return,
            };
            events.push(Event {
                kind: request.query.kind(),
                generation: request.generation,
                outcome,
            });
        }
        Work::Metadata(request) => {
            let result = match result {
                Ok(Refreshed::Metadata(snapshot)) => Ok(*snapshot),
                Err(message) => Err(message),
                _ => return,
            };
            metadata_events.push(MetadataEvent {
                generation: request.generation,
                action: request.action.clone(),
                result,
                saved: false,
                refreshing: false,
            });
        }
    }
}

/// A failed optional acquisition probe must never turn usable cached details
/// into an error. Do not log the backend's String: only categories are safe.
fn metadata_refresh_required<B: Backend>(backend: &mut B, action: &MetadataAction) -> bool {
    match backend.metadata_needs_refresh(action) {
        Ok(true) => {
            tracing::debug!(
                refresh_reason = "facts_unacquired",
                "Saved metadata needs remote detail facts"
            );
            true
        }
        Ok(false) => false,
        Err(_) => {
            tracing::debug!(
                category = "facts_acquisition_check_failed",
                outcome = "cache_only",
                "Keeping saved metadata after acquisition check failed"
            );
            false
        }
    }
}

fn read_before_fetch<B: Backend>(
    mailbox: &Mailbox,
    events: &EventBox,
    metadata_events: &MetadataEventBox,
    backend: &mut B,
    work: &Work,
    cancel: &AtomicBool,
) -> bool {
    let mut refreshing = match work {
        Work::Search(r) => r.mode,
        Work::Metadata(r) => r.mode,
    } == Mode::Refresh;
    let result = match work {
        Work::Search(r) => backend.read(&r.query).map(|s| s.map(Refreshed::Search)),
        Work::Metadata(r) => backend
            .read_metadata(&r.action)
            .map(|s| s.map(Refreshed::metadata)),
    };
    if !refreshing
        && let Work::Metadata(request) = work
        && matches!(&result, Ok(Some(Refreshed::Metadata(_))))
    {
        refreshing = metadata_refresh_required(backend, &request.action);
    }
    let state = mailbox.lock();
    if state.closed || cancel.load(Ordering::Acquire) {
        tracing::debug!(outcome = "cancelled", "Catalog cache result discarded");
        return false;
    }
    match &result {
        Ok(Some(value)) => tracing::debug!(
            cache = if refreshing { "stale" } else { "hit" },
            count = refreshed_count(value),
            refreshing,
            "Catalog cache read completed"
        ),
        Ok(None) => tracing::debug!(cache = "miss", "Catalog cache read completed"),
        Err(_) => tracing::warn!(
            category = "cache_read_failed",
            refreshing,
            "Catalog cache read failed"
        ),
    }
    match (work, result) {
        (Work::Search(r), Ok(Some(Refreshed::Search(snapshot)))) => events.push(Event {
            kind: r.query.kind(),
            generation: r.generation,
            outcome: Outcome::CacheHit {
                snapshot,
                remote_follows: refreshing,
            },
        }),
        (Work::Metadata(r), Ok(Some(Refreshed::Metadata(snapshot)))) => {
            metadata_events.push(MetadataEvent {
                generation: r.generation,
                action: r.action.clone(),
                result: Ok(*snapshot),
                saved: true,
                refreshing,
            })
        }
        (_, Err(message)) if !refreshing => {
            drop(state);
            send_terminal(mailbox, events, metadata_events, work, cancel, Err(message));
            return false;
        }
        (Work::Search(r), _) => {
            events.push(Event {
                kind: r.query.kind(),
                generation: r.generation,
                outcome: Outcome::CacheMiss,
            });
            return true;
        }
        _ => return true,
    }
    refreshing
}

#[cfg(test)]
struct MetadataInFlight<'a>(&'a Mailbox, &'a MetadataRequest);
#[cfg(test)]
impl Drop for MetadataInFlight<'_> {
    fn drop(&mut self) {
        self.0.finish_metadata(self.1);
    }
}

#[cfg(test)]
fn process_metadata<B: Backend>(
    mailbox: &Mailbox,
    events: &MetadataEventBox,
    backend: &mut Option<B>,
    open: &mut impl FnMut() -> Result<B, String>,
    request: MetadataRequest,
) {
    let cancellation = mailbox.metadata_cancellation(&request);
    let _active = MetadataInFlight(mailbox, &request);
    if cancellation.load(Ordering::Acquire) {
        return;
    }
    let mut refreshing = request.mode == Mode::Refresh;
    let send = |result, saved, refreshing| {
        // Serialize the final cancellation check with cancel/close. UI cancel
        // then purges already queued events, so none can be inserted behind it.
        let state = mailbox.lock();
        if !state.closed && !cancellation.load(Ordering::Acquire) {
            events.push(MetadataEvent {
                generation: request.generation,
                action: request.action.clone(),
                result,
                saved,
                refreshing,
            });
        }
    };
    if backend.is_none() {
        match open() {
            Ok(opened) => *backend = Some(opened),
            Err(message) => {
                send(Err(message), false, false);
                return;
            }
        }
    }
    let Some(backend) = backend.as_mut() else {
        return;
    };
    if mailbox.metadata_superseded(&request.action) {
        return;
    }
    match backend.read_metadata(&request.action) {
        Ok(Some(snapshot)) => {
            if !refreshing {
                refreshing = metadata_refresh_required(backend, &request.action);
            }
            send(Ok(snapshot), true, refreshing);
            if !refreshing {
                return;
            }
        }
        Err(message) if !refreshing => {
            send(Err(message), false, false);
            return;
        }
        Err(_) | Ok(None) => {}
    }
    if mailbox.metadata_superseded(&request.action) {
        return;
    }
    let result = backend.refresh_metadata(&request.action, &cancellation);
    if !mailbox.metadata_superseded(&request.action) {
        send(result, false, false);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    struct CompletionProbe(u64, mpsc::Sender<u64>);
    impl Drop for CompletionProbe {
        fn drop(&mut self) {
            let _ = self.1.send(self.0);
        }
    }
    struct ConcurrentBackend {
        started: mpsc::Sender<u64>,
        retired: mpsc::Sender<u64>,
        release: Arc<(Mutex<bool>, Condvar)>,
        persisted: Arc<Mutex<Vec<u64>>>,
    }
    impl Backend for ConcurrentBackend {
        fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
            Ok(None)
        }
        fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
            panic!("network must use fetch pool")
        }
        fn prepare(
            &mut self,
            work: &Work,
            _: &Arc<AtomicBool>,
        ) -> Option<Result<Fetch<Self>, String>> {
            let Work::Search(request) = work else {
                return None;
            };
            let generation = request.generation;
            let started = self.started.clone();
            let probe = CompletionProbe(generation, self.retired.clone());
            let release = Arc::clone(&self.release);
            let persisted = Arc::clone(&self.persisted);
            Some(Ok(Box::new(move || {
                assert!(
                    std::thread::current()
                        .name()
                        .is_some_and(|name| name.starts_with("kinema-fetch-"))
                );
                started.send(generation).expect("started receiver");
                if generation == 1 {
                    let (lock, wake) = &*release;
                    let guard = lock.lock().expect("gate lock");
                    let _ = wake
                        .wait_timeout_while(guard, Duration::from_secs(5), |open| !*open)
                        .expect("gate wait");
                }
                Box::new(move |_: &mut Self| {
                    let _probe = probe;
                    assert_eq!(std::thread::current().name(), Some("kinema-catalog"));
                    persisted.lock().expect("persisted lock").push(generation);
                    Ok(Refreshed::Search(Snapshot {
                        titles: vec![],
                        fetched_at: generation,
                    }))
                })
            })))
        }
    }

    type ConcurrentFixture = (
        CatalogWorker,
        mpsc::Receiver<u64>,
        mpsc::Receiver<u64>,
        Arc<(Mutex<bool>, Condvar)>,
        Arc<Mutex<Vec<u64>>>,
    );
    fn concurrent_worker() -> ConcurrentFixture {
        let (started, receiver) = mpsc::channel();
        let (retired, completions) = mpsc::channel();
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let persisted = Arc::new(Mutex::new(vec![]));
        let gate = Arc::clone(&release);
        let saved = Arc::clone(&persisted);
        let worker = CatalogWorker::spawn(move || {
            Ok(ConcurrentBackend {
                started: started.clone(),
                retired: retired.clone(),
                release: Arc::clone(&gate),
                persisted: Arc::clone(&saved),
            })
        })
        .expect("spawn");
        (worker, receiver, completions, release, persisted)
    }
    fn release_fetch(gate: &Arc<(Mutex<bool>, Condvar)>) {
        *gate.0.lock().expect("gate lock") = true;
        gate.1.notify_all();
    }
    fn wait_fetched(worker: &CatalogWorker, generation: u64) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Poll::Event(Event {
                generation: actual,
                outcome: Outcome::Fetched(_),
                ..
            }) = worker.poll()
                && actual == generation
            {
                return;
            }
            assert!(Instant::now() < deadline, "fetch did not finish");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn concurrent_request(kind: MediaKind, generation: u64) -> Request {
        Request {
            generation,
            query: SearchQuery::new(kind, format!("title {generation}")).expect("query"),
            mode: Mode::Refresh,
        }
    }

    #[test]
    fn catalog_diagnostics_omit_raw_search_and_measure_queue_wait() {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "catalog_diagnostics_omit_raw_search_and_measure_queue_wait",
        ) {
            return;
        }

        use crate::providers::test_logging::{capture, excludes};
        let mailbox = Mailbox::default();
        let request = Request {
            generation: 44,
            query: SearchQuery::new(
                MediaKind::Movie,
                "https://private.invalid/?token=search-secret",
            )
            .unwrap_or_else(|_| panic!("fixture query")),
            mode: Mode::Refresh,
        };
        let (_, output) = capture(|| {
            mailbox.submit(request);
            std::thread::sleep(Duration::from_millis(2));
            let Some(Ready::Work(work, waited)) = mailbox.next::<ConcurrentBackend>(0) else {
                panic!("queued work expected")
            };
            assert!(waited >= Duration::from_millis(1));
            let _span = work.diagnostic_span().entered();
            tracing::debug!(
                queue_wait_ms = waited.as_millis() as u64,
                "Catalog diagnostic fixture"
            );
            send_terminal(
                &mailbox,
                &EventBox::default(),
                &MetadataEventBox::default(),
                &work,
                &AtomicBool::new(false),
                Err("backend-url-secret?token=error-secret".into()),
            );
        });
        assert!(output.contains("search"));
        assert!(output.contains("queue_wait_ms"));
        excludes(
            &output,
            &[
                "private.invalid",
                "search-secret",
                "backend-url-secret",
                "error-secret",
                "token=",
            ],
        );
    }

    #[test]
    fn completed_fetch_is_served_before_queued_preparation() {
        let mailbox = Mailbox::default();
        let request = concurrent_request(MediaKind::Movie, 2);
        mailbox.submit(request.clone());
        mailbox
            .lock()
            .completed
            .push_back(Box::new(Completed::<ConcurrentBackend> {
                work: Work::Search(concurrent_request(MediaKind::Series, 1)),
                cancel: Arc::new(AtomicBool::new(false)),
                persist: Box::new(|_| {
                    Ok(Refreshed::Search(Snapshot {
                        titles: vec![],
                        fetched_at: 1,
                    }))
                }),
            }));
        assert!(matches!(
            mailbox.next::<ConcurrentBackend>(0),
            Some(Ready::Completed(_))
        ));
        assert!(
            matches!(mailbox.next::<ConcurrentBackend>(0), Some(Ready::Work(Work::Search(actual), _)) if actual == request)
        );
    }

    #[test]
    fn fetches_in_two_slots_overlap() {
        let (worker, started, _retired, gate, persisted) = concurrent_worker();
        worker.submit(concurrent_request(MediaKind::Movie, 1));
        assert_eq!(
            started
                .recv_timeout(Duration::from_secs(2))
                .expect("first start"),
            1
        );
        worker.submit(concurrent_request(MediaKind::Series, 2));
        assert_eq!(
            started
                .recv_timeout(Duration::from_secs(2))
                .expect("second start before release"),
            2
        );
        wait_fetched(&worker, 2);
        assert_eq!(*persisted.lock().expect("persisted"), vec![2]);
        release_fetch(&gate);
        wait_fetched(&worker, 1);
        assert_eq!(*persisted.lock().expect("persisted"), vec![2, 1]);
    }

    #[test]
    fn replacement_fetch_starts_before_obsolete_fetch_finishes_and_never_persists_it() {
        let (worker, started, retired, gate, persisted) = concurrent_worker();
        worker.submit(concurrent_request(MediaKind::Movie, 1));
        assert_eq!(
            started
                .recv_timeout(Duration::from_secs(2))
                .expect("first start"),
            1
        );
        worker.submit(concurrent_request(MediaKind::Movie, 2));
        assert_eq!(
            started
                .recv_timeout(Duration::from_secs(2))
                .expect("replacement before release"),
            2
        );
        wait_fetched(&worker, 2);
        release_fetch(&gate);
        while retired
            .recv_timeout(Duration::from_secs(2))
            .expect("completion dropped")
            != 1
        {}
        // The obsolete persist closure has been dropped, never executed.
        worker.submit(concurrent_request(MediaKind::Movie, 3));
        wait_fetched(&worker, 3);
        assert_eq!(*persisted.lock().expect("persisted"), vec![2, 3]);
    }

    #[test]
    fn shutdown_does_not_wait_for_fetch_pool() {
        let (worker, started, retired, gate, persisted) = concurrent_worker();
        worker.submit(concurrent_request(MediaKind::Movie, 1));
        started
            .recv_timeout(Duration::from_secs(2))
            .expect("started");
        let start = Instant::now();
        drop(worker);
        assert!(start.elapsed() < Duration::from_millis(100));
        release_fetch(&gate);
        assert_eq!(
            retired
                .recv_timeout(Duration::from_secs(2))
                .expect("discarded after shutdown"),
            1
        );
        assert!(persisted.lock().expect("persisted").is_empty());
    }

    #[test]
    fn exact_cancellation_discards_fetch_without_persisting() {
        let (worker, started, retired, gate, persisted) = concurrent_worker();
        worker.submit(concurrent_request(MediaKind::Movie, 1));
        started
            .recv_timeout(Duration::from_secs(2))
            .expect("started");
        worker.cancel_search(MediaKind::Movie, 1);
        release_fetch(&gate);
        assert_eq!(
            retired
                .recv_timeout(Duration::from_secs(2))
                .expect("discarded completion"),
            1
        );
        assert!(persisted.lock().expect("persisted").is_empty());
        worker.submit(concurrent_request(MediaKind::Movie, 2));
        wait_fetched(&worker, 2);
        assert_eq!(*persisted.lock().expect("persisted"), vec![2]);
    }

    use std::{
        sync::{Mutex, mpsc},
        time::{Duration, Instant},
    };

    use super::*;

    struct MetadataBackend {
        saved: Result<Option<MetadataSnapshot>, String>,
        remote: Result<MetadataSnapshot, String>,
        refreshes: Arc<std::sync::atomic::AtomicUsize>,
        obsolete_on_read: Option<(Arc<Mailbox>, MetadataRequest)>,
    }

    impl Backend for MetadataBackend {
        fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
            Ok(None)
        }
        fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
            Err("not a search test".into())
        }
        fn read_metadata(
            &mut self,
            _: &MetadataAction,
        ) -> Result<Option<MetadataSnapshot>, String> {
            if let Some((mailbox, request)) = self.obsolete_on_read.take() {
                mailbox.submit_metadata(request);
            }
            self.saved.clone()
        }
        fn refresh_metadata(
            &mut self,
            _: &MetadataAction,
            _: &Arc<AtomicBool>,
        ) -> Result<MetadataSnapshot, String> {
            self.refreshes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.remote.clone()
        }
    }

    fn browse_request(generation: u64, offset: u32, mode: Mode) -> MetadataRequest {
        MetadataRequest {
            generation,
            action: MetadataAction::Browse {
                kind: MediaKind::Movie,
                offset,
            },
            mode,
        }
    }

    fn empty_browse() -> MetadataSnapshot {
        MetadataSnapshot::Browse(CachedSearch {
            titles: Vec::new(),
            fetched_at: 123,
        })
    }

    #[test]
    fn metadata_saved_empty_load_is_terminal_without_remote() {
        let mailbox = Mailbox::default();
        let events = MetadataEventBox::default();
        let refreshes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut backend = Some(MetadataBackend {
            saved: Ok(Some(empty_browse())),
            remote: Err("offline".into()),
            refreshes: Arc::clone(&refreshes),
            obsolete_on_read: None,
        });
        process_metadata(
            &mailbox,
            &events,
            &mut backend,
            &mut || Err("not opened".into()),
            browse_request(1, 0, Mode::Load),
        );
        let event = events.pop().expect("saved event");
        assert_eq!(event.result, Ok(empty_browse()));
        assert!(event.saved);
        assert!(!event.refreshing);
        assert!(events.pop().is_none());
        assert_eq!(refreshes.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn metadata_refresh_emits_saved_then_failure_without_erasing_success() {
        let mailbox = Mailbox::default();
        let events = MetadataEventBox::default();
        let mut backend = Some(MetadataBackend {
            saved: Ok(Some(empty_browse())),
            remote: Err("offline".into()),
            refreshes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            obsolete_on_read: None,
        });
        process_metadata(
            &mailbox,
            &events,
            &mut backend,
            &mut || Err("not opened".into()),
            browse_request(5, 0, Mode::Refresh),
        );
        let saved = events.pop().expect("saved");
        assert!(saved.saved && saved.refreshing);
        assert_eq!(saved.result, Ok(empty_browse()));
        let failure = events.pop().expect("failure");
        assert_eq!(failure.result, Err("offline".into()));
        assert!(!failure.saved && !failure.refreshing);
        assert!(events.pop().is_none());
    }

    #[test]
    fn metadata_refresh_bypasses_cache_error_but_load_does_not() {
        for mode in [Mode::Load, Mode::Refresh] {
            let mailbox = Mailbox::default();
            let events = MetadataEventBox::default();
            let refreshes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut backend = Some(MetadataBackend {
                saved: Err("cache failed".into()),
                remote: Ok(empty_browse()),
                refreshes: Arc::clone(&refreshes),
                obsolete_on_read: None,
            });
            process_metadata(
                &mailbox,
                &events,
                &mut backend,
                &mut || Err("not opened".into()),
                browse_request(1, 0, mode),
            );
            let event = events.pop().expect("terminal");
            assert_eq!(
                event.result,
                if mode == Mode::Load {
                    Err("cache failed".into())
                } else {
                    Ok(empty_browse())
                }
            );
            assert_eq!(
                refreshes.load(std::sync::atomic::Ordering::SeqCst),
                usize::from(mode == Mode::Refresh)
            );
        }
    }

    #[test]
    fn obsolete_metadata_queued_during_read_skips_remote() {
        let mailbox = Arc::new(Mailbox::default());
        let events = MetadataEventBox::default();
        let refreshes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut backend = Some(MetadataBackend {
            saved: Ok(None),
            remote: Ok(empty_browse()),
            refreshes: Arc::clone(&refreshes),
            obsolete_on_read: Some((Arc::clone(&mailbox), browse_request(2, 100, Mode::Load))),
        });
        process_metadata(
            &mailbox,
            &events,
            &mut backend,
            &mut || Err("not opened".into()),
            browse_request(1, 0, Mode::Refresh),
        );
        assert!(events.pop().is_none());
        assert_eq!(refreshes.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(matches!(
            mailbox.take_work(),
            Some(Work::Metadata(MetadataRequest { generation: 2, .. }))
        ));
    }

    #[test]
    fn metadata_mailbox_coalesces_replaces_and_is_fair_with_search() {
        let mailbox = Mailbox::default();
        assert_eq!(
            mailbox.submit_metadata(browse_request(1, 0, Mode::Refresh)),
            Submit::Queued
        );
        assert_eq!(
            mailbox.submit_metadata(browse_request(2, 0, Mode::Load)),
            Submit::Coalesced
        );
        assert_eq!(
            mailbox.submit(req(1, MediaKind::Movie, "movie", Mode::Load)),
            Submit::Queued
        );
        assert!(matches!(mailbox.take_work(), Some(Work::Search(_))));
        // Replenishing search cannot starve the waiting metadata slot.
        mailbox.submit(req(2, MediaKind::Movie, "new", Mode::Load));
        let Some(Work::Metadata(request)) = mailbox.take_work() else {
            panic!("metadata starved")
        };
        assert_eq!(request.generation, 2);
        assert_eq!(request.mode, Mode::Refresh);
        mailbox.submit_metadata(browse_request(3, 0, Mode::Load));
        assert_eq!(
            mailbox.submit_metadata(browse_request(4, 100, Mode::Load)),
            Submit::Replaced
        );
        mailbox.close();
        assert_eq!(
            mailbox.submit_metadata(browse_request(5, 0, Mode::Load)),
            Submit::Closed
        );
    }

    #[test]
    fn owned_worker_routes_metadata_requests_and_events() {
        let mut backend = Some(MetadataBackend {
            saved: Ok(None),
            remote: Ok(empty_browse()),
            refreshes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            obsolete_on_read: None,
        });
        let worker =
            CatalogWorker::spawn(move || backend.take().ok_or_else(|| "opened twice".into()))
                .expect("spawn");
        assert_eq!(
            worker.submit_metadata(browse_request(7, 0, Mode::Load)),
            Submit::Queued
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let event = loop {
            if let Some(event) = worker.poll_metadata() {
                break event;
            }
            assert!(Instant::now() < deadline, "metadata worker stalled");
            std::thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(event.generation, 7);
        assert_eq!(event.result, Ok(empty_browse()));
        assert!(!event.saved && !event.refreshing);
        assert!(matches!(worker.poll(), Poll::Empty));
    }

    #[test]
    fn tmdb_work_is_bounded_separate_coalesced_and_cancellable() {
        let mailbox = Mailbox::default();
        let request = |generation, page, mode| MetadataRequest {
            generation,
            action: MetadataAction::Tmdb(TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page,
            }),
            mode,
        };
        assert_eq!(
            mailbox.submit_metadata(browse_request(1, 0, Mode::Load)),
            Submit::Queued
        );
        assert_eq!(
            mailbox.submit_metadata(request(1, 1, Mode::Refresh)),
            Submit::Queued
        );
        assert_eq!(
            mailbox.submit_metadata(request(2, 1, Mode::Load)),
            Submit::Coalesced
        );
        assert!(matches!(
            mailbox.take_work(),
            Some(Work::Metadata(MetadataRequest {
                action: MetadataAction::Browse { .. },
                ..
            }))
        ));
        let Some(Work::Metadata(pending)) = mailbox.take_work() else {
            panic!("TMDB missing")
        };
        assert_eq!(pending.generation, 2);
        assert_eq!(pending.mode, Mode::Refresh);
        mailbox.submit_metadata(request(3, 1, Mode::Load));
        assert_eq!(
            mailbox.submit_metadata(request(4, 2, Mode::Load)),
            Submit::Replaced
        );
        assert!(mailbox.metadata_superseded(&request(3, 1, Mode::Load).action));
        let Some(Work::Metadata(pending)) = mailbox.take_work() else {
            panic!("replacement missing")
        };
        assert_eq!(pending.action, request(4, 2, Mode::Load).action);
    }

    #[test]
    fn tmdb_metadata_refresh_keeps_saved_data_when_authentication_fails() {
        let mailbox = Mailbox::default();
        let events = MetadataEventBox::default();
        let snapshot = MetadataSnapshot::Tmdb(CachedTmdb {
            data: crate::tmdb::TmdbData::Page(crate::tmdb::TmdbPage {
                titles: Vec::new(),
                page: 1,
                total_pages: 0,
            }),
            fetched_at: 123,
        });
        let mut backend = Some(MetadataBackend {
            saved: Ok(Some(snapshot.clone())),
            remote: Err("TMDB credential is missing.".into()),
            refreshes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            obsolete_on_read: None,
        });
        let request = MetadataRequest {
            generation: 8,
            action: MetadataAction::Tmdb(TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page: 1,
            }),
            mode: Mode::Refresh,
        };
        process_metadata(
            &mailbox,
            &events,
            &mut backend,
            &mut || Err("not opened".into()),
            request,
        );
        let saved = events.pop().expect("saved");
        assert_eq!(saved.result, Ok(snapshot));
        assert!(saved.saved && saved.refreshing);
        let failure = events.pop().expect("auth failure");
        assert!(failure.result.is_err());
        assert!(!failure.saved && !failure.refreshing);
    }

    #[test]
    fn search_replacement_exact_cancel_and_shutdown_signal_active_flags() {
        let mailbox = Mailbox::default();
        mailbox.submit(req(1, MediaKind::Movie, "old", Mode::Refresh));
        let _ = mailbox.take_work();
        let old = Arc::clone(
            &mailbox.lock().active_search[0]
                .as_ref()
                .unwrap_or_else(|| panic!("missing active search"))
                .cancel,
        );
        mailbox.submit(req(2, MediaKind::Movie, "new", Mode::Refresh));
        assert!(old.load(Ordering::Acquire));
        let _ = mailbox.take_work();
        let new = Arc::clone(
            &mailbox.lock().active_search[0]
                .as_ref()
                .unwrap_or_else(|| panic!("missing new search"))
                .cancel,
        );
        mailbox.cancel_search(MediaKind::Movie, 1);
        assert!(!new.load(Ordering::Acquire));
        mailbox.cancel_search(MediaKind::Movie, 2);
        assert!(new.load(Ordering::Acquire));
        mailbox.submit(req(3, MediaKind::Series, "series", Mode::Load));
        let _ = mailbox.take_work();
        let series = Arc::clone(
            &mailbox.lock().active_search[1]
                .as_ref()
                .unwrap_or_else(|| panic!("missing series search"))
                .cancel,
        );
        mailbox.close();
        assert!(series.load(Ordering::Acquire));
    }

    #[test]
    fn cancel_removes_only_matching_queued_metadata_generation() {
        let mailbox = Mailbox::default();
        let old = browse_request(1, 0, Mode::Load);
        let newer = browse_request(2, 100, Mode::Load);
        mailbox.submit_metadata(old.clone());
        mailbox.cancel_metadata(&old.action, old.generation);
        assert!(mailbox.lock().metadata.iter().all(Option::is_none));
        mailbox.submit_metadata(newer.clone());
        mailbox.cancel_metadata(&old.action, old.generation);
        assert_eq!(
            mailbox.lock().metadata[metadata_slot(&newer.action)],
            Some(newer)
        );
    }

    #[test]
    fn metadata_replacement_and_shutdown_signal_active_cancellation() {
        let mailbox = Mailbox::default();
        let old = browse_request(1, 0, Mode::Load);
        mailbox.submit_metadata(old.clone());
        assert!(matches!(mailbox.take_work(), Some(Work::Metadata(_))));
        let flag = mailbox.metadata_cancellation(&old);
        assert!(!flag.load(Ordering::Acquire));
        let newer = browse_request(2, 100, Mode::Load);
        mailbox.submit_metadata(newer.clone());
        assert!(flag.load(Ordering::Acquire));
        mailbox.cancel_metadata(&old.action, old.generation);
        assert_eq!(
            mailbox.lock().metadata[metadata_slot(&newer.action)],
            Some(newer.clone())
        );
        mailbox.finish_metadata(&old);
        assert!(matches!(mailbox.take_work(), Some(Work::Metadata(_))));
        let newer_flag = mailbox.metadata_cancellation(&newer);
        assert!(!newer_flag.load(Ordering::Acquire));
        mailbox.close();
        assert!(newer_flag.load(Ordering::Acquire));
    }

    #[test]
    fn canceled_inflight_success_is_not_forwarded() {
        struct CancelingBackend {
            mailbox: Arc<Mailbox>,
            request: MetadataRequest,
        }
        impl Backend for CancelingBackend {
            fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
                Ok(None)
            }
            fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
                Err("not a search".into())
            }
            fn read_metadata(
                &mut self,
                _: &MetadataAction,
            ) -> Result<Option<MetadataSnapshot>, String> {
                Ok(None)
            }
            fn refresh_metadata(
                &mut self,
                _: &MetadataAction,
                cancel: &Arc<AtomicBool>,
            ) -> Result<MetadataSnapshot, String> {
                self.mailbox
                    .cancel_metadata(&self.request.action, self.request.generation);
                assert!(cancel.load(Ordering::Acquire));
                // Even a backend that returns a success after cancellation must
                // not forward that obsolete result to presentation.
                Ok(empty_browse())
            }
        }
        let mailbox = Arc::new(Mailbox::default());
        let events = MetadataEventBox::default();
        let request = browse_request(4, 0, Mode::Refresh);
        let mut backend = Some(CancelingBackend {
            mailbox: Arc::clone(&mailbox),
            request: request.clone(),
        });
        process_metadata(
            &mailbox,
            &events,
            &mut backend,
            &mut || Err("not opened".into()),
            request,
        );
        assert!(events.pop().is_none());
        assert!(mailbox.lock().active.iter().all(Option::is_none));
    }

    #[test]
    fn metadata_cancel_purges_already_queued_events_without_other_generations() {
        let events = MetadataEventBox::default();
        let action = browse_request(1, 0, Mode::Load).action;
        events.push(MetadataEvent {
            generation: 1,
            action: action.clone(),
            result: Ok(empty_browse()),
            saved: true,
            refreshing: false,
        });
        events.cancel(&action, 2);
        assert_eq!(
            events.pop().expect("other generation retained").generation,
            1
        );
        events.push(MetadataEvent {
            generation: 1,
            action: action.clone(),
            result: Ok(empty_browse()),
            saved: true,
            refreshing: false,
        });
        events.cancel(&action, 1);
        assert!(events.pop().is_none());
    }

    #[test]
    fn metadata_events_are_bounded_and_discard_obsolete_actions() {
        let events = MetadataEventBox::default();
        for generation in 0..1000 {
            for saved in [true, false] {
                events.push(MetadataEvent {
                    generation,
                    action: browse_request(generation, generation as u32, Mode::Load).action,
                    result: Ok(empty_browse()),
                    saved,
                    refreshing: saved,
                });
            }
        }
        assert_eq!(events.pop().expect("saved").generation, 999);
        assert_eq!(events.pop().expect("terminal").generation, 999);
        assert!(events.pop().is_none());
    }

    fn query(kind: MediaKind, text: &str) -> SearchQuery {
        SearchQuery::new(kind, text).expect("valid query")
    }

    fn req(generation: u64, kind: MediaKind, text: &str, mode: Mode) -> Request {
        Request {
            generation,
            query: query(kind, text),
            mode,
        }
    }

    #[test]
    fn mailbox_replaces_obsolete_and_coalesces_duplicates() {
        let mailbox = Mailbox::default();
        assert_eq!(
            mailbox.submit(req(1, MediaKind::Movie, "a", Mode::Load)),
            Submit::Queued
        );
        assert_eq!(
            mailbox.submit(req(2, MediaKind::Movie, "a", Mode::Refresh)),
            Submit::Coalesced
        );
        assert_eq!(
            mailbox.submit(req(3, MediaKind::Series, "a", Mode::Load)),
            Submit::Queued
        );
        assert_eq!(
            mailbox.submit(req(4, MediaKind::Movie, "b", Mode::Load)),
            Submit::Replaced
        );
        let first = mailbox.take().expect("movie");
        assert_eq!((first.generation, first.query.text()), (4, "b"));
        let second = mailbox.take().expect("series");
        assert_eq!(second.generation, 3);
    }

    #[test]
    fn coalesced_refresh_keeps_newest_generation_and_strongest_mode() {
        let mailbox = Mailbox::default();
        mailbox.submit(req(1, MediaKind::Movie, "a", Mode::Refresh));
        mailbox.submit(req(2, MediaKind::Movie, "a", Mode::Load));
        let pending = mailbox.take().expect("pending");
        assert_eq!((pending.generation, pending.mode), (2, Mode::Refresh));
    }

    #[test]
    fn closed_mailbox_rejects_and_ends_take() {
        let mailbox = Mailbox::default();
        mailbox.close();
        assert_eq!(
            mailbox.submit(req(1, MediaKind::Movie, "a", Mode::Load)),
            Submit::Closed
        );
        assert!(mailbox.take().is_none());
    }

    #[derive(Default)]
    struct Fake {
        /// (read started, release read): blocks the first read only.
        gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
        cached: Option<Snapshot>,
        fail_read: bool,
        remote: Option<Result<Snapshot, String>>,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Backend for Fake {
        fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
            self.calls.lock().expect("lock").push("read");
            if let Some((started, release)) = self.gate.take() {
                let _ = started.send(());
                let _ = release.recv();
            }
            if self.fail_read {
                return Err("db".into());
            }
            Ok(self.cached.clone())
        }
        fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
            self.calls.lock().expect("lock").push("refresh");
            self.remote
                .clone()
                .unwrap_or_else(|| Err("no remote".into()))
        }
    }

    fn once(fake: Fake) -> impl FnMut() -> Result<Fake, String> + Send + 'static {
        let mut slot = Some(fake);
        move || slot.take().ok_or_else(|| "opened twice".to_string())
    }

    fn snap(at: u64) -> Snapshot {
        Snapshot {
            titles: Vec::new(),
            fetched_at: at,
        }
    }

    fn next(worker: &CatalogWorker) -> Event {
        for _ in 0..200 {
            if let Poll::Event(event) = worker.poll() {
                return event;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("worker produced no event");
    }

    #[test]
    fn empty_saved_snapshot_is_a_hit_without_remote_fetch() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            cached: Some(snap(7)),
            calls: Arc::clone(&calls),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(fake)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Load));
        assert_eq!(
            next(&worker).outcome,
            Outcome::CacheHit {
                snapshot: snap(7),
                remote_follows: false
            }
        );
        drop(worker);
        assert_eq!(*calls.lock().expect("lock"), ["read"]);
    }

    #[test]
    fn refresh_surfaces_cache_before_remote_and_miss_fetches() {
        let fake = Fake {
            cached: Some(snap(1)),
            remote: Some(Ok(snap(2))),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(fake)).expect("spawn");
        worker.submit(req(5, MediaKind::Series, "a", Mode::Refresh));
        assert!(matches!(
            next(&worker).outcome,
            Outcome::CacheHit {
                remote_follows: true,
                ..
            }
        ));
        let fetched = next(&worker);
        assert_eq!(
            (fetched.generation, fetched.outcome),
            (5, Outcome::Fetched(snap(2)))
        );

        let miss = Fake {
            remote: Some(Ok(snap(3))),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(miss)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Load));
        assert_eq!(next(&worker).outcome, Outcome::CacheMiss);
        assert_eq!(next(&worker).outcome, Outcome::Fetched(snap(3)));
    }

    #[test]
    fn startup_read_and_remote_errors_surface_as_failures() {
        let worker =
            CatalogWorker::spawn(|| Err::<Fake, _>("cannot open db".to_string())).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Load));
        assert_eq!(
            next(&worker).outcome,
            Outcome::Failed("cannot open db".into())
        );

        let bad_read = Fake {
            fail_read: true,
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(bad_read)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Load));
        assert_eq!(next(&worker).outcome, Outcome::Failed("db".into()));

        let down = Fake {
            cached: Some(snap(1)),
            remote: Some(Err("offline".into())),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(down)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Refresh));
        assert!(matches!(next(&worker).outcome, Outcome::CacheHit { .. }));
        assert_eq!(next(&worker).outcome, Outcome::Failed("offline".into()));
    }

    #[test]
    fn event_box_is_bounded_and_keeps_latest_terminal_event() {
        let events = EventBox::default();
        for generation in 0..50 {
            for outcome in [
                Outcome::CacheMiss,
                Outcome::Failed(format!("e{generation}")),
            ] {
                events.push(Event {
                    kind: MediaKind::Movie,
                    generation,
                    outcome,
                });
            }
        }
        assert_eq!(events.len(), EVENTS_PER_KIND);
        assert_eq!(events.pop().expect("miss").outcome, Outcome::CacheMiss);
        let last = events.pop().expect("terminal");
        assert_eq!(
            (last.generation, last.outcome),
            (49, Outcome::Failed("e49".into()))
        );
        assert!(events.pop().is_none());
    }

    #[test]
    fn stalled_receiver_never_blocks_worker_and_drop_does_not_hang() {
        let fake = Fake {
            cached: Some(snap(1)),
            remote: Some(Ok(snap(2))),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(fake)).expect("spawn");
        for generation in 1..=200 {
            let mode = Mode::Refresh;
            let kind = if generation % 2 == 0 {
                MediaKind::Movie
            } else {
                MediaKind::Series
            };
            worker.submit(req(generation, kind, &format!("q{generation}"), mode));
            if generation % 20 == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        std::thread::sleep(Duration::from_millis(50));
        assert!(worker.events.len() <= 2 * EVENTS_PER_KIND);
        let started = Instant::now();
        drop(worker);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn obsolete_request_queued_during_read_skips_remote_fetch() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            gate: Some((started_tx, release_rx)),
            remote: Some(Ok(snap(9))),
            calls: Arc::clone(&calls),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(fake)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "old", Mode::Refresh));
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("read started");
        assert_eq!(
            worker.submit(req(2, MediaKind::Movie, "new", Mode::Load)),
            Submit::Queued
        );
        release_tx.send(()).expect("release");
        let mut last = next(&worker);
        while last.generation != 2 || !matches!(last.outcome, Outcome::Fetched(_)) {
            last = next(&worker);
        }
        // read(old), read(new), refresh(new): the old request never fetched.
        assert_eq!(*calls.lock().expect("lock"), ["read", "read", "refresh"]);
    }

    #[test]
    fn stalled_ui_still_gets_cache_hit_then_failure_for_latest_generation() {
        let events = EventBox::default();
        let push = |generation, outcome| {
            events.push(Event {
                kind: MediaKind::Movie,
                generation,
                outcome,
            });
        };
        push(1, Outcome::Fetched(snap(1)));
        push(
            2,
            Outcome::CacheHit {
                snapshot: snap(5),
                remote_follows: true,
            },
        );
        push(2, Outcome::Failed("offline".into()));
        let first = events.pop().expect("hit");
        assert!(matches!(first.outcome, Outcome::CacheHit { .. }));
        assert_eq!(first.generation, 2);
        assert_eq!(
            events.pop().expect("failure").outcome,
            Outcome::Failed("offline".into())
        );
        assert!(events.pop().is_none());
    }

    #[test]
    fn refresh_with_failed_read_still_fetches_remote() {
        let fake = Fake {
            fail_read: true,
            remote: Some(Ok(snap(4))),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(fake)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Refresh));
        assert_eq!(next(&worker).outcome, Outcome::CacheMiss);
        assert_eq!(next(&worker).outcome, Outcome::Fetched(snap(4)));
    }

    #[test]
    fn refresh_with_failed_read_and_remote_reports_remote_error() {
        let fake = Fake {
            fail_read: true,
            remote: Some(Err("offline".into())),
            ..Fake::default()
        };
        let worker = CatalogWorker::spawn(once(fake)).expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Refresh));
        assert_eq!(next(&worker).outcome, Outcome::CacheMiss);
        assert_eq!(next(&worker).outcome, Outcome::Failed("offline".into()));
    }

    #[test]
    fn open_failure_is_retried_on_the_next_request() {
        let mut attempts = 0;
        let worker = CatalogWorker::spawn(move || {
            attempts += 1;
            if attempts == 1 {
                Err("disk busy".to_string())
            } else {
                Ok(Fake {
                    remote: Some(Ok(snap(8))),
                    ..Fake::default()
                })
            }
        })
        .expect("spawn");
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Load));
        assert_eq!(next(&worker).outcome, Outcome::Failed("disk busy".into()));
        worker.submit(req(2, MediaKind::Movie, "a", Mode::Load));
        assert_eq!(next(&worker).outcome, Outcome::CacheMiss);
        assert_eq!(next(&worker).outcome, Outcome::Fetched(snap(8)));
    }

    #[test]
    fn panicking_worker_disconnects_and_closes_submit() {
        struct Boom;
        impl Backend for Boom {
            fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
                panic!("boom");
            }
            fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
                panic!("boom");
            }
        }
        let worker = CatalogWorker::spawn(|| Ok(Boom)).expect("spawn");
        assert_eq!(worker.poll(), Poll::Empty);
        worker.submit(req(1, MediaKind::Movie, "a", Mode::Load));
        let deadline = Instant::now() + Duration::from_secs(2);
        while worker.poll() != Poll::Disconnected {
            assert!(Instant::now() < deadline, "never disconnected");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            worker.submit(req(2, MediaKind::Movie, "a", Mode::Load)),
            Submit::Closed
        );
    }

    #[test]
    fn mailbox_alternates_between_kinds() {
        let mailbox = Mailbox::default();
        mailbox.submit(req(1, MediaKind::Series, "s", Mode::Load));
        mailbox.submit(req(2, MediaKind::Movie, "m", Mode::Load));
        assert_eq!(mailbox.take().expect("first").generation, 2);
        mailbox.submit(req(3, MediaKind::Movie, "m2", Mode::Load));
        assert_eq!(mailbox.take().expect("second").generation, 1);
        assert_eq!(mailbox.take().expect("third").generation, 3);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod facts_refresh_tests {
    use super::*;
    use kinema_core::catalog::{ImdbRating, TitleDetails, TitleFacts};

    struct Fixture {
        saved: Option<MetadataSnapshot>,
        remote: Result<MetadataSnapshot, String>,
        needs_refresh: Result<bool, String>,
        probes: usize,
        refreshes: usize,
        on_probe: Option<Box<dyn FnOnce()>>,
    }
    impl Backend for Fixture {
        fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
            Ok(None)
        }
        fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
            Err("not search".into())
        }
        fn read_metadata(
            &mut self,
            _: &MetadataAction,
        ) -> Result<Option<MetadataSnapshot>, String> {
            Ok(self.saved.clone())
        }
        fn metadata_needs_refresh(&mut self, _: &MetadataAction) -> Result<bool, String> {
            self.probes += 1;
            if let Some(callback) = self.on_probe.take() {
                callback();
            }
            self.needs_refresh.clone()
        }
        fn refresh_metadata(
            &mut self,
            _: &MetadataAction,
            _: &Arc<AtomicBool>,
        ) -> Result<MetadataSnapshot, String> {
            self.refreshes += 1;
            let result = self.remote.clone();
            if let Ok(snapshot) = &result {
                self.saved = Some(snapshot.clone());
                self.needs_refresh = Ok(false);
            }
            result
        }
    }
    fn details(rating: Option<ImdbRating>, fetched_at: u64) -> MetadataSnapshot {
        MetadataSnapshot::Details(CachedDetails {
            details: TitleDetails {
                facts: TitleFacts {
                    imdb_rating: rating,
                    ..Default::default()
                },
                title: CatalogTitle::new(
                    ImdbId::new("tt1234567").expect("id"),
                    MediaKind::Movie,
                    "Fixture",
                    None,
                )
                .expect("title"),
                poster_url: None,
                background_url: None,
                release_info: None,
                genres: vec![],
                episodes: vec![],
            },
            fetched_at,
        })
    }
    fn request(generation: u64, mode: Mode) -> MetadataRequest {
        MetadataRequest {
            generation,
            action: MetadataAction::Details {
                kind: MediaKind::Movie,
                id: ImdbId::new("tt1234567").expect("id"),
            },
            mode,
        }
    }
    fn fixture(rating: Option<ImdbRating>) -> Fixture {
        Fixture {
            saved: Some(details(None, 1)),
            remote: Ok(details(rating, 2)),
            needs_refresh: Ok(true),
            probes: 0,
            refreshes: 0,
            on_probe: None,
        }
    }

    #[test]
    fn load_facts_cache_hit_then_fetch_then_cache_only_even_when_facts_are_empty() {
        for rating in [None, ImdbRating::new(8.1)] {
            let mailbox = Mailbox::default();
            let events = MetadataEventBox::default();
            let mut backend = Some(fixture(rating));
            process_metadata(
                &mailbox,
                &events,
                &mut backend,
                &mut || Err("already open".into()),
                request(7, Mode::Load),
            );
            let saved = events.pop().expect("saved");
            assert!(saved.saved && saved.refreshing);
            assert_eq!(saved.generation, 7);
            assert_eq!(saved.result, Ok(details(None, 1)));
            let fetched = events.pop().expect("fetched");
            assert!(!fetched.saved && !fetched.refreshing);
            assert_eq!(fetched.generation, 7);
            assert_eq!(fetched.result, Ok(details(rating, 2)));
            assert!(events.pop().is_none());
            process_metadata(
                &mailbox,
                &events,
                &mut backend,
                &mut || Err("already open".into()),
                request(8, Mode::Load),
            );
            let saved = events.pop().expect("acquired cache");
            assert!(saved.saved && !saved.refreshing);
            assert_eq!(saved.generation, 8);
            assert_eq!(saved.result, Ok(details(rating, 2)));
            assert!(events.pop().is_none());
            assert_eq!(backend.as_ref().expect("backend").refreshes, 1);
        }
    }

    #[test]
    fn production_cache_read_requests_remote_only_for_successful_load_hit_with_unacquired_facts() {
        for (mode, saved, needs_refresh, expected_fetch, expected_probes) in [
            (Mode::Load, Some(details(None, 1)), Ok(true), true, 1),
            (Mode::Load, Some(details(None, 1)), Ok(false), false, 1),
            (
                Mode::Load,
                Some(details(None, 1)),
                Err("predicate failed".into()),
                false,
                1,
            ),
            (Mode::Load, None, Ok(true), true, 0),
            (Mode::Refresh, Some(details(None, 1)), Ok(false), true, 0),
        ] {
            let mailbox = Mailbox::default();
            let events = MetadataEventBox::default();
            let req = request(7, mode);
            let cancel = mailbox.metadata_cancellation(&req);
            let mut backend = fixture(None);
            backend.saved = saved.clone();
            backend.needs_refresh = needs_refresh;
            assert_eq!(
                read_before_fetch(
                    &mailbox,
                    &EventBox::default(),
                    &events,
                    &mut backend,
                    &Work::Metadata(req),
                    &cancel
                ),
                expected_fetch
            );
            assert_eq!(backend.probes, expected_probes);
            if saved.is_some() {
                let cached = events.pop().expect("cache event");
                assert!(cached.saved);
                assert_eq!(cached.refreshing, expected_fetch);
                assert_eq!(cached.result, Ok(details(None, 1)));
            }
            assert!(events.pop().is_none());
        }
    }

    #[test]
    fn failed_predicate_is_cache_only_and_failed_remote_keeps_saved_acquisition_pending() {
        for predicate_failure in [true, false] {
            let mailbox = Mailbox::default();
            let events = MetadataEventBox::default();
            let mut value = fixture(None);
            if predicate_failure {
                value.needs_refresh = Err("predicate failed".into());
            }
            value.remote = Err("offline".into());
            let mut backend = Some(value);
            process_metadata(
                &mailbox,
                &events,
                &mut backend,
                &mut || Err("already open".into()),
                request(7, Mode::Load),
            );
            let saved = events.pop().expect("saved");
            assert!(saved.saved);
            assert_eq!(saved.refreshing, !predicate_failure);
            assert_eq!(saved.result, Ok(details(None, 1)));
            if !predicate_failure {
                assert_eq!(
                    events.pop().expect("remote failure").result,
                    Err("offline".into())
                );
            }
            assert!(events.pop().is_none());
            let value = backend.as_ref().expect("backend");
            assert_eq!(value.refreshes, usize::from(!predicate_failure));
            assert_eq!(value.saved, Some(details(None, 1)));
            assert_eq!(
                value.needs_refresh,
                if predicate_failure {
                    Err("predicate failed".into())
                } else {
                    Ok(true)
                }
            );
        }
    }

    #[test]
    fn cancellation_or_generation_replacement_during_probe_rejects_saved_and_remote() {
        for production in [true, false] {
            for replace in [true, false] {
                let mailbox = Arc::new(Mailbox::default());
                let events = MetadataEventBox::default();
                let req = request(7, Mode::Load);
                let cancel = mailbox.metadata_cancellation(&req);
                let owner = Arc::clone(&mailbox);
                let mut backend = fixture(None);
                backend.on_probe = Some(Box::new(move || {
                    if replace {
                        owner.submit_metadata(request(8, Mode::Load));
                    } else {
                        let old = request(7, Mode::Load);
                        owner.cancel_metadata(&old.action, old.generation);
                    }
                }));
                if production {
                    assert!(!read_before_fetch(
                        &mailbox,
                        &EventBox::default(),
                        &events,
                        &mut backend,
                        &Work::Metadata(req),
                        &cancel
                    ));
                    assert_eq!(backend.refreshes, 0);
                } else {
                    let mut backend = Some(backend);
                    process_metadata(
                        &mailbox,
                        &events,
                        &mut backend,
                        &mut || Err("already open".into()),
                        req,
                    );
                    assert_eq!(backend.as_ref().expect("backend").refreshes, 0);
                }
                assert!(cancel.load(Ordering::Acquire));
                assert!(events.pop().is_none());
            }
        }
    }

    #[test]
    fn acquisition_probe_logs_reason_but_never_backend_error_text() {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "acquisition_probe_logs_reason_but_never_backend_error_text",
        ) {
            return;
        }
        use crate::providers::test_logging::{capture, excludes};
        let mut backend = fixture(None);
        let (_, output) = capture(|| {
            assert!(metadata_refresh_required(
                &mut backend,
                &request(1, Mode::Load).action
            ));
            backend.needs_refresh = Err("https://secret.invalid/?token=error-secret".into());
            assert!(!metadata_refresh_required(
                &mut backend,
                &request(1, Mode::Load).action
            ));
        });
        assert!(output.contains("facts_unacquired"));
        assert!(output.contains("facts_acquisition_check_failed"));
        excludes(&output, &["secret.invalid", "error-secret", "token="]);
    }
}
