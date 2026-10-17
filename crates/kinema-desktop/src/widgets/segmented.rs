//! Controlled segmented selection, with wraparound arrow-key navigation.
use super::{
    TypeExt,
    tokens::{palette, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;

/// Return a target for recognized navigation keys; empty groups never select.
pub fn navigate(index: usize, len: usize, key: &str) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let index = index.min(len - 1);
    match key {
        "left" | "up" => Some(if index == 0 { len - 1 } else { index - 1 }),
        "right" | "down" => Some((index + 1) % len),
        "home" => Some(0),
        "end" => Some(len - 1),
        _ => None,
    }
}
pub type ChangeHandler<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
#[derive(IntoElement)]
pub struct Segmented<T: Clone + PartialEq + 'static> {
    id: ElementId,
    options: Vec<(T, SharedString)>,
    selected: T,
    on_change: Option<ChangeHandler<T>>,
}
impl<T: Clone + PartialEq + 'static> Segmented<T> {
    pub fn new(
        id: impl Into<ElementId>,
        options: impl IntoIterator<Item = (T, SharedString)>,
        selected: T,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().collect(),
            selected,
            on_change: None,
        }
    }
    pub fn on_change(mut self, handler: impl Fn(&T, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}
impl<T: Clone + PartialEq + 'static> RenderOnce for Segmented<T> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let selected = self
            .options
            .iter()
            .position(|(value, _)| value == &self.selected)
            .unwrap_or(0);
        let options = self.options.clone();
        let keyboard_handler = self.on_change.clone();
        div()
            .id(self.id)
            .flex()
            .border(space::FOCUS_RING)
            .border_color(transparent_black())
            .p(space::S2)
            .m(-space::S4)
            .focusable()
            .tab_stop(true)
            .role(Role::TabList)
            .focus_visible(move |s| s.border_color(p.focus_ring))
            .on_key_down(move |event, window, cx| {
                if let Some(index) = navigate(selected, options.len(), &event.keystroke.key) {
                    cx.stop_propagation();
                    if let Some(handler) = &keyboard_handler {
                        handler(&options[index].0, window, cx);
                    }
                }
            })
            .child(
                div()
                    .flex()
                    .h(space::CONTROL_HEIGHT)
                    .border(space::HAIRLINE)
                    .border_color(p.line_hairline)
                    .children(self.options.into_iter().enumerate().map(
                        |(index, (value, label))| {
                            let active = value == self.selected;
                            let handler = self.on_change.clone();
                            div()
                                .id(("segment", index))
                                .relative()
                                .h_full()
                                .px(space::S12)
                                .flex()
                                .items_center()
                                .justify_center()
                                .type_style(typeface::SMALL_MEDIUM)
                                .role(Role::Tab)
                                .aria_label(label.clone())
                                .aria_selected(active)
                                .text_color(if active {
                                    p.accent_text
                                } else {
                                    p.text_secondary
                                })
                                .when(active, |el| {
                                    el.bg(p.accent_tint).child(
                                        div()
                                            .absolute()
                                            .bottom_0()
                                            .left_0()
                                            .right_0()
                                            .h(space::S2)
                                            .bg(p.accent),
                                    )
                                })
                                .hover(move |s| {
                                    s.bg(if active { p.accent_tint } else { p.ctl_hover })
                                })
                                .child(label)
                                .on_click(move |_, window, cx| {
                                    if let Some(handler) = &handler {
                                        handler(&value, window, cx);
                                    }
                                })
                        },
                    )),
            )
    }
}
#[cfg(test)]
mod tests {
    use super::navigate;
    #[test]
    fn navigation_wraps_and_handles_empty_groups() {
        assert_eq!(navigate(0, 0, "right"), None);
        assert_eq!(navigate(0, 2, "left"), Some(1));
        assert_eq!(navigate(1, 2, "right"), Some(0));
        assert_eq!(navigate(1, 2, "home"), Some(0));
        assert_eq!(navigate(0, 2, "end"), Some(1));
        assert_eq!(navigate(0, 1, "left"), Some(0));
        assert_eq!(navigate(99, 2, "right"), Some(0));
        assert_eq!(navigate(0, 2, "enter"), None);
    }
}
