//! Hero-style action button: one 32 px box, radius `SM`, three variants.
//!
//! A typed front for [`Button`] (which owns hover, focus, busy and disabled
//! rendering) so hero, season header and drawer actions share one face.
//! Inputs are explicit: a visible `label` that doubles as the accessible name,
//! an optional glyph, an `on` accent state, and a `disabled` reason that
//! becomes the tooltip. It performs no I/O.
use crate::{
    ui::primitives::IconName,
    ui::primitives::button::{Button, ButtonSize, ButtonVariant},
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, IntoElement, RenderOnce, SharedString,
    Window,
};

/// Visual variant of an [`ActionButton`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionKind {
    /// Accent fill: the one main action of a row.
    Primary,
    /// Neutral control fill with a label.
    #[default]
    Secondary,
    /// Neutral 32 × 32 square showing only a glyph; the label is its name.
    Icon,
}

impl ActionKind {
    pub const fn is_square(self) -> bool {
        matches!(self, ActionKind::Icon)
    }
    const fn variant(self) -> ButtonVariant {
        match self {
            ActionKind::Primary => ButtonVariant::Primary,
            ActionKind::Secondary | ActionKind::Icon => ButtonVariant::Secondary,
        }
    }
}

#[derive(IntoElement)]
pub struct ActionButton {
    id: ElementId,
    kind: ActionKind,
    label: SharedString,
    icon: Option<IconName>,
    glyph: Option<AnyElement>,
    sub: Option<SharedString>,
    on: bool,
    busy: bool,
    disabled_reason: Option<SharedString>,
    tooltip: Option<SharedString>,
    aria_toggled: Option<bool>,
    aria_mixed: bool,
    name: Option<SharedString>,
    focus: Option<FocusHandle>,
    handler: Option<crate::ui::primitives::button::ClickHandler>,
}

impl ActionButton {
    /// `label` is shown (except for `Icon`) and is the accessible name.
    pub fn new(id: impl Into<ElementId>, kind: ActionKind, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            kind,
            label: label.into(),
            icon: None,
            glyph: None,
            sub: None,
            on: false,
            busy: false,
            disabled_reason: None,
            tooltip: None,
            aria_toggled: None,
            aria_mixed: false,
            name: None,
            focus: None,
            handler: None,
        }
    }
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    /// A custom leading glyph, e.g. a `WatchMark`; replaces `icon`.
    pub fn glyph(mut self, glyph: impl IntoElement) -> Self {
        self.glyph = Some(glyph.into_any_element());
        self
    }
    /// Muted trailing text, hidden for `Icon`.
    pub fn trailing(mut self, sub: impl Into<SharedString>) -> Self {
        self.sub = Some(sub.into());
        self
    }
    /// Accent tint, accent edge and accent ink.
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }
    /// A write is in flight: width is kept, the control is inert.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }
    /// Disable the button; `reason` is its tooltip.
    pub fn disabled(mut self, reason: impl Into<SharedString>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }
    /// Tooltip (defaults to the label for `Icon`).
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    /// Expose toggle semantics; `mixed` reports a partial state while `on` is false.
    pub fn toggled(mut self, on: bool, mixed: bool) -> Self {
        self.aria_toggled = Some(on);
        self.aria_mixed = mixed;
        self
    }
    /// Accessible name when it should differ from the visible label.
    pub fn name(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn focus_handle(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for ActionButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let square = self.kind.is_square();
        let tooltip = self
            .disabled_reason
            .clone()
            .or(self.tooltip)
            .or_else(|| square.then(|| self.label.clone()));
        let name = self.name.unwrap_or_else(|| self.label.clone());
        let mut button = Button::new(self.id, self.label)
            .variant(self.kind.variant())
            .size(ButtonSize::Large)
            .disabled(self.disabled_reason.is_some())
            .busy(self.busy);
        if let Some(icon) = self.icon.filter(|_| self.glyph.is_none()) {
            button = button.icon(icon);
        }
        if let Some(focus) = self.focus {
            button = button.focus_handle(focus);
        }
        if let Some(handler) = self.handler {
            button = button.on_click(move |e, w, cx| handler(e, w, cx));
        }
        button.square = square;
        button.on = self.on;
        button.glyph = self.glyph;
        button.sub = self.sub;
        button.quiet = square;
        button.tooltip = tooltip;
        button.aria_name = Some(name);
        button.aria_toggled = self.aria_toggled;
        button.aria_mixed = self.aria_mixed;
        button
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ui::primitives::button::on_colors, ui::theme::Palette};
    use ::core::prelude::v1::test;

    #[test]
    fn only_icon_kind_is_square_and_neutral() {
        assert!(ActionKind::Icon.is_square());
        assert!(!ActionKind::Primary.is_square());
        assert!(!ActionKind::Secondary.is_square());
        assert_eq!(ActionKind::Primary.variant(), ButtonVariant::Primary);
        assert_eq!(ActionKind::Icon.variant(), ButtonVariant::Secondary);
    }

    #[test]
    fn on_state_uses_accent_tokens_in_both_themes() {
        for p in [Palette::dark(), Palette::light()] {
            let (rest, hover, ink, edge) = on_colors(&p);
            assert_eq!(rest, p.accent_tint);
            assert_eq!(ink, p.accent_text);
            assert!(hover.a > rest.a, "hover must deepen the tint");
            assert_eq!(
                (edge.h, edge.s, edge.l),
                (p.accent.h, p.accent.s, p.accent.l)
            );
            assert!(edge.a > 0.0 && edge.a < 1.0);
        }
    }
}
