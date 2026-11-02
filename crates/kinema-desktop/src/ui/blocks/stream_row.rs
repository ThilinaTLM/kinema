//! Stream release row: quality tile, release name with quiet tags and
//! marks, then right-aligned size and seeders. No table header, no resting
//! fill: rows sit on the canvas, gain a surface fill on hover, and selection
//! is the v3 list rule (accent tint plus a 2 px leading accent marker).
//!
//! The row is presentation only. [`StreamRow::build`] returns the styled
//! element so the screen attaches focus, click, key and context-menu
//! behaviour without this pattern knowing about sources or playback.
use gpui::{prelude::*, *};

use crate::{
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::TypeExt,
    ui::theme::{palette, radius, space, typeface},
};

use crate::ui::motion::interaction::focus_ring;

/// Right-hand numeric columns; fixed so values align down the list.
const SIZE_WIDTH: Pixels = px(84.);
const SEEDERS_WIDTH: Pixels = px(72.);
/// Seeder count at which the value earns the success ink (the arrow glyph
/// and the "seeders" caption keep the meaning without color).
pub const HEALTHY_SEEDERS: u32 = 50;

pub struct StreamRow {
    id: ElementId,
    leading: Option<AnyElement>,
    title: SharedString,
    meta: Vec<AnyElement>,
    size: Option<SharedString>,
    seeders: Option<u32>,
    selected: bool,
}

impl StreamRow {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            leading: None,
            title: title.into(),
            meta: Vec::new(),
            size: None,
            seeders: None,
            selected: false,
        }
    }
    /// Usually a `QualityTile`.
    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }
    /// Items on the line under the release name (tags, account marks…).
    pub fn meta(mut self, item: impl IntoElement) -> Self {
        self.meta.push(item.into_any_element());
        self
    }
    pub fn size(mut self, size: Option<impl Into<SharedString>>) -> Self {
        self.size = size.map(Into::into);
        self
    }
    pub fn seeders(mut self, seeders: Option<u32>) -> Self {
        self.seeders = seeders;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The styled, focusable row; callers add listeners and `track_focus`.
    pub fn build(self, cx: &App) -> Stateful<Div> {
        let p = *palette(cx);
        let numeric = |value: AnyElement, caption: &'static str, width: Pixels| {
            div()
                .w(width)
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .type_style(typeface::BODY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text_primary)
                        .child(value),
                )
                .child(
                    div()
                        .type_style(typeface::CAPTION)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text_muted)
                        .child(caption),
                )
        };
        let size = div()
            .child(self.size.unwrap_or_else(|| "\u{2014}".into()))
            .into_any_element();
        let seeders = match self.seeders {
            Some(n) => {
                let ink = if n >= HEALTHY_SEEDERS {
                    p.status_ok
                } else {
                    p.text_secondary
                };
                div()
                    .flex()
                    .items_center()
                    .gap(space::S4)
                    .text_color(ink)
                    .child(Icon::new(IconName::ArrowUp).size(space::S12).color(ink))
                    .child(n.to_string())
                    .into_any_element()
            }
            None => div().child("\u{2014}").into_any_element(),
        };
        let selected = self.selected;
        div()
            .id(self.id)
            .relative()
            .w_full()
            .min_h(space::ROW_TWO_LINE)
            .px(space::S12 + space::S2)
            .py(space::S12 - space::S2)
            .flex()
            .items_center()
            .gap(space::S16)
            .rounded(radius::LG)
            .role(Role::ListItem)
            .aria_label(self.title.clone())
            .aria_selected(selected)
            .cursor_pointer()
            .when(selected, |el| el.bg(p.accent_tint))
            .when(!selected, |el| el.hover(move |s| s.bg(p.bg_surface)))
            .focus_visible(move |s| s.shadow(vec![focus_ring(&p)]))
            .when(selected, |el| {
                el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(space::S12 - space::S2)
                        .bottom(space::S12 - space::S2)
                        .w(space::S2)
                        .rounded_full()
                        .bg(p.primary),
                )
            })
            .children(self.leading.map(|leading| div().flex_none().child(leading)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(space::S6)
                    .child(
                        div()
                            .type_style(typeface::BODY)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.text_primary)
                            .clamp_lines(2)
                            .child(self.title),
                    )
                    .when(!self.meta.is_empty(), |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(space::S8)
                                .children(self.meta),
                        )
                    }),
            )
            .child(numeric(size, "size", SIZE_WIDTH))
            .child(numeric(seeders, "seeders", SEEDERS_WIDTH))
    }
}
