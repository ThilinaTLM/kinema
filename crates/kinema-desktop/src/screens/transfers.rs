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
        let events=cx.subscribe(&coordinator,|this,_,event:&CoordinatorEvent,cx|{
            if let CoordinatorEvent::App(event)=event && Some(event.generation)==this.cleanup {
                this.cleanup=None;
                this.result=Some(match &event.outcome {AppOutcome::Cleaned{bytes}=>format!("Freed {bytes} bytes. Active files protected; original local files untouched."),AppOutcome::Failed{message}=>message.clone(),_=>"Cleanup did not complete.".into()});cx.notify();
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
            .w(px(354.))
            .min_h(px(140.
                + sessions.len() as f32 * 96.
                + if self.stop.is_some() { 100. } else { 0. }))
            .flex_none()
            .p(s::S16)
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(div().type_style(t::HEADING).child("Transfers"));
        if sessions.is_empty() {
            content = content.child("Nothing playing or downloading");
        }
        for (i, session) in sessions.iter().enumerate() {
            let id = session.session.id.clone();
            let stop = id.clone();
            let url = session.session.url.clone();
            let weak = cx.entity().downgrade();
            content = content.child(
                div()
                    .border_b_1()
                    .border_color(p.line_hairline)
                    .pb(s::S8)
                    .child(session.file_name.clone())
                    .child(
                        div()
                            .text_color(p.text_secondary)
                            .child("Launch requested · serving"),
                    )
                    .child(
                        Button::new(("stop-session", i), "Stop…").on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.stop = Some(id.clone());
                                cx.notify();
                            },
                        )),
                    )
                    .context_menu(("transfer-menu", i), move |_, _| {
                        vec![
                            super::browse::menu_item("Copy local URL", {
                                let url = url.clone();
                                move |_, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()))
                                }
                            }),
                            super::browse::menu_item("Stop serving…", {
                                let weak = weak.clone();
                                let stop = stop.clone();
                                move |_, cx| {
                                    let _ = weak.update(cx, |s, cx| {
                                        s.stop = Some(stop.clone());
                                        cx.notify();
                                    });
                                }
                            }),
                        ]
                    }),
            );
        }
        if let Some(id) = self.stop.clone() {
            content = content
                .child("Stop serving? Your player may lose access.")
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
                )));
        }
        if let Some(usage) = usage {
            content = content.child(format!(
                "Cache {:.1} of {:.1} GiB · {:.1} GiB downloaded",
                usage.reserved_bytes as f64 / 1_073_741_824.,
                usage.limit_bytes as f64 / 1_073_741_824.,
                usage.downloaded_bytes as f64 / 1_073_741_824.
            ));
        }
        if self.coordinator.read(cx).usage.is_none() {
            content = content.child("Cache usage unavailable");
        }
        if let Some(result) = &self.result {
            content = content.child(result.clone());
        }
        content.child(
            Button::new("clean-unused", "Clean unused").on_click(cx.listener(|this, _, _, cx| {
                this.cleanup = this
                    .coordinator
                    .update(cx, |c, cx| c.command(AppAction::CleanupUnused, cx));
                if this.cleanup.is_none() {
                    this.result = Some("Application service unavailable or busy. Retry.".into());
                }
                cx.notify();
            })),
        )
    }
}
