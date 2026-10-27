//! Modal sheet. Render above the screen; remove it in `on_dismiss`.
use super::{
    TypeExt,
    motion::{self, Motion},
    tokens::{elevation, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;
type DismissHandler = Rc<dyn Fn(&mut Window, &mut App)>;
struct SheetState {
    root: FocusHandle,
    end: FocusHandle,
    previous: Option<FocusHandle>,
}
#[derive(IntoElement)]
pub struct Sheet {
    id: ElementId,
    title: SharedString,
    body: Vec<AnyElement>,
    footer: Vec<AnyElement>,
    initial: Option<FocusHandle>,
    dismiss: Option<DismissHandler>,
}
impl Sheet {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: Vec::new(),
            footer: Vec::new(),
            initial: None,
            dismiss: None,
        }
    }
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body.push(body.into_any_element());
        self
    }
    pub fn footer(mut self, action: impl IntoElement) -> Self {
        self.footer.push(action.into_any_element());
        self
    }
    pub fn initial_focus(mut self, handle: FocusHandle) -> Self {
        self.initial = Some(handle);
        self
    }
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.dismiss = Some(Rc::new(handler));
        self
    }
}
impl RenderOnce for Sheet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let initial = self.initial;
        let state = window.use_keyed_state(self.id.clone(), cx, move |window, cx| {
            let root = cx.focus_handle();
            let end = cx.focus_handle();
            let previous = window.focused(cx);
            // Also restore focus when a footer action/programmatic update removes us.
            let window_handle = window.window_handle();
            cx.on_release(move |state: &mut SheetState, cx| {
                let root = state.root.clone();
                let previous = state.previous.clone();
                cx.defer(move |cx| {
                    let _ = window_handle.update(cx, |_, window, cx| {
                        if (window.focused(cx).is_none() || root.contains_focused(window, cx))
                            && let Some(previous) = previous
                        {
                            previous.focus(window, cx);
                        }
                    });
                });
            })
            .detach();
            let target = initial.unwrap_or_else(|| root.clone());
            window.defer(cx, move |window, cx| target.focus(window, cx));
            SheetState {
                root,
                end,
                previous,
            }
        });
        let root = state.read(cx).root.clone();
        let end = state.read(cx).end.clone();
        let handler = self.dismiss;
        let panel = div()
            .relative()
            .w(space::SHEET_WIDTH)
            .max_w_full()
            .bg(p.bg_overlay)
            .rounded(radius::LG)
            .border(space::HAIRLINE)
            .border_color(p.line_subtle)
            .shadow(elevation::sheet(&p))
            .p(space::S24)
            .flex()
            .flex_col()
            .gap(space::S16)
            .child(
                div()
                    .type_style(typeface::TITLE)
                    .text_color(p.text_primary)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .type_style(typeface::BODY)
                    .text_color(p.text_primary)
                    .flex()
                    .flex_col()
                    .gap(space::S12)
                    .children(self.body),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(space::S8)
                    .children(self.footer),
            )
            .child(
                div()
                    .id("sheet-end")
                    .track_focus(&end)
                    .tab_stop(false)
                    .size_0(),
            );
        let duration = motion::duration(Motion::Sheet, cx);
        let scrim = div().absolute().inset_0().bg(p.dialog_scrim);
        let (panel, scrim) = if duration.is_zero() {
            (panel.into_any_element(), scrim.into_any_element())
        } else {
            (
                panel
                    .with_animation(
                        "sheet-open",
                        Animation::new(duration).with_easing(motion::ease_out_cubic),
                        |el, t| el.opacity(t).top(space::S24 * (1.0 - t)),
                    )
                    .into_any_element(),
                scrim
                    .with_animation(
                        "sheet-scrim",
                        Animation::new(duration).with_easing(motion::ease_out_cubic),
                        |el, t| el.opacity(t),
                    )
                    .into_any_element(),
            )
        };
        div()
            .id(self.id)
            .absolute()
            .inset_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&root)
            .tab_group()
            .tab_stop(false)
            .role(Role::Dialog)
            .aria_label(self.title)
            .occlude()
            .capture_key_down(
                move |event, window, cx| match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        if let Some(handler) = &handler {
                            if let Some(previous) = state.read(cx).previous.clone() {
                                previous.focus(window, cx);
                            }
                            handler(window, cx);
                        }
                    }
                    "tab" => {
                        cx.stop_propagation();
                        let root = state.read(cx).root.clone();
                        let end = state.read(cx).end.clone();
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        if !root.contains_focused(window, cx) {
                            if event.keystroke.modifiers.shift {
                                end.focus(window, cx);
                                window.focus_prev(cx);
                            } else {
                                root.focus(window, cx);
                                window.focus_next(cx);
                            }
                            if !root.contains_focused(window, cx) {
                                root.focus(window, cx);
                            }
                        }
                    }
                    _ => {}
                },
            )
            .child(scrim)
            .child(panel)
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    reason = "test setup and event dispatch must succeed"
)]
mod interaction_tests {
    use super::Sheet;
    use gpui::{
        AppContext, Context, InputEvent, IntoElement, KeyDownEvent, Keystroke, Render,
        TestAppContext, Window,
    };
    use std::{cell::Cell, rc::Rc};
    struct Fixture(Rc<Cell<bool>>);
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let dismissed = self.0.clone();
            Sheet::new("test-sheet", "Confirm").on_dismiss(move |_, _| dismissed.set(true))
        }
    }
    #[test]
    fn escape_dismisses_sheet() {
        let mut cx = TestAppContext::single();
        cx.update(crate::widgets::tokens::init);
        let dismissed = Rc::new(Cell::new(false));
        let window = cx.add_window({
            let dismissed = dismissed.clone();
            move |_, _| Fixture(dismissed)
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        assert!(dismissed.get());
    }
}
