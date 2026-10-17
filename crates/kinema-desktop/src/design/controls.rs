//! Thin Kit `Button` presets that apply Graphite geometry and state policy.
//! Disabled controls are neutral and legible (no opacity, no hover response).
use gpui_kit::base::Button;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, px,
};

use super::{theme::tokens, tokens::metrics};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 28 px
    Default,
    /// 24 px
    Compact,
    /// 32 px task CTA
    Prominent,
}

impl Size {
    fn height(self) -> f32 {
        match self {
            Self::Default => metrics::CONTROL_HEIGHT,
            Self::Compact => metrics::CONTROL_COMPACT_HEIGHT,
            Self::Prominent => metrics::CONTROL_PROMINENT_HEIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    Neutral,
    Primary,
    /// Outlined error-ink destructive action.
    Danger,
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    emphasis: Emphasis,
    size: Size,
    disabled: bool,
    cx: &App,
) -> Button {
    let t = tokens(cx);
    let label: SharedString = label.into();
    let b = Button::new(id)
        .child(label.clone())
        .accessibility_label(label)
        .h(px(size.height()))
        .px(px(if size == Size::Compact { 6. } else { 8. }))
        .flex_none()
        .border_1()
        .rounded(px(metrics::CONTROL_RADIUS))
        .text_size(px(if size == Size::Compact {
            metrics::FONT_LABEL
        } else {
            metrics::FONT_BODY
        }));
    if disabled {
        // Priority over every variant: neutral fill, readable muted label.
        return b
            .disabled(true)
            .bg(t.raised)
            .border_color(t.border)
            .text_color(t.muted);
    }
    let (fill, hover, pressed, ink, border) = match emphasis {
        Emphasis::Neutral => (
            t.control,
            t.control_hover,
            t.control_pressed,
            t.text,
            t.control_border,
        ),
        Emphasis::Primary => (
            t.primary,
            t.primary_hover,
            t.primary_pressed,
            t.on_primary,
            t.primary,
        ),
        Emphasis::Danger => (
            t.control,
            t.control_hover,
            t.control_pressed,
            t.error,
            t.error,
        ),
    };
    b.bg(fill)
        .text_color(ink)
        .border_color(border)
        .hover(move |s| s.bg(hover))
        .active(move |s| s.bg(pressed))
        .focus(move |s| s.border_color(t.focus))
}

pub fn neutral(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    button(id, label, Emphasis::Neutral, Size::Default, false, cx)
}

pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    button(id, label, Emphasis::Primary, Size::Default, false, cx)
}
