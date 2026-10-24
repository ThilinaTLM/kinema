//! Library membership and manual watched controls. All state lives in the shared
//! coordinator; this module only maps it to controls, notices and confirmations.
use super::*;
use crate::widgets::{menu::MenuItem, patterns::menu_item};
use crate::{
    screens::coordinator::{EpisodeRef, PersonalLoad, SeasonSnapshot},
    widgets::state_toggle::{StateToggle, ToggleKind, ToggleStatus},
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

/// Everything a detail toggle needs; `write` maps the desired value to its intent.
pub(super) struct ToggleSpec {
    id: ElementId,
    kind: ToggleKind,
    status: ToggleStatus,
    subject: String,
    over_art: bool,
    write: Box<dyn Fn(bool) -> Option<Write>>,
}

/// A season bulk action awaiting explicit confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SeasonConfirm {
    pub season: u32,
    pub watched: bool,
}

pub(super) fn season_label(season: u32) -> String {
    if season == 0 {
        "Specials".into()
    } else {
        format!("Season {season}")
    }
}

pub(super) fn summary_text(s: kinema_core::library::ShowSummary) -> String {
    if s.complete && s.total > 0 && s.watched <= s.total {
        format!("{} of {} watched", s.watched, s.total)
    } else if s.watched > 0 {
        format!("{} watched \u{b7} total unknown", s.watched)
    } else {
        "Watched status unavailable".into()
    }
}

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
    fn title_id(&self, cx: &App) -> Option<TitleId> {
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
        let (Some(id), Some(season)) = (self.title_id(cx), self.season) else {
            return;
        };
        let episodes = self.episodes(cx);
        // Request only episodes not yet asked for, so episodes that arrive later in
        // the same season (refresh/enrichment) are never left Loading.
        let missing: Vec<_> = episodes
            .iter()
            .filter(|e| {
                e.season == season
                    && !self
                        .episode_personal_requested
                        .contains(&(id, e.season, e.number))
            })
            .map(|e| (e.season, e.number))
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
    pub(super) fn retry_personal(&mut self, cx: &mut Context<Self>) {
        self.episode_personal_requested.clear();
        self.ensure_personal(cx);
        self.ensure_episode_personal(cx);
        cx.notify();
    }
    /// Submit one write. A synchronous rejection keeps exactly this intent for Retry;
    /// asynchronous commit failures are retried by the coordinator's own record.
    pub(super) fn perform(&mut self, write: Write, cx: &mut Context<Self>) {
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
    pub(super) fn toggle(&self, spec: ToggleSpec, cx: &mut Context<Self>) -> StateToggle {
        let this = cx.entity().downgrade();
        let retry = this.clone();
        let write = spec.write;
        StateToggle::new(spec.id, spec.kind, spec.status, spec.subject)
            .over_art(spec.over_art)
            .on_change(move |wanted, _, cx| {
                if let Some(write) = write(wanted) {
                    let _ = this.update(cx, |d, cx| d.perform(write, cx));
                }
            })
            .on_retry(move |_, cx| {
                let _ = retry.update(cx, |d, cx| d.retry_personal(cx));
            })
    }
    pub(super) fn membership_toggle(&self, over_art: bool, cx: &mut Context<Self>) -> StateToggle {
        self.toggle(
            ToggleSpec {
                id: "detail-library".into(),
                kind: ToggleKind::Membership,
                status: self.membership_status(cx),
                subject: self.title.name().to_owned(),
                over_art,
                write: Box::new(|on| Some(Write::Membership(on))),
            },
            cx,
        )
    }
    pub(super) fn movie_watched_toggle(
        &self,
        over_art: bool,
        cx: &mut Context<Self>,
    ) -> StateToggle {
        self.toggle(
            ToggleSpec {
                id: "detail-watched".into(),
                kind: ToggleKind::Watched,
                status: self.movie_watched_status(cx),
                subject: self.title.name().to_owned(),
                over_art,
                write: Box::new(|on| Some(Write::Movie(on))),
            },
            cx,
        )
    }
    pub(super) fn episode_toggle(
        &self,
        season: u32,
        number: u32,
        cx: &mut Context<Self>,
    ) -> StateToggle {
        let reference = self.episode_ref(season, number);
        let ready = self.title_id(cx).is_some();
        self.toggle(
            ToggleSpec {
                id: ElementId::Name(format!("episode-watched-s{season}-e{number}").into()),
                kind: ToggleKind::Watched,
                status: self.episode_status(season, number, cx),
                subject: format!("episode S{season:02}E{number:02}"),
                over_art: true,
                write: Box::new(move |on| ready.then(|| Write::Episode(reference.clone(), on))),
            },
            cx,
        )
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
        // Commit failures, success and Undo use the shared MainWindow toast, so they
        // are never duplicated here.
        any.then(|| hero::body_shell(viewport, stack).into_any_element())
    }

    // ── season bulk actions ──────────────────────────────────────────────
    pub(super) fn request_season(&mut self, season: u32, watched: bool, cx: &mut Context<Self>) {
        let Some(id) = self.title_id(cx) else {
            return;
        };
        self.season_confirm = Some(SeasonConfirm { season, watched });
        self.coordinator
            .update(cx, |c, cx| c.load_season_snapshot(id, season, cx));
        cx.notify();
    }
    fn cancel_season(&mut self, cx: &mut Context<Self>) {
        self.season_confirm = None;
        self.coordinator.update(cx, |c, cx| {
            c.season_snapshot.cancel();
            cx.notify();
        });
        cx.notify();
    }
    fn confirm_season(&mut self, snapshot: SeasonSnapshot, watched: bool, cx: &mut Context<Self>) {
        self.season_confirm = None;
        self.perform(Write::Season(snapshot, watched), cx);
    }

    /// Computed show progress; unknown completeness never reads as complete.
    pub(super) fn show_progress(&self, cx: &App) -> Option<String> {
        let v = self.coordinator.read(cx).personal(&self.title);
        let PersonalLoad::Ready(t) = v.state else {
            return None;
        };
        t.show_summary.map(summary_text)
    }

    /// Season command buttons plus the exact-count confirmation panel.
    pub(super) fn season_actions(&self, season: u32, cx: &mut Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let ready = self.title_id(cx).is_some();
        let label = season_label(season);
        let mut column = div().w_full().flex().flex_col().gap(s::S8);
        column =
            column.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S8)
                    .child(
                        Button::new("season-mark-watched", "Mark season watched")
                            .variant(ButtonVariant::Ghost)
                            .icon(IconName::CircleCheck)
                            .disabled(!ready)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_season(season, true, cx)
                            })),
                    )
                    .child(
                        Button::new("season-mark-unwatched", "Mark season unwatched")
                            .variant(ButtonVariant::Ghost)
                            .icon(IconName::Circle)
                            .disabled(!ready)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_season(season, false, cx)
                            })),
                    ),
            );
        let Some(confirm) = self.season_confirm.filter(|c| c.season == season) else {
            return column.into_any_element();
        };
        let verb = if confirm.watched {
            "watched"
        } else {
            "unwatched"
        };
        let c = self.coordinator.read(cx);
        let snapshot = c
            .season_snapshot
            .value()
            .filter(|s| s.season == season)
            .cloned();
        let busy = c.season_snapshot.busy();
        let error = c.season_snapshot.error().map(str::to_owned);
        let (text, confirm_button): (String, Option<AnyElement>) = match (snapshot, error, busy) {
            (_, Some(error), _) => (format!("Can’t change {label}: {error}"), None),
            (Some(snapshot), None, false) if snapshot.episodes.is_empty() => {
                (format!("{label} has no episodes to change."), None)
            }
            (Some(snapshot), None, false) => {
                let n = snapshot.episodes.len();
                (
                    format!(
                        "Mark all {n} {} of {label} as {verb}? Episodes added later are not included.",
                        if n == 1 { "episode" } else { "episodes" }
                    ),
                    Some(
                        Button::new("season-confirm", format!("Mark {n} {verb}"))
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirm_season(snapshot.clone(), confirm.watched, cx)
                            }))
                            .into_any_element(),
                    ),
                )
            }
            _ => (format!("Checking {label}…"), None),
        };
        column
            .child(
                div()
                    .id("season-confirm-panel")
                    .role(Role::Group)
                    .aria_label(text.clone())
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .p(s::S12)
                    .rounded(radius::SM)
                    .bg(p.bg_surface)
                    .border_1()
                    .border_color(p.line_subtle)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .type_style(t::SMALL)
                            .text_color(p.text_primary)
                            .child(text),
                    )
                    .children(confirm_button)
                    .child(
                        Button::new("season-cancel", "Cancel")
                            .variant(ButtonVariant::Ghost)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_season(cx))),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    use kinema_core::library::ShowSummary;

    #[test]
    fn summary_never_claims_unknown_totals() {
        let s = |watched, total, complete| {
            summary_text(ShowSummary {
                watched,
                total,
                complete,
            })
        };
        assert_eq!(s(3, 12, true), "3 of 12 watched");
        assert_eq!(s(3, 12, false), "3 watched \u{b7} total unknown");
        assert_eq!(s(0, 0, true), "Watched status unavailable");
        assert_eq!(s(0, 5, false), "Watched status unavailable");
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
