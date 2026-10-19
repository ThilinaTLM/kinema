//! Rounded, keyboard-activatable buttons. GPUI generates Enter/Space clicks.
use std::rc::Rc;

use gpui::{prelude::*, *};

use super::{
    Icon, IconName, TypeExt, motion,
    tokens::{Palette, palette, radius, space, typeface},
};

pub type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    #[default]
    Compact,
    Large,
}

/// The ID must remain stable across renders. Busy preserves the label and width.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<IconName>,
    disabled: bool,
    busy: bool,
    handler: Option<ClickHandler>,
    focus: Option<FocusHandle>,
    pub(crate) square: bool,
    pub(crate) badge: Option<usize>,
    pub(crate) toggled: bool,
    pub(crate) over_art: bool,
    pub(crate) tooltip: Option<SharedString>,
}
impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Compact,
            icon: None,
            disabled: false,
            busy: false,
            handler: None,
            focus: None,
            square: false,
            badge: None,
            toggled: false,
            over_art: false,
            tooltip: None,
        }
    }
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Dark-surface ink and fills regardless of theme, for dark artwork banners.
    pub fn over_art(mut self, over_art: bool) -> Self {
        self.over_art = over_art;
        self
    }
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }
    pub fn focus_handle(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Some(Rc::new(handler));
        self
    }
}

/// Palette used by a button: the dark palette when over artwork, else the theme's.
fn surface_palette(over_art: bool, themed: Palette) -> Palette {
    if over_art {
        let mut dark = Palette::dark();
        dark.ctl_rest = dark.chip_over_art;
        dark
    } else {
        themed
    }
}

/// Disabled styling takes precedence over the variant and toggled state.
fn disabled_colors(variant: ButtonVariant, p: &Palette) -> (Hsla, Hsla, Hsla) {
    let fill = match variant {
        ButtonVariant::Primary | ButtonVariant::Secondary => p.ctl_rest,
        ButtonVariant::Ghost | ButtonVariant::Danger => transparent_black(),
    };
    let border = match variant {
        ButtonVariant::Ghost => transparent_black(),
        _ => p.line_subtle,
    };
    (fill, p.text_muted, border)
}

