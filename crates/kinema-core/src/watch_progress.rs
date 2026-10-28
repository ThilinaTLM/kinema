//! Pure watched-progress policies for series: next-up, default season,
//! tri-state derivation and the collapsed season-tile window.
//!
//! Inputs are only what the caller actually *knows*. An episode whose state is
//! unknown must be left out of the marks, never reported as unwatched.

use std::collections::BTreeMap;

/// One episode with a known watched state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EpisodeMark {
    pub season: u32,
    pub number: u32,
    pub watched: bool,
}

/// The first unwatched regular episode after the latest watched one, ordered by
/// (season, episode). Specials (season 0) are ignored. `seasons` is
/// `(number, episode_count)`: when the latest watched episode is the last of its
/// season, next-up is the next regular season's first episode. `None` when
/// nothing is tracked as watched, or when everything is watched (the latest
/// watched episode ends the final season).
pub fn next_up(marks: &[EpisodeMark], seasons: &[(u32, u32)]) -> Option<(u32, u32)> {
    let ordered: BTreeMap<(u32, u32), bool> = marks
        .iter()
        .filter(|m| m.season > 0)
        .map(|m| ((m.season, m.number), m.watched))
        .collect();
    let latest = ordered.iter().rev().find(|(_, w)| **w).map(|(k, _)| *k)?;
    let known_after = ordered
        .range((
            std::ops::Bound::Excluded(latest),
            std::ops::Bound::Unbounded,
        ))
        .find(|(_, w)| !**w)
        .map(|(k, _)| *k);
    if known_after.is_some() {
        return known_after;
    }
    let count_of = |season: u32| {
        seasons
            .iter()
            .find(|(n, _)| *n == season)
            .map_or(0, |(_, c)| *c)
    };
    if latest.1 < count_of(latest.0) || count_of(latest.0) == 0 {
        // More episodes follow in this season but their state is not known.
        return None;
    }
    seasons
        .iter()
        .filter(|(n, c)| *n > latest.0 && *c > 0)
        .map(|(n, _)| *n)
        .min()
        .map(|n| (n, 1))
}

/// Prefer the most recently resumed incomplete regular episode over binary
/// next-up. Callers supply only known-unwatched, positive-position checkpoints;
/// watched replays and absent/unknown progress must not enter this candidate set.
/// Specials never displace next-up. Timestamp ties prefer later coordinates,
/// independent of input order. `None` means fall back to [`next_up`].
pub fn latest_resume(
    candidates: impl IntoIterator<Item = ((u32, u32), i64)>,
) -> Option<(u32, u32)> {
    candidates
        .into_iter()
        .filter(|((season, _), _)| *season > 0)
        .max_by_key(|(coordinates, updated_at)| (*updated_at, *coordinates))
        .map(|(coordinates, _)| coordinates)
}

/// Season to show. An explicit choice wins when it exists; otherwise the
/// next-up season, else the first regular season, else the first season.
/// `seasons` is `(number, episode_count)`.
pub fn default_season(
    chosen: Option<u32>,
    next_up: Option<(u32, u32)>,
    seasons: &[(u32, u32)],
) -> Option<u32> {
    let available = |n: u32| seasons.iter().any(|(s, count)| *s == n && *count > 0);
    chosen
        .filter(|n| available(*n))
        .or_else(|| next_up.map(|(s, _)| s).filter(|n| available(*n)))
        .or_else(|| {
            seasons
                .iter()
                .filter(|(n, count)| *n > 0 && *count > 0)
                .map(|(n, _)| *n)
                .min()
        })
        .or_else(|| {
            seasons
                .iter()
                .filter(|(_, count)| *count > 0)
                .map(|(n, _)| *n)
                .min()
        })
}

/// Watched count over a total.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tally {
    pub watched: usize,
    pub total: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchedState {
    None,
    Partial,
    All,
}

