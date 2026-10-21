//! Side-panel choice controls: a full-width [`ChoiceCard`] (radio-style, with a
//! title and a status line) and a compact [`ChoiceChip`] (label plus count).
//!
//! Both are stateless and controlled: the owner passes `selected` and reacts to
//! `on_click`. Each is its own tab stop and activates with Enter/Space.
use super::{
    Icon, IconName, TypeExt,
    patterns::focus_ring,
    tokens::{palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;

type ClickHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A selectable card: radio mark, title, optional one-line status.
#[derive(IntoElement)]
pub struct ChoiceCard {
    id: ElementId,
    title: SharedString,
    subtitle: Option<SharedString>,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl ChoiceCard {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            selected: false,
            on_click: None,
        }
    }
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ChoiceCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let selected = self.selected;
        let handler = self.on_click;
        let label: SharedString = match &self.subtitle {
            Some(sub) => format!("{}, {sub}", self.title).into(),
            None => self.title.clone(),
        };
        let mark = div().size(space::S16).flex_none().child(
            Icon::new(if selected {
                IconName::CircleDot
            } else {
                IconName::Circle
            })
            .color(if selected {
                p.accent_text
            } else {
                p.text_muted
            }),
        );
        div()
            .id(self.id)
            .w_full()
            .px(space::S12)
            .py(space::S8)
            .flex()
            .items_center()
            .gap(space::S12)
            .rounded(radius::SM)
            .border(space::HAIRLINE)
            .border_color(if selected { p.accent } else { p.line_subtle })
            .bg(if selected { p.accent_tint } else { p.ctl_rest })
            .cursor_pointer()
            .focusable()
            .tab_stop(true)
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_selected(selected)
            .when(!selected, |el| el.hover(move |s| s.bg(p.ctl_hover)))
            .focus_visible(move |s| s.shadow(vec![focus_ring(p.focus_ring)]))
            .child(mark)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .type_style(typeface::BODY_STRONG)
                            .text_color(p.text_primary)
                            .child(self.title),
                    )
                    .children(self.subtitle.map(|sub| {
                        div()
                            .type_style(typeface::SMALL)
                            .text_color(if selected {
                                p.accent_text
                            } else {
                                p.text_muted
                            })
                            .truncate()
                            .child(sub)
                    })),
            )
            .on_click(move |_, window, cx| {
                if let Some(handler) = &handler {
                    handler(window, cx);
                }
            })
    }
}

/// A compact selectable chip: label with an optional trailing count.
#[derive(IntoElement)]
pub struct ChoiceChip {
    id: ElementId,
    label: SharedString,
    count: Option<usize>,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl ChoiceChip {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            count: None,
            selected: false,
            on_click: None,
        }
    }
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ChoiceChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let selected = self.selected;
        let handler = self.on_click;
        let label: SharedString = match self.count {
            Some(n) => format!("{}, {n}", self.label).into(),
            None => self.label.clone(),
        };
        div()
            .id(self.id)
            .h(space::CONTROL_HEIGHT)
            .px(space::S12)
            .flex()
            .items_center()
            .justify_center()
            .gap(space::S6)
            .rounded(radius::SM)
            .border(space::HAIRLINE)
            .border_color(if selected { p.accent } else { p.line_subtle })
            .bg(if selected { p.accent_tint } else { p.ctl_rest })
            .cursor_pointer()
            .focusable()
            .tab_stop(true)
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_selected(selected)
            .when(!selected, |el| el.hover(move |s| s.bg(p.ctl_hover)))
            .focus_visible(move |s| s.shadow(vec![focus_ring(p.focus_ring)]))
            .child(
                div()
                    .type_style(typeface::SMALL_MEDIUM)
                    .text_color(if selected {
                        p.accent_text
                    } else {
                        p.text_primary
                    })
                    .child(self.label),
            )
            .children(self.count.map(|n| {
                div()
                    .type_style(typeface::SMALL)
                    .text_color(if selected {
                        p.accent_text
                    } else {
                        p.text_muted
                    })
                    .child(n.to_string())
            }))
            .on_click(move |_, window, cx| {
                if let Some(handler) = &handler {
                    handler(window, cx);
                }
            })
    }
}
