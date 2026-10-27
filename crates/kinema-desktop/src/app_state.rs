//! Pure, reusable UI state shared by catalog, detail and source screens.
//!
//! Every remote-backed view holds a [`Resource`]. It keeps previous data when a
//! refresh fails, distinguishes "no data yet" from "empty data", and rejects
//! results from superseded requests by generation. Desktop-only: it is not a
//! domain type and never carries credentials.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    Idle,
    /// Reading saved results.
    Reading,
    /// Waiting on a remote fetch.
    Fetching,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Origin {
    None,
    Saved,
    Fetched,
}

/// One step of a cache-then-remote load, as reported by a worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Stage<T> {
    CacheHit {
        value: T,
        fetched_at: u64,
        remote_follows: bool,
    },
    Fetched {
        value: T,
        fetched_at: u64,
    },
    Failed(String),
}

/// A remote-backed value with freshness and failure state.
#[derive(Debug)]
pub(crate) struct Resource<T> {
    generation: u64,
    value: Option<T>,
    fetched_at: Option<u64>,
    origin: Origin,
    phase: Phase,
    error: Option<String>,
}

impl<T> Default for Resource<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            value: None,
            fetched_at: None,
            origin: Origin::None,
            phase: Phase::Idle,
            error: None,
        }
    }
}

impl<T> Resource<T> {
    /// Starts generations at `base`, so a recreated view never accepts a late
    /// result addressed to an earlier one. Callers hand out disjoint bases.
    pub(crate) fn with_base(base: u64) -> Self {
        Self {
            generation: base,
            ..Self::default()
        }
    }

    pub(crate) fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// In-place refinement (optional enrichment) that keeps freshness, origin
    /// and phase untouched; callers must have matched the current generation.
    pub(crate) fn value_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut()
    }

    pub(crate) fn fetched_at(&self) -> Option<u64> {
        self.fetched_at
    }

    pub(crate) fn origin(&self) -> Origin {
        self.origin
    }

    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Starts a request and returns its generation. `replace` discards the old
    /// value (a different subject); otherwise it stays visible while loading.
    pub(crate) fn begin(&mut self, replace: bool) -> u64 {
        self.generation += 1;
        if replace {
            self.value = None;
            self.fetched_at = None;
            self.origin = Origin::None;
        }
        self.error = None;
        self.phase = Phase::Reading;
        self.generation
    }

    /// Abandons in-flight work: late results are rejected afterwards.
    pub(crate) fn cancel(&mut self) {
        self.generation += 1;
        self.phase = Phase::Idle;
    }

    /// Applies a worker stage; false when it belongs to a superseded request.
    pub(crate) fn apply(&mut self, generation: u64, stage: Stage<T>) -> bool {
        match stage {
            Stage::CacheHit {
                value,
                fetched_at,
                remote_follows,
            } => self.cache_hit(generation, value, fetched_at, remote_follows),
            Stage::Fetched { value, fetched_at } => self.fetched(generation, value, fetched_at),
            Stage::Failed(message) => self.failed(generation, message),
        }
    }

    fn current(&self, generation: u64) -> bool {
        generation == self.generation && self.busy()
    }

    pub(crate) fn cache_hit(
        &mut self,
        generation: u64,
        value: T,
        fetched_at: u64,
        remote_follows: bool,
    ) -> bool {
        if !self.current(generation) {
            return false;
        }
        self.store(value, fetched_at, Origin::Saved);
        self.phase = if remote_follows {
            Phase::Fetching
        } else {
            Phase::Idle
        };
        true
    }

    pub(crate) fn fetched(&mut self, generation: u64, value: T, fetched_at: u64) -> bool {
        if !self.current(generation) {
            return false;
        }
        self.store(value, fetched_at, Origin::Fetched);
        self.error = None;
        self.phase = Phase::Idle;
        true
    }

    /// Records a failure; previous value and timestamp are kept.
    pub(crate) fn failed(&mut self, generation: u64, message: impl Into<String>) -> bool {
        if !self.current(generation) {
            return false;
        }
        self.error = Some(message.into());
        self.phase = Phase::Idle;
        true
    }

    fn store(&mut self, value: T, fetched_at: u64, origin: Origin) {
        self.value = Some(value);
        self.fetched_at = Some(fetched_at);
        self.origin = origin;
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stale_generations_are_rejected_and_replace_clears() {
        let mut r = Resource::<Vec<u8>>::default();
        let first = r.begin(true);
        assert!(r.fetched(first, vec![1], 5));
        let second = r.begin(true);
        assert!(r.value().is_none() && r.fetched_at().is_none());
        assert!(!r.fetched(first, vec![9], 9));
        assert!(r.value().is_none());
        assert!(r.fetched(second, vec![2], 6));
        assert_eq!((r.value(), r.fetched_at()), (Some(&vec![2]), Some(6)));
    }

    #[test]
    fn failure_keeps_previous_value_and_timestamp() {
        let mut r = Resource::<u8>::default();
        let g = r.begin(true);
        r.cache_hit(g, 1, 10, false);
        let g = r.begin(false);
        assert_eq!(r.value(), Some(&1));
        assert!(r.cache_hit(g, 1, 10, true));
        assert_eq!(r.phase(), Phase::Fetching);
        assert!(r.failed(g, "offline"));
        assert_eq!(
            (r.value(), r.fetched_at(), r.origin()),
            (Some(&1), Some(10), Origin::Saved)
        );
        assert_eq!(r.error(), Some("offline"));
        assert!(!r.busy());
    }

    #[test]
    fn apply_routes_stages_and_checks_generation() {
        let mut r = Resource::<u8>::default();
        let g = r.begin(true);
        assert!(!r.apply(g + 1, Stage::Failed("x".into())));
        assert!(r.apply(
            g,
            Stage::CacheHit {
                value: 1,
                fetched_at: 2,
                remote_follows: true
            }
        ));
        assert_eq!(r.phase(), Phase::Fetching);
        assert!(r.apply(
            g,
            Stage::Fetched {
                value: 4,
                fetched_at: 9
            }
        ));
        assert_eq!((r.value(), r.origin()), (Some(&4), Origin::Fetched));
    }

    #[test]
    fn empty_saved_value_is_final_and_cancel_rejects_late_results() {
        let mut r = Resource::<Vec<u8>>::default();
        let g = r.begin(true);
        assert!(r.cache_hit(g, Vec::new(), 3, false));
        assert!(!r.busy() && r.error().is_none());
        assert!(r.value().is_some_and(Vec::is_empty));
        let g = r.begin(false);
        r.cancel();
        assert!(!r.fetched(g, vec![1], 4) && !r.failed(g, "late"));
        assert!(r.error().is_none());
    }
}
