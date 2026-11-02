//! Horizontal strip of fixed-gap items with optional header, edge fades and
//! paging buttons.
//!
//! Accessibility: the strip is a `Group` named by the title; Previous/Next are
//! icon `Button`s (named "Previous in <title>"), disabled at the ends so they
//! leave the tab order. Items keep their own roles and focus. Known gap: a
//! focused item is not scrolled into view automatically, so keyboard users
//! page with the buttons.
use std::time::{Duration, Instant};

use gpui::{prelude::*, *};

use crate::{
    ui::motion::{self, transition::COLUMN},
    ui::primitives::IconName,
    ui::primitives::button::Button,
    ui::theme::{palette, space},
};

use super::section::header;

/// Width of the gradient that hints at clipped items.
const FADE_WIDTH: Pixels = space::S32;
/// Fraction of the viewport a paging click scrolls.
const PAGE_FRACTION: f32 = 0.8;
/// Offsets within this distance of an end count as "at the end".
const EDGE_SLOP: Pixels = px(1.0);

#[derive(IntoElement)]
pub struct Carousel {
    id: ElementId,
    title: Option<SharedString>,
    count: Option<usize>,
    trailing: Option<AnyElement>,
    items: Vec<AnyElement>,
    gap: Pixels,
    inset: Pixels,
    edge: Option<Hsla>,
    handle: Option<ScrollHandle>,
    paging: bool,
}

impl Carousel {
    /// `id` must be stable across renders: it keys the scroll position.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            title: None,
            count: None,
            trailing: None,
            items: Vec::new(),
            gap: space::GRID_GAP,
            inset: space::GRID_INSET,
            edge: None,
            handle: None,
            paging: true,
        }
    }
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }
    pub fn item(mut self, item: impl IntoElement) -> Self {
        self.items.push(item.into_any_element());
        self
    }
    pub fn items(mut self, items: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.items
            .extend(items.into_iter().map(IntoElement::into_any_element));
        self
    }
    /// Space between items (default: grid gap).
    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }
    /// Leading/trailing padding of the strip (default: grid inset).
    pub fn inset(mut self, inset: Pixels) -> Self {
        self.inset = inset;
        self
    }
    /// Color the edge fades blend into (default: canvas).
    pub fn edge_color(mut self, color: Hsla) -> Self {
        self.edge = Some(color);
        self
    }
    /// Share a scroll handle to restore or drive the position.
    pub fn scroll_handle(mut self, handle: ScrollHandle) -> Self {
        self.handle = Some(handle);
        self
    }
    /// Previous/next buttons in the header (needs a title).
    pub fn paging(mut self, paging: bool) -> Self {
        self.paging = paging;
        self
    }
}

/// Which edges still hide content. Before the first layout the viewport is
/// unmeasured, so the trailing edge is assumed to overflow.
fn overflow_edges(offset: Pixels, max: Pixels, measured: bool) -> (bool, bool) {
    if !measured {
        return (false, true);
    }
    (offset < -EDGE_SLOP, max + offset > EDGE_SLOP)
}

/// Scroll offset a paging click moves to, clamped to the scrollable range.
fn page_target(offset: Pixels, viewport: Pixels, direction: f32, max: Pixels) -> Pixels {
    (offset + viewport * PAGE_FRACTION * direction).clamp(-max, px(0.0))
}

/// An in-flight animated page. `last` is the offset we wrote last frame: if
/// the handle differs, the user scrolled and the animation yields.
#[derive(Clone, Copy)]
struct PageAnim {
    from: Pixels,
    to: Pixels,
    start: Instant,
    last: Pixels,
}

