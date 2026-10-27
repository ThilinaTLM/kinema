//! Library membership and manual watched controls. All state lives in the shared
//! coordinator; this module only maps it to controls, notices and confirmations.
use super::*;
use crate::widgets::{menu::MenuItem, patterns::menu_item};
use crate::{
    screens::coordinator::{EpisodeRef, PersonalLoad, SeasonSnapshot},
    widgets::{
        overlay::OverlayButton,
        state_toggle::ToggleStatus,
        watch_mark::{WatchMark, WatchMarkSize, WatchMarkState},
    },
};
use kinema_core::library::TitleId;

/// The last write the user asked for, kept so Retry repeats exactly that intent.
#[derive(Clone, Debug)]
pub(super) enum Write {
    Membership(bool),
    Movie(bool),
    Episode(EpisodeRef, bool),
    /// Exact frozen season snapshot, never a recomputed one.
    Season(SeasonSnapshot, bool),
}

/// A season bulk change applied as soon as its snapshot is ready (no confirmation
/// step, no undo). `failure` keeps a rejected request visible with Retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SeasonConfirm {
    pub season: u32,
    pub watched: bool,
    pub failure: Option<String>,
}

pub(super) fn season_label(season: u32) -> String {
    if season == 0 {
        "Specials".into()
    } else {
        format!("Season {season}")
    }
}

/// What activating an episode mark does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MarkIntent {
    Set(bool),
    Retry,
    None,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MarkTip {
    pub text: &'static str,
    pub intent: MarkIntent,
}

/// Tooltip and intent of an episode mark. Unknown state is never writable.
pub(super) fn mark_tooltip(status: ToggleStatus, ready: bool) -> MarkTip {
    let (text, intent) = match status {
        ToggleStatus::Known { on, pending } => (
            if on { "Mark unwatched" } else { "Mark watched" },
            if pending || !ready {
                MarkIntent::None
            } else {
                MarkIntent::Set(!on)
            },
        ),
        ToggleStatus::Failed => ("Watched status unavailable. Retry", MarkIntent::Retry),
        ToggleStatus::Loading => ("Loading watched status\u{2026}", MarkIntent::None),
    };
    MarkTip { text, intent }
}

/// Why the season watched toggle is disabled, when it is.
pub(super) const TRACKING_UNAVAILABLE: &str =
    "Watched tracking isn\u{2019}t available for this title yet.";

/// `Loading` and `Failed` stay distinct from off: unknown is never "not saved".
fn status<T>(load: &PersonalLoad<T>, pending: bool, on: impl Fn(&T) -> bool) -> ToggleStatus {
    match load {
        PersonalLoad::Loading => ToggleStatus::Loading,
        PersonalLoad::Failed(_) => ToggleStatus::Failed,
        PersonalLoad::Ready(value) => ToggleStatus::Known {
            on: on(value),
            pending,
        },
    }
}

impl Detail {
    pub(super) fn title_id(&self, cx: &App) -> Option<TitleId> {
        match self.coordinator.read(cx).personal(&self.title).state {
            PersonalLoad::Ready(t) => Some(t.title_id),
            _ => None,
        }
    }
    /// Episode identity: known canonical show + season/episode coordinates.
    pub(super) fn episode_ref(&self, season: u32, number: u32) -> EpisodeRef {
        EpisodeRef::Coordinates {
            title: self.title.personal_ref(),
            season,
            episode: number,
        }
    }
    pub(super) fn membership_status(&self, cx: &App) -> ToggleStatus {
        let v = self.coordinator.read(cx).personal(&self.title);
        status(&v.state, v.pending, |t| t.added_at.is_some())
    }
    pub(super) fn movie_watched_status(&self, cx: &App) -> ToggleStatus {
        let v = self.coordinator.read(cx).personal(&self.title);
        status(&v.state, v.pending, |t| {
            t.movie_watch.is_some_and(|w| w.watched)
        })
    }
    pub(super) fn episode_status(&self, season: u32, number: u32, cx: &App) -> ToggleStatus {
        let c = self.coordinator.read(cx);
        let v = c.episode_personal(&self.episode_ref(season, number));
        // Episode reads wait for the show identity; a failed show read is a failure.
        if matches!(v.state, PersonalLoad::Loading)
            && matches!(c.personal(&self.title).state, PersonalLoad::Failed(_))
        {
            return ToggleStatus::Failed;
        }
        status(&v.state, v.pending, |e| e.watch.is_some_and(|w| w.watched))
    }

