//! Provider-independent personal library policy. Times are UTC Unix milliseconds.
use crate::MediaKind;

macro_rules! canonical_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(i64);
        impl $name {
            pub const fn new(value: i64) -> Option<Self> {
                if value > 0 { Some(Self(value)) } else { None }
            }
            pub const fn get(self) -> i64 {
                self.0
            }
        }
    };
}
canonical_id!(TitleId);
canonical_id!(EpisodeId);
canonical_id!(SeasonId);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LibraryKind {
    #[default]
    All,
    Movies,
    Series,
}
impl LibraryKind {
    pub fn accepts(self, kind: MediaKind) -> bool {
        matches!(
            (self, kind),
            (Self::All, _) | (Self::Movies, MediaKind::Movie) | (Self::Series, MediaKind::Series)
        )
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WatchFilter {
    #[default]
    Any,
    Unwatched,
    InProgress,
    Watched,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LibrarySort {
    #[default]
    RecentlyAdded,
    Title,
    NewestRelease,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryQuery {
    pub kind: LibraryKind,
    pub search: String,
    pub status: WatchFilter,
    pub sort: LibrarySort,
    pub offset: usize,
    /// Clamped to 1..=500 by persistence.
    pub limit: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WatchState {
    pub watched: bool,
    pub updated_at: i64,
}
/// Absence is unwatched, but does not override explicit state during merges.
pub fn reconcile_watch(a: Option<WatchState>, b: Option<WatchState>) -> Option<WatchState> {
    match (a, b) {
        (None, value) | (value, None) => value,
        (Some(a), Some(b)) => Some(if a.updated_at > b.updated_at {
            a
        } else if b.updated_at > a.updated_at {
            b
        } else {
            WatchState {
                watched: a.watched && b.watched,
                updated_at: a.updated_at,
            }
        }),
    }
}
pub fn next_watch_timestamp(wall: i64, last: i64) -> Option<i64> {
    if wall < 0 || last < 0 {
        return None;
    }
    Some(wall.max(last.checked_add(1)?))
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShowSummary {
    pub watched: usize,
    pub total: usize,
    pub complete: bool,
}
impl ShowSummary {
    pub fn status(self) -> Option<WatchFilter> {
        if !self.complete || self.total == 0 || self.watched > self.total {
            return None;
        }
        Some(if self.watched == 0 {
            WatchFilter::Unwatched
        } else if self.watched == self.total {
            WatchFilter::Watched
        } else {
            WatchFilter::InProgress
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conservative_completion() {
        assert_eq!(
            ShowSummary {
                watched: 0,
                total: 0,
                complete: true
            }
            .status(),
            None
        );
        assert_eq!(
            ShowSummary {
                watched: 3,
                total: 3,
                complete: false
            }
            .status(),
            None
        );
        assert_eq!(
            ShowSummary {
                watched: 1,
                total: 3,
                complete: true
            }
            .status(),
            Some(WatchFilter::InProgress)
        );
    }
    #[test]
    fn merge_and_clock() {
        let a = WatchState {
            watched: true,
            updated_at: 10,
        };
        let b = WatchState {
            watched: false,
            updated_at: 10,
        };
        assert_eq!(reconcile_watch(Some(a), Some(b)), Some(b));
        assert_eq!(reconcile_watch(None, Some(b)), Some(b));
        assert_eq!(next_watch_timestamp(2, 10), Some(11));
        assert_eq!(next_watch_timestamp(10, 10), Some(11));
        assert_eq!(next_watch_timestamp(12, 10), Some(12));
        assert_eq!(next_watch_timestamp(2, i64::MAX), None);
        assert!(TitleId::new(0).is_none());
    }
}
