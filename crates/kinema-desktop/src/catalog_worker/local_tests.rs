#![allow(
    clippy::unwrap_used,
    reason = "Tests fail immediately when fixture setup or assertions are invalid"
)]
use super::*;
use crate::catalog::library::{LibraryOperation, LibraryResult, TitleRef};
use kinema_core::library::TitleId;

fn request(id: u64, watched: bool) -> LocalRequest {
    LocalRequest {
        request_id: id,
        generation: 0,
        operation: LibraryOperation::SetMovieWatched {
            title: TitleRef::Canonical(TitleId::new(1).unwrap()),
            watched,
        },
    }
}
#[derive(Default)]
struct LocalBackend {
    committed: Vec<bool>,
}
impl Backend for LocalBackend {
    fn read(&mut self, _: &SearchQuery) -> Result<Option<Snapshot>, String> {
        Ok(None)
    }
    fn refresh(&mut self, _: &SearchQuery) -> Result<Snapshot, String> {
        Err("not used".into())
    }
    fn local_work(&mut self, operation: &LibraryOperation) -> Result<LibraryResult, String> {
        match operation {
            LibraryOperation::SetMovieWatched { watched, .. } => {
                if !watched {
                    return Err("commit failed".into());
                }
                self.committed.push(*watched);
                Ok(LibraryResult::Titles(vec![]))
            }
            _ => Ok(LibraryResult::Titles(vec![])),
        }
    }
}
fn wait(worker: &CatalogWorker) -> LocalEvent {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(event) = worker.poll_local() {
            return event;
        }
        assert!(Instant::now() < until, "local acknowledgement timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn saturation_is_explicit_and_never_replaces_durable_intents() {
    let mailbox = Mailbox::default();
    for id in 0..LOCAL_CAPACITY {
        assert_eq!(
            mailbox.submit_local(request(id as u64, true)),
            LocalSubmit::Queued
        );
    }
    assert_eq!(mailbox.submit_local(request(100, true)), LocalSubmit::Full);
    // Even with all HTTP slots busy, local work is selected in acceptance order.
    for id in 0..LOCAL_CAPACITY {
        let Some(Ready::Local(actual)) = mailbox.next::<LocalBackend>(FETCH_THREADS) else {
            panic!("local FIFO")
        };
        assert_eq!(actual.request_id, id as u64);
    }
    // Unread acknowledgements still consume capacity: events cannot grow unbounded.
    assert_eq!(mailbox.submit_local(request(101, true)), LocalSubmit::Full);
}
#[test]
fn closing_or_canceling_navigation_does_not_drop_accepted_local_work() {
    let mailbox = Mailbox::default();
    mailbox.submit_local(request(1, true));
    mailbox.cancel_search(MediaKind::Movie, 7);
    mailbox.close();
    assert!(matches!(
        mailbox.next::<LocalBackend>(FETCH_THREADS),
        Some(Ready::Local(LocalRequest { request_id: 1, .. }))
    ));
    assert!(mailbox.next::<LocalBackend>(0).is_none());
    assert_eq!(mailbox.submit_local(request(2, true)), LocalSubmit::Closed);
}
#[test]
fn fifo_acknowledges_failed_commits_without_advancing_revision() {
    let worker = CatalogWorker::spawn(|| Ok(LocalBackend::default())).unwrap();
    for (id, watched) in [(1, true), (2, false), (3, true)] {
        assert_eq!(
            worker.submit_local(request(id, watched)),
            LocalSubmit::Queued
        );
    }
    let a = wait(&worker);
    let b = wait(&worker);
    let c = wait(&worker);
    assert_eq!(
        (
            a.request.request_id,
            b.request.request_id,
            c.request.request_id
        ),
        (1, 2, 3)
    );
    assert_eq!((a.revision, b.revision, c.revision), (1, 1, 2));
    assert!(a.result.is_ok());
    assert_eq!(b.result, Err("commit failed".into()));
    assert!(c.result.is_ok());
}
#[test]
fn backend_open_failure_is_acknowledged_and_next_local_request_retries() {
    let mut first = true;
    let worker = CatalogWorker::spawn(move || {
        if std::mem::take(&mut first) {
            Err("unavailable".into())
        } else {
            Ok(LocalBackend::default())
        }
    })
    .unwrap();
    worker.submit_local(request(1, true));
    assert!(wait(&worker).result.is_err());
    worker.submit_local(request(2, true));
    let event = wait(&worker);
    assert!(event.result.is_ok());
    assert_eq!(event.revision, 1);
}
#[test]
fn bounded_local_burst_gives_completed_persistence_a_turn() {
    let mailbox = Mailbox::default();
    for id in 0..8 {
        mailbox.submit_local(request(id, true));
    }
    mailbox
        .lock()
        .completed
        .push_back(Box::new(Completed::<LocalBackend> {
            work: Work::Search(Request {
                generation: 1,
                query: SearchQuery::new(MediaKind::Movie, "test").unwrap(),
                mode: Mode::Load,
            }),
            cancel: Arc::new(AtomicBool::new(false)),
            persist: Box::new(|_| Err("not executed".into())),
        }));
    for _ in 0..4 {
        assert!(matches!(
            mailbox.next::<LocalBackend>(FETCH_THREADS),
            Some(Ready::Local(_))
        ));
    }
    assert!(matches!(
        mailbox.next::<LocalBackend>(FETCH_THREADS),
        Some(Ready::Completed(_))
    ));
    assert!(matches!(
        mailbox.next::<LocalBackend>(FETCH_THREADS),
        Some(Ready::Local(_))
    ));
}
#[test]
fn no_fetch_threads_still_runs_local_owner() {
    let worker = CatalogWorker::spawn_configured(|| Ok(LocalBackend::default()), 0).unwrap();
    worker.submit(Request {
        generation: 1,
        query: SearchQuery::new(MediaKind::Movie, "remote").unwrap(),
        mode: Mode::Refresh,
    });
    worker.submit_local(request(1, true));
    let event = wait(&worker);
    assert!(event.result.is_ok());
    assert_eq!(event.revision, 1);
    worker.submit_local(request(2, true));
    assert!(wait(&worker).result.is_ok());
}
#[test]
fn revision_overflow_is_rejected_before_commit_and_reads_still_work() {
    let mut backend = LocalBackend::default();
    let mut revision = u64::MAX;
    assert!(execute_local(&mut backend, &request(1, true).operation, &mut revision).is_err());
    assert!(backend.committed.is_empty());
    assert_eq!(revision, u64::MAX);
    assert!(
        execute_local(
            &mut backend,
            &LibraryOperation::ReadTitles(vec![]),
            &mut revision
        )
        .is_ok()
    );
}
#[test]
fn search_replacement_cannot_replace_an_accepted_mutation() {
    let mailbox = Mailbox::default();
    mailbox.submit_local(request(1, true));
    for generation in 0..3 {
        mailbox.submit(Request {
            generation,
            query: SearchQuery::new(MediaKind::Movie, format!("query {generation}")).unwrap(),
            mode: Mode::Refresh,
        });
    }
    assert!(matches!(
        mailbox.next::<LocalBackend>(0),
        Some(Ready::Local(LocalRequest { request_id: 1, .. }))
    ));
}

#[test]
fn worker_shutdown_drains_and_acknowledges_accepted_mutation() {
    let worker = CatalogWorker::spawn(|| Ok(LocalBackend::default())).unwrap();
    let mailbox = Arc::clone(&worker.mailbox);
    assert_eq!(worker.submit_local(request(1, true)), LocalSubmit::Queued);
    drop(worker);
    let until = Instant::now() + Duration::from_secs(3);
    let event = loop {
        if let Some(event) = mailbox.poll_local() {
            break event;
        }
        assert!(Instant::now() < until, "accepted write lost on close");
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(event.request.request_id, 1);
    assert!(event.result.is_ok());
    assert_eq!(event.revision, 1);
}