impl Tally {
    /// `None` when there is nothing to watch, so an empty set never reads as "All".
    pub fn state(self) -> Option<WatchedState> {
        if self.total == 0 {
            return None;
        }
        let watched = self.watched.min(self.total);
        Some(match watched {
            0 => WatchedState::None,
            w if w == self.total => WatchedState::All,
            _ => WatchedState::Partial,
        })
    }
    /// Watched fraction in `0.0..=1.0`.
    pub fn fraction(self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.watched.min(self.total) as f32 / self.total as f32
        }
    }
}

/// Tally of one season, only when every one of its `total` episodes is known.
pub fn season_tally(marks: &[EpisodeMark], season: u32, total: u32) -> Option<Tally> {
    let known: BTreeMap<u32, bool> = marks
        .iter()
        .filter(|m| m.season == season)
        .map(|m| (m.number, m.watched))
        .collect();
    (total > 0 && known.len() >= total as usize).then(|| Tally {
        watched: known.values().filter(|w| **w).count(),
        total: known.len(),
    })
}

/// Rows the season grid may use while collapsed.
pub const COLLAPSED_ROWS: usize = 2;

/// Indices of the season tiles to show, in display order.
///
/// `complete[i]` is whether tile `i` is fully watched. When the tiles fit in two
/// rows, or `expanded`, all are shown. Otherwise fully-watched tiles before the
/// `anchor` (next-up) tile are folded away so it starts the first row, then the
/// list is cut to two rows; the `selected` tile is always kept visible.
pub fn visible_seasons(
    complete: &[bool],
    anchor: Option<usize>,
    selected: Option<usize>,
    cols: usize,
    expanded: bool,
) -> Vec<usize> {
    let cap = cols.max(1) * COLLAPSED_ROWS;
    if expanded || complete.len() <= cap {
        return (0..complete.len()).collect();
    }
    let mut shown: Vec<usize> = (0..complete.len())
        .filter(|i| !(complete[*i] && anchor.is_some_and(|a| *i < a)))
        .collect();
    shown.truncate(cap);
    if let Some(sel) = selected.filter(|s| *s < complete.len() && !shown.contains(s)) {
        // Keep display order: swap out the last tile for the selected one.
        shown.pop();
        shown.push(sel);
        shown.sort_unstable();
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(season: u32, number: u32, watched: bool) -> EpisodeMark {
        EpisodeMark {
            season,
            number,
            watched,
        }
    }

    #[test]
    fn latest_resume_orders_by_recency_then_coordinates_and_ignores_specials() {
        assert_eq!(latest_resume([]), None);
        assert_eq!(latest_resume([((0, 1), 999)]), None);
        assert_eq!(
            latest_resume([((1, 2), 20), ((2, 1), 10), ((0, 9), 999)]),
            Some((1, 2))
        );
        let tied = [((2, 1), 20), ((1, 9), 20), ((2, 2), 20)];
        assert_eq!(latest_resume(tied), Some((2, 2)));
        assert_eq!(latest_resume(tied.into_iter().rev()), Some((2, 2)));
    }

    #[test]
    fn next_up_follows_latest_watched() {
        let seasons = [(1, 4)];
        let marks = [m(1, 1, true), m(1, 2, true), m(1, 3, false), m(1, 4, false)];
        assert_eq!(next_up(&marks, &seasons), Some((1, 3)));
        // A gap before the latest watched episode is not next-up.
        let marks = [m(1, 1, false), m(1, 2, true), m(1, 3, false)];
        assert_eq!(next_up(&marks, &seasons), Some((1, 3)));
    }

    #[test]
    fn next_up_orders_across_seasons_regardless_of_input_order() {
        let marks = [m(2, 1, false), m(1, 2, true), m(1, 1, true), m(2, 2, false)];
        assert_eq!(next_up(&marks, &[(1, 2), (2, 2)]), Some((2, 1)));
    }

    #[test]
    fn next_up_ignores_specials_and_handles_empty_cases() {
        let seasons = [(0, 2), (1, 2)];
        assert_eq!(next_up(&[], &seasons), None);
        // Nothing tracked as watched.
        assert_eq!(next_up(&[m(1, 1, false), m(1, 2, false)], &seasons), None);
        // Watched special alone, or an unwatched special after, never counts.
        assert_eq!(next_up(&[m(0, 1, true), m(1, 1, false)], &seasons), None);
        assert_eq!(next_up(&[m(1, 1, true), m(0, 1, false)], &seasons), None);
    }

    #[test]
    fn finishing_a_season_continues_with_the_next_regular_season() {
        let seasons = [(0, 3), (1, 2), (2, 0), (3, 5)];
        // Last episode of S1 watched; S2 is empty so S3 E1 follows, even with
        // the later seasons' states unknown.
        let marks = [m(1, 1, true), m(1, 2, true)];
        assert_eq!(next_up(&marks, &seasons), Some((3, 1)));
        // Known unwatched episodes win over the season-boundary rule.
        let marks = [m(1, 1, true), m(1, 2, true), m(3, 1, false)];
        assert_eq!(next_up(&marks, &seasons), Some((3, 1)));
        // Mid-season with unknown followers: nothing is guessed.
        assert_eq!(next_up(&[m(1, 1, true)], &seasons), None);
        // Unknown season counts never invent a next season.
        assert_eq!(next_up(&[m(1, 2, true)], &[]), None);
    }

    #[test]
    fn everything_watched_has_no_next_up() {
        let seasons = [(0, 1), (1, 2), (2, 1)];
        let marks = [m(1, 1, true), m(1, 2, true), m(2, 1, true)];
        assert_eq!(next_up(&marks, &seasons), None);
        // Specials after the final season don't revive it.
        let marks = [m(2, 1, true), m(0, 1, false)];
        assert_eq!(next_up(&marks, &seasons), None);
    }

    #[test]
    fn default_season_prefers_choice_then_next_up_then_first_regular() {
        let seasons = [(0, 3), (1, 10), (2, 10), (3, 8)];
        assert_eq!(default_season(Some(1), Some((3, 2)), &seasons), Some(1));
        assert_eq!(default_season(None, Some((3, 2)), &seasons), Some(3));
        assert_eq!(default_season(None, None, &seasons), Some(1));
        // Unavailable choices and next-up fall through.
        assert_eq!(default_season(Some(9), Some((7, 1)), &seasons), Some(1));
        assert_eq!(default_season(None, None, &[(0, 2)]), Some(0));
        assert_eq!(default_season(None, None, &[(1, 0)]), None);
    }

    #[test]
    fn tri_state_derivation() {
        let t = |watched, total| Tally { watched, total }.state();
        assert_eq!(t(0, 0), None);
        assert_eq!(t(0, 5), Some(WatchedState::None));
        assert_eq!(t(2, 5), Some(WatchedState::Partial));
        assert_eq!(t(5, 5), Some(WatchedState::All));
        assert_eq!(t(9, 5), Some(WatchedState::All));
        assert_eq!(
            Tally {
                watched: 1,
                total: 4
            }
            .fraction(),
            0.25
        );
        assert_eq!(
            Tally {
                watched: 0,
                total: 0
            }
            .fraction(),
            0.0
        );
    }

    #[test]
    fn season_tally_requires_every_episode_known() {
        let marks = [m(1, 1, true), m(1, 2, false), m(2, 1, true)];
        assert_eq!(
            season_tally(&marks, 1, 2),
            Some(Tally {
                watched: 1,
                total: 2
            })
        );
        assert_eq!(season_tally(&marks, 1, 3), None);
        assert_eq!(season_tally(&marks, 3, 2), None);
        assert_eq!(season_tally(&marks, 1, 0), None);
    }

    #[test]
    fn season_window_folds_watched_before_next_up_and_caps_rows() {
        // 8 tiles, 3 columns → 6 visible. Seasons 0..=3 complete, next-up is 4.
        let complete = [true, true, true, true, false, false, false, false];
        assert_eq!(
            visible_seasons(&complete, Some(4), Some(4), 3, false),
            vec![4, 5, 6, 7]
        );
        // More tiles than two rows after folding: cut to the cap.
        let complete = [true, true, false, false, false, false, false, false, false];
        assert_eq!(
            visible_seasons(&complete, Some(2), Some(2), 3, false),
            vec![2, 3, 4, 5, 6, 7]
        );
        // Incomplete seasons before next-up are never folded.
        let complete = [true, false, true, false, false, false, false, false];
        assert_eq!(
            visible_seasons(&complete, Some(3), None, 3, false),
            vec![1, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn season_window_keeps_selection_and_expands() {
        let complete = [false; 9];
        // No next-up: first two rows, but the selected tile stays reachable.
        assert_eq!(
            visible_seasons(&complete, None, Some(8), 3, false),
            vec![0, 1, 2, 3, 4, 8]
        );
        // A folded selected tile comes back in order.
        let complete = [true, true, true, false, false, false, false, false, false];
        assert_eq!(
            visible_seasons(&complete, Some(3), Some(0), 3, false),
            vec![0, 3, 4, 5, 6, 7]
        );
        assert_eq!(visible_seasons(&complete, Some(3), None, 3, true).len(), 9);
        // Fits in two rows: nothing is hidden.
        assert_eq!(
            visible_seasons(&[true, true, false], Some(2), None, 2, false).len(),
            3
        );
    }
}

/// Canonical identity only; unmatched files have no durable progress target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackProgressTarget {
    Movie(crate::library::TitleId),
    Episode(crate::library::EpisodeId),
}

/// Confirmed observation, in integer milliseconds. Construct only after the
/// player reports a real position (not a loading default or speculative seek).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackObservation {
    position_ms: i64,
    duration_ms: Option<i64>,
}
impl PlaybackObservation {
    pub fn new(position_ms: i64, duration_ms: Option<i64>) -> Option<Self> {
        (position_ms >= 0 && duration_ms.is_none_or(|v| v > 0)).then_some(Self {
            position_ms,
            duration_ms,
        })
    }
    /// Invalid/unknown duration becomes unknown; invalid position rejects the sample.
    pub fn from_seconds(position: f64, duration: Option<f64>) -> Option<Self> {
        fn millis(value: f64) -> Option<i64> {
            let value = value * 1000.0;
            (value.is_finite() && value >= 0.0 && value < i64::MAX as f64).then_some(value as i64)
        }
        Self::new(
            millis(position)?,
            duration.and_then(millis).filter(|v| *v > 0),
        )
    }
    pub const fn position_ms(self) -> i64 {
        self.position_ms
    }
    pub const fn duration_ms(self) -> Option<i64> {
        self.duration_ms
    }
    pub fn qualifies_watched(self, suppressed: bool) -> bool {
        !suppressed
            && self.duration_ms.is_some_and(|duration| {
                i128::from(self.position_ms) * 10 >= i128::from(duration) * 9
            })
    }
}

#[cfg(test)]
mod playback_tests {
    use super::*;
    fn required(value: Option<PlaybackObservation>) -> PlaybackObservation {
        match value {
            Some(value) => value,
            None => panic!("invalid fixture"),
        }
    }
    #[test]
    fn confirmed_threshold_and_invalid_values() {
        assert!(!required(PlaybackObservation::new(899, Some(1000))).qualifies_watched(false));
        let at = required(PlaybackObservation::new(900, Some(1000)));
        assert!(at.qualifies_watched(false));
        assert!(!at.qualifies_watched(true));
        assert!(!required(PlaybackObservation::new(i64::MAX, None)).qualifies_watched(false));
        assert!(PlaybackObservation::new(-1, None).is_none());
        assert!(PlaybackObservation::new(1, Some(0)).is_none());
        for invalid in [f64::NAN, f64::INFINITY, -1.0, f64::MAX] {
            assert!(PlaybackObservation::from_seconds(invalid, None).is_none());
            assert_eq!(
                required(PlaybackObservation::from_seconds(1.0, Some(invalid))).duration_ms(),
                None
            );
        }
        assert!(
            required(PlaybackObservation::new(i64::MAX, Some(i64::MAX))).qualifies_watched(false)
        );
    }
}
