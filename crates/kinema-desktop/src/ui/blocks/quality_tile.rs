//! `QualityTile`: a fixed-size resolution tile with an optional dynamic-range
//! line, used as the leading slot of stream rows and the play dock.
//!
//! Width is constant so release names align down a list. 4K is the only
//! resolution that earns the accent tint; everything else stays neutral so
//! accent remains scarce.
use crate::{
    ui::theme::typography::TypeExt,
    ui::theme::{palette, radius, typeface},
};
use gpui::{prelude::*, *};

/// Tile footprint; fits "1080p" in `type.small` bold with side padding.
pub const WIDTH: Pixels = px(56.);
pub const HEIGHT: Pixels = px(40.);

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
            (p.accent_tint, p.accent_text)
        } else {
            (p.ctl_rest, p.text_primary)
        };
        div()
            .w(WIDTH)
            .h(HEIGHT)
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .rounded(radius::SM)
            .bg(fill)
            .child(
                div()
                    .type_style(typeface::SMALL_MEDIUM)
                    .font_weight(FontWeight::BOLD)
                    .text_color(ink)
                    .child(self.resolution.unwrap_or_else(|| "\u{2014}".into())),
            )
            .children(self.dynamic_range.map(|range| {
                div()
                    .type_style(typeface::CAPTION)
                    .text_color(p.text_secondary)
                    .child(range.to_uppercase())
            }))
    }
}
