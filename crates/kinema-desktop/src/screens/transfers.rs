//! Transfer controls use session snapshots, never infer external player state.
use super::coordinator::{AppCoordinator, CoordinatorEvent};
use crate::{
    app::{AppAction, AppOutcome},
    widgets::{
        button::{Button, ButtonVariant},
        icon::IconName,
        menu::ContextMenuExt,
        notice::{Notice, NoticeKind},
        patterns::{Badge, BadgeTone, StateView, ViewState},
        text::TypeExt,
        tokens::{self, space as s, typeface as t},
    },
};
use gpui::{prelude::*, *};
mod rows;

const PANEL_WIDTH: Pixels = px(352.);
/// Session list scrolls beyond this height instead of growing the popover.
const LIST_MAX_HEIGHT: Pixels = px(336.);
pub(crate) struct Transfers {
    coordinator: Entity<AppCoordinator>,
    stop: Option<String>,
    cleanup: Option<u64>,
    result: Option<String>,
    _events: Subscription,
    _observe: Subscription,
}
impl Transfers {
    pub(crate) fn new(coordinator: Entity<AppCoordinator>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&coordinator, |_, _, cx| cx.notify());
        let events = cx.subscribe(&coordinator, |this, _, event: &CoordinatorEvent, cx| {
            if let CoordinatorEvent::App(event) = event
                && Some(event.generation) == this.cleanup
            {
                tracing::debug!(
                    generation = event.generation,
                    outcome = "applied",
                    "UI cleanup result received"
                );
                if let AppOutcome::Failed { message } = &event.outcome {
                    tracing::warn!(message, "UI cleanup failure displayed");
                }
                this.cleanup = None;
                this.result = Some(match &event.outcome {
                    AppOutcome::Cleaned {
                        bytes,
                        protected_files,
                    } => format!(
                        "Freed {} · {} protected files. Original local files untouched.",
                        super::play_session::format_bytes(*bytes),
                        protected_files
                    ),
                    AppOutcome::Failed { message } => message.clone(),
                    _ => "Cleanup did not complete.".into(),
                });
                cx.notify();
            }
        });
        Self {
            coordinator,
            cleanup: None,
            result: None,
            _events: events,
            stop: None,
            _observe: observe,
        }
    }
}
impl Render for Transfers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let c = self.coordinator.read(cx);
        let sessions = c.sessions.clone();
        let usage = c.usage.clone();
        let mut content = div()
            .w(PANEL_WIDTH)
            .flex_none()
            .type_style(t::BODY)
            .p(s::S16)
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().type_style(t::HEADING).child("Transfers"))
                    .when(!sessions.is_empty(), |el| {
                        el.child(
                            Badge::new(format!("{} active", sessions.len()))
                                .tone(BadgeTone::Accent),
                        )
                    }),
            );
        if sessions.is_empty() {
            content = content.child(StateView::new("transfers-empty", ViewState::Empty).empty(
                IconName::Transfers,
                "Nothing playing or downloading",
                Some("Sessions you start from a title appear here.".into()),
            ));
        } else {
            let mut rows = div()
                .id("transfer-sessions")
                .max_h(LIST_MAX_HEIGHT)
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(s::S8);
            for (i, session) in sessions.iter().enumerate() {
                rows = rows.child(self.session_row(session, i, cx));
            }
            content = content.child(rows);
        }
        if let Some(id) = self.stop.clone() {
            content = content
                .child(
                    Notice::new(NoticeKind::Warning, "Stop serving?")
                        .body("Your player may lose access to this media."),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(s::S8)
                        .child(Button::new("stop-cancel", "Cancel").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.stop = None;
                                cx.notify();
                            },
                        )))
                        .child(
                            Button::new("stop-confirm", "Stop")
                                .variant(ButtonVariant::Danger)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.coordinator.update(cx, |c, cx| {
                                        c.command(AppAction::ReleaseSession(id.clone()), cx)
                                    });
                                    this.stop = None;
                                    cx.notify();
                                })),
                        ),
                );
        }
        let muted = |text: String| {
            div()
                .type_style(t::SMALL)
                .text_color(p.text_secondary)
                .child(text)
        };
        content = content.child(match usage {
            Some(usage) => muted(format!(
                "Cache {} of {} · {} downloaded",
                super::play_session::format_bytes(usage.reserved_bytes),
                super::play_session::format_bytes(usage.limit_bytes),
                super::play_session::format_bytes(usage.downloaded_bytes)
            )),
            None => muted("Cache usage unavailable".into()),
        });
        if let Some(result) = &self.result {
            content = content.child(Notice::new(NoticeKind::Info, result.clone()));
        }
        content.child(
            div().flex().justify_end().child(
                Button::new("clean-unused", "Clean unused")
                    .busy(self.cleanup.is_some())
                    .disabled(self.cleanup.is_some())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.cleanup.is_some() {
                            return;
                        }
                        this.cleanup = this
                            .coordinator
                            .update(cx, |c, cx| c.command(AppAction::CleanupUnused, cx));
                        if this.cleanup.is_none() {
                            this.result =
                                Some("Application service unavailable or busy. Retry.".into());
                        }
                        cx.notify();
                    })),
            ),
        )
    }
}
