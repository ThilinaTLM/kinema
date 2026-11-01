//! Noninteractive loading placeholder using the poster card's geometry.
//! A gentle pulse runs unless reduced motion is on, where it is static.
use crate::{
    ui::motion,
    ui::theme::{self, radius},
};
use gpui::{prelude::*, *};
use std::time::Duration;

const PULSE: Duration = Duration::from_millis(1600);

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
            .rounded(radius::MD)
            .bg(p.ctl_hover);
        let sheen = if motion::reduced(cx) {
            sheen.opacity(0.35).into_any_element()
        } else {
            sheen
                .with_animation(
                    "skeleton-pulse",
                    Animation::new(PULSE).repeat().with_easing(|t| {
                        // 0 -> 1 -> 0 smooth triangle wave.
                        let w = 1. - (2. * t - 1.).abs();
                        w * w * (3. - 2. * w)
                    }),
                    |el, t| el.opacity(0.15 + 0.5 * t),
                )
                .into_any_element()
        };
        div()
            .w(px(self.width))
            .h(px(self.width * 1.5))
            .flex_none()
            .relative()
            .rounded(radius::MD)
            .bg(p.bg_placeholder)
            .child(sheen)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius::MD)
                    .border_1()
                    .border_color(p.poster_edge),
            )
    }
}
