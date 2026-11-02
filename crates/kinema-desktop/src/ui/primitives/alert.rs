//! Inline status panels and compact under-toolbar strips.
//!
//! Accessibility: the panel exposes `Role::Alert` for warnings and errors and
//! `Role::Status` otherwise, with the title (plus body) as its accessible name;
//! the tone is always paired with a distinct icon, never colour alone. An
//! optional action is a regular [`Button`] with its own focus and name.
use crate::{
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::primitives::button::Button,
    ui::theme::typography::TypeExt,
    ui::theme::{opacity, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Alert tone. `Success` is kept beside the shadcn-style
/// `Info | Warning | Error | Empty` because the personal-library confirmation
/// needs a positive state.
pub enum AlertTone {
    Info,
    Success,
    Warning,
    Error,
    Empty,
}
#[derive(IntoElement)]
pub struct Alert {
    kind: AlertTone,
    title: SharedString,
    body: Option<SharedString>,
    action: Option<Button>,
    strip: bool,
}
impl Alert {
    pub fn new(kind: AlertTone, title: impl Into<SharedString>) -> Self {
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
/// Status-tinted fill and edge for an alert; neutral kinds stay untinted.
fn tint(kind: AlertTone, ink: Hsla, surface: Hsla, subtle: Hsla) -> (Hsla, Hsla) {
    match kind {
        AlertTone::Info | AlertTone::Empty => (surface, subtle),
        _ => (
            surface.blend(ink.opacity(opacity::BADGE_WASH)),
            ink.opacity(opacity::ALERT_EDGE),
        ),
    }
}

impl RenderOnce for Alert {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let (icon, ink) = match self.kind {
            AlertTone::Info => (IconName::Info, p.muted_foreground),
            AlertTone::Success => (IconName::Check, p.success),
            AlertTone::Warning => (IconName::Warning, p.warning),
            AlertTone::Error => (IconName::Error, p.destructive),
            AlertTone::Empty => (IconName::Search, p.subtle_foreground),
        };
        let (fill, edge) = tint(self.kind, ink, p.surface, p.border);
        let name: SharedString = match &self.body {
            Some(body) => format!("{}. {body}", self.title).into(),
            None => self.title.clone(),
        };
        div()
            .id("alert")
            .role(match self.kind {
                AlertTone::Warning | AlertTone::Error => Role::Alert,
                _ => Role::Status,
            })
            .aria_label(name)
            .flex()
            .items_center()
            .rounded(radius::MD)
            .gap(space::S12)
            .px(space::S12)
            .py(if self.strip { space::S4 } else { space::S12 })
            .bg(fill)
            .border(space::HAIRLINE)
            .border_color(edge)
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
                            .text_color(p.foreground)
                            .child(self.title),
                    )
                    .when_some(self.body, |el, body| {
                        el.child(
                            div()
                                .type_style(typeface::SMALL)
                                .text_color(p.muted_foreground)
                                .child(body),
                        )
                    }),
            )
            .when_some(self.action, |el, action| el.child(action))
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertTone, tint};
    use crate::ui::theme::Palette;

    #[test]
    fn neutral_kinds_are_untinted_and_status_kinds_tint() {
        let p = Palette::dark();
        for kind in [AlertTone::Info, AlertTone::Empty] {
            assert_eq!(
                tint(kind, p.success, p.surface, p.border),
                (p.surface, p.border)
            );
        }
        for kind in [AlertTone::Success, AlertTone::Warning, AlertTone::Error] {
            let (fill, edge) = tint(kind, p.destructive, p.surface, p.border);
            assert_ne!(fill, p.surface);
            assert!(edge.a < 0.5);
        }
    }
}