    pub(super) fn ensure_personal(&mut self, cx: &mut Context<Self>) {
        let title = self.title.clone();
        self.coordinator
            .update(cx, |c, cx| c.load_personal(&[title], cx));
    }
    /// Request the displayed season's episode state once its show identity is known.
    pub(super) fn ensure_episode_personal(&mut self, cx: &mut Context<Self>) {
        if !self.is_series() {
            return;
        }
        let (Some(id), Some(_)) = (self.title_id(cx), self.season) else {
            return;
        };
        let episodes = self.personal_scope(cx);
        // Request only episodes not yet asked for, so episodes that arrive later in
        // the same season (refresh/enrichment) are never left Loading.
        let missing: Vec<_> = episodes
            .iter()
            .filter(|(s, n)| !self.episode_personal_requested.contains(&(id, *s, *n)))
            .copied()
            .collect();
        if missing.is_empty() {
            return;
        }
        let refs: Vec<_> = missing
            .iter()
            .map(|(s, n)| self.episode_ref(*s, *n))
            .collect();
        self.episode_personal_requested
            .extend(missing.into_iter().map(|(s, n)| (id, s, n)));
        self.coordinator
            .update(cx, |c, cx| c.load_episode_personal(&refs, cx));
    }
    /// Episode coordinates whose watched state is read: everything the provider
    /// listed, plus (for TMDB, which loads one season at a time) `1..=count` of
    /// every season from the season summaries. Unloaded seasons are therefore
    /// covered by one batched read instead of one per season.
    pub(super) fn episode_coordinates(&self, options: &[(u32, u32)], cx: &App) -> Vec<(u32, u32)> {
        let mut set: std::collections::BTreeSet<(u32, u32)> = self
            .all_episodes(cx)
            .iter()
            .map(|e| (e.season, e.number))
            .collect();
        if matches!(self.title, BrowseTitle::Tmdb(_)) {
            for (season, count) in options {
                set.extend((1..=*count).map(|n| (*season, n)));
            }
        }
        set.into_iter().collect()
    }
    /// Coordinates requested from the store; very long shows read the shown season only.
    fn personal_scope(&self, cx: &App) -> Vec<(u32, u32)> {
        const WHOLE_SHOW_MAX: usize = 2_000;
        let all = self.episode_coordinates(&self.season_options, cx);
        if all.len() <= WHOLE_SHOW_MAX && !matches!(self.title, BrowseTitle::Local(_)) {
            return all;
        }
        self.episodes(cx)
            .iter()
            .map(|e| (e.season, e.number))
            .collect()
    }
    /// Known watched state of one episode; unknown (loading/failed) is `None`,
    /// never unwatched. Local titles fall back to their persisted detail, and an
    /// episode with no stored row is unwatched by construction.
    pub(super) fn episode_known_watched(&self, season: u32, number: u32, cx: &App) -> Option<bool> {
        match self.episode_status(season, number, cx) {
            ToggleStatus::Known { on, .. } => return Some(on),
            ToggleStatus::Failed
                if self.title_id(cx).is_some()
                    && self
                        .coordinator
                        .read(cx)
                        .episode_has_no_row(&self.episode_ref(season, number)) =>
            {
                return Some(false);
            }
            _ => {}
        }
        self.local_detail(cx)?
            .episodes
            .iter()
            .find(|e| e.season == season && e.metadata.number == number)
            .map(|e| e.personal.watch.is_some_and(|w| w.watched))
    }
    /// Every episode with a known watched state.
    pub(super) fn episode_marks(
        &self,
        options: &[(u32, u32)],
        cx: &App,
    ) -> Vec<kinema_core::watch_progress::EpisodeMark> {
        self.episode_coordinates(options, cx)
            .into_iter()
            .filter_map(|(season, number)| {
                Some(kinema_core::watch_progress::EpisodeMark {
                    season,
                    number,
                    watched: self.episode_known_watched(season, number, cx)?,
                })
            })
            .collect()
    }
    pub(super) fn retry_personal(&mut self, cx: &mut Context<Self>) {
        self.episode_personal_requested.clear();
        self.ensure_personal(cx);
        self.ensure_episode_personal(cx);
        cx.notify();
    }
    /// Submit one write. A synchronous rejection keeps exactly this intent for Retry;
    /// asynchronous commit failures are retried by the coordinator's own record.
    pub(super) fn perform(&mut self, write: Write, cx: &mut Context<Self>) {
        // Acting on the shown season keeps it shown (no smart-default jump).
        self.season_chosen |= matches!(write, Write::Episode(..) | Write::Season(..));
        let title = self.title.clone();
        let result = self.coordinator.update(cx, |c, cx| match &write {
            Write::Membership(on) => c.set_membership(&title, *on, cx),
            Write::Movie(on) => c.set_movie_watched(&title, *on, cx),
            Write::Episode(reference, on) => c.set_episode_watched(reference.clone(), *on, cx),
            Write::Season(snapshot, on) => c.set_season_watched(snapshot.clone(), *on, cx),
        });
        self.write_error = result.err().map(|error| (write, error));
        cx.notify();
    }
    pub(super) fn membership_toggle(
        &self,
        cx: &mut Context<Self>,
    ) -> crate::widgets::library_toggle::LibraryToggle {
        use crate::widgets::library_toggle::LibraryToggle;
        let (change, retry) = (cx.entity().downgrade(), cx.entity().downgrade());
        LibraryToggle::new(
            "detail-library",
            self.membership_status(cx),
            self.title.name().to_owned(),
        )
        .on_change(move |wanted, _, cx| {
            let _ = change.update(cx, |d, cx| d.perform(Write::Membership(wanted), cx));
        })
        .on_retry(move |_, cx| {
            let _ = retry.update(cx, |d, cx| d.retry_personal(cx));
        })
    }
    /// Top-right watched mark on an episode still.
    pub(super) fn episode_mark(
        &self,
        season: u32,
        number: u32,
        cx: &mut Context<Self>,
    ) -> OverlayButton {
        let reference = self.episode_ref(season, number);
        let ready = self.title_id(cx).is_some();
        let status = self.episode_status(season, number, cx);
        let tip = mark_tooltip(status, ready);
        let on = matches!(status, ToggleStatus::Known { on: true, .. });
        let this = cx.entity().downgrade();
        OverlayButton::new(
            ElementId::Name(format!("episode-watched-s{season}-e{number}").into()),
            tip.text,
        )
        .accessible_name(format!(
            "{} \u{b7} episode S{season:02}E{number:02}",
            tip.text
        ))
        .child(
            WatchMark::new(WatchMarkState::from_watched(on))
                .size(WatchMarkSize::S18)
                .over_art(true),
        )
        .inert(tip.intent == MarkIntent::None)
        .on_click(move |_, _, cx| {
            let _ = this.update(cx, |d, cx| match tip.intent {
                MarkIntent::Set(wanted) => d.perform(Write::Episode(reference.clone(), wanted), cx),
                MarkIntent::Retry => d.retry_personal(cx),
                MarkIntent::None => {}
            });
        })
    }

