//! `TagList`: quiet inline technical tags (codec, audio, languages) followed
//! by an optional lighter origin label. No boxes: rows stay calm and tags
//! never compete with badges or selection.
use super::{
    TypeExt,
    tokens::{palette, space, typeface},
};
use gpui::{prelude::*, *};

#[derive(IntoElement, Default)]
pub struct TagList {
    tags: Vec<SharedString>,
    origin: Option<SharedString>,
}

impl TagList {
    pub fn new(tags: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self {
            tags: tags.into_iter().map(Into::into).collect(),
            origin: None,
        }
    }
    /// Provider or tracker, rendered after the tags in a lighter weight.
    pub fn origin(mut self, origin: impl Into<SharedString>) -> Self {
        self.origin = Some(origin.into());
        self
    }
}

impl RenderOnce for TagList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(space::S12 - space::S2)
            .gap_y(space::S2)
            .type_style(typeface::CAPTION)
            .text_color(p.text_muted)
            .children(self.tags.into_iter().map(|tag| div().child(tag)))
            .children(
                self.origin
                    .map(|origin| div().font_weight(FontWeight::MEDIUM).child(origin)),
            )
    }
}
