//! `StatTile`: one labelled technical fact (caption label over a heading
//! value) on a raised tile. Tiles are laid out in equal-width rows by callers.
use crate::{
    ui::theme::typography::TypeExt,
    ui::theme::{palette, radius, space, typeface},
};
use gpui::{prelude::*, *};

#[derive(IntoElement)]
pub struct StatTile {
    label: SharedString,
    value: Option<SharedString>,
    ink: Option<Hsla>,
}

impl StatTile {
    /// A missing value shows an em dash, never an empty tile.
    pub fn new(label: impl Into<SharedString>, value: Option<impl Into<SharedString>>) -> Self {
        Self {
            label: label.into(),
            value: value.map(Into::into).filter(|v| !v.is_empty()),
            ink: None,
        }
    }
    /// Semantic value ink (e.g. `status_ok`); callers keep a text cue as well.
    pub fn ink(mut self, ink: Hsla) -> Self {
        self.ink = Some(ink);
        self
    }
}

impl RenderOnce for StatTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        div()
            .flex_1()
            .min_w_0()
            .px(space::S12 - space::S2)
            .py(space::S8)
            .rounded(radius::MD)
            .bg(p.bg_raised)
            .border(space::HAIRLINE)
            .border_color(p.line_subtle)
            .flex()
            .flex_col()
            .gap(space::S2)
            .child(
                div()
                    .type_style(typeface::CAPTION)
                    .text_color(p.text_muted)
                    .truncate()
                    .child(self.label),
            )
            .child(
                div()
                    .type_style(typeface::HEADING)
                    .text_color(self.ink.unwrap_or(p.text_primary))
                    .truncate()
                    .child(self.value.unwrap_or_else(|| "\u{2014}".into())),
            )
    }
}