    /// Text commands mirroring the icon toggles, for the keyboard/context menu.
    /// Unknown, failed or pending state offers no write command.
    pub(super) fn hero_commands(&self, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let weak = cx.entity().downgrade();
        let mut items = Vec::new();
        let command = |label: &'static str, write: Write| {
            let weak = weak.clone();
            menu_item(label, move |_, cx| {
                let write = write.clone();
                let _ = weak.update(cx, |d, cx| d.perform(write, cx));
            })
        };
        if let ToggleStatus::Known { on, pending: false } = self.membership_status(cx) {
            items.push(if on {
                command("Remove from Library", Write::Membership(false))
            } else {
                command("Add to Library", Write::Membership(true))
            });
        }
        if !self.is_series()
            && let ToggleStatus::Known { on, pending: false } = self.movie_watched_status(cx)
        {
            items.push(if on {
                command("Mark as unwatched", Write::Movie(false))
            } else {
                command("Mark as watched", Write::Movie(true))
            });
        }
        items
    }

    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(super) fn hero_commands_is_empty(&self, cx: &App) -> bool {
        !matches!(
            self.membership_status(cx),
            ToggleStatus::Known { pending: false, .. }
        )
    }

    /// Error / undo notices below the hero. Unknown or failed reads are never
    /// hidden: a failed personal read offers Retry without hiding metadata.
    pub(super) fn personal_notices(
        &self,
        viewport: Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use crate::widgets::notice::{Notice, NoticeKind};
        let c = self.coordinator.read(cx);
        let view = c.personal(&self.title);
        let mut stack = div().w_full().flex().flex_col().gap(s::S8).pt(s::S12);
        let mut any = false;
        if let PersonalLoad::Failed(error) = &view.state {
            any = true;
            stack = stack.child(
                Notice::new(NoticeKind::Warning, "Library status unavailable")
                    .body(error.clone())
                    .action(
                        Button::new("personal-retry", "Retry")
                            .on_click(cx.listener(|this, _, _, cx| this.retry_personal(cx))),
                    ),
            );
        }
        // A synchronous rejection retries exactly the rejected write.
        if let Some((write, error)) = self.write_error.clone() {
            any = true;
            stack = stack.child(
                Notice::new(NoticeKind::Warning, "Couldn’t save your change")
                    .body(error)
                    .action(Button::new("personal-write-retry", "Retry").on_click(
                        cx.listener(move |this, _, _, cx| this.perform(write.clone(), cx)),
                    )),
            );
        }
        // A bulk season change that could not start keeps its own Retry.
        if let Some(request) = self.season_confirm.clone()
            && let Some(error) = request.failure.clone()
        {
            any = true;
            stack = stack.child(
                Notice::new(
                    NoticeKind::Warning,
                    format!("Couldn\u{2019}t update {}", season_label(request.season)),
                )
                .body(error)
                .action(
                    Button::new("season-write-retry", "Retry").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.request_season(request.season, request.watched, cx)
                        },
                    )),
                ),
            );
        }
        // Commit failures, success and Undo use the shared MainWindow toast, so they
        // are never duplicated here.
        any.then(|| hero::body_shell(viewport, stack).into_any_element())
    }

    // ── season bulk actions ──────────────────────────────────────────────
    /// Apply a bulk season change immediately: freeze the exact persisted episode
    /// set first, then write it as soon as the snapshot arrives (`apply_season_request`).
    pub(super) fn request_season(&mut self, season: u32, watched: bool, cx: &mut Context<Self>) {
        let Some(id) = self.title_id(cx) else {
            return;
        };
        self.season_confirm = Some(SeasonConfirm {
            season,
            watched,
            failure: None,
        });
        self.coordinator
            .update(cx, |c, cx| c.load_season_snapshot(id, season, cx));
        cx.notify();
    }
    /// Complete a requested bulk change once its snapshot is ready. Called on every
    /// coordinator update; a snapshot for another season is never applied.
    pub(super) fn apply_season_request(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.season_confirm.clone().filter(|r| r.failure.is_none()) else {
            return;
        };
        let (snapshot, busy, error) = {
            let c = self.coordinator.read(cx);
            (
                c.season_snapshot
                    .value()
                    .filter(|s| s.season == request.season)
                    .cloned(),
                c.season_snapshot.busy(),
                c.season_snapshot.error().map(str::to_owned),
            )
        };
        if busy {
            return;
        }
        match (snapshot, error) {
            (_, Some(error)) => self.fail_season_request(request, error, cx),
            // Whole-series change: seasons with nothing saved are skipped.
            (Some(snapshot), None) if snapshot.episodes.is_empty() && self.series_active() => {
                self.coordinator
                    .update(cx, |c, _| c.season_snapshot.cancel());
                self.season_confirm = None;
                self.advance_series(cx);
            }
            (Some(snapshot), None) if snapshot.episodes.is_empty() => {
                self.fail_season_request(request, "No saved episodes to change yet.".to_owned(), cx)
            }
            (Some(snapshot), None) => {
                self.season_confirm = None;
                self.coordinator
                    .update(cx, |c, _| c.season_snapshot.cancel());
                self.season_chosen = true;
                self.series_applied = true;
                self.perform(Write::Season(snapshot, request.watched), cx);
                self.advance_series(cx);
            }
            (None, None) => {}
        }
    }
    fn fail_season_request(
        &mut self,
        request: SeasonConfirm,
        error: String,
        cx: &mut Context<Self>,
    ) {
        self.series_queue.clear();
        self.season_confirm = Some(SeasonConfirm {
            failure: Some(error),
            ..request
        });
        cx.notify();
    }
    fn series_active(&self) -> bool {
        !self.series_queue.is_empty() || self.series_applied
    }
    /// Mark every known season watched/unwatched, one season snapshot at a time
    /// (the library has no whole-series write). Applies immediately.
    pub(super) fn request_series(&mut self, watched: bool, cx: &mut Context<Self>) {
        let mut seasons: Vec<u32> = self
            .season_options
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(n, _)| *n)
            .collect();
        seasons.sort_unstable();
        self.series_queue = seasons;
        self.series_watched = watched;
        self.series_applied = false;
        self.advance_series(cx);
    }
    /// Start the next queued season; when the queue ends with nothing written,
    /// report that there was nothing saved to change.
    fn advance_series(&mut self, cx: &mut Context<Self>) {
        if self.series_queue.is_empty() {
            let none = !std::mem::take(&mut self.series_applied);
            if none && self.season_confirm.is_none() {
                self.season_confirm = Some(SeasonConfirm {
                    season: self.season.unwrap_or(1),
                    watched: self.series_watched,
                    failure: Some("No saved episodes to change yet.".to_owned()),
                });
                cx.notify();
            }
            return;
        }
        let season = self.series_queue.remove(0);
        self.request_season(season, self.series_watched, cx);
    }
    /// Watched tally over the whole series, only when the library knows its
    /// completeness (or every episode has a known state).
    pub(super) fn series_tally(&self, cx: &App) -> Option<kinema_core::watch_progress::Tally> {
        use kinema_core::watch_progress::Tally;
        if let PersonalLoad::Ready(t) = self.coordinator.read(cx).personal(&self.title).state
            && let Some(s) = t.show_summary
            && s.complete
            && s.total > 0
        {
            return Some(Tally {
                watched: s.watched,
                total: s.total,
            });
        }
        let total: usize = self
            .season_options
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(_, c)| *c as usize)
            .sum();
        let marks: Vec<_> = self
            .episode_marks(&self.season_options, cx)
            .into_iter()
            .filter(|m| m.season > 0)
            .collect();
        (total > 0 && marks.len() >= total).then(|| Tally {
            watched: marks.iter().filter(|m| m.watched).count(),
            total: marks.len(),
        })
    }
    /// Hero toggle for the whole series.
    pub(super) fn series_watched_toggle(
        &self,
        cx: &mut Context<Self>,
    ) -> crate::widgets::watched_toggle::WatchedToggle {
        use crate::widgets::watched_toggle::{WatchedLabels, WatchedToggle};
        let weak = cx.entity().downgrade();
        let pending = self.coordinator.read(cx).personal(&self.title).pending;
        let busy = self.season_request_busy() || self.series_active() || pending;
        let ready = self.title_id(cx).is_some();
        let toggle = match self.series_tally(cx) {
            Some(tally) if ready => WatchedToggle::new("detail-series-watched", tally),
            other => WatchedToggle::new(
                "detail-series-watched",
                other.unwrap_or(kinema_core::watch_progress::Tally {
                    watched: 0,
                    total: 0,
                }),
            )
            .unavailable(if ready {
                "Open each season to load its watched status."
            } else {
                TRACKING_UNAVAILABLE
            }),
        };
        toggle
            .labels(WatchedLabels::SERIES_HERO)
            .subject("series")
            .busy(busy)
            .on_change(move |wanted, _, cx| {
                let _ = weak.update(cx, |d, cx| d.request_series(wanted, cx));
            })
    }
    /// Hero toggle for a movie: one title, driving the existing watched state.
    pub(super) fn movie_hero_toggle(
        &self,
        cx: &mut Context<Self>,
    ) -> crate::widgets::watched_toggle::WatchedToggle {
        use crate::widgets::watched_toggle::{WatchedLabels, WatchedToggle};
        use kinema_core::watch_progress::Tally;
        let weak = cx.entity().downgrade();
        let toggle = |watched| {
            WatchedToggle::new("detail-watched", Tally { watched, total: 1 })
                .labels(WatchedLabels::MOVIE)
                .show_count(false)
        };
        match self.movie_watched_status(cx) {
            ToggleStatus::Known { on, pending } => toggle(usize::from(on))
                .subject(self.title.name().to_owned())
                .busy(pending)
                .on_change(move |wanted, _, cx| {
                    let _ = weak.update(cx, |d, cx| d.perform(Write::Movie(wanted), cx));
                }),
            ToggleStatus::Loading => toggle(0)
                .subject(self.title.name().to_owned())
                .unavailable("Loading watched status…"),
            _ => toggle(0)
                .subject(self.title.name().to_owned())
                .unavailable("Watched status isn\u{2019}t available right now."),
        }
    }
    /// A bulk change is waiting for its snapshot.
    pub(super) fn season_request_busy(&self) -> bool {
        self.season_confirm
            .as_ref()
            .is_some_and(|r| r.failure.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn mark_intent_follows_status_and_readiness() {
        let known = |on, pending| ToggleStatus::Known { on, pending };
        let t = mark_tooltip(known(false, false), true);
        assert_eq!((t.text, t.intent), ("Mark watched", MarkIntent::Set(true)));
        let t = mark_tooltip(known(true, false), true);
        assert_eq!(
            (t.text, t.intent),
            ("Mark unwatched", MarkIntent::Set(false))
        );
        assert_eq!(
            mark_tooltip(known(true, true), true).intent,
            MarkIntent::None
        );
        assert_eq!(
            mark_tooltip(known(false, false), false).intent,
            MarkIntent::None
        );
        assert_eq!(
            mark_tooltip(ToggleStatus::Loading, true).intent,
            MarkIntent::None
        );
        assert_eq!(
            mark_tooltip(ToggleStatus::Failed, true).intent,
            MarkIntent::Retry
        );
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test setup must succeed"
)]
mod interaction_tests {
    use super::*;
    use crate::screens::coordinator::AppCoordinator;
    use ::core::prelude::v1::test;
    use gpui::TestAppContext;

    /// The offline fixture coordinator has no catalog worker, so every write is
    /// rejected synchronously; each rejection must record exactly its own intent
    /// and leave the previously known personal snapshot untouched.
    #[gpui::test]
    fn rejected_writes_retry_only_their_own_intent(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let title = super::super::fixtures::local_fixture_title(true);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let detail = cx.new(|cx| Detail::fixture(coordinator.clone(), title, false, None, cx));
        detail.update(cx, |d, cx| {
            d.seed_personal_fixture("detail-live-local-series", cx)
        });
        let before = detail.read_with(cx, |d, cx| d.membership_status(cx));
        assert_eq!(
            before,
            ToggleStatus::Known {
                on: true,
                pending: false
            }
        );
        let snapshot = SeasonSnapshot {
            title_id: kinema_core::library::TitleId::new(8).unwrap(),
            season_id: kinema_core::library::SeasonId::new(1).unwrap(),
            season: 1,
            episodes: vec![kinema_core::library::EpisodeId::new(1101).unwrap()],
            title_identifiers: Vec::new(),
            episode_numbers: vec![1],
        };
        for write in [Write::Membership(false), Write::Season(snapshot, true)] {
            detail.update(cx, |d, cx| d.perform(write.clone(), cx));
            detail.read_with(cx, |d, _| {
                let (kept, error) = d.write_error.as_ref().expect("rejected");
                assert!(!error.is_empty());
                match (kept, &write) {
                    (Write::Membership(a), Write::Membership(b)) => assert_eq!(a, b),
                    (Write::Season(a, x), Write::Season(b, y)) => {
                        assert_eq!((a, x), (b, y))
                    }
                    other => panic!("stale intent kept: {other:?}"),
                }
            });
            // Rejection never mutates the shared snapshot.
            assert_eq!(
                detail.read_with(cx, |d, cx| d.membership_status(cx)),
                before
            );
        }
    }

    #[gpui::test]
    fn unknown_and_failed_reads_are_not_writable_or_checked(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let title = super::super::fixtures::local_fixture_title(false);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let detail = cx.new(|cx| Detail::fixture(coordinator, title, false, None, cx));
        // Unseeded offline read: the fixture has no worker, so the read failed.
        let status = detail.read_with(cx, |d, cx| d.membership_status(cx));
        assert!(
            matches!(status, ToggleStatus::Failed | ToggleStatus::Loading),
            "{status:?}"
        );
        assert!(detail.read_with(cx, |d, cx| d.hero_commands_is_empty(cx)));
    }
}
