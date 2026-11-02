//! Contrast-safe controls drawn on top of artwork.
//!
//! Both pieces sit on a dark backing (`chip_on_art`, #0B0C0E at 80 %, the same
//! in both themes) with fixed light ink, so legibility never depends on the
//! image beneath. Hosts position them absolutely inside a `Thumb`.
use crate::ui::theme::opacity;
use crate::{
    ui::motion::interaction::focus_ring,
    ui::primitives::tooltip::tooltip,
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{Palette, palette, radius, size, space},
};
use gpui::{prelude::*, *};
use std::rc::Rc;

/// A 20 px label chip, e.g. "Next up".
#[derive(IntoElement)]
pub struct OverlayBadge {
    text: SharedString,
    accent: bool,
}

impl OverlayBadge {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            accent: false,
        }
    }
    /// Accent fill instead of the dark backing.
    pub fn accent(mut self, accent: bool) -> Self {
        self.accent = accent;
        self
    }
}

impl RenderOnce for OverlayBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let dark = Palette::dark();
        let (bg, ink) = if self.accent {
            let p = palette(cx);
            (p.primary, p.primary_foreground)
        } else {
            (dark.chip_on_art, dark.muted_foreground)
        };
        div()
            .h(size::CHIP)
            .px(space::S6)
            .flex()
            .items_center()
            .rounded(radius::SM)
            .bg(bg)
            .text_color(ink)
            .text(TextSize::Xs)
            .weight(Weight::Semibold)
            .child(self.text)
    }
}

type Click = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A 28 px icon button. Pointer events are consumed so a host card never also
/// activates. `name` is both tooltip and accessible name.
#[derive(IntoElement)]
pub struct OverlayButton {
    id: ElementId,
    name: SharedString,
    accessible: Option<SharedString>,
    child: Option<AnyElement>,
    inert: bool,
    on_click: Option<Click>,
}

impl OverlayButton {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            accessible: None,
            child: None,
            inert: false,
            on_click: None,
        }
    }
    /// Accessible name when it should be more specific than the tooltip.
    pub fn accessible_name(mut self, name: impl Into<SharedString>) -> Self {
        self.accessible = Some(name.into());
        self
    }
    /// The glyph, typically a `WatchMark` with `over_art(true)`.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(child.into_any_element());
        self
    }
    /// Inert (write pending or unavailable): dimmed, no activation.
    pub fn inert(mut self, inert: bool) -> Self {
        self.inert = inert;
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for OverlayButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let dark = Palette::dark();
        let p = *palette(cx);
        let hover = dark.popover.opacity(opacity::OVERLAY_HOVER);
        let mut button = div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.accessible.unwrap_or_else(|| self.name.clone()))
            .size(size::CONTROL_SM)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::MD)
            .bg(dark.chip_on_art)
            .children(self.child)
            // The card below never sees this activation.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .tooltip(tooltip(self.name));
        if self.inert {
            button = button.opacity(opacity::DISABLED_OVER_ART).cursor_default();
        } else {
            let handler = self.on_click;
            button = button
                .cursor_pointer()
                .focusable()
                .tab_stop(true)
                .hover(move |el| el.bg(hover))
                .focus_visible(move |el| el.shadow(vec![focus_ring(&p)]))
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    if let Some(handler) = &handler {
                        handler(event, window, cx);
                    }
                });
        }
        button
    }
}
