//! Anchored, nonmodal, focus-returning panels built directly on GPUI.
use crate::{
    ui::motion::{self, Motion},
    ui::theme::{elevation, palette, radius, space},
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

/// Lets popover content close its own panel (e.g. after a single choice).
#[derive(Clone)]
pub struct PopoverHandle(Entity<PopoverState>);
impl PopoverHandle {
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn is_open(&self, cx: &App) -> bool {
        self.0.read(cx).open && !self.0.read(cx).closing
    }

    /// Open the existing panel and focus its content.
    pub fn show(&self, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| this.show(window, cx));
    }

    /// Close the panel and return focus to the trigger.
    pub fn dismiss(&self, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |this, cx| this.dismiss(window, cx));
    }
}

/// Which trigger edge the panel lines up with; triggers near the right window
/// edge use [`PopoverAlign::End`] so the panel grows leftwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverAlign {
    #[default]
    Start,
    End,
}

type TriggerBuilder = Box<dyn FnOnce(bool) -> AnyElement>;
type ContentBuilder = Box<dyn FnOnce(PopoverHandle, &mut Window, &mut App) -> AnyElement>;

/// The trigger toggles a floating panel below its bounds. Content may contain
/// arbitrary interactive children; this is intentionally not a modal focus trap.
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    trigger: Option<TriggerBuilder>,
    content: Option<ContentBuilder>,
    align: PopoverAlign,
    label: Option<SharedString>,
    on_handle: Option<Box<dyn FnOnce(PopoverHandle)>>,
}
impl Popover {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            trigger: None,
            content: None,
            align: PopoverAlign::Start,
            label: None,
            on_handle: None,
        }
    }
    pub fn trigger(mut self, trigger: impl IntoElement + 'static) -> Self {
        self.trigger = Some(Box::new(move |_| trigger.into_any_element()));
        self
    }
    /// Trigger that reflects whether the panel is open (pressed styling).
    pub fn trigger_with<E: IntoElement>(mut self, build: impl FnOnce(bool) -> E + 'static) -> Self {
        self.trigger = Some(Box::new(move |open| build(open).into_any_element()));
        self
    }
    pub fn content(mut self, content: impl IntoElement + 'static) -> Self {
        self.content = Some(Box::new(move |_, _, _| content.into_any_element()));
        self
    }
    /// Content built only while open, with a handle to close the panel.
    pub fn content_with<E: IntoElement>(
        mut self,
        build: impl FnOnce(PopoverHandle, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.content = Some(Box::new(move |handle, window, cx| {
            build(handle, window, cx).into_any_element()
        }));
        self
    }
    /// Receive the current rendered panel's handle for programmatic show/dismiss.
    /// This uses the same keyed state and focus behavior as clicking its trigger.
    pub fn on_handle(mut self, callback: impl FnOnce(PopoverHandle) + 'static) -> Self {
        self.on_handle = Some(Box::new(callback));
        self
    }

    pub fn align(mut self, align: PopoverAlign) -> Self {
        self.align = align;
        self
    }
    /// Accessible name of the trigger when its content is icon-only.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
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
        if let Some(callback) = self.on_handle {
            callback(PopoverHandle(state.clone()));
        }
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
        let open = state.read(cx).open && !state.read(cx).closing;
        let trigger = self.trigger.map(|build| build(open));
        let mut root = div().id(self.id).relative().flex().flex_col().child(
            div()
                .id("popover-trigger")
                .track_focus(&state.read(cx).trigger)
                .tab_stop(true)
                .role(Role::Button)
                .aria_expanded(open)
                .when_some(self.label, |el, label| el.aria_label(label))
                .rounded(radius::LG)
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
                .children(trigger),
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
                .rounded(radius::XL)
                .border_1()
                .border_color(p.line_subtle)
                .shadow(elevation::lg(&p))
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
                .children(
                    self.content
                        .map(|build| build(PopoverHandle(state.clone()), window, cx)),
                );
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
            let (edge, corner) = match self.align {
                PopoverAlign::Start => (div().absolute().top_full().left_0(), Anchor::TopLeft),
                PopoverAlign::End => (div().absolute().top_full().right_0(), Anchor::TopRight),
            };
            root = root.child(
                edge.child(
                    deferred(
                        anchored()
                            .anchor(corner)
                            .offset(point(px(0.), space::S6))
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
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(true, cx);
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
