//! Popover body for header filters: a titled panel of labelled chip sections
//! with a single Reset and an optional footer note. Choices apply immediately,
//! so there is no Apply button; Reset is disabled when nothing is changed.
use super::{
    TypeExt,
    button::{Button, ButtonVariant},
    tokens::{palette, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;

type ResetHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct FilterPanel {
    title: SharedString,
    sections: Vec<FilterSection>,
    reset: Option<ResetHandler>,
    footer: Option<SharedString>,
}
impl FilterPanel {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            sections: Vec::new(),
            reset: None,
            footer: None,
        }
    }
    pub fn section(mut self, section: FilterSection) -> Self {
        self.sections.push(section);
        self
    }
    /// Enable Reset; omit while every facet is at its default.
    pub fn on_reset(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.reset = Some(Rc::new(handler));
        self
    }
    pub fn footer(mut self, note: impl Into<SharedString>) -> Self {
        self.footer = Some(note.into());
        self
    }
}
impl RenderOnce for FilterPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let reset = self.reset;
        div()
            .w(space::POPOVER_WIDTH)
            .flex()
            .flex_col()
            .gap(space::S16)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .type_style(typeface::HEADING)
                            .text_color(p.text_primary)
                            .child(self.title),
                    )
                    .child(
                        Button::new("filter-reset", "Reset")
                            .variant(ButtonVariant::Ghost)
                            .disabled(reset.is_none())
                            .on_click(move |_, window, cx| {
                                if let Some(reset) = &reset {
                                    reset(window, cx);
                                }
                            }),
                    ),
            )
            .children(self.sections)
            .children(self.footer.map(|note| {
                div()
                    .pt(space::S12)
                    .border_t(space::HAIRLINE)
                    .border_color(p.line_subtle)
                    .type_style(typeface::SMALL)
                    .text_color(p.text_muted)
                    .child(note)
            }))
    }
}

/// One labelled facet: caption, optional hint, then wrapping chips.
#[derive(IntoElement)]
pub struct FilterSection {
    label: SharedString,
    hint: Option<SharedString>,
    children: Vec<AnyElement>,
}
impl FilterSection {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            hint: None,
            children: Vec::new(),
        }
    }
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}
impl ParentElement for FilterSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
impl RenderOnce for FilterSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        div()
            .flex()
            .flex_col()
            .gap(space::S8)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(space::S8)
                    .child(
                        div()
                            .type_style(typeface::CAPTION)
                            .text_color(p.text_secondary)
                            .child(self.label),
                    )
                    .children(self.hint.map(|hint| {
                        div()
                            .type_style(typeface::SMALL)
                            .text_color(p.text_muted)
                            .child(hint)
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(space::S6)
                    .children(self.children),
            )
    }
}
