//! Side-panel choice controls: a full-width [`ChoiceCard`] (radio-style, with a
//! title and a status line) and a compact [`ChoiceChip`] (label plus count).
//!
//! Both are stateless and controlled: the owner passes `selected` and reacts to
//! `on_click`. Each is its own tab stop and activates with Enter/Space.
//!
//! Accessibility: cards are `RadioButton`s and chips are `RadioButton`s (or
//! `CheckBox`es when multi-select), named by their label, with `selected`
//! exposed. The focus ring is a shadow that follows the host radius. Selection
//! is also shown by an edge and fill (and a check on multi-select chips), not
//! by colour alone. Arrow keys between siblings are not provided: each option
//! is a separate tab stop and the owner owns the group.
use crate::ui::theme::layout::controls::CHOICE_TILE_WIDTH;
use crate::{
    ui::motion::interaction::focus_ring,
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{palette, radius, size, space},
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
                p.primary_text
            } else {
                p.subtle_foreground
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
            .rounded(radius::MD)
            .border(size::HAIRLINE)
            .border_color(if selected { p.primary } else { p.border })
            .bg(if selected { p.selection } else { p.secondary })
            .cursor_pointer()
            .focusable()
            .tab_stop(true)
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_selected(selected)
            .when(!selected, |el| el.hover(move |s| s.bg(p.accent)))
            .focus_visible(move |s| s.shadow(vec![focus_ring(&p)]))
            .child(mark)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text(TextSize::Base)
                            .weight(Weight::Semibold)
                            .text_color(p.foreground)
                            .child(self.title),
                    )
                    .children(self.subtitle.map(|sub| {
                        div()
                            .text(TextSize::Sm)
                            .text_color(if selected {
                                p.primary_text
                            } else {
                                p.subtle_foreground
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
    multi: bool,
    on_click: Option<ClickHandler>,
}

impl ChoiceChip {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            count: None,
            selected: false,
            multi: false,
            on_click: None,
        }
    }
    /// Independent (checkbox) chip in a multi-select set; shows a check when on.
    pub fn multi(mut self) -> Self {
        self.multi = true;
        self
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
            .h(size::CONTROL_SM)
            .px(space::S12)
            .flex()
            .items_center()
            .justify_center()
            .gap(space::S6)
            .rounded(radius::MD)
            .border(size::HAIRLINE)
            .border_color(if selected { p.primary } else { p.border })
            .bg(if selected { p.selection } else { p.secondary })
            .cursor_pointer()
            .focusable()
            .tab_stop(true)
            .role(if self.multi {
                Role::CheckBox
            } else {
                Role::RadioButton
            })
            .aria_label(label)
            .aria_selected(selected)
            .when(!selected, |el| el.hover(move |s| s.bg(p.accent)))
            .focus_visible(move |s| s.shadow(vec![focus_ring(&p)]))
            .when(self.multi && selected, |el| {
                el.pl(space::S8).child(
                    Icon::new(IconName::Check)
                        .size(space::S12)
                        .color(p.primary_text),
                )
            })
            .child(
                div()
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .text_color(if selected {
                        p.primary_text
                    } else {
                        p.foreground
                    })
                    .child(self.label),
            )
            .children(self.count.map(|n| {
                div()
                    .text(TextSize::Sm)
                    .text_color(if selected {
                        p.primary_text
                    } else {
                        p.subtle_foreground
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

/// A large, icon-led choice for popover pickers (e.g. Discover / Library):
/// glyph tile, title, a one-to-two line description and an optional meta line.
/// Selection shows an accent edge, tinted glyph and a check mark.
#[derive(IntoElement)]
pub struct ChoiceTile {
    id: ElementId,
    icon: IconName,
    title: SharedString,
    description: SharedString,
    meta: Option<SharedString>,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl ChoiceTile {
    pub fn new(
        id: impl Into<ElementId>,
        icon: IconName,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            title: title.into(),
            description: description.into(),
            meta: None,
            selected: false,
            on_click: None,
        }
    }
    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
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

impl RenderOnce for ChoiceTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let selected = self.selected;
        let handler = self.on_click;
        let label: SharedString = format!("{}, {}", self.title, self.description).into();
        let glyph = div()
            .size(size::TOOLBAR_CONTROL)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::LG)
            .bg(if selected { p.selection } else { p.accent })
            .child(Icon::new(self.icon).color(if selected {
                p.primary_text
            } else {
                p.foreground
            }));
        div()
            .id(self.id)
            .w(CHOICE_TILE_WIDTH)
            .p(space::S12)
            .flex()
            .flex_col()
            .gap(space::S8)
            .rounded(radius::LG)
            .border(size::HAIRLINE)
            .border_color(if selected { p.primary } else { p.border })
            .bg(if selected { p.selection } else { p.secondary })
            .cursor_pointer()
            .focusable()
            .tab_stop(true)
            .role(Role::RadioButton)
            .aria_label(label)
            .aria_selected(selected)
            .when(!selected, |el| el.hover(move |s| s.bg(p.accent)))
            .focus_visible(move |s| s.shadow(vec![focus_ring(&p)]))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(glyph)
                    .when(selected, |el| {
                        el.child(Icon::new(IconName::CircleCheck).color(p.primary_text))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S2)
                    .child(
                        div()
                            .text(TextSize::Base)
                            .weight(Weight::Semibold)
                            .text_color(p.foreground)
                            .child(self.title),
                    )
                    .child(
                        div()
                            .text(TextSize::Sm)
                            .text_color(p.muted_foreground)
                            .child(self.description),
                    ),
            )
            .children(self.meta.map(|meta| {
                div()
                    .pt(space::S4)
                    .border_t(size::HAIRLINE)
                    .border_color(p.border)
                    .text(TextSize::Xs)
                    .weight(Weight::Semibold)
                    .text_color(if selected {
                        p.primary_text
                    } else {
                        p.subtle_foreground
                    })
                    .child(meta)
            }))
            .on_click(move |_, window, cx| {
                if let Some(handler) = &handler {
                    handler(window, cx);
                }
            })
    }
}
