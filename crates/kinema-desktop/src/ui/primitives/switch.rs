//! Controlled rounded boolean controls. Switch is for immediate settings only.
//!
//! Accessibility: `Switch` is `Role::Switch`, `Checkbox` is `CheckBox` and
//! `Radio` is `RadioButton`; the label is the name and the checked state is
//! reported as toggled. The whole row is one tab stop; Enter/Space (or a click
//! on the label) activates it. A switch flips, a radio only selects and never
//! clears. Disabled controls leave the tab order and ignore input (GPUI has no
//! disabled attribute). The focus ring follows the row radius; state shows in
//! the knob position / check / dot as well as colour. The host owns grouping,
//! so arrow-key navigation between radios is not provided here.
use crate::{
    ui::motion::{interaction::state_spring, transition::FEEDBACK},
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::TypeExt,
    ui::theme::{palette, radius, size, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;
type ToggleHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    label: SharedString,
    checked: bool,
    disabled: bool,
    handler: Option<ToggleHandler>,
    role: Role,
    // ── sessions: file-row typography ──
    typography: crate::ui::theme::TypeStyle,
}
impl Checkbox {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, checked: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            checked,
            disabled: false,
            handler: None,
            role: Role::CheckBox,
            typography: typeface::BODY,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.handler = Some(Rc::new(handler));
        self
    }
}
impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let indicator =
            match self.role {
                Role::Switch => switch_indicator(self.checked, self.disabled, cx),
                Role::RadioButton => {
                    div()
                        .size(size::ICON_SM)
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(radius::FULL)
                        .border(space::HAIRLINE)
                        .border_color(if self.checked && !self.disabled {
                            p.primary
                        } else {
                            p.line_control
                        })
                        .when(self.checked, |el| {
                            el.child(div().size(space::S6).rounded(radius::FULL).bg(
                                if self.disabled {
                                    p.text_muted
                                } else {
                                    p.primary
                                },
                            ))
                        })
                        .into_any_element()
                }
                _ => div()
                    .size(space::ICON)
                    .flex_none()
                    .rounded(radius::SM)
                    .border(space::HAIRLINE)
                    .border_color(if self.checked && !self.disabled {
                        p.primary
                    } else {
                        p.line_control
                    })
                    .bg(if self.checked {
                        p.accent_tint
                    } else {
                        p.ctl_rest
                    })
                    .when(self.checked, |el| {
                        el.child(Icon::new(IconName::Check).size(space::S12).color(
                            if self.disabled {
                                p.text_muted
                            } else {
                                p.accent_text
                            },
                        ))
                    })
                    .into_any_element(),
            };
        let ink = if self.disabled {
            p.text_muted
        } else {
            p.text_primary
        };
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(space::S8)
            .h(space::CONTROL_HEIGHT + space::S8)
            .border(space::FOCUS_RING)
            .border_color(transparent_black())
            .p(space::S2)
            .m(-space::S4)
            .rounded(radius::MD + space::S4)
            .role(self.role)
            .aria_label(self.label.clone())
            .aria_toggled(toggled_state(self.checked))
            .when(!self.disabled, |el| {
                el.focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .focus_visible(move |s| s.border_color(p.focus_ring))
            })
            .child(indicator)
            .child(
                div()
                    .type_style(self.typography)
                    .text_color(ink)
                    .child(self.label),
            )
            .when_some(self.handler.filter(|_| !self.disabled), |el, handler| {
                el.on_click(move |_, window, cx| {
                    handler(self.role == Role::RadioButton || !self.checked, window, cx)
                })
            })
    }
}
fn toggled_state(checked: bool) -> Toggled {
    if checked {
        Toggled::True
    } else {
        Toggled::False
    }
}
fn switch_indicator(checked: bool, disabled: bool, cx: &App) -> AnyElement {
    let p = *palette(cx);
    let knob = div()
        .relative()
        .size(space::S12)
        .rounded(radius::FULL)
        .bg(if disabled {
            p.text_muted
        } else if checked {
            p.on_accent
        } else {
            p.text_secondary
        });
    // Retarget from the current position when the setting changes mid-slide.
    let knob = state_spring(knob, "switch-knob", checked, FEEDBACK, cx, |el, t| {
        el.left(space::HAIRLINE + space::S12 * t)
    });
    div()
        .relative()
        .w(space::CONTROL_HEIGHT)
        .h(space::ICON)
        .flex()
        .items_center()
        .flex_none()
        .rounded(radius::FULL)
        .border(space::HAIRLINE)
        .border_color(if checked && !disabled {
            p.primary
        } else {
            p.line_control
        })
        .bg(if checked && !disabled {
            p.primary
        } else {
            p.ctl_rest
        })
        .child(knob)
        .into_any_element()
}

