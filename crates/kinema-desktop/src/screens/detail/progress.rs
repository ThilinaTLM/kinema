//! Saved-playback presentation policy: which checkpoints are shown, their
//! labels, and the start intent chosen on Streams. Pure helpers plus one
//! accessor seam; storage and the player live elsewhere.
use super::*;
use crate::screens::coordinator::PersonalLoad;
use crate::screens::coordinator::PlaybackIntent;
use crate::widgets::{menu::MenuItem, state_toggle::ToggleStatus};

/// A saved playback position as the UI needs it. Mirrors the checkpoint row in
/// docs/design/playback-progress-desktop.md (`position_ms`, nullable `duration_ms`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResumePoint {
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    /// Catalog timestamp of the last save; orders recently resumed episodes.
    pub updated_at: i64,
}

/// Context actions that touch saved progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProgressAction {
    /// Delete the checkpoint only; watched state is unchanged.
    Clear,
    PlayFromBeginning,
}

impl ResumePoint {
    pub(super) fn from_observation(
        o: kinema_core::watch_progress::PlaybackObservation,
        updated_at: i64,
    ) -> Option<Self> {
        Some(Self {
            updated_at,
            position_ms: u64::try_from(o.position_ms()).ok()?,
            duration_ms: o.duration_ms().and_then(|d| u64::try_from(d).ok()),
        })
    }
    /// Resume is offered only above zero.
    pub(super) fn resumable(self) -> bool {
        self.position_ms > 0
    }
    /// Elapsed fraction, `None` without a finite positive duration (never invented).
    pub(super) fn fraction(self) -> Option<f32> {
        let duration = self.duration_ms.filter(|d| *d > 0)?;
        Some((self.position_ms as f64 / duration as f64).clamp(0., 1.) as f32)
    }
    pub(super) fn timestamp(self) -> String {
        timestamp(self.position_ms)
    }
    /// `24:18 of 1:42:07` for assistive text; elapsed only when duration is unknown.
    pub(super) fn spoken(self) -> String {
        match self.duration_ms.filter(|d| *d > 0) {
            Some(d) => format!("{} of {}", self.timestamp(), timestamp(d)),
            None => format!("{} elapsed", self.timestamp()),
        }
    }
}

