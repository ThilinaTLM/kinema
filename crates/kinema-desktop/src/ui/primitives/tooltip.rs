//! Text-only native GPUI tooltips. The interaction owner controls lifetime.
use crate::{
    ui::theme::typography::TypeExt,
    ui::theme::{elevation, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};

struct Tooltip {
    text: SharedString,
}
impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .bg(p.bg_overlay)
            .rounded(radius::SM)
            .border_1()
            .border_color(p.line_subtle)
            .shadow(elevation::raised(p))
            .px(space::S8)
            .py(space::S4)
            .type_style(typeface::SMALL)
            .text_color(p.text_primary)
            .child(self.text.clone())
    }
}
/// Pass to `.tooltip(...)`. GPUI's default show delay is 500 ms; hide is immediate.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tooltip { text: text.clone() }).into()
}
