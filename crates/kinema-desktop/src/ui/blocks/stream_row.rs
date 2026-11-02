//! Stream release card: one per row, built on [`MediaCard`] so it shares the
//! raised fill, edge, radius and hover lift of the poster and episode cards.
//! Inside: quality tile, release name with quiet tags and marks, then
//! right-aligned size and seeders. Selection is the accent edge (the same
//! treatment as the next-up episode) plus a soft selection tint.
//!
//! The card is presentation only. [`StreamRow::build`] returns the styled
//! host so the screen attaches focus, click, key and context-menu behaviour
//! without this pattern knowing about sources or playback; the hover lift
//! happens inside the host, so the hit area never moves.
use crate::ui::theme::layout::detail::{STREAM_SEEDERS_WIDTH, STREAM_SIZE_WIDTH};
use gpui::{prelude::*, *};

use crate::{
    ui::blocks::media_card::MediaCard,
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{palette, radius, size, space},
};

use crate::ui::motion::interaction::focus_ring;

/// Seeder count at which the value earns the success ink (the arrow glyph
/// and the "seeders" caption keep the meaning without color).
pub const HEALTHY_SEEDERS: u32 = 50;

pub struct StreamRow {
    index: usize,
    leading: Option<AnyElement>,
    title: SharedString,
    meta: Vec<AnyElement>,
    size: Option<SharedString>,
    seeders: Option<u32>,
    selected: bool,
}

impl StreamRow {
    /// `index` must be stable across renders: it keys the row and its hover tween.
    pub fn new(index: usize, title: impl Into<SharedString>) -> Self {
        Self {
            index,
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

    /// The styled, focusable host; callers add listeners and `track_focus`.
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
                        .text(TextSize::Base)
                        .text_color(p.foreground)
                        .child(value),
                )
                .child(
                    div()
                        .text(TextSize::Xs)
                        .text_color(p.subtle_foreground)
                        .child(caption),
                )
        };
        let size = div()
            .child(self.size.unwrap_or_else(|| "\u{2014}".into()))
            .into_any_element();
        let seeders = match self.seeders {
            Some(n) => {
                let ink = if n >= HEALTHY_SEEDERS {
                    p.success
                } else {
                    p.muted_foreground
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
        let content = div()
            .w_full()
            .min_h(size::ROW_TALL)
            .px(space::S16)
            .py(space::S12)
            .flex()
            .items_center()
            .gap(space::S16)
            .when(selected, |el| el.bg(p.selection))
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
                            .text(TextSize::Base)
                            .weight(Weight::Regular)
                            .text_color(p.foreground)
                            .clamp_lines(2)
                            .child(self.title.clone()),
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
            .child(numeric(size, "size", STREAM_SIZE_WIDTH))
            .child(numeric(seeders, "seeders", STREAM_SEEDERS_WIDTH));
        div()
            .id(("stream", self.index))
            .w_full()
            .flex_none()
            .rounded(radius::LG)
            .role(Role::ListItem)
            .aria_label(self.title)
            .aria_selected(selected)
            .cursor_pointer()
            .focus_visible(move |s| s.shadow(vec![focus_ring(&p)]))
            .child(
                MediaCard::new(("stream-card", self.index))
                    .accent_edge(selected)
                    .child(content),
            )
    }
}
