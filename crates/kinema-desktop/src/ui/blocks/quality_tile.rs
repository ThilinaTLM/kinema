//! `QualityTile`: a fixed-size resolution tile with an optional dynamic-range
//! line, used as the leading slot of stream rows and the play dock.
//!
//! Width is constant so release names align down a list. 4K is the only
//! resolution that earns the accent tint; everything else stays neutral so
//! accent remains scarce.
use crate::ui::theme::layout::detail::{QUALITY_TILE_HEIGHT, QUALITY_TILE_WIDTH};
use crate::{
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{palette, radius},
};
use gpui::{prelude::*, *};

#[derive(IntoElement)]
pub struct QualityTile {
    resolution: Option<SharedString>,
    dynamic_range: Option<SharedString>,
    emphasized: bool,
}

impl QualityTile {
    /// `None` resolution renders an em dash rather than collapsing the column.
    pub fn new(resolution: Option<impl Into<SharedString>>) -> Self {
        Self {
            resolution: resolution.map(Into::into),
            dynamic_range: None,
            emphasized: false,
        }
    }
    /// HDR / DV line under the resolution.
    pub fn dynamic_range(mut self, range: Option<impl Into<SharedString>>) -> Self {
        self.dynamic_range = range.map(Into::into);
        self
    }
    /// Accent-tinted face (4K).
    pub fn emphasized(mut self, emphasized: bool) -> Self {
        self.emphasized = emphasized;
        self
    }
}

impl RenderOnce for QualityTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let (fill, ink) = if self.emphasized {
            (p.selection, p.primary_text)
        } else {
            (p.secondary, p.foreground)
        };
        div()
            .w(QUALITY_TILE_WIDTH)
            .h(QUALITY_TILE_HEIGHT)
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .rounded(radius::MD)
            .bg(fill)
            .child(
                div()
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .weight(Weight::Bold)
                    .text_color(ink)
                    .child(self.resolution.unwrap_or_else(|| "\u{2014}".into())),
            )
            .children(self.dynamic_range.map(|range| {
                div()
                    .text(TextSize::Xs)
                    .weight(Weight::Semibold)
                    .text_color(p.muted_foreground)
                    .child(range.to_uppercase())
            }))
    }
}
