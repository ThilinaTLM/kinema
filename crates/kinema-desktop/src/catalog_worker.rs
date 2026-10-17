//! Single owned catalog worker thread and its bounded request mailbox.
//!
//! The backend is constructed and used only on the worker thread. The mailbox
//! holds two search slots and fifteen metadata operation/kind/indexer slots.
//! A newer request for a slot replaces queued obsolete work; identical actions
//! coalesce. A round-robin scheduler is fair across search and metadata.
//!
//! Limit: a blocking HTTP request already in flight cannot be cancelled. Its
//! duration is bounded only by the backend client timeout. Replacement/shutdown
//! flags prevent stale persistence before transaction entry and commit, in both
//! search and metadata lanes; UI generation checks additionally reject events. Dropping [`CatalogWorker`] signals
//! the thread and does not join it, so the UI never waits on that timeout; the
//! thread exits after the current backend call finishes.

use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
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
    /// Show saved results; fetch remotely only on a cache miss.
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
enum Work {
    Search(Request),
    Metadata(MetadataRequest),
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
    metadata: [Option<MetadataRequest>; METADATA_SLOTS],
    active: [Option<ActiveMetadata>; METADATA_SLOTS],
    active_search: [Option<ActiveSearch>; 2],
    /// Kind served first next, so one kind cannot starve the other.
    next: usize,
    closed: bool,
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
        let index = slot(request.query.kind());
        if let Some(active) = &state.active_search[index]
            && (active.generation != request.generation || active.query != request.query)
        {
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
        self.wake.notify_one();
        outcome
    }

    fn submit_metadata(&self, request: MetadataRequest) -> Submit {
        let mut state = self.lock();
        if state.closed {
            return Submit::Closed;
        }
        let index = metadata_slot(&request.action);
        if let Some(active) = &state.active[index]
            && (active.generation != request.generation || active.action != request.action)
        {
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
        self.wake.notify_one();
        result
    }

    /// Blocks until work is available; `None` once closed.
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

    #[cfg(test)]
    fn take(&self) -> Option<Request> {
        match self.take_work()? {
            Work::Search(request) => Some(request),
            Work::Metadata(_) => None,
        }
    }

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

    /// True when a newer request is queued for the kind, or shutdown began.
    fn superseded(&self, kind: MediaKind) -> bool {
        let state = self.lock();
        state.closed || state.slots[slot(kind)].is_some()
    }

    fn is_closed(&self) -> bool {
        self.lock().closed
    }

    fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        state.slots = Default::default();
        state.metadata = Default::default();
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

/// Blocking catalog operations; errors are already user-safe messages.
pub(crate) trait Backend {
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
    fn refresh_metadata(
        &mut self,
        _action: &MetadataAction,
        _cancel: &Arc<AtomicBool>,
    ) -> Result<MetadataSnapshot, String> {
        Err("Metadata is unavailable from this backend.".into())
    }
}

fn snapshot(search: CachedSearch) -> Snapshot {
    Snapshot {
        titles: search.titles,
        fetched_at: search.fetched_at,
    }
}

impl Backend for CatalogService {
    fn read(&mut self, query: &SearchQuery) -> Result<Option<Snapshot>, String> {
        CatalogService::read(self, query)
            .map(|found| found.map(snapshot))
            .map_err(|error| error.to_string())
    }

    fn refresh(&mut self, query: &SearchQuery) -> Result<Snapshot, String> {
        CatalogService::refresh(self, query)
            .map(snapshot)
            .map_err(|error| error.to_string())
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
        .map_err(|error| error.to_string())
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
        result.map_err(|error| error.to_string())
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
        Self::spawn(|| CatalogService::open_default().map_err(|error| error.to_string()))
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
        self.mailbox.close();
    }
}

/// `open` is retried on the next request after a failure, so a transient
/// storage or client error recovers without restarting the application.
fn run<B: Backend>(
    mailbox: &Mailbox,
    events: &EventBox,
    metadata_events: &MetadataEventBox,
    mut open: impl FnMut() -> Result<B, String>,
) {
    let mut backend: Option<B> = None;
    while let Some(work) = mailbox.take_work() {
        if let Work::Metadata(request) = work {
            process_metadata(mailbox, metadata_events, &mut backend, &mut open, request);
            continue;
        }
        let Work::Search(request) = work else {
            continue;
        };
        let kind = request.query.kind();
        let cancel = mailbox.lock().active_search[slot(kind)]
            .as_ref()
            .map(|active| Arc::clone(&active.cancel))
            .unwrap_or_else(|| Arc::new(AtomicBool::new(true)));
        let _active = SearchInFlight(mailbox, kind);
        let send = |outcome| {
            let state = mailbox.lock();
            if state.closed || cancel.load(Ordering::Acquire) {
                return !state.closed;
            }
            events.push(Event {
                kind,
                generation: request.generation,
                outcome,
            });
            !state.closed
        };
        if backend.is_none() {
            match open() {
                Ok(opened) => backend = Some(opened),
                Err(message) => {
                    if !send(Outcome::Failed(message)) {
                        return;
                    }
                    continue;
                }
            }
        }
        let Some(backend) = backend.as_mut() else {
            continue;
        };
        let refreshing = request.mode == Mode::Refresh;
        let sent = match backend.read(&request.query) {
            // Load: a failed read is terminal.
            Err(message) if !refreshing => {
                if !send(Outcome::Failed(message)) {
                    return;
                }
                continue;
            }
            // Refresh must still fetch remotely; the UI keeps its results.
            Err(_) | Ok(None) => send(Outcome::CacheMiss),
            Ok(Some(snapshot)) => {
                let sent = send(Outcome::CacheHit {
                    snapshot,
                    remote_follows: refreshing,
                });
                if !refreshing {
                    if !sent {
                        return;
                    }
                    continue;
                }
                sent
            }
        };
        if !sent {
            return;
        }
        if mailbox.superseded(kind) {
            continue;
        }
        let outcome = match backend.refresh_search(&request.query, &cancel) {
            Ok(snapshot) => Outcome::Fetched(snapshot),
            Err(message) => Outcome::Failed(message),
        };
        if !send(outcome) {
            return;
        }
    }
}

struct SearchInFlight<'a>(&'a Mailbox, MediaKind);
impl Drop for SearchInFlight<'_> {
    fn drop(&mut self) {
        self.0.lock().active_search[slot(self.1)] = None;
    }
}

struct MetadataInFlight<'a>(&'a Mailbox, &'a MetadataRequest);
impl Drop for MetadataInFlight<'_> {
    fn drop(&mut self) {
        self.0.finish_metadata(self.1);
    }
}

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
    let refreshing = request.mode == Mode::Refresh;
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
