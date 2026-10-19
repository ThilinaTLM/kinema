//! Anchored, nonmodal, focus-returning panels built directly on GPUI.
use super::{
    motion::{self, Motion},
    tokens::{elevation, palette, radius, space},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};

struct PopoverState {
    open: bool,
    closing: bool,
    focus: FocusHandle,
    trigger: FocusHandle,
    task: Option<Task<()>>,
    progress: Rc<Cell<f32>>,
    from: f32,
    generation: usize,
}
impl PopoverState {
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open && !self.closing {
            return;
        }
        self.task = None;
        self.from = if self.open { self.progress.get() } else { 0. };
        self.generation = self.generation.wrapping_add(1);
        self.open = true;
        self.closing = false;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.closing {
            return;
        }
        self.from = self.progress.get();
        self.closing = true;
        self.trigger.focus(window, cx);
        let delay = motion::duration(Motion::PopupOut, cx);
        if delay.is_zero() {
            self.open = false;
            self.closing = false;
        } else {
            self.task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |this, cx| {
                    this.open = false;
                    this.closing = false;
                    cx.notify();
                });
            }));
        }
        cx.notify();
    }
}

/// The trigger toggles a floating panel below its bounds. Content may contain
/// arbitrary interactive children; this is intentionally not a modal focus trap.
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    trigger: Option<AnyElement>,
    content: Option<AnyElement>,
}
impl Popover {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            trigger: None,
            content: None,
        }
    }
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}
impl RenderOnce for Popover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| PopoverState {
            open: false,
            closing: false,
            focus: cx.focus_handle(),
            trigger: cx.focus_handle().tab_stop(true),
            task: None,
            progress: Rc::new(Cell::new(0.)),
            from: 0.,
            generation: 0,
        });
        if state.read(cx).closing && motion::duration(Motion::PopupOut, cx).is_zero() {
            state.update(cx, |this, _| {
                this.task = None;
                this.open = false;
                this.closing = false;
            });
        }
        let p = *palette(cx);
        let click_state = state.clone();
        let key_state = state.clone();
        let mut root = div().id(self.id).relative().flex().flex_col().child(
            div()
                .id("popover-trigger")
                .track_focus(&state.read(cx).trigger)
                .tab_stop(true)
                .rounded(radius::SM)
                .focus_visible(move |s| s.border(space::FOCUS_RING).border_color(p.focus_ring))
                .on_click(move |_, window, cx| {
                    click_state.update(cx, |this, cx| {
                        if this.open && !this.closing {
                            this.dismiss(window, cx);
                        } else {
                            this.show(window, cx);
                        }
                    });
                })
                .on_key_down(move |event, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                        key_state.update(cx, |this, cx| {
                            this.show(window, cx);
                        });
                        cx.stop_propagation();
                    }
                })
                .children(self.trigger),
        );
        if state.read(cx).open {
            let outside_state = state.clone();
            let key_state = state.clone();
            let panel = div()
                .id("popover-panel")
                .track_focus(&state.read(cx).focus)
                .occlude()
                .max_w(window.viewport_size().width - space::S16)
                .bg(p.bg_overlay)
                .rounded(radius::LG)
                .border_1()
                .border_color(p.line_subtle)
                .shadow(elevation::overlay(&p))
                .text_color(p.text_primary)
                .max_h((window.viewport_size().height - space::S16).max(space::ROW))
                .overflow_y_scroll()
                .p(space::S12)
                .on_mouse_down_out(move |_, window, cx| {
                    outside_state.update(cx, |this, cx| this.dismiss(window, cx));
                })
                .on_key_down(move |event, window, cx| {
                    if event.keystroke.key == "escape" {
                        key_state.update(cx, |this, cx| this.dismiss(window, cx));
                        cx.stop_propagation();
                    }
                })
                .children(self.content);
            let closing = state.read(cx).closing;
            let duration = motion::duration(
                if closing {
                    Motion::PopupOut
                } else {
                    Motion::PopupIn
                },
                cx,
            );
            let progress = state.read(cx).progress.clone();
            let from = state.read(cx).from;
            let generation = state.read(cx).generation;
            let panel = if duration.is_zero() {
                progress.set(1.);
                panel.into_any_element()
            } else {
                panel
                    .with_animation(
                        (
                            if closing { "popover-out" } else { "popover-in" },
                            generation,
                        ),
                        Animation::new(duration).with_easing(motion::ease_out_cubic),
                        move |el, t| {
                            let t = if closing {
                                from * (1. - t)
                            } else {
                                from + (1. - from) * t
                            };
                            progress.set(t);
                            el.opacity(t).relative().top(px(4. * (1. - t)))
                        },
                    )
                    .into_any_element()
            };
            root = root.child(
                div().absolute().top_full().left_0().child(
                    deferred(
                        anchored()
                            .snap_to_window_with_margin(space::S8)
                            .child(panel),
                    )
                    .priority(5),
                ),
            );
        }
        root
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
mod interaction_tests {
    use super::Popover;
    use gpui::{
        AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div,
    };
    struct Fixture;
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            Popover::new("test-popover")
                .trigger(div().child("Open"))
                .content(div().child("Panel"))
        }
    }
    #[gpui::test]
    fn escape_returns_to_trigger(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::widgets::tokens::init(cx);
            crate::widgets::tokens::set_reduce_motion(true, cx);
        });
        let window = cx.add_window(|_, _| Fixture);
        let Ok(previous) = cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.focus_next(cx);
            let Some(previous) = window.focused(cx) else {
                panic!("trigger did not receive focus");
            };
            previous
        }) else {
            panic!("could not draw trigger");
        };
        cx.simulate_keystrokes(window.into(), "space");
        let drawn = cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(!previous.is_focused(window));
        });
        assert!(drawn.is_ok(), "could not draw panel: {drawn:?}");
        cx.simulate_keystrokes(window.into(), "escape");
        let returned = cx.update_window(window.into(), |_, window, _| {
            assert!(previous.is_focused(window))
        });
        assert!(returned.is_ok(), "focus return update failed: {returned:?}");
    }
}
