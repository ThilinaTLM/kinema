//! Neighbours from the canonical season/episode listing, without inventing one.

use super::Neighbours;
use std::collections::{BTreeMap, BTreeSet};

/// Most episodes a season's advertised count may add. A larger (corrupt) count
/// is capped rather than expanded.
const MAX_ADVERTISED: u32 = 10_000;

/// One season row of the canonical catalog.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeasonListing {
    pub number: u32,
    /// The provider's episode count for the season, when it gave one. Seasons
    /// run `1..=advertised`, the same shape [`crate::watch_progress::next_up`]
    /// uses.
    pub advertised: Option<u32>,
    /// Episode numbers that have a canonical row.
    pub episodes: Vec<u32>,
}

enum SeasonState {
    Present(BTreeSet<u32>),
    /// Advertised as having no episodes: skipped, like `next_up` does.
    KnownEmpty,
    /// Listed, but with neither rows nor a count: nothing is known about it.
    Unknown,
}

/// Neighbours of `current` in a canonical listing. A neighbour is returned only
/// when the listing proves it; anything unknown yields `None`, never a guess:
///
/// - Specials (season 0) are neither neighbours nor have neighbours.
/// - A season is its rows plus `1..=advertised`. If the current season is not
///   listed, or is listed with no data, it has no neighbours.
/// - Crossing a season boundary needs the adjacent seasons to be listed. A
///   season advertised as empty is skipped; an absent or data-less one stops
///   the search.
pub fn neighbours_in_listing(seasons: &[SeasonListing], current: (u32, u32)) -> Neighbours {
    let (season, episode) = current;
    if season == 0 {
        return Neighbours::default();
    }
    let mut states: BTreeMap<u32, SeasonState> = BTreeMap::new();
    for listed in seasons.iter().filter(|s| s.number > 0) {
        let mut set: BTreeSet<u32> = match states.remove(&listed.number) {
            Some(SeasonState::Present(set)) => set,
            _ => BTreeSet::new(),
        };
        set.extend(listed.episodes.iter().copied().filter(|n| *n > 0));
        let advertised = listed.advertised.map(|n| n.min(MAX_ADVERTISED));
        if let Some(n) = advertised {
            set.extend(1..=n);
        }
        states.insert(
            listed.number,
            if !set.is_empty() {
                SeasonState::Present(set)
            } else if advertised == Some(0) {
                SeasonState::KnownEmpty
            } else {
                SeasonState::Unknown
            },
        );
    }
    let Some(SeasonState::Present(own)) = states.get(&season) else {
        return Neighbours::default();
    };
    let last_season = states.keys().next_back().copied().unwrap_or(season);

    let next = own
        .range(episode.saturating_add(1)..)
        .next()
        .map(|n| (season, *n))
        .or_else(|| {
            for candidate in season.saturating_add(1)..=last_season {
                match states.get(&candidate) {
                    Some(SeasonState::Present(set)) => {
                        return set.iter().next().map(|n| (candidate, *n));
                    }
                    Some(SeasonState::KnownEmpty) => {}
                    Some(SeasonState::Unknown) | None => return None,
                }
            }
            None
        });
    let previous = own
        .range(..episode)
        .next_back()
        .map(|n| (season, *n))
        .or_else(|| {
            for candidate in (1..season).rev() {
                match states.get(&candidate) {
                    Some(SeasonState::Present(set)) => {
                        return set.iter().next_back().map(|n| (candidate, *n));
                    }
                    Some(SeasonState::KnownEmpty) => {}
                    Some(SeasonState::Unknown) | None => return None,
                }
            }
            None
        });
    Neighbours { previous, next }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn season(number: u32, advertised: Option<u32>, episodes: &[u32]) -> SeasonListing {
        SeasonListing {
            number,
            advertised,
            episodes: episodes.to_vec(),
        }
    }

    #[test]
    fn rows_alone_define_a_season_and_cross_into_listed_seasons() {
        let listing = [
            season(1, None, &[1, 2, 3]),
            season(2, None, &[1, 2]),
            season(0, None, &[1]),
        ];
        assert_eq!(
            neighbours_in_listing(&listing, (1, 3)),
            Neighbours {
                previous: Some((1, 2)),
                next: Some((2, 1))
            }
        );
        assert_eq!(
            neighbours_in_listing(&listing, (2, 1)),
            Neighbours {
                previous: Some((1, 3)),
                next: Some((2, 2))
            }
        );
        assert_eq!(neighbours_in_listing(&listing, (2, 2)).next, None);
        assert_eq!(neighbours_in_listing(&listing, (1, 1)).previous, None);
    }

    #[test]
    fn advertised_counts_extend_a_season_past_its_loaded_rows() {
        // Season 1 has one loaded row but ten advertised episodes.
        let listing = [season(1, Some(10), &[1]), season(2, Some(8), &[])];
        assert_eq!(neighbours_in_listing(&listing, (1, 1)).next, Some((1, 2)));
        assert_eq!(neighbours_in_listing(&listing, (1, 10)).next, Some((2, 1)));
        assert_eq!(
            neighbours_in_listing(&listing, (2, 1)).previous,
            Some((1, 10))
        );
    }

    #[test]
    fn specials_have_no_neighbours_and_are_never_neighbours() {
        let listing = [season(0, Some(3), &[]), season(1, Some(2), &[])];
        assert_eq!(
            neighbours_in_listing(&listing, (0, 2)),
            Neighbours::default()
        );
        assert_eq!(neighbours_in_listing(&listing, (1, 1)).previous, None);
    }

    #[test]
    fn a_season_advertised_empty_is_skipped() {
        let listing = [
            season(1, Some(2), &[]),
            season(2, Some(0), &[]),
            season(3, Some(2), &[]),
        ];
        assert_eq!(neighbours_in_listing(&listing, (1, 2)).next, Some((3, 1)));
        assert_eq!(
            neighbours_in_listing(&listing, (3, 1)).previous,
            Some((1, 2))
        );
    }

    #[test]
    fn unknown_or_missing_seasons_do_not_invent_a_neighbour() {
        // Season 2 is listed with no data; season 3 is absent from a gap.
        let listing = [
            season(1, Some(2), &[]),
            season(2, None, &[]),
            season(4, Some(3), &[]),
        ];
        assert_eq!(neighbours_in_listing(&listing, (1, 2)).next, None);
        assert_eq!(neighbours_in_listing(&listing, (4, 1)).previous, None);
        // A gap (season 3 absent) also stops the search.
        let gap = [season(1, Some(2), &[]), season(3, Some(2), &[])];
        assert_eq!(neighbours_in_listing(&gap, (1, 2)).next, None);
        assert_eq!(neighbours_in_listing(&gap, (3, 1)).previous, None);
        // Within a season the neighbours still resolve.
        assert_eq!(neighbours_in_listing(&gap, (3, 1)).next, Some((3, 2)));
    }

    #[test]
    fn a_current_season_missing_from_the_listing_has_no_neighbours() {
        let listing = [season(1, Some(5), &[])];
        assert_eq!(
            neighbours_in_listing(&listing, (2, 1)),
            Neighbours::default()
        );
        assert_eq!(neighbours_in_listing(&[], (1, 1)), Neighbours::default());
        // Listed but without any data.
        assert_eq!(
            neighbours_in_listing(&[season(1, None, &[])], (1, 1)),
            Neighbours::default()
        );
    }

    #[test]
    fn duplicate_rows_and_huge_counts_are_bounded() {
        let listing = [
            season(1, None, &[2, 1, 2]),
            season(1, None, &[3]),
            season(2, Some(u32::MAX), &[]),
        ];
        assert_eq!(neighbours_in_listing(&listing, (1, 3)).next, Some((2, 1)));
        assert_eq!(
            neighbours_in_listing(&listing, (2, 1)).previous,
            Some((1, 3))
        );
    }
}
