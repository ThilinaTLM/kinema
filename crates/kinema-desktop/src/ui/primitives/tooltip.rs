//! Text-only native GPUI tooltips. The interaction owner controls lifetime.
use crate::{
    ui::motion::{
        Presence,
        transition::{POPUP_IN, POPUP_OUT},
    },
    ui::theme::typography::TypeExt,
    ui::theme::{elevation, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};

struct Tooltip {
    text: SharedString,
    /// GPUI drops the view on hide, so only the enter plays.
    presence: Entity<Presence>,
}
impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let tip = div()
            .bg(p.bg_overlay)
            .rounded(radius::MD)
            .border_1()
            .border_color(p.line_subtle)
            .shadow(elevation::sm(p))
            .px(space::S8)
            .py(space::S4)
            .type_style(typeface::SMALL)
            .text_color(p.text_primary)
            .child(self.text.clone());
        self.presence.read(cx).animate(tip, "tooltip", cx)
    }
}
/// Pass to `.tooltip(...)`. GPUI's default show delay is 500 ms
/// (`timing::TOOLTIP_DELAY`); hide is immediate.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| {
        cx.new(|cx| Tooltip {
            text: text.clone(),
            presence: cx.new(|_| Presence::opened(POPUP_IN, POPUP_OUT)),
        })
        .into()
    }
}
