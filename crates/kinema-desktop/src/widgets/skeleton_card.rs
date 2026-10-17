//! Static, noninteractive loading placeholder using the poster card's geometry.
//! No animation is scheduled, including when reduced motion is enabled.
use super::tokens;
use gpui::{prelude::*, *};

#[derive(IntoElement)]
pub struct SkeletonCard {
    width: f32,
}

impl SkeletonCard {
    pub fn new(width: f32) -> Self {
        Self { width }
    }
}

impl RenderOnce for SkeletonCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = tokens::palette(cx);
        div()
            .w(px(self.width))
            .h(px(self.width * 1.5))
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .border_1()
                    .border_color(p.poster_edge),
            )
    }
}