/// Advance the page animation by one frame; returns the clamped progress.
fn animate_page(
    state: &Entity<Option<PageAnim>>,
    handle: &ScrollHandle,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(anim) = *state.read(cx) else {
        return;
    };
    let mut offset = handle.offset();
    if (offset.x - anim.last).abs() > px(0.5) {
        state.update(cx, |state, _| *state = None);
        return;
    }
    let t = if duration.is_zero() {
        1.0
    } else {
        (anim.start.elapsed().as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    };
    offset.x = px(motion::lerp(
        f32::from(anim.from),
        f32::from(anim.to),
        COLUMN.ease.apply(t),
    ));
    handle.set_offset(offset);
    let written = handle.offset().x;
    state.update(cx, |state, _| {
        *state = (t < 1.0).then_some(PageAnim {
            last: written,
            ..anim
        })
    });
    if t < 1.0 {
        window.request_animation_frame();
    }
}

fn fade(color: Hsla, leading: bool) -> Div {
    let (from, to) = if leading {
        (color, color.opacity(0.0))
    } else {
        (color.opacity(0.0), color)
    };
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(FADE_WIDTH)
        .when(leading, |el| el.left_0())
        .when(!leading, |el| el.right_0())
        .bg(linear_gradient(
            90.,
            linear_color_stop(from, 0.),
            linear_color_stop(to, 1.),
        ))
}

/// Accessible name of a paging control: "Next" or "Next in <title>".
fn page_name(action: &str, title: Option<&SharedString>) -> SharedString {
    match title {
        Some(title) => format!("{action} in {title}").into(),
        None => action.to_owned().into(),
    }
}

impl RenderOnce for Carousel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let handle = self.handle.unwrap_or_else(|| {
            window
                .use_keyed_state((self.id.clone(), "scroll"), cx, |_, _| ScrollHandle::new())
                .read(cx)
                .clone()
        });
        let measured = handle.bounds().size.width > px(0.0);
        if !measured {
            window.request_animation_frame();
        }
        let duration = motion::duration(COLUMN, cx);
        let anim = window.use_keyed_state((self.id.clone(), "page"), cx, |_, _| None::<PageAnim>);
        animate_page(&anim, &handle, duration, window, cx);
        let (more_before, more_after) =
            overflow_edges(handle.offset().x, handle.max_offset().x, measured);
        let page = |direction: f32, handle: ScrollHandle| {
            let anim = anim.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                let current = handle.offset().x;
                // Repeated clicks stack onto the pending destination.
                let base = anim.read(cx).map_or(current, |a| a.to);
                let target = page_target(
                    base,
                    handle.bounds().size.width,
                    direction,
                    handle.max_offset().x,
                );
                if duration.is_zero() {
                    let mut offset = handle.offset();
                    offset.x = target;
                    handle.set_offset(offset);
                    anim.update(cx, |state, _| *state = None);
                } else {
                    anim.update(cx, |state, _| {
                        *state = Some(PageAnim {
                            from: current,
                            to: target,
                            start: Instant::now(),
                            last: current,
                        })
                    });
                }
                window.refresh();
            }
        };
        let title_text = self.title.clone();
        let show_paging = self.paging && self.title.is_some();
        let mut trailing: Vec<AnyElement> = self.trailing.into_iter().collect();
        if show_paging {
            trailing.push(
                Button::icon_button((self.id.clone(), "prev"), IconName::ChevronLeft, "Previous")
                    .name(page_name("Previous", title_text.as_ref()))
                    .disabled(!more_before)
                    .on_click(page(1.0, handle.clone()))
                    .into_any_element(),
            );
            trailing.push(
                Button::icon_button((self.id.clone(), "next"), IconName::ChevronRight, "Next")
                    .name(page_name("Next", title_text.as_ref()))
                    .disabled(!more_after)
                    .on_click(page(-1.0, handle.clone()))
                    .into_any_element(),
            );
        }
        let edge = self.edge.unwrap_or(p.bg_canvas);
        div()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(space::S4)
            .when_some(self.title, |el, title| {
                el.child(
                    div()
                        .w_full()
                        .px(self.inset)
                        .child(header(&p, title, self.count, trailing)),
                )
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id((self.id, "strip"))
                            .role(Role::Group)
                            .when_some(title_text.clone(), |el, title| el.aria_label(title))
                            .w_full()
                            .min_w_0()
                            .flex()
                            .gap(self.gap)
                            .px(self.inset)
                            // Room for focus rings and card shadows inside the clip.
                            .py(space::S8)
                            .overflow_x_scroll()
                            .track_scroll(&handle)
                            .children(
                                self.items
                                    .into_iter()
                                    .map(|item| div().flex_none().child(item)),
                            ),
                    )
                    .when(more_before, |el| el.child(fade(edge, true)))
                    .when(more_after, |el| el.child(fade(edge, false))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{overflow_edges, page_name, page_target};
    use gpui::px;

    #[test]
    fn paging_controls_are_named_after_their_carousel() {
        assert_eq!(
            page_name("Next", Some(&"Similar".into())).as_ref(),
            "Next in Similar"
        );
        assert_eq!(page_name("Previous", None).as_ref(), "Previous");
    }

    #[test]
    fn edge_fades_follow_scroll_position() {
        let max = px(400.0);
        assert_eq!(overflow_edges(px(0.0), max, true), (false, true));
        assert_eq!(overflow_edges(px(-200.0), max, true), (true, true));
        assert_eq!(overflow_edges(px(-400.0), max, true), (true, false));
        assert_eq!(overflow_edges(px(0.0), px(0.0), true), (false, false));
        assert_eq!(overflow_edges(px(0.0), px(0.0), false), (false, true));
    }

    #[test]
    fn paging_clamps_to_scroll_range() {
        let (view, max) = (px(500.0), px(1000.0));
        assert_eq!(page_target(px(0.0), view, -1.0, max), px(-400.0));
        assert_eq!(page_target(px(-900.0), view, -1.0, max), px(-1000.0));
        assert_eq!(page_target(px(-200.0), view, 1.0, max), px(0.0));
        assert_eq!(page_target(px(0.0), view, 1.0, max), px(0.0));
    }
}
