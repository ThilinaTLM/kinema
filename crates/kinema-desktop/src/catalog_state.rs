//! Pure per-screen catalog state. Each media kind owns an independent screen;
//! generations reject results for replaced or obsolete requests.

use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, SearchQuery},
};

use crate::{
    app_state::{Origin, Phase},
    catalog_worker::{Event, Mode, Outcome, Request, Snapshot},
};

#[derive(Debug)]
pub(crate) struct ScreenState {
    kind: MediaKind,
    generation: u64,
    in_flight: Mode,
    active: Option<SearchQuery>,
    resume_pending: bool,
    pub titles: Vec<CatalogTitle>,
    pub fetched_at: Option<u64>,
    pub source: Origin,
    pub phase: Phase,
    pub error: Option<String>,
    pub input_error: Option<String>,
}

impl ScreenState {
    pub(crate) fn new(kind: MediaKind) -> Self {
        Self {
            kind,
            generation: 0,
            in_flight: Mode::Load,
            active: None,
            resume_pending: false,
            titles: Vec::new(),
            fetched_at: None,
            source: Origin::None,
            phase: Phase::Idle,
            error: None,
            input_error: None,
        }
    }

    pub(crate) fn active(&self) -> Option<&SearchQuery> {
        self.active.as_ref()
    }

    pub(crate) fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Original identity to forward to the worker before suspending/clearing.
    pub(crate) fn pending_generation(&self) -> Option<u64> {
        self.busy().then_some(self.generation)
    }

    /// Hide this screen without discarding its query or any loaded snapshot.
    pub(crate) fn suspend(&mut self) {
        if self.busy() {
            self.resume_pending = self.fetched_at.is_none();
            self.generation += 1;
            self.phase = Phase::Idle;
        }
    }

    /// Only interrupted first loads restart automatically. An empty snapshot is
    /// still a successful load, and failures require an explicit retry.
    pub(crate) fn resume(&mut self) -> Option<Request> {
        if !self.resume_pending {
            return None;
        }
        let query = self.active.clone()?;
        Some(self.begin(query, Mode::Load))
    }

    /// Explicit search. Returns a request to queue, or `None` when the text is
    /// invalid or an equivalent request is already in flight.
    pub(crate) fn search(&mut self, text: &str) -> Option<Request> {
        let query = match SearchQuery::new(self.kind, text) {
            Ok(query) => query,
            Err(error) => {
                self.input_error = Some(error.to_string());
                return None;
            }
        };
        self.input_error = None;
        if self.active.as_ref() == Some(&query) && self.busy() {
            return None;
        }
        Some(self.begin(query, Mode::Load))
    }

    /// Explicit refresh of the active query; always fetches remotely.
    pub(crate) fn refresh(&mut self) -> Option<Request> {
        let query = self.active.clone()?;
        if self.phase == Phase::Fetching
            || (self.phase == Phase::Reading && self.in_flight == Mode::Refresh)
        {
            return None;
        }
        Some(self.begin(query, Mode::Refresh))
    }

    fn begin(&mut self, query: SearchQuery, mode: Mode) -> Request {
        self.generation += 1;
        self.resume_pending = false;
        if self.active.as_ref() != Some(&query) {
            self.titles.clear();
            self.fetched_at = None;
            self.source = Origin::None;
        }
        self.error = None;
        self.phase = Phase::Reading;
        self.in_flight = mode;
        self.active = Some(query.clone());
        Request {
            generation: self.generation,
            query,
            mode,
        }
    }

    /// Leaves search mode; late results for the abandoned query are rejected.
    pub(crate) fn clear(&mut self) {
        self.generation += 1;
        self.active = None;
        self.resume_pending = false;
        self.titles.clear();
        self.fetched_at = None;
        self.source = Origin::None;
        self.phase = Phase::Idle;
        self.error = None;
        self.input_error = None;
    }

    /// The request could not be queued (worker gone); keeps prior results.
    pub(crate) fn fail_current(&mut self, message: &str) {
        self.phase = Phase::Idle;
        self.error = Some(message.to_owned());
    }

