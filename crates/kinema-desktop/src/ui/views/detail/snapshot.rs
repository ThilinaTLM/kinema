//! Pure whole-show values derived from the coordinator, shared by every
//! element of one overview build (and by every build until the next change).
//!
//! The overview used to recompute watched state, saved checkpoints, the
//! continue target and season summaries per card and per tile on every
//! repaint, including wheel-scroll repaints. Everything here is computed by
//! the same functions as before, once per coordinator revision. No elements,
//! handlers or menus are cached: those are rebuilt from these values.
//!
//! The snapshot is keyed by the coordinator notification revision
//! (`Detail::data_rev`) and the season options it was built for. All inputs
//! (metadata, personal state, local detail, TMDB seasons, settings) live in the
//! coordinator, and every change to them notifies it. Artwork is deliberately
//! absent: its completion does not bump the revision.
use super::episodes::year_span;
use super::progress::{ResumePoint, visible_resume};
use super::*;
use crate::ui::blocks::state_toggle::ToggleStatus;
use kinema_core::watch_progress::{self, EpisodeMark};

/// Everything the UI shows about one episode's personal state. `status` keeps
/// Loading, Failed and Known{pending} distinct; `known_watched` is `None`
/// while unknown, never "unwatched".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EpisodeView {
    pub status: ToggleStatus,
    pub known_watched: Option<bool>,
    /// Raw saved checkpoint, shown or not.
    pub saved: Option<ResumePoint>,
    /// Checkpoint to present (known-unwatched and resumable only).
    pub visible: Option<ResumePoint>,
}

pub(super) struct ShowSnapshot {
    rev: u64,
    options: Vec<(u32, u32)>,
    episodes: HashMap<(u32, u32), EpisodeView>,
    pub marks: Vec<EpisodeMark>,
    pub next_up: Option<(u32, u32)>,
    years: HashMap<u32, Option<String>>,
    posters: HashMap<u32, Option<String>>,
    /// Embedded player (or settings still unknown): checkpoints can be resumed.
    pub embedded: bool,
    /// The show identity is known, so writes are possible.
    pub title_ready: bool,
}

impl ShowSnapshot {
    pub(super) fn episode(&self, season: u32, number: u32) -> Option<EpisodeView> {
        self.episodes.get(&(season, number)).copied()
    }
}

impl Detail {
    /// The snapshot for the current revision and season options.
    pub(super) fn show_snapshot(&self, cx: &App) -> Rc<ShowSnapshot> {
        if let Some(snapshot) = self.snapshot_memo.borrow().as_ref()
            && snapshot.rev == self.data_rev
            && snapshot.options == self.season_options
        {
            return snapshot.clone();
        }
        let snapshot = Rc::new(self.build_snapshot(cx));
        *self.snapshot_memo.borrow_mut() = Some(snapshot.clone());
        snapshot
    }

    /// Personal state of one episode. Episodes outside the snapshot (none are
    /// expected) are computed directly.
    pub(super) fn episode_view(&self, season: u32, number: u32, cx: &App) -> EpisodeView {
        self.show_snapshot(cx)
            .episode(season, number)
            .unwrap_or_else(|| self.episode_view_direct(season, number, cx))
    }

    pub(super) fn season_years_cached(&self, season: u32, cx: &App) -> Option<String> {
        match self.show_snapshot(cx).years.get(&season) {
            Some(years) => years.clone(),
            None => self.season_years(season, cx),
        }
    }

    pub(super) fn season_poster_cached(&self, season: u32, cx: &App) -> Option<String> {
        match self.show_snapshot(cx).posters.get(&season) {
            Some(poster) => poster.clone(),
            None => self.season_poster(season, cx),
        }
    }

    fn episode_view_direct(&self, season: u32, number: u32, cx: &App) -> EpisodeView {
        let saved = self.saved_progress_direct(Some((season, number)), cx);
        let known_watched = self.episode_known_watched(season, number, cx);
        EpisodeView {
            status: self.episode_status(season, number, cx),
            known_watched,
            saved,
            visible: visible_resume(saved, known_watched),
        }
    }

    fn build_snapshot(&self, cx: &App) -> ShowSnapshot {
        #[cfg(all(test, feature = "ui-probe-test"))]
        super::work::count(super::work::Section::SnapshotBuild, 1);
        let options = self.season_options.clone();
        // One whole-show collection feeds coordinates and season years.
        let all = self.all_episodes(cx);
        let coordinates = self.coordinates_of(&all, &options);
        // Local rows are indexed once; the first row of a coordinate wins, as
        // the direct lookup's `find` does.
        let mut local_watch = HashMap::new();
        if let Some(detail) = self.local_detail(cx) {
            for e in &detail.episodes {
                local_watch
                    .entry((e.season, e.metadata.number))
                    .or_insert_with(|| e.personal.watch.is_some_and(|w| w.watched));
            }
        }
        let title_ready = self.title_id(cx).is_some();
        let mut episodes = HashMap::with_capacity(coordinates.len());
        let mut marks = Vec::new();
        for &(season, number) in &coordinates {
            let status = self.episode_status(season, number, cx);
            let known_watched = match status {
                ToggleStatus::Known { on, .. } => Some(on),
                ToggleStatus::Failed
                    if title_ready
                        && self
                            .coordinator
                            .read(cx)
                            .episode_has_no_row(&self.episode_ref(season, number)) =>
                {
                    Some(false)
                }
                _ => local_watch.get(&(season, number)).copied(),
            };
            let saved = self.saved_progress_direct(Some((season, number)), cx);
            if let Some(watched) = known_watched {
                marks.push(EpisodeMark {
                    season,
                    number,
                    watched,
                });
            }
            episodes.insert(
                (season, number),
                EpisodeView {
                    status,
                    known_watched,
                    saved,
                    visible: visible_resume(saved, known_watched),
                },
            );
        }
        let resumed = watch_progress::latest_resume(
            coordinates
                .iter()
                .filter_map(|c| Some((*c, episodes.get(c)?.visible?.updated_at))),
        );
        let next_up = resumed.or_else(|| watch_progress::next_up(&marks, &options));
        let mut released: HashMap<u32, Vec<&str>> = HashMap::new();
        for e in &all {
            if let Some(date) = e.released.as_deref() {
                released.entry(e.season).or_default().push(date);
            }
        }
        let mut years = HashMap::new();
        let mut posters = HashMap::new();
        for (season, _) in &options {
            let span =
                year_span(released.get(season).into_iter().flatten().copied()).or_else(|| {
                    self.tmdb_season_summary(*season, cx)?
                        .year()
                        .map(|y| y.to_string())
                });
            years.insert(*season, span);
            posters.insert(*season, self.season_poster(*season, cx));
        }
        ShowSnapshot {
            rev: self.data_rev,
            options,
            episodes,
            marks,
            next_up,
            years,
            posters,
            embedded: self.embedded_player(cx),
            title_ready,
        }
    }
}
