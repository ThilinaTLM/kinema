//! Single owned catalog worker thread and its bounded request mailbox.
//!
//! The backend is constructed and used only on the worker thread. The mailbox
//! holds at most one pending request per media kind, so it is bounded by
//! construction: a newer request for the same kind replaces the queued one
//! (obsolete work is cancelled), and an identical one is coalesced.
//!
//! Limit: a blocking HTTP request already in flight cannot be cancelled. Its
//! duration is bounded only by the backend client timeout, and its result is
//! rejected by generation checks in the UI. Dropping [`CatalogWorker`] signals
//! the thread and does not join it, so the UI never waits on that timeout; the
//! thread exits after the current backend call finishes.

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
};

use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, SearchQuery},
};

use crate::catalog::{CachedSearch, CatalogService};

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
    /// Kind served first next, so one kind cannot starve the other.
    next: usize,
    closed: bool,
}

fn slot(kind: MediaKind) -> usize {
    match kind {
        MediaKind::Movie => 0,
        MediaKind::Series => 1,
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

    /// Blocks until work is available; `None` once closed.
    fn take(&self) -> Option<Request> {
        let mut state = self.lock();
        loop {
            if state.closed {
                return None;
            }
            for offset in 0..state.slots.len() {
                let index = (state.next + offset) % state.slots.len();
                if let Some(request) = state.slots[index].take() {
                    state.next = (index + 1) % state.slots.len();
                    return Some(request);
                }
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
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
        self.lock().closed = true;
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

/// Blocking catalog operations; errors are already user-safe messages.
pub(crate) trait Backend {
    fn read(&mut self, query: &SearchQuery) -> Result<Option<Snapshot>, String>;
    fn refresh(&mut self, query: &SearchQuery) -> Result<Snapshot, String>;
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
        let worker_mailbox = Arc::clone(&mailbox);
        let worker_events = Arc::clone(&events);
        let thread = std::thread::Builder::new()
            .name("kinema-catalog".into())
            .spawn(move || {
                let _close = CloseOnExit(Arc::clone(&worker_mailbox));
                run(&worker_mailbox, &worker_events, open);
            })?;
        Ok(Self {
            mailbox,
            events,
            _thread: thread,
        })
    }

    pub(crate) fn submit(&self, request: Request) -> Submit {
        self.mailbox.submit(request)
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
        self.mailbox.close();
    }
}

/// `open` is retried on the next request after a failure, so a transient
/// storage or client error recovers without restarting the application.
fn run<B: Backend>(
    mailbox: &Mailbox,
    events: &EventBox,
    mut open: impl FnMut() -> Result<B, String>,
) {
    let mut backend: Option<B> = None;
    while let Some(request) = mailbox.take() {
        let kind = request.query.kind();
        let send = |outcome| {
            events.push(Event {
                kind,
                generation: request.generation,
                outcome,
            });
            !mailbox.is_closed()
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
        let outcome = match backend.refresh(&request.query) {
            Ok(snapshot) => Outcome::Fetched(snapshot),
            Err(message) => Outcome::Failed(message),
        };
        if !send(outcome) {
            return;
        }
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
