//! Transfer controls use session snapshots, never infer external player state.
use super::coordinator::{AppCoordinator, CoordinatorEvent};
use crate::{
    app::{AppAction, AppOutcome},
    widgets::{
        button::Button,
        menu::ContextMenuExt,
        text::TypeExt,
        tokens::{self, space as s, typeface as t},
    },
};
use gpui::{prelude::*, *};
mod rows;
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
                this.cleanup = None;
                this.result = Some(match &event.outcome {
                    AppOutcome::Cleaned {
                        bytes,
                        protected_files,
                    } => format!(
                        "Freed {} · {} protected files. Original local files untouched.",
                        super::play_sheet::format_bytes(*bytes),
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
        let c = self.coordinator.read(cx);
        let sessions = c.sessions.clone();
        let usage = c.usage.clone();
        let rows_height = sessions
            .iter()
            .fold(px(0.), |height, row| {
                height + self.session_row_height(row) + s::S12
            })
            .min(s::S48 * 7.);
        let content_height = rows_height
            + s::S48 * 4.
            + if self.stop.is_some() {
                s::S48 * 2.
            } else {
                px(0.)
            }
            + if self.result.is_some() {
                s::S48
            } else {
                px(0.)
            };
        let mut content = div()
            .w(s::S48 * 7. + s::S16)
            .h(content_height)
            .flex_none()
            .type_style(t::BODY)
            .p(s::S16)
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(div().type_style(t::HEADING).child("Transfers"));
        if sessions.is_empty() {
            content = content.child("Nothing playing or downloading");
        }
        if !sessions.is_empty() {
            let mut rows = div()
                .id("transfer-sessions")
                .h(rows_height)
                .flex_none()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(s::S12);
            for (i, session) in sessions.iter().enumerate() {
                rows = rows.child(
                    div()
                        .h(self.session_row_height(session))
                        .flex_none()
                        .w_full()
                        .child(self.session_row(session, i, cx)),
                );
            }
            content = content.child(rows);
        }
        if let Some(id) = self.stop.clone() {
            content = content
                .child("Stop serving? Your player may lose access.")
                .child(
                    div()
                        .flex()
                        .gap(s::S8)
                        .child(Button::new("stop-cancel", "Cancel").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.stop = None;
                                cx.notify();
                            },
                        )))
                        .child(Button::new("stop-confirm", "Stop").on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.coordinator.update(cx, |c, cx| {
                                    c.command(AppAction::ReleaseSession(id.clone()), cx)
                                });
                                this.stop = None;
                                cx.notify();
                            },
                        ))),
                );
        }
        if let Some(usage) = usage {
            content = content.child(format!(
                "Cache {} of {} · {} downloaded",
                super::play_sheet::format_bytes(usage.reserved_bytes),
                super::play_sheet::format_bytes(usage.limit_bytes),
                super::play_sheet::format_bytes(usage.downloaded_bytes)
            ));
        }
        if self.coordinator.read(cx).usage.is_none() {
            content = content.child("Cache usage unavailable");
        }
        if let Some(result) = &self.result {
            content = content.child(result.clone());
        }
        content.child(
            Button::new("clean-unused", "Clean unused")
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
        )
    }
}