/// Immediate boolean setting with a round sliding knob.
#[derive(IntoElement)]
pub struct Switch(Checkbox);
impl Switch {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, checked: bool) -> Self {
        let mut control = Checkbox::new(id, label, checked);
        control.role = Role::Switch;
        Self(control)
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.0 = self.0.disabled(disabled);
        self
    }
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.0 = self.0.on_change(handler);
        self
    }
}
impl RenderOnce for Switch {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

/// Controlled single-choice option. Selecting an already-selected option never clears it.
/// The host owns the group selection and deselects sibling options.
#[derive(IntoElement)]
pub struct Radio(Checkbox);
impl Radio {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, selected: bool) -> Self {
        let mut control = Checkbox::new(id, label, selected);
        control.role = Role::RadioButton;
        Self(control)
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.0 = self.0.disabled(disabled);
        self
    }
    pub fn on_select(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.0 = self.0.on_change(move |_, window, cx| handler(window, cx));
        self
    }
}
impl RenderOnce for Radio {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    reason = "test setup and event dispatch must succeed"
)]
mod interaction_tests {
    use super::{Radio, Switch, toggled_state};
    use gpui::{AppContext, Context, InputEvent, IntoElement, Render, TestAppContext, Window};
    use std::{cell::Cell, rc::Rc};

    struct Fixture {
        checked: bool,
        radio: bool,
        disabled: bool,
        changed: Rc<Cell<Option<bool>>>,
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let changed = self.changed.clone();
            if self.radio {
                Radio::new("option", "Example", self.checked)
                    .disabled(self.disabled)
                    .on_select(move |_, _| changed.set(Some(true)))
                    .into_any_element()
            } else {
                Switch::new("option", "Example", self.checked)
                    .disabled(self.disabled)
                    .on_change(move |value, _, _| changed.set(Some(value)))
                    .into_any_element()
            }
        }
    }

    #[test]
    fn checked_state_maps_to_toggled() {
        assert_eq!(toggled_state(true), gpui::Toggled::True);
        assert_eq!(toggled_state(false), gpui::Toggled::False);
    }

    #[test]
    fn keyboard_toggles_a_focused_switch_unless_disabled() {
        for (disabled, expected) in [(false, Some(true)), (true, None)] {
            let mut cx = TestAppContext::single();
            cx.update(crate::ui::theme::init);
            let changed = Rc::new(Cell::new(None));
            let window = cx.add_window({
                let changed = changed.clone();
                move |_, _| Fixture {
                    checked: false,
                    radio: false,
                    disabled,
                    changed,
                }
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                window.focus_next(cx);
            })
            .unwrap();
            for key in ["space", "enter"] {
                changed.set(None);
                let keystroke = gpui::Keystroke::parse(key).unwrap();
                cx.update_window(window.into(), |_, window, cx| {
                    window.dispatch_event(
                        gpui::KeyDownEvent {
                            keystroke: keystroke.clone(),
                            is_held: false,
                            prefer_character_input: false,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    window.dispatch_event(gpui::KeyUpEvent { keystroke }.to_platform_input(), cx);
                })
                .unwrap();
                assert_eq!(changed.get(), expected, "{key} disabled={disabled}");
            }
        }
    }

    #[test]
    fn labels_toggle_switches_and_select_radios_without_clearing() {
        for (radio, checked, disabled, expected) in [
            (false, false, false, Some(true)),
            (false, true, false, Some(false)),
            (true, false, false, Some(true)),
            (true, true, false, Some(true)),
            (false, false, true, None),
            (true, false, true, None),
        ] {
            let mut cx = TestAppContext::single();
            cx.update(crate::ui::theme::init);
            let changed = Rc::new(Cell::new(None));
            let window = cx.add_window({
                let changed = changed.clone();
                move |_, _| Fixture {
                    checked,
                    radio,
                    disabled,
                    changed,
                }
            });
            cx.update_window(window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                let position = gpui::point(gpui::px(40.0), gpui::px(12.0));
                window.dispatch_event(
                    gpui::MouseDownEvent {
                        position,
                        button: gpui::MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    gpui::MouseUpEvent {
                        position,
                        button: gpui::MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
            })
            .unwrap();
            assert_eq!(
                changed.get(),
                expected,
                "radio={radio}, checked={checked}, disabled={disabled}"
            );
        }
    }
}

// ── sessions: optional mono typography for explicit file choices ──
impl Radio {
    pub fn typography(mut self, style: crate::ui::theme::TypeStyle) -> Self {
        self.0.typography = style;
        self
    }
}
