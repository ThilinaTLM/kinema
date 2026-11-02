//! Vertical scrolling with an overlay scrollbar and keyboard paging.
use crate::{
    ui::motion::{self, transition::CROSSFADE},
    ui::theme::{palette, space},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc, time::Instant};

/// Idle thumb opacity; the thumb rises to 1.0 while scrolling or hovered.
const IDLE_ALPHA: f32 = 0.35;

fn thumb_height(handle: &ScrollHandle) -> Pixels {
    let viewport = handle.bounds().size.height;
    let content = viewport + handle.max_offset().y;
    if content <= px(0.0) {
        return viewport;
    }
    (viewport * (viewport / content))
        .max(space::S20)
        .min(viewport)
}
#[derive(IntoElement)]
pub struct ScrollArea {
    id: ElementId,
    handle: ScrollHandle,
    gutter: Pixels,
    children: Vec<AnyElement>,
}
impl ScrollArea {
    pub fn new(id: impl Into<ElementId>, handle: ScrollHandle) -> Self {
        Self {
            id: id.into(),
            handle,
            gutter: px(0.0),
            children: Vec::new(),
        }
    }
    /// Reserve end padding so content never touches the overlay scrollbar.
    /// Full-bleed pages leave it off; columns beside other content use it.
    pub fn gutter(mut self, gutter: Pixels) -> Self {
        self.gutter = gutter;
        self
    }
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }
}
impl RenderOnce for ScrollArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let keys = self.handle.clone();
        let paint_handle = self.handle.clone();
        let pointer = self.handle.clone();
        let drag = window
            .use_keyed_state(self.id.clone(), cx, |_, _| {
                std::rc::Rc::new(std::cell::Cell::new(None::<(Pixels, Pixels)>))
            })
            .read(cx)
            .clone();
        let start_drag = drag.clone();
        let release_drag = drag.clone();
        let release_outside = drag.clone();
        let drag_handle = self.handle.clone();
        let activity = window
            .use_keyed_state((self.id.clone(), "activity"), cx, |_, _| {
                Rc::new(Cell::new((px(0.0), Instant::now())))
            })
            .read(cx)
            .clone();
        let fade_drag = drag.clone();
        let fade = motion::resolve(CROSSFADE, cx);
        let reduced = motion::reduced(cx);
        div()
            .id(self.id)
            .relative()
            .size_full()
            .min_h_0()
            .on_mouse_move(move |event, window, cx| {
                if let Some((start_y, start_offset)) = drag.get() {
                    let max = drag_handle.max_offset().y;
                    let travel = drag_handle.bounds().size.height - thumb_height(&drag_handle);
                    if travel > px(0.0) {
                        let mut offset = drag_handle.offset();
                        offset.y = (start_offset - max * ((event.position.y - start_y) / travel))
                            .clamp(-max, px(0.0));
                        drag_handle.set_offset(offset);
                        window.refresh();
                    }
                    cx.stop_propagation();
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| release_drag.set(None))
            .on_mouse_up_out(MouseButton::Left, move |_, _, _| release_outside.set(None))
            .child(
                div()
                    .id("viewport")
                    .size_full()
                    .pr(self.gutter)
                    .overflow_y_scroll()
                    .track_scroll(&self.handle)
                    .focusable()
                    .tab_stop(true)
                    .focus_visible(move |s| s.border(space::FOCUS_RING).border_color(p.focus_ring))
                    .on_key_down(move |event, window, cx| {
                        let direction = match event.keystroke.key.as_str() {
                            "pageup" => 1.0,
                            "pagedown" => -1.0,
                            _ => return,
                        };
                        let mut offset = keys.offset();
                        offset.y = (offset.y + keys.bounds().size.height * direction)
                            .clamp(-keys.max_offset().y, px(0.0));
                        keys.set_offset(offset);
                        window.refresh();
                        cx.stop_propagation();
                    })
                    .children(self.children),
            )
            .child(
                div()
                    .id("scrollbar")
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(space::S6)
                    .hover(|s| s.w(px(10.0)))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        let bounds = pointer.bounds();
                        let max = pointer.max_offset().y;
                        let height = thumb_height(&pointer);
                        let travel = bounds.size.height - height;
                        if max > px(0.0) && travel > px(0.0) {
                            let top =
                                bounds.top() + travel * (-pointer.offset().y / max).clamp(0.0, 1.0);
                            if event.position.y < top || event.position.y > top + height {
                                let fraction = ((event.position.y - bounds.top() - height / 2.0)
                                    / travel)
                                    .clamp(0.0, 1.0);
                                let mut offset = pointer.offset();
                                offset.y = -max * fraction;
                                pointer.set_offset(offset);
                                window.refresh();
                            }
                            start_drag.set(Some((event.position.y, pointer.offset().y)));
                        }
                        cx.stop_propagation();
                    })
                    .child(
                        canvas(
                            move |_, _, _| (),
                            move |bounds, _, window, _| {
                                let viewport = paint_handle.bounds().size.height;
                                let max = paint_handle.max_offset().y;
                                if max <= px(0.0) || viewport <= px(0.0) {
                                    return;
                                }
                                let fraction = viewport / (viewport + max);
                                let height = (bounds.size.height * fraction)
                                    .max(space::S20)
                                    .min(bounds.size.height);
                                let progress = (-paint_handle.offset().y / max).clamp(0.0, 1.0);
                                let top = bounds.top() + (bounds.size.height - height) * progress;
                                // Fade: full while scrolling/dragging/hovered, then ease to idle.
                                let offset = paint_handle.offset().y;
                                let (last, _) = activity.get();
                                if (last - offset).abs() > px(0.5) {
                                    activity.set((offset, Instant::now()));
                                }
                                let active = fade_drag.get().is_some()
                                    || bounds.contains(&window.mouse_position());
                                let alpha = if reduced || active {
                                    1.0
                                } else {
                                    let hold = crate::ui::theme::motion::timing::SCROLLBAR_IDLE;
                                    let elapsed = activity.get().1.elapsed();
                                    let t = if elapsed <= hold {
                                        window.request_animation_frame();
                                        0.0
                                    } else if fade.is_instant() {
                                        1.0
                                    } else {
                                        // Fades over twice the preset duration.
                                        let t = ((elapsed - hold).as_secs_f32()
                                            / (fade.duration.as_secs_f32() * 2.0))
                                            .min(1.0);
                                        if t < 1.0 {
                                            window.request_animation_frame();
                                        }
                                        fade.ease.apply(t)
                                    };
                                    1.0 - t * (1.0 - IDLE_ALPHA)
                                };
                                // Slim rounded thumb inset from the edge; widens on hover.
                                let width = if active {
                                    bounds.size.width.min(px(6.0))
                                } else {
                                    bounds.size.width.min(px(4.0))
                                };
                                let left = bounds.right() - width - px(2.0);
                                window.paint_quad(
                                    fill(
                                        Bounds::new(point(left, top), size(width, height)),
                                        p.line_control.opacity(p.line_control.a * alpha),
                                    )
                                    .corner_radii(width / 2.0),
                                );
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}
