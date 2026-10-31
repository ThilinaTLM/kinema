//! Test-only counters for derived-data work done while building the overview.
//! They make "scrolling must not recompute data models" a reproducible assertion.
use std::cell::Cell;

#[derive(Clone, Copy)]
pub(super) enum Section {
    /// Overview element tree builds.
    Overview,
    /// `facts_view` model computations.
    FactsModel,
    /// Episode list collect/sort computations.
    EpisodeList,
    /// Episode cards constructed.
    EpisodeCards,
    /// Uncached `episode_status` coordinator lookups.
    EpisodeStatus,
    /// Uncached `episode_known_watched` evaluations.
    KnownWatched,
    /// `episode_coordinates` set builds.
    EpisodeCoordinates,
    /// `all_episodes` whole-show collections.
    AllEpisodes,
    /// Uncached `saved_progress` lookups.
    SavedProgress,
    /// Artwork cache lookups made while resolving a chain.
    ArtLookup,
    /// Whole-show personal snapshot builds.
    SnapshotBuild,
}
const SECTIONS: usize = 11;

/// Timed regions, accumulated in nanoseconds.
#[derive(Clone, Copy)]
pub(super) enum Timed {
    /// `episodes_section` (tabs, header and every card).
    EpisodesSection,
    /// Only the per-card construction loop.
    CardLoop,
    /// The season tile grid.
    SeasonSelector,
    /// Hero (primary action, tally, metadata).
    Hero,
}
const TIMERS: usize = 4;

thread_local! {
    static COUNTS: [Cell<usize>; SECTIONS] = const { [const { Cell::new(0) }; SECTIONS] };
    static NANOS: [Cell<u128>; TIMERS] = const { [const { Cell::new(0) }; TIMERS] };
}
pub(super) fn count(section: Section, n: usize) {
    COUNTS.with(|c| c[section as usize].set(c[section as usize].get() + n));
}
pub(super) fn get(section: Section) -> usize {
    COUNTS.with(|c| c[section as usize].get())
}
pub(super) fn reset() {
    COUNTS.with(|c| c.iter().for_each(|v| v.set(0)));
    NANOS.with(|c| c.iter().for_each(|v| v.set(0)));
}
/// Accumulated wall time of `timed` in microseconds.
pub(super) fn micros(timed: Timed) -> u128 {
    NANOS.with(|c| c[timed as usize].get()) / 1_000
}
/// Adds the guard's lifetime to `timed` when dropped.
pub(super) struct Stopwatch(Timed, std::time::Instant);
pub(super) fn stopwatch(timed: Timed) -> Stopwatch {
    Stopwatch(timed, std::time::Instant::now())
}
impl Drop for Stopwatch {
    fn drop(&mut self) {
        let elapsed = self.1.elapsed().as_nanos();
        NANOS.with(|c| c[self.0 as usize].set(c[self.0 as usize].get() + elapsed));
    }
}