    /// Applies a worker event; returns false when it was stale and ignored.
    pub(crate) fn apply(&mut self, event: Event) -> bool {
        if event.kind != self.kind || event.generation != self.generation || !self.busy() {
            return false;
        }
        match event.outcome {
            Outcome::CacheHit {
                snapshot,
                remote_follows,
            } => {
                self.store(snapshot, Origin::Saved);
                self.phase = if remote_follows {
                    Phase::Fetching
                } else {
                    Phase::Idle
                };
            }
            Outcome::CacheMiss => self.phase = Phase::Fetching,
            Outcome::Fetched(snapshot) => {
                self.store(snapshot, Origin::Fetched);
                self.error = None;
                self.phase = Phase::Idle;
            }
            Outcome::Failed(message) => {
                self.error = Some(message);
                self.phase = Phase::Idle;
            }
        }
        true
    }

    fn store(&mut self, snapshot: Snapshot, source: Origin) {
        self.titles = snapshot.titles;
        self.fetched_at = Some(snapshot.fetched_at);
        self.source = source;
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn snap(at: u64, names: &[&str]) -> Snapshot {
        Snapshot {
            titles: names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    CatalogTitle::new(
                        kinema_core::catalog::ImdbId::new(format!("tt{:07}", i + 1)).expect("id"),
                        MediaKind::Movie,
                        *name,
                        None,
                    )
                    .expect("title")
                })
                .collect(),
            fetched_at: at,
        }
    }

    fn ev(generation: u64, outcome: Outcome) -> Event {
        Event {
            kind: MediaKind::Movie,
            generation,
            outcome,
        }
    }

    #[test]
    fn suspended_first_load_resumes_once_and_rejects_original_events() -> Result<(), &'static str> {
        let mut state = ScreenState::new(MediaKind::Movie);
        let original = state.search("matrix").ok_or("request")?;
        assert_eq!(state.pending_generation(), Some(original.generation));
        state.suspend();
        assert_eq!(state.pending_generation(), None);
        assert!(!state.apply(ev(
            original.generation,
            Outcome::Fetched(snap(1, &["late"]))
        )));
        state.suspend(); // Repeated departure must not lose the pending resume.
        let resumed = state.resume().ok_or("resume first load")?;
        assert_eq!(resumed.query, original.query);
        assert!(resumed.generation > original.generation);
        assert!(state.resume().is_none());
        assert!(!state.apply(ev(original.generation, Outcome::Failed("late".to_owned()))));
        assert!(state.apply(ev(resumed.generation, Outcome::Fetched(snap(2, &["new"])))));
        Ok(())
    }

    #[test]
    fn suspension_preserves_snapshots_including_successful_empty_results()
    -> Result<(), &'static str> {
        for names in [&[][..], &["saved"][..]] {
            let mut state = ScreenState::new(MediaKind::Movie);
            let original = state.search("matrix").ok_or("request")?;
            assert!(state.apply(ev(
                original.generation,
                Outcome::CacheHit {
                    snapshot: snap(7, names),
                    remote_follows: true,
                }
            )));
            state.suspend();
            assert_eq!(state.titles.len(), names.len());
            assert_eq!(state.fetched_at, Some(7));
            assert_eq!(state.source, Origin::Saved);
            assert!(state.resume().is_none());
            assert!(!state.apply(ev(
                original.generation,
                Outcome::Fetched(snap(8, &["late"]))
            )));
        }
        Ok(())
    }

    #[test]
    fn clear_and_explicit_retry_reset_suspension_without_retrying_failures()
    -> Result<(), &'static str> {
        let mut state = ScreenState::new(MediaKind::Movie);
        state.search("matrix").ok_or("request")?;
        state.suspend();
        state.clear();
        assert!(state.active().is_none());
        assert!(state.resume().is_none());
        let request = state.search("matrix").ok_or("request")?;
        assert!(state.apply(ev(
            request.generation,
            Outcome::Failed("offline".to_owned())
        )));
        state.suspend();
        assert!(state.resume().is_none());
        assert_eq!(state.error.as_deref(), Some("offline"));
        state.search("matrix").ok_or("retry")?;
        state.suspend();
        let retry = state.search("different").ok_or("explicit replacement")?;
        assert!(state.resume().is_none());
        assert_eq!(state.pending_generation(), Some(retry.generation));
        Ok(())
    }

    #[test]
    fn invalid_text_is_reported_without_touching_results() {
        let mut s = ScreenState::new(MediaKind::Movie);
        let r = s.search("matrix").expect("request");
        s.apply(ev(r.generation, Outcome::Fetched(snap(5, &["A"]))));
        assert!(s.search("   ").is_none());
        assert!(s.input_error.is_some());
        assert_eq!(s.titles.len(), 1);
    }

    #[test]
    fn query_replacement_clears_old_results_and_rejects_stale_events() {
        let mut s = ScreenState::new(MediaKind::Movie);
        let first = s.search("one").expect("request");
        s.apply(ev(first.generation, Outcome::Fetched(snap(5, &["A"]))));
        let second = s.search("two").expect("request");
        assert!(s.titles.is_empty() && s.fetched_at.is_none());
        assert!(!s.apply(ev(first.generation, Outcome::Fetched(snap(9, &["Old"])))));
        assert!(s.titles.is_empty());
        assert!(s.apply(ev(
            second.generation,
            Outcome::Fetched(snap(6, &["B", "C"]))
        )));
        assert_eq!(s.titles.len(), 2);
        assert_eq!(s.fetched_at, Some(6));
    }

    #[test]
    fn events_for_other_kind_or_finished_requests_are_rejected() {
        let mut s = ScreenState::new(MediaKind::Series);
        let r = s.search("x").expect("request");
        assert!(!s.apply(ev(r.generation, Outcome::CacheMiss)));
        let mut m = ScreenState::new(MediaKind::Movie);
        let r = m.search("x").expect("request");
        assert!(m.apply(ev(r.generation, Outcome::Fetched(snap(1, &[])))));
        assert!(!m.apply(ev(r.generation, Outcome::Failed("late".into()))));
        assert!(m.error.is_none());
    }

    #[test]
    fn refresh_failure_retains_titles_and_timestamp() {
        let mut s = ScreenState::new(MediaKind::Movie);
        let r = s.search("q").expect("request");
        s.apply(ev(
            r.generation,
            Outcome::CacheHit {
                snapshot: snap(10, &["A"]),
                remote_follows: false,
            },
        ));
        assert_eq!((s.phase, s.source), (Phase::Idle, Origin::Saved));
        let r = s.refresh().expect("refresh");
        assert_eq!(r.mode, Mode::Refresh);
        s.apply(ev(
            r.generation,
            Outcome::CacheHit {
                snapshot: snap(10, &["A"]),
                remote_follows: true,
            },
        ));
        assert_eq!(s.phase, Phase::Fetching);
        assert!(s.refresh().is_none());
        s.apply(ev(r.generation, Outcome::Failed("offline".into())));
        assert_eq!((s.titles.len(), s.fetched_at), (1, Some(10)));
        assert_eq!(s.error.as_deref(), Some("offline"));
        assert_eq!(s.source, Origin::Saved);
    }

    #[test]
    fn empty_saved_snapshot_is_final_and_distinct_from_error() {
        let mut s = ScreenState::new(MediaKind::Movie);
        let r = s.search("nothing").expect("request");
        s.apply(ev(
            r.generation,
            Outcome::CacheHit {
                snapshot: snap(3, &[]),
                remote_follows: false,
            },
        ));
        assert_eq!(s.phase, Phase::Idle);
        assert!(s.titles.is_empty() && s.error.is_none());
        assert_eq!(s.fetched_at, Some(3));
    }

    #[test]
    fn clear_leaves_search_and_rejects_late_results() {
        let mut s = ScreenState::new(MediaKind::Movie);
        let r = s.search("q").expect("request");
        s.clear();
        assert!(s.active().is_none() && !s.busy());
        assert!(!s.apply(ev(r.generation, Outcome::Fetched(snap(1, &["A"])))));
        assert!(s.titles.is_empty());
    }

    #[test]
    fn duplicate_search_in_flight_is_coalesced_and_refresh_needs_a_query() {
        let mut s = ScreenState::new(MediaKind::Movie);
        assert!(s.refresh().is_none());
        assert!(s.search("q").is_some());
        assert!(s.search(" q ").is_none());
        s.fail_current("worker stopped");
        assert!(!s.busy() && s.error.is_some());
    }
}