/// `m:ss`, or `h:mm:ss` from one hour. Truncates sub-second remainders.
pub(super) fn timestamp(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Catalog/detail visibility: only a resumable checkpoint of a title that is
/// *known* unwatched is shown. Watched, unknown and loading states hide it
/// (the checkpoint itself is retained elsewhere).
pub(super) fn visible_resume(
    point: Option<ResumePoint>,
    watched: Option<bool>,
) -> Option<ResumePoint> {
    point.filter(|p| p.resumable() && watched == Some(false))
}

/// Primary hero label. Movies show the timestamp only; series add `SxEy`.
pub(super) fn resume_label(episode: Option<(u32, u32)>, point: ResumePoint) -> String {
    match episode {
        Some((s, e)) => format!("Resume \u{b7} S{s}E{e} \u{b7} {}", point.timestamp()),
        None => format!("Resume \u{b7} {}", point.timestamp()),
    }
}

/// Shown before Clear: watched is independent and a running player may save again.
pub(super) const CLEAR_HELP: &str = "Watched status is not changed. If this is playing now, playback continues and can save a new position.";

/// Resume is promised only by the embedded player, which performs the seek
/// handshake. External mpv/VLC/custom players always start at the beginning, so
/// the saved position stays visible but nothing offers to resume it.
pub(super) fn resume_offer(embedded: bool, shown: Option<ResumePoint>) -> Option<ResumePoint> {
    shown.filter(|_| embedded)
}

/// Which progress actions the context menu offers. Play from beginning only
/// while the checkpoint is shown (known unwatched); Clear whenever any
/// resumable checkpoint is saved, so watched titles can still drop it.
pub(super) fn offered_actions(saved: Option<ResumePoint>, shown: bool) -> Vec<ProgressAction> {
    if !saved.is_some_and(ResumePoint::resumable) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if shown {
        actions.push(ProgressAction::PlayFromBeginning);
    }
    actions.push(ProgressAction::Clear);
    actions
}

/// Intent for a play action. A checkpoint is resumed only while shown (known
/// unwatched); watched titles play normally. Start-over needs a checkpoint to
/// replace, otherwise it is an ordinary play.
pub(super) fn intent_for(visible: Option<ResumePoint>, from_beginning: bool) -> PlaybackIntent {
    match (visible, from_beginning) {
        (Some(_), false) => PlaybackIntent::Resume,
        (Some(_), true) => PlaybackIntent::StartOver,
        (None, _) => PlaybackIntent::Normal,
    }
}

impl Detail {
    /// Raw saved checkpoint (shown or not) for the movie, or the episode when
    /// given. Loading, failed and missing reads all report `None`: unknown is
    /// never presented as a position.
    pub(super) fn saved_progress(
        &self,
        episode: Option<(u32, u32)>,
        cx: &App,
    ) -> Option<ResumePoint> {
        match episode {
            Some((s, n)) => self.episode_view(s, n, cx).saved,
            None => self.saved_progress_direct(None, cx),
        }
    }
    /// [`Self::saved_progress`] read from the coordinator, not the snapshot.
    pub(super) fn saved_progress_direct(
        &self,
        episode: Option<(u32, u32)>,
        cx: &App,
    ) -> Option<ResumePoint> {
        #[cfg(all(test, feature = "ui-probe-test"))]
        super::work::count(super::work::Section::SavedProgress, 1);
        let c = self.coordinator.read(cx);
        let checkpoint = match episode {
            Some((s, n)) => match &c.episode_personal(&self.episode_ref(s, n)).state {
                PersonalLoad::Ready(e) => e.playback_progress.checkpoint,
                _ => None,
            },
            None => match &c.personal(&self.title).state {
                PersonalLoad::Ready(t) => t.playback_progress.and_then(|p| p.checkpoint),
                _ => None,
            },
        }?;
        ResumePoint::from_observation(checkpoint.observation, checkpoint.updated_at)
    }

    /// Checkpoint to present, per [`visible_resume`].
    pub(super) fn visible_progress(
        &self,
        episode: Option<(u32, u32)>,
        cx: &App,
    ) -> Option<ResumePoint> {
        let Some((s, n)) = episode else {
            let watched = match self.movie_watched_status(cx) {
                ToggleStatus::Known { on, .. } => Some(on),
                _ => None,
            };
            return visible_resume(self.saved_progress_direct(None, cx), watched);
        };
        self.episode_view(s, n, cx).visible
    }

    /// The one Continue / Next up target used by the hero, the card badge and
    /// the default season: the most recently resumed unfinished episode, else
    /// the binary next-up policy. Calls the core policy directly (no recursion).
    pub(super) fn continue_target_in(
        &self,
        options: &[(u32, u32)],
        cx: &App,
    ) -> Option<(u32, u32)> {
        // Only shown (known-unwatched, positive) checkpoints are candidates.
        let resumed = kinema_core::watch_progress::latest_resume(
            self.episode_coordinates(options, cx)
                .into_iter()
                .filter_map(|c| Some((c, self.visible_progress(Some(c), cx)?.updated_at))),
        );
        resumed.or_else(|| {
            kinema_core::watch_progress::next_up(&self.episode_marks(options, cx), options)
        })
    }

    /// True for the embedded player (or while settings are unknown, the default).
    pub(super) fn embedded_player(&self, cx: &App) -> bool {
        self.coordinator
            .read(cx)
            .settings
            .as_ref()
            .is_none_or(|s| s.player.is_managed())
    }

    /// Checkpoint the primary action may offer to resume, per [`resume_offer`].
    pub(super) fn resume_offered(
        &self,
        episode: Option<(u32, u32)>,
        cx: &App,
    ) -> Option<ResumePoint> {
        resume_offer(
            self.show_snapshot(cx).embedded,
            self.visible_progress(episode, cx),
        )
    }

    /// Intent for the next play request on the open Streams page.
    /// `from_beginning` is an explicit choice; otherwise a pending
    /// "Play from beginning" from the context menu wins over resuming.
    pub(super) fn play_intent(&self, from_beginning: Option<bool>, cx: &App) -> PlaybackIntent {
        let episode = self.episode.as_ref().map(|e| (e.season, e.number));
        let start_over = from_beginning.unwrap_or(self.start_over_requested);
        intent_for(self.resume_offered(episode, cx), start_over)
    }

    /// Context-menu entries for saved progress of `episode` (movie when `None`):
    /// Play from beginning only while the checkpoint is shown, Clear whenever
    /// any checkpoint exists. Empty without one.
    pub(super) fn progress_menu(
        &self,
        episode: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) -> Vec<MenuItem> {
        let saved = self.saved_progress(episode, cx);
        let shown = self.resume_offered(episode, cx).is_some();
        let weak = cx.entity().downgrade();
        let mut items = Vec::new();
        for action in offered_actions(saved, shown) {
            let weak = weak.clone();
            let label = match action {
                ProgressAction::PlayFromBeginning => "Play from beginning",
                ProgressAction::Clear => "Clear saved progress",
            };
            items.push(menu_item(label, move |_, cx| {
                let _ = weak.update(cx, |d, cx| d.run_progress_action(action, episode, cx));
            }));
        }
        items
    }

    pub(super) fn run_progress_action(
        &mut self,
        action: ProgressAction,
        episode: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) {
        match (action, episode) {
            (ProgressAction::PlayFromBeginning, Some((s, n))) => {
                self.continue_episode(s, n, cx);
                self.start_over_requested = true;
            }
            (ProgressAction::PlayFromBeginning, None) => {
                self.open_streams(cx);
                self.start_over_requested = true;
            }
            // Asks first: playback may still be running and can save again.
            (ProgressAction::Clear, target) => {
                self.clear_confirm = Some(target);
                cx.notify();
            }
        }
    }

    /// Apply a confirmed Clear. Watched state is untouched.
    pub(super) fn confirm_clear(&mut self, target: Option<(u32, u32)>, cx: &mut Context<Self>) {
        self.clear_confirm = None;
        self.perform(
            match target {
                Some((s, n)) => personal::Write::ClearEpisodeProgress(self.episode_ref(s, n)),
                None => personal::Write::ClearMovieProgress,
            },
            cx,
        );
    }

    /// Confirmation shown below the hero before a checkpoint is deleted.
    pub(super) fn clear_confirmation(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::widgets::notice::{Notice, NoticeKind};
        let target = self.clear_confirm?;
        let what = match target {
            Some((s, n)) => format!("Clear saved progress for S{s}E{n}?"),
            None => "Clear saved progress?".to_owned(),
        };
        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(s::S8)
                .child(
                    div().flex_1().min_w_0().child(
                        Notice::new(NoticeKind::Info, what).body(CLEAR_HELP).action(
                            Button::new("progress-clear-confirm", "Clear saved progress")
                                .variant(ButtonVariant::Danger)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.confirm_clear(target, cx)
                                })),
                        ),
                    ),
                )
                .child(
                    Button::new("progress-clear-cancel", "Cancel")
                        .variant(ButtonVariant::Ghost)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_confirm = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// 2 px elapsed line for an unwatched episode's artwork; nothing otherwise
    /// (unknown duration shows no fabricated percentage).
    pub(super) fn episode_progress_bar(&self, season: u32, number: u32, cx: &App) -> Option<Div> {
        let p = *tokens::palette(cx);
        let fraction = self.episode_view(season, number, cx).visible?.fraction()?;
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(s::S2)
                .bg(p.line_subtle)
                .child(div().h_full().w(relative(fraction)).bg(p.accent)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PlaybackIntent, ProgressAction, ResumePoint, intent_for, offered_actions, resume_label,
        resume_offer, timestamp, visible_resume,
    };

    const P: ResumePoint = ResumePoint {
        position_ms: 1_458_000,
        duration_ms: Some(6_000_000),
        updated_at: 10,
    };

    #[test]
    fn timestamps_use_hours_only_when_needed() {
        assert_eq!(timestamp(0), "0:00");
        assert_eq!(timestamp(1_458_999), "24:18");
        assert_eq!(timestamp(3_723_000), "1:02:03");
    }

    #[test]
    fn fraction_needs_a_positive_duration_and_is_clamped() {
        assert_eq!(P.fraction(), Some(0.243));
        let unknown = ResumePoint {
            duration_ms: None,
            ..P
        };
        assert_eq!(unknown.fraction(), None);
        let zero = ResumePoint {
            duration_ms: Some(0),
            ..P
        };
        assert_eq!(zero.fraction(), None);
        let over = ResumePoint {
            position_ms: 9_000_000,
            ..P
        };
        assert_eq!(over.fraction(), Some(1.));
    }

    #[test]
    fn only_known_unwatched_resumable_points_are_visible() {
        assert_eq!(visible_resume(Some(P), Some(false)), Some(P));
        assert_eq!(visible_resume(Some(P), Some(true)), None);
        assert_eq!(visible_resume(Some(P), None), None);
        assert_eq!(visible_resume(None, Some(false)), None);
        let zero = ResumePoint {
            position_ms: 0,
            ..P
        };
        assert_eq!(visible_resume(Some(zero), Some(false)), None);
    }

    #[test]
    fn labels_add_the_episode_only_for_series() {
        assert_eq!(resume_label(None, P), "Resume \u{b7} 24:18");
        assert_eq!(
            resume_label(Some((2, 5)), P),
            "Resume \u{b7} S2E5 \u{b7} 24:18"
        );
    }

    #[test]
    fn spoken_text_never_invents_a_total() {
        assert_eq!(P.spoken(), "24:18 of 1:40:00");
        let unknown = ResumePoint {
            duration_ms: None,
            ..P
        };
        assert_eq!(unknown.spoken(), "24:18 elapsed");
    }

    #[test]
    fn external_players_are_never_offered_a_resume() {
        assert_eq!(resume_offer(true, Some(P)), Some(P));
        assert_eq!(resume_offer(false, Some(P)), None);
        assert_eq!(resume_offer(true, None), None);
        // With nothing offered, intent and menu entries fall back to ordinary play.
        assert_eq!(
            intent_for(resume_offer(false, Some(P)), false),
            PlaybackIntent::Normal
        );
        assert_eq!(
            intent_for(resume_offer(false, Some(P)), true),
            PlaybackIntent::Normal
        );
    }

    #[test]
    fn intent_follows_visibility() {
        assert_eq!(intent_for(Some(P), false), PlaybackIntent::Resume);
        assert_eq!(intent_for(Some(P), true), PlaybackIntent::StartOver);
        assert_eq!(intent_for(None, false), PlaybackIntent::Normal);
        assert_eq!(intent_for(None, true), PlaybackIntent::Normal);
    }

    #[test]
    fn menu_offers_clear_independent_of_watched_state() {
        // Unwatched with a shown checkpoint: both actions.
        assert_eq!(
            offered_actions(Some(P), true),
            vec![ProgressAction::PlayFromBeginning, ProgressAction::Clear]
        );
        // Watched: the checkpoint is hidden but still clearable.
        assert_eq!(offered_actions(Some(P), false), vec![ProgressAction::Clear]);
        // Nothing saved, or a zero position: nothing to act on.
        assert!(offered_actions(None, false).is_empty());
        let zero = ResumePoint {
            position_ms: 0,
            ..P
        };
        assert!(offered_actions(Some(zero), true).is_empty());
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test setup must succeed"
)]
mod fixture_tests {
    use super::{Detail, ProgressAction, personal::Write};
    use crate::screens::coordinator::AppCoordinator;
    use crate::widgets::tokens;
    use gpui::{AppContext, TestAppContext};

    fn detail(series: bool, scene: &str, cx: &mut TestAppContext) -> gpui::Entity<Detail> {
        cx.update(tokens::init);
        let title = super::super::fixtures::local_fixture_title(series);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let detail = cx.new(|cx| Detail::fixture(coordinator, title, false, None, cx));
        detail.update(cx, |d, cx| d.seed_personal_fixture(scene, cx));
        detail
    }

    #[gpui::test]
    fn movie_shows_only_the_timestamp_for_an_unwatched_checkpoint(cx: &mut TestAppContext) {
        let d = detail(false, "detail-live-local-movie-resume", cx);
        d.read_with(cx, |d, cx| {
            let point = d.visible_progress(None, cx).expect("checkpoint shown");
            assert_eq!(point.timestamp(), "24:18");
            assert_eq!(super::resume_label(None, point), "Resume \u{b7} 24:18");
        });
    }

    #[gpui::test]
    fn watched_movie_hides_the_checkpoint_but_keeps_clear(cx: &mut TestAppContext) {
        // The plain movie scene is watched and has no checkpoint at all.
        let d = detail(false, "detail-live-local-movie", cx);
        d.read_with(cx, |d, cx| {
            assert!(d.visible_progress(None, cx).is_none());
            assert!(d.saved_progress(None, cx).is_none());
        });
    }

    #[gpui::test]
    fn series_continue_badge_and_card_bar_agree_on_the_resumed_episode(cx: &mut TestAppContext) {
        let d = detail(true, "detail-live-local-series-resume", cx);
        d.read_with(cx, |d, cx| {
            // Hero Continue, Next up badge and default season share one target.
            assert_eq!(d.next_up(cx), Some((1, 3)));
            assert_eq!(d.next_up_in(&d.season_options, cx), d.next_up(cx));
            // Thin bar and resume text only on the unwatched checkpoint.
            assert!(d.episode_progress_bar(1, 3, cx).is_some());
            assert!(d.episode_progress_bar(1, 4, cx).is_none());
            // Watched episodes show neither.
            assert!(d.visible_progress(Some((1, 1)), cx).is_none());
        });
    }

    #[gpui::test]
    fn clear_asks_first_and_does_not_repeat_the_action(cx: &mut TestAppContext) {
        let d = detail(true, "detail-live-local-series-resume", cx);
        d.update(cx, |d, cx| {
            d.run_progress_action(ProgressAction::Clear, Some((1, 3)), cx)
        });
        d.read_with(cx, |d, _| {
            // Only the confirmation is pending; no write has been issued.
            assert_eq!(d.clear_confirm, Some(Some((1, 3))));
            assert!(d.write_error.is_none());
        });
        d.update(cx, |d, cx| d.confirm_clear(Some((1, 3)), cx));
        d.read_with(cx, |d, _| {
            assert_eq!(d.clear_confirm, None);
            // The offline fixture rejects the write, recording exactly this intent.
            assert!(matches!(
                d.write_error.as_ref().map(|(w, _)| w),
                Some(Write::ClearEpisodeProgress(_))
            ));
        });
    }

    #[gpui::test]
    fn context_menu_offers_play_from_beginning_only_while_shown(cx: &mut TestAppContext) {
        let d = detail(true, "detail-live-local-series-resume", cx);
        let shown = d.update(cx, |d, cx| d.progress_menu(Some((1, 3)), cx).len());
        let none = d.update(cx, |d, cx| d.progress_menu(Some((1, 4)), cx).len());
        assert_eq!((shown, none), (2, 0));
    }
}
