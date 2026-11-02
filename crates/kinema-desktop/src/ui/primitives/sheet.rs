//! Modal sheet. Render above the screen; remove it in `on_dismiss`.
//!
//! Accessibility: `Role::Dialog` named by the title. Focus moves into the
//! sheet on open (the `initial_focus` handle, else the dialog), Tab/Shift-Tab
//! wrap inside it (a focus trap), and Esc restores focus to the element that
//! held it before opening, then calls `on_dismiss`. The scrim occludes the
//! content behind; GPUI has no `aria-modal` flag, so assistive technology is
//! not told the background is inert. Focus rings come from the controls inside.
use crate::{
    ui::motion::{AnimateExt, Presence, transition::SHEET},
    ui::theme::typography::TypeExt,
    ui::theme::{elevation, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;
type DismissHandler = Rc<dyn Fn(&mut Window, &mut App)>;
struct SheetState {
    presence: Entity<Presence>,
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
                presence: cx.new(|_| Presence::opened(SHEET, SHEET)),
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
            .rounded(radius::XL)
            .border(space::HAIRLINE)
            .border_color(p.line_subtle)
            .shadow(elevation::xl(&p))
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
        let presence = state.read(cx).presence.clone();
        let scrim = div().absolute().inset_0().bg(p.dialog_scrim);
        // The owner removes the sheet to dismiss it, so only the enter plays.
        let (panel, scrim) = (
            presence.read(cx).animate(panel, "sheet-open", cx),
            scrim.animate_enter("sheet-scrim", crate::ui::motion::transition::CROSSFADE, cx),
        );
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
    use crate::ui::primitives::button::Button;
    use gpui::{
        AppContext, Context, FocusHandle, InputEvent, IntoElement, KeyDownEvent, Keystroke,
        ParentElement, Render, TestAppContext, Window, div,
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
        cx.update(crate::ui::theme::init);
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

    struct TrapFixture {
        outside: FocusHandle,
        inside: [FocusHandle; 2],
    }
    impl Render for TrapFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(Button::new("outside", "Outside").focus_handle(self.outside.clone()))
                .child(
                    Sheet::new("trap-sheet", "Trap")
                        .on_dismiss(|_, _| {})
                        .footer(Button::new("one", "One").focus_handle(self.inside[0].clone()))
                        .footer(Button::new("two", "Two").focus_handle(self.inside[1].clone())),
                )
        }
    }

    #[test]
    fn tab_is_trapped_inside_the_sheet() {
        let mut cx = TestAppContext::single();
        cx.update(crate::ui::theme::init);
        let (outside, inside) = cx.update(|cx| {
            (
                cx.focus_handle().tab_stop(true),
                [cx.focus_handle(), cx.focus_handle()],
            )
        });
        let window = cx.add_window({
            let (outside, inside) = (outside.clone(), inside.clone());
            move |_, _| TrapFixture { outside, inside }
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        cx.run_until_parked();
        let visited = std::cell::Cell::new([false; 2]);
        for shift in [false, true] {
            for _ in 0..6 {
                cx.update_window(window.into(), |_, window, cx| {
                    window.draw(cx).clear(cx);
                    let mut keystroke = Keystroke::parse("tab").unwrap();
                    keystroke.modifiers.shift = shift;
                    window.dispatch_event(
                        KeyDownEvent {
                            keystroke,
                            is_held: false,
                            prefer_character_input: false,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    window.draw(cx).clear(cx);
                    assert!(!outside.is_focused(window), "focus escaped the sheet");
                    let mut seen = visited.get();
                    for (i, handle) in inside.iter().enumerate() {
                        seen[i] |= handle.is_focused(window);
                    }
                    visited.set(seen);
                })
                .unwrap();
            }
        }
        assert_eq!(
            visited.get(),
            [true, true],
            "both sheet controls are reachable"
        );
    }
}
