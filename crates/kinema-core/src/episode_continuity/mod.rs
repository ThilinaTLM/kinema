//! Pure policies for moving between episodes of a series without a trip back
//! to Detail: neighbour order, release-trait parsing and similarity, and the
//! neighbour-stream resolution flow. See `docs/proposals/episode-continuity.md`.
//!
//! Nothing here performs I/O or logs. Release names, file names and hashes are
//! personal data: callers persist the RAW strings, never parsed traits, and
//! re-derive traits with [`ReleaseTraits::parse`], which is deterministic.

mod listing;
mod resolve;
mod traits;

use std::collections::BTreeSet;

pub use listing::{SeasonListing, neighbours_in_listing};
pub use resolve::{
    ChoiceReason, IndexerFailed, KeptFiles, MAX_PRESELECTED, NeighbourChoice, NeighbourQuery,
    RankedCandidate, ReferenceRelease, Similarity, candidate_traits, choose_neighbour_stream,
    preselected_count, rank_candidates, rank_order, similarity,
};
pub use traits::{AudioFormat, Codec, HdrFlags, ReleaseTraits, Resolution, Source};

/// Fraction of a finite duration, in percent, after which the next episode may
/// be resolved ahead of time ([`prefetch_due`]).
pub const PREFETCH_PERCENT: i128 = 80;

/// True once `observation` is at or past [`PREFETCH_PERCENT`] of a known, finite
/// duration. An unknown duration never triggers it.
pub fn prefetch_due(observation: crate::watch_progress::PlaybackObservation) -> bool {
    observation.duration_ms().is_some_and(|duration| {
        i128::from(observation.position_ms()) * 100 >= i128::from(duration) * PREFETCH_PERCENT
    })
}

/// The regular episodes immediately before and after the current one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Neighbours {
    /// `(season, episode)` of the previous regular episode, if any.
    pub previous: Option<(u32, u32)>,
    /// `(season, episode)` of the next regular episode, if any.
    pub next: Option<(u32, u32)>,
}

/// Neighbours of `current` among known `(season, episode)` coordinates.
///
/// Specials (season 0) are neither neighbours nor have neighbours. Order is by
/// `(season, episode)` across season boundaries and does not depend on input
/// order or duplicates. `current` need not be in `episodes`.
pub fn neighbours(episodes: &[(u32, u32)], current: (u32, u32)) -> Neighbours {
    if current.0 == 0 {
        return Neighbours::default();
    }
    let regular: BTreeSet<(u32, u32)> = episodes.iter().copied().filter(|e| e.0 > 0).collect();
    Neighbours {
        previous: regular.range(..current).next_back().copied(),
        next: regular
            .range((
                std::ops::Bound::Excluded(current),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .copied(),
    }
}

/// [`neighbours`] for the `(season number, episode count)` shape used by
/// [`crate::watch_progress`], where a season holds episodes `1..=count`. The
/// next episode after a season's last is the first of the next regular season
/// with episodes, matching [`crate::watch_progress::next_up`].
pub fn neighbours_from_counts(seasons: &[(u32, u32)], current: (u32, u32)) -> Neighbours {
    let episodes: Vec<(u32, u32)> = seasons
        .iter()
        .filter(|(season, _)| *season > 0)
        .flat_map(|(season, count)| (1..=*count).map(move |episode| (*season, episode)))
        .collect();
    neighbours(&episodes, current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch_progress::{EpisodeMark, next_up};

    #[test]
    fn neighbours_cross_seasons_and_skip_specials() {
        let eps = [(2, 1), (0, 1), (1, 2), (1, 1), (2, 2), (0, 2)];
        assert_eq!(
            neighbours(&eps, (1, 2)),
            Neighbours {
                previous: Some((1, 1)),
                next: Some((2, 1))
            }
        );
        assert_eq!(
            neighbours(&eps, (2, 1)),
            Neighbours {
                previous: Some((1, 2)),
                next: Some((2, 2))
            }
        );
    }

    #[test]
    fn no_neighbour_at_the_ends_and_for_specials() {
        let eps = [(1, 1), (1, 2), (0, 1)];
        assert_eq!(neighbours(&eps, (1, 1)).previous, None);
        assert_eq!(neighbours(&eps, (1, 2)).next, None);
        assert_eq!(neighbours(&eps, (0, 1)), Neighbours::default());
        assert_eq!(neighbours(&[], (1, 1)), Neighbours::default());
        assert_eq!(neighbours(&[(1, 1)], (1, 1)), Neighbours::default());
    }

    #[test]
    fn input_order_and_duplicates_do_not_matter() {
        let a = [(1, 1), (1, 2), (1, 3), (2, 1)];
        let b = [(2, 1), (1, 3), (1, 3), (1, 1), (1, 2)];
        for current in [(1, 1), (1, 2), (1, 3), (2, 1)] {
            assert_eq!(neighbours(&a, current), neighbours(&b, current));
        }
    }

    #[test]
    fn gaps_and_unlisted_current_use_known_coordinates() {
        let eps = [(1, 1), (1, 4), (3, 1)];
        assert_eq!(
            neighbours(&eps, (1, 2)),
            Neighbours {
                previous: Some((1, 1)),
                next: Some((1, 4))
            }
        );
        assert_eq!(neighbours(&eps, (1, 4)).next, Some((3, 1)));
    }

    #[test]
    fn counts_shape_matches_next_up() {
        let seasons = [(0, 3), (1, 2), (2, 0), (3, 2)];
        // Season 2 is empty, so the season-1 finale leads to season 3.
        assert_eq!(neighbours_from_counts(&seasons, (1, 2)).next, Some((3, 1)));
        assert_eq!(
            neighbours_from_counts(&seasons, (3, 1)).previous,
            Some((1, 2))
        );
        // next_up only continues past a season's last episode when it is watched.
        for watched in [(1, 2), (3, 2)] {
            let marks = [EpisodeMark {
                season: watched.0,
                number: watched.1,
                watched: true,
            }];
            assert_eq!(
                neighbours_from_counts(&seasons, watched).next,
                next_up(&marks, &seasons),
                "{watched:?}"
            );
        }
    }
}

#[cfg(test)]
mod prefetch_tests {
    use super::*;
    use crate::watch_progress::PlaybackObservation;

    fn at(position: i64, duration: Option<i64>) -> PlaybackObservation {
        PlaybackObservation::new(position, duration).expect("observation")
    }

    #[test]
    fn prefetch_is_due_at_eighty_percent_of_a_finite_duration() {
        assert!(!prefetch_due(at(799, Some(1000))));
        assert!(prefetch_due(at(800, Some(1000))));
        assert!(prefetch_due(at(1000, Some(1000))));
        assert!(!prefetch_due(at(10_000_000, None)));
    }
}
