//! Vertical scrolling with an overlay scrollbar and keyboard paging.
use super::tokens::{palette, space};
use gpui::{prelude::*, *};

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
    children: Vec<AnyElement>,
}
impl ScrollArea {
    pub fn new(id: impl Into<ElementId>, handle: ScrollHandle) -> Self {
        Self {
            id: id.into(),
            handle,
            children: Vec::new(),
        }
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
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(bounds.left(), top),
                                        size(bounds.size.width, height),
                                    ),
                                    p.line_control,
                                ));
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}
