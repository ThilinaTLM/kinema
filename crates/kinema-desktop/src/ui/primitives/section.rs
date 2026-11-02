//! Titled content block: heading, optional count and trailing action, body.
use gpui::{prelude::*, *};

use crate::{
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{palette, size, space},
};

/// Heading row shared by [`Section`] and `Carousel`.
pub(super) fn header(
    palette: &crate::ui::theme::Palette,
    title: SharedString,
    count: Option<usize>,
    trailing: Vec<AnyElement>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(space::S8)
        .min_h(size::CONTROL_SM)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_baseline()
                .gap(space::S8)
                .child(
                    div()
                        .text(TextSize::Lg)
                        .weight(Weight::Semibold)
                        .text_color(palette.foreground)
                        .truncate()
                        .child(title),
                )
                .when_some(count, |el, count| {
                    el.child(
                        div()
                            .text(TextSize::Sm)
                            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                            .text_color(palette.subtle_foreground)
                            .child(count.to_string()),
                    )
                }),
        )
        // Direct children: nesting negative-margin buttons in a wrapper collapses its width.
        .children(trailing)
}

#[derive(IntoElement)]
pub struct Section {
    title: SharedString,
    count: Option<usize>,
    trailing: Option<AnyElement>,
    base: Div,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            count: None,
            trailing: None,
            base: div().flex().flex_col().gap(space::S8).w_full(),
            children: Vec::new(),
        }
    }
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
    /// Right-aligned header slot, usually a ghost button or segmented control.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Section {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Section {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        self.base
            .child(header(
                &p,
                self.title,
                self.count,
                self.trailing.into_iter().collect(),
            ))
            .children(self.children)
    }
}
