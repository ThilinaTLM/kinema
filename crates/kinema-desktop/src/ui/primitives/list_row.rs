//! Dense list row: leading slot, title/subtitle, trailing slot. The highlight
//! is an inset rounded rectangle so adjacent rows never touch.
//!
//! Accessibility: `Role::ListItem` named by the title, with `selected`
//! exposed. With `on_click` the row is one tab stop, Enter/Space activate it
//! and a focus ring (inset, following the row radius) shows keyboard focus.
//! The selected row also gets a leading marker bar, so selection is not
//! colour-only. Without `on_click` it is static content.
use std::rc::Rc;

use gpui::{prelude::*, *};

use crate::{
    ui::motion::{
        interaction::{StateStyle, state_fill},
        transition::FEEDBACK,
    },
    ui::primitives::button::ClickHandler,
    ui::theme::typography::{TextSize, TypeExt},
    ui::theme::{opacity, palette, radius, size, space},
};

/// Gap between the row highlight and the container edge.
const INSET: Pixels = space::S4;

#[derive(IntoElement)]
pub struct ListRow {
    id: ElementId,
    leading: Option<AnyElement>,
    title: SharedString,
    subtitle: Option<SharedString>,
    subtitle_lines: usize,
    trailing: Option<AnyElement>,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl ListRow {
    /// `id` must be stable across renders: it keys hover and focus state.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            leading: None,
            title: title.into(),
            subtitle: None,
            subtitle_lines: 1,
            trailing: None,
            selected: false,
            on_click: None,
        }
    }
    /// Thumbnail, icon or index placeholder.
    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
    /// Let the subtitle wrap to this many lines (default 1, truncated with an
    /// ellipsis) before it is cut.
    pub fn subtitle_lines(mut self, lines: usize) -> Self {
        self.subtitle_lines = lines.max(1);
        self
    }
    /// Badges, duration, overflow button…
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Makes the row focusable; Enter/Space activate it like a click.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ListRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let interactive = self.on_click.is_some();
        let hovered = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let active = interactive && *hovered.read(cx);
        let (rest, hover) = if self.selected {
            (
                p.selection,
                p.selection.blend(p.accent.opacity(opacity::SELECTED_HOVER)),
            )
        } else {
            (transparent_black(), p.accent)
        };
        let state = StateStyle::new(&p, rest, hover, hover, p.foreground);
        // The focus ring is an inset shadow: no border, so focus never shifts layout.
        let min_height = if self.subtitle.is_some() {
            size::ROW_TALL
        } else {
            size::ROW
        };
        let lines = self.subtitle_lines;
        let face = div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .min_h(min_height)
            .flex()
            .items_center()
            .gap(space::S12)
            .px(space::S8)
            .rounded(radius::MD)
            .role(Role::ListItem)
            .aria_label(self.title.clone())
            .aria_selected(self.selected)
            .when(interactive, |el| {
                state
                    .focus(el.focusable().tab_stop(true).cursor_pointer())
                    .on_hover(move |value, _, cx| {
                        hovered.update(cx, |state, cx| {
                            *state = *value;
                            cx.notify();
                        })
                    })
            })
            .when_some(self.on_click, |el, handler| {
                el.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .when(self.selected, |el| {
                el.child(
                    div()
                        .absolute()
                        .left(space::S2)
                        .top(space::S8)
                        .bottom(space::S8)
                        .w(space::S2)
                        .rounded(radius::FULL)
                        .bg(p.primary),
                )
            })
            .when_some(self.leading, |el, leading| {
                el.child(div().flex_none().child(leading))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text(TextSize::Base)
                            .text_color(p.foreground)
                            .truncate()
                            .child(self.title),
                    )
                    .when_some(self.subtitle, |el, subtitle| {
                        el.child(
                            div()
                                .text(TextSize::Sm)
                                .text_color(p.muted_foreground)
                                .when(lines <= 1, |el| el.truncate())
                                .when(lines > 1, |el| el.clamp_lines(lines))
                                .child(subtitle),
                        )
                    }),
            )
            .when_some(self.trailing, |el, trailing| {
                el.child(div().flex_none().child(trailing))
            });
        let key = (self.id, "fill");
        div().w_full().px(INSET).child(state_fill(
            face,
            ElementId::from(key),
            active,
            rest,
            hover,
            FEEDBACK,
            cx,
        ))
    }
}
