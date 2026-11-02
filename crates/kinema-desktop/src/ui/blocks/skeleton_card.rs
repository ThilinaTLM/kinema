//! Noninteractive loading placeholder using the poster card's geometry.
//! A gentle pulse runs unless reduced motion is on, where it is static.
use crate::{
    ui::motion::{self, AnimateExt},
    ui::theme::motion::timing,
    ui::theme::{self, radius},
};
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
        let p = theme::palette(cx);
        let sheen = div()
            .absolute()
            .inset_0()
            .rounded(radius::LG)
            .bg(p.ctl_hover);
        let sheen = if motion::reduced(cx) {
            sheen.opacity(0.35).into_any_element()
        } else {
            sheen.animate_loop("skeleton-pulse", timing::PULSE_PERIOD, cx, |el, t| {
                // 0 -> 1 -> 0 smooth triangle wave.
                let w = 1. - (2. * t - 1.).abs();
                el.opacity(0.15 + 0.5 * (w * w * (3. - 2. * w)))
            })
        };
        div()
            .w(px(self.width))
            .h(px(self.width * 1.5))
            .flex_none()
            .relative()
            .rounded(radius::LG)
            .bg(p.bg_placeholder)
            .child(sheen)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius::LG)
                    .border_1()
                    .border_color(p.poster_edge),
            )
    }
}
