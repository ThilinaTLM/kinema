//! Search editor with cancellable, generation-checked live queries.
use super::{
    icon::{Icon, IconName},
    text_input::{TextInput, TextInputEvent},
    tokens::{palette, space},
};
use gpui::{prelude::*, *};
use std::time::Duration;

/// Live queries are delayed; submissions are immediate. Read the query via `text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFieldEvent {
    Changed,
    Submitted,
}
impl EventEmitter<SearchFieldEvent> for SearchField {}

pub struct SearchField {
    input: Entity<TextInput>,
    clear_focus: FocusHandle,
    generation: u64,
    debounce: Option<Task<()>>,
    _subscription: Subscription,
}
impl SearchField {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(window, cx));
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                TextInputEvent::Changed => {
                    this.generation = this.generation.wrapping_add(1);
                    let generation = this.generation;
                    this.debounce = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(300))
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.generation == generation {
                                cx.emit(SearchFieldEvent::Changed);
                            }
                        });
                    }));
                    cx.notify();
                }
                TextInputEvent::Submitted => {
                    this.cancel_debounce();
                    cx.emit(SearchFieldEvent::Submitted);
                }
                TextInputEvent::Cancelled => {
                    this.input.update(cx, |input, cx| input.set_text("", cx));
                    window.blur(cx);
                }
                _ => {}
            });
        Self {
            input,
            clear_focus: cx.focus_handle().tab_stop(true),
            generation: 0,
            debounce: None,
            _subscription: subscription,
        }
    }
    fn cancel_debounce(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.debounce = None;
    }
    pub fn text<'a>(&'a self, cx: &'a App) -> &'a str {
        self.input.read(cx).text()
    }
    pub fn input(&self) -> Entity<TextInput> {
        self.input.clone()
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text(text, cx));
    }
    pub fn set_placeholder(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_placeholder(text, cx));
    }
}
impl Focusable for SearchField {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}
impl Render for SearchField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let nonempty = !self.text(cx).is_empty();
        div()
            .id("kinema-search-field")
            .flex()
            .items_center()
            .gap(space::S4)
            .h(space::CONTROL_HEIGHT)
            .w_full()
            .child(Icon::new(IconName::Search).color(p.text_secondary))
            .child(div().flex_1().min_w(px(0.)).child(self.input.clone()))
            .when(nonempty, |s| {
                s.child(
                    div()
                        .id("clear-search")
                        .track_focus(&self.clear_focus)
                        .focus_visible(|s| s.border_2().border_color(p.focus_ring))
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                                this.input.update(cx, |input, cx| input.set_text("", cx));
                                this.focus(window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .size(space::ICON_TARGET)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(p.ctl_hover).text_color(p.text_primary))
                        .child(Icon::new(IconName::Close).color(p.text_secondary))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.input.update(cx, |input, cx| input.set_text("", cx));
                            this.focus(window, cx);
                        })),
                )
            })
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used)]
mod interaction_tests {
    use super::{SearchField, SearchFieldEvent};
    use gpui::TestAppContext;
    use std::{cell::Cell, rc::Rc, time::Duration};

    #[gpui::test]
    fn debounce_supersedes(cx: &mut TestAppContext) {
        cx.update(super::super::tokens::init);
        let handle = cx.add_window(SearchField::new);
        let changed = Rc::new(Cell::new(0));
        let subscription = cx.update(|cx| {
            let count = changed.clone();
            let root = handle.root(cx).unwrap();
            cx.subscribe(&root, move |_, e: &SearchFieldEvent, _| {
                if *e == SearchFieldEvent::Changed {
                    count.set(count.get() + 1);
                }
            })
        });
        for text in ["one", "two"] {
            handle
                .update(cx, |field, _, cx| {
                    field.set_text(text, cx);
                })
                .unwrap();
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_millis(200));
        }
        cx.run_until_parked();
        assert_eq!(changed.get(), 0);
        cx.background_executor
            .advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert_eq!(changed.get(), 1);
        drop(subscription);
    }
}