fn busy_bar(ink: Hsla, cx: &App) -> AnyElement {
    let duration = motion::duration(motion::Motion::Disclose, cx);
    let line = div().absolute().h_full();
    let line = if duration.is_zero() {
        line.w_full().bg(palette(cx).text_muted).into_any_element()
    } else {
        line.w(relative(0.25))
            .bg(ink)
            .with_animation(
                "button-busy-sweep",
                Animation::new(duration * 6).repeat(),
                |el, t| el.left(relative(t * 1.25 - 0.25)),
            )
            .into_any_element()
    };
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(space::S2)
        .overflow_hidden()
        .child(line)
        .into_any_element()
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = surface_palette(self.over_art, *palette(cx));
        let blocked = self.disabled || self.busy;
        let (rest, hover, pressed, ink, border) = match self.variant {
            ButtonVariant::Primary => (
                p.accent,
                p.accent_hover,
                p.accent_pressed,
                p.on_accent,
                transparent_black(),
            ),
            ButtonVariant::Secondary => (
                p.ctl_rest,
                p.ctl_hover,
                p.ctl_pressed,
                p.text_primary,
                p.line_subtle,
            ),
            ButtonVariant::Ghost => (
                if self.toggled {
                    p.accent_tint
                } else {
                    transparent_black()
                },
                p.ctl_hover,
                p.ctl_pressed,
                if self.toggled {
                    p.accent_text
                } else {
                    p.text_secondary
                },
                transparent_black(),
            ),
            ButtonVariant::Danger => (
                transparent_black(),
                p.ctl_hover,
                p.ctl_pressed,
                p.status_error,
                p.status_error.opacity(0.4),
            ),
        };
        let (rest, ink, border) = if self.disabled {
            disabled_colors(self.variant, &p)
        } else {
            (rest, ink, border)
        };
        // A keyed spring retains interpolated progress when hover reverses.
        let hovered = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let target = !blocked && *hovered.read(cx);
        let duration = motion::duration(motion::Motion::Feedback, cx);
        let hover_state = hovered.clone();
        let mut button = div()
            .id("button-face")
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .gap(space::S6)
            .h(match self.size {
                ButtonSize::Compact => space::CONTROL_HEIGHT,
                ButtonSize::Large => space::CONTROL_HEIGHT_LARGE,
            })
            .px(space::S12)
            .rounded(radius::SM)
            .border(space::HAIRLINE)
            .border_color(border)
            .bg(rest)
            .text_color(ink)
            .type_style(typeface::BODY_STRONG)
            .when(self.square && self.badge.is_none(), |el| {
                el.w(space::ICON_TARGET).px_0()
            })
            .when(self.square && self.badge.is_some(), |el| {
                el.min_w(space::ICON_TARGET).px(space::S6).gap(space::S4)
            })
            .when(!blocked, |el| {
                el.group_active("button", move |s| s.bg(pressed))
            })
            .when_some(self.icon, |el, icon| el.child(Icon::new(icon).color(ink)))
            .when(!self.square, |el| el.child(self.label.clone()))
            .when_some(self.badge, |el, count| {
                el.child(
                    div()
                        .type_style(typeface::CAPTION)
                        .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                        .text_color(if self.disabled {
                            p.text_muted
                        } else {
                            p.accent_text
                        })
                        .child(count.to_string()),
                )
            })
            // Absolute placement preserves label metrics and keeps the sweep inside.
            .when(self.busy, |el| el.child(busy_bar(ink, cx)));
        let face = if duration.is_zero() || blocked {
            if target {
                button = button.bg(hover);
            }
            button.into_any_element()
        } else {
            button
                .with_spring(
                    "button-feedback",
                    SpringAnimation::new(SpringConfig::new(
                        (6.0 / duration.as_secs_f32()).powi(2),
                        12.0 / duration.as_secs_f32(),
                        1.0,
                    ))
                    .to(if target { 1.0_f32 } else { 0.0 }),
                    move |el, t| el.bg(rest.blend(hover.opacity(t.clamp(0.0, 1.0)))),
                )
                .into_any_element()
        };
        // Transparent, out-of-flow ring geometry avoids layout shifts on focus.
        // 2 px ring, 2 px gap, offset outwards by 4 px on each side.
        div()
            .id(self.id)
            .group("button")
            .relative()
            .flex_none()
            .border(space::FOCUS_RING)
            .border_color(transparent_black())
            .p(space::S2)
            .m(-space::S4)
            // Ring follows the host radius: face radius + gap + ring width.
            .rounded(radius::SM + space::S4)
            .role(Role::Button)
            .aria_label(self.tooltip.clone().unwrap_or(self.label.clone()))
            .when(!blocked, |el| {
                el.focusable()
                    .cursor_pointer()
                    .focus_visible(move |s| s.border_color(p.focus_ring))
                    .on_hover(move |value, _, cx| {
                        hover_state.update(cx, |state, cx| {
                            *state = *value;
                            cx.notify();
                        })
                    })
            })
            .when_some(self.focus, |el, focus| {
                el.track_focus(&focus.tab_stop(!blocked))
            })
            .tab_stop(!blocked)
            .when_some(self.tooltip, |el, label| {
                el.tooltip(super::tooltip::tooltip(label))
            })
            .when_some(self.handler.filter(|_| !blocked), |el, handler| {
                el.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(face)
    }
}

#[cfg(test)]
mod tests {
    use super::{ButtonVariant, disabled_colors, surface_palette};
    use crate::widgets::tokens::Palette;

    #[test]
    fn over_art_uses_dark_surface_in_any_theme() {
        let dark = Palette::dark();
        let p = surface_palette(true, Palette::light());
        assert_eq!(p.text_primary, dark.text_primary);
        assert_eq!(p.ctl_rest, dark.chip_over_art);
        assert_eq!(
            surface_palette(false, Palette::light()).text_primary,
            Palette::light().text_primary
        );
    }

    #[test]
    fn disabled_palette_overrides_every_variant_in_both_themes() {
        for p in [Palette::dark(), Palette::light()] {
            for variant in [
                ButtonVariant::Primary,
                ButtonVariant::Secondary,
                ButtonVariant::Ghost,
                ButtonVariant::Danger,
            ] {
                let (fill, ink, border) = disabled_colors(variant, &p);
                assert_eq!(ink, p.text_muted, "{variant:?}");
                assert_eq!(
                    fill,
                    if matches!(variant, ButtonVariant::Primary | ButtonVariant::Secondary) {
                        p.ctl_rest
                    } else {
                        gpui::transparent_black()
                    },
                    "{variant:?}"
                );
                assert_eq!(
                    border,
                    if variant == ButtonVariant::Ghost {
                        gpui::transparent_black()
                    } else {
                        p.line_subtle
                    },
                    "{variant:?}"
                );
            }
        }
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    reason = "test setup and event dispatch must succeed"
)]
mod interaction_tests {
    use super::{Button, ButtonVariant};
    use gpui::{
        App, AppContext, Context, FocusHandle, InputEvent, IntoElement, KeyDownEvent, KeyUpEvent,
        Keystroke, Render, TestAppContext, Window,
    };
    use std::{cell::Cell, rc::Rc};

    struct Fixture {
        focus: FocusHandle,
        clicks: Rc<Cell<usize>>,
        disabled: bool,
        busy: bool,
        variant: ButtonVariant,
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicks = self.clicks.clone();
            Button::new("test-button", "Activate")
                .variant(self.variant)
                .focus_handle(self.focus.clone())
                .disabled(self.disabled)
                .busy(self.busy)
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
        }
    }

    #[test]
    fn enter_and_space_activate_but_disabled_and_busy_block() {
        for (variant, disabled, busy, expected) in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Ghost,
            ButtonVariant::Danger,
        ]
        .into_iter()
        .flat_map(|variant| {
            [
                (false, false, 3),
                (true, false, 0),
                (false, true, 0),
                (true, true, 0),
            ]
            .map(|(disabled, busy, expected)| (variant, disabled, busy, expected))
        }) {
            let mut cx = TestAppContext::single();
            cx.update(crate::widgets::tokens::init);
            let focus = cx.update(|cx: &mut App| cx.focus_handle());
            let clicks = Rc::new(Cell::new(0));
            let window = cx.add_window({
                let focus = focus.clone();
                let clicks = clicks.clone();
                move |_, _| Fixture {
                    focus,
                    clicks,
                    disabled,
                    busy,
                    variant,
                }
            });
            cx.update_window(window.into(), |_, window, cx| {
                focus.focus(window, cx);
                window.draw(cx).clear(cx);
            })
            .unwrap();
            for key in ["enter", "space"] {
                let keystroke = Keystroke::parse(key).unwrap();
                cx.update_window(window.into(), |_, window, cx| {
                    window.dispatch_event(
                        KeyDownEvent {
                            keystroke: keystroke.clone(),
                            is_held: false,
                            prefer_character_input: false,
                        }
                        .to_platform_input(),
                        cx,
                    );
                    window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
                })
                .unwrap();
            }
            cx.update_window(window.into(), |_, window, cx| {
                let position = gpui::point(gpui::px(12.0), gpui::px(12.0));
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
                clicks.get(),
                expected,
                "variant={variant:?}, disabled={disabled}, busy={busy}"
            );
        }
    }
}
