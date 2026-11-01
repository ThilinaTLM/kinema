//! Controlled rounded boolean controls. Switch is for immediate settings only.
use crate::{
    ui::motion,
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::TypeExt,
    ui::theme::{palette, radius, space, typeface},
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
                Role::RadioButton => div()
                    .size(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border(space::HAIRLINE)
                    .border_color(if self.checked && !self.disabled {
                        p.primary
                    } else {
                        p.line_control
                    })
                    .when(self.checked, |el| {
                        el.child(div().size(space::S6).rounded_full().bg(if self.disabled {
                            p.text_muted
                        } else {
                            p.primary
                        }))
                    })
                    .into_any_element(),
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
            .aria_toggled(if self.checked {
                Toggled::True
            } else {
                Toggled::False
            })
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
fn switch_indicator(checked: bool, disabled: bool, cx: &App) -> AnyElement {
    let p = *palette(cx);
    let duration = motion::duration(motion::Motion::Feedback, cx);
    let knob = div()
        .relative()
        .size(space::S12)
        .rounded_full()
        .bg(if disabled {
            p.text_muted
        } else if checked {
            p.on_accent
        } else {
            p.text_secondary
        });
    let target = if checked { 1.0_f32 } else { 0.0 };
    let knob = if duration.is_zero() {
        knob.left(space::HAIRLINE + space::S12 * target)
            .into_any_element()
    } else {
        // Retarget from the current position when the setting changes mid-slide.
        knob.with_spring(
            "switch-knob",
            SpringAnimation::new(SpringConfig::new(
                (6.0 / duration.as_secs_f32()).powi(2),
                12.0 / duration.as_secs_f32(),
                1.0,
            ))
            .to(target),
            |el, t| el.left(space::HAIRLINE + space::S12 * t.clamp(0.0, 1.0)),
        )
        .into_any_element()
    };
    div()
        .relative()
        .w(space::CONTROL_HEIGHT)
        .h(space::ICON)
        .flex()
        .items_center()
        .flex_none()
        .rounded_full()
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
    use super::{Radio, Switch};
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
