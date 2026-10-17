//! Neutral inline status panels and compact under-toolbar strips.
use super::{
    Icon, IconName, TypeExt,
    button::Button,
    tokens::{palette, space, typeface},
};
use gpui::{prelude::*, *};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Success,
    Warning,
    Error,
    Empty,
}
#[derive(IntoElement)]
pub struct Notice {
    kind: NoticeKind,
    title: SharedString,
    body: Option<SharedString>,
    action: Option<Button>,
    strip: bool,
}
impl Notice {
    pub fn new(kind: NoticeKind, title: impl Into<SharedString>) -> Self {
        Self {
            kind,
            title: title.into(),
            body: None,
            action: None,
            strip: false,
        }
    }
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }
    pub fn action(mut self, action: Button) -> Self {
        self.action = Some(action);
        self
    }
    pub fn strip(mut self) -> Self {
        self.strip = true;
        self
    }
}
impl RenderOnce for Notice {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let (icon, ink) = match self.kind {
            NoticeKind::Info => (IconName::Info, p.text_secondary),
            NoticeKind::Success => (IconName::Check, p.status_ok),
            NoticeKind::Warning => (IconName::Warning, p.status_warn),
            NoticeKind::Error => (IconName::Error, p.status_error),
            NoticeKind::Empty => (IconName::Search, p.text_muted),
        };
        div()
            .flex()
            .items_center()
            .gap(space::S12)
            .px(space::S12)
            .py(if self.strip { space::S4 } else { space::S12 })
            .bg(p.bg_surface)
            .border(space::HAIRLINE)
            .border_color(p.line_hairline)
            .child(Icon::new(icon).color(ink))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(space::S4)
                    .when(!self.strip, |el| el.flex_col())
                    .child(
                        div()
                            .type_style(if self.strip {
                                typeface::SMALL
                            } else {
                                typeface::BODY_STRONG
                            })
                            .text_color(p.text_primary)
                            .child(self.title),
                    )
                    .when_some(self.body, |el, body| {
                        el.child(
                            div()
                                .type_style(typeface::SMALL)
                                .text_color(p.text_secondary)
                                .child(body),
                        )
                    }),
            )
            .when_some(self.action, |el, action| el.child(action))
    }
}
