//! The one button: variants, sizes, square/icon form, selected state, busy
//! sweep, badge and tooltip. GPUI generates Enter/Space clicks.
//!
//! `Button::icon_button` replaces the old `IconButton` and the `Icon`/`Secondary`
//! kinds of the old `ActionButton` map to `.square(true)` on a `Md` button.
//!
//! Accessibility: `Role::Button`; the accessible name is `name(..)`, else the
//! tooltip, else the label (icon-only buttons therefore need a tooltip or
//! name). `toggled(..)` (implied by `selected(..)`) reports toggled/mixed.
//! Disabled and busy buttons drop the tab stop, the focus handle's tab stop
//! and the click action; GPUI has no `disabled` attribute, so assistive
//! technology sees a button without a click action. Tab focuses, Enter/Space
//! activate. The focus ring is a transparent out-of-flow border that follows
//! the face radius plus gap and is never removed. A selected button also gets
//! a tinted fill, ink and edge, but that is colour-only for the glyph: pair it
//! with `toggled` semantics and a distinct label or glyph where it matters.
use std::rc::Rc;

use gpui::{prelude::*, *};

use crate::{
    ui::motion::{self, AnimateExt, interaction::state_fill, transition::FEEDBACK},
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::motion::timing,
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{Palette, opacity, palette, radius, size, space},
};

pub type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Brand fill: the one main action of a row.
    Primary,
    /// Neutral control fill with an edge.
    #[default]
    Secondary,
    /// No fill or edge until hovered.
    Ghost,
    /// Transparent fill with a control edge.
    Outline,
    /// Destructive ink and edge on a transparent fill (was `Danger`).
    Destructive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// 28 px tall (was `Compact`).
    #[default]
    Sm,
    /// 32 px tall (was `Large`).
    Md,
    /// 28 × 28 px square, glyph only; the name comes from tooltip/name.
    Icon,
}

impl ButtonSize {
    const fn height(self) -> Pixels {
        match self {
            Self::Sm | Self::Icon => size::CONTROL_SM,
            Self::Md => size::CONTROL,
        }
    }
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
    square: bool,
    badge: Option<usize>,
    over_art: bool,
    tooltip: Option<SharedString>,
    /// Selected look (tinted fill, edge and ink); ignored when disabled.
    selected: bool,
    /// Toggle semantics; `None` for ordinary actions and unknown state.
    aria_toggled: Option<bool>,
    /// Report `Toggled::Mixed` when `aria_toggled` is `Some(false)`.
    aria_mixed: bool,
    /// Custom leading glyph (e.g. a watch mark) drawn instead of `icon`.
    glyph: Option<AnyElement>,
    /// Muted trailing text after the label (e.g. a count).
    sub: Option<SharedString>,
    /// Accessible name overriding tooltip and label.
    aria_name: Option<SharedString>,
}

/// Colours of the selected state: `(rest, hover, ink, edge)`.
pub(crate) fn selected_colors(p: &Palette) -> (Hsla, Hsla, Hsla, Hsla) {
    let hover = p
        .selection
        .alpha((p.selection.a * opacity::SELECTION_HOVER_GAIN).min(1.0));
    (
        p.selection,
        hover,
        p.primary_text,
        p.primary.opacity(opacity::SELECTED_EDGE),
    )
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Sm,
            icon: None,
            disabled: false,
            busy: false,
            handler: None,
            focus: None,
            square: false,
            badge: None,
            over_art: false,
            tooltip: None,
            selected: false,
            aria_toggled: None,
            aria_mixed: false,
            glyph: None,
            sub: None,
            aria_name: None,
        }
    }
    /// Ghost, square, glyph-only button. `label` is both the accessible name
    /// and the tooltip; it must describe the action.
    pub fn icon_button(
        id: impl Into<ElementId>,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Self {
        let label = label.into();
        let mut button = Self::new(id, "")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Icon)
            .icon(icon);
        button.tooltip = Some(label);
        button
    }
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
    /// Glyph-only 28 px wide box at the chosen height; the label becomes the
    /// accessible name and default tooltip.
    pub fn square(mut self, square: bool) -> Self {
        self.square = square;
        self
    }
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    /// A custom leading glyph, e.g. a `WatchMark`; replaces `icon`.
    pub fn glyph(mut self, glyph: impl IntoElement) -> Self {
        self.glyph = Some(glyph.into_any_element());
        self
    }
    /// Muted trailing text, hidden for square buttons.
    pub fn trailing(mut self, sub: impl Into<SharedString>) -> Self {
        self.sub = Some(sub.into());
        self
    }
    /// Count shown beside the glyph of square buttons; zero hides it.
    pub fn badge(mut self, count: usize) -> Self {
        self.badge = (count > 0).then_some(count);
        self
    }
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    /// Accessible name when it should differ from the visible label/tooltip.
    pub fn name(mut self, name: impl Into<SharedString>) -> Self {
        self.aria_name = Some(name.into());
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Disable the button; `reason` becomes its tooltip.
    pub fn disabled_because(mut self, reason: impl Into<SharedString>) -> Self {
        self.disabled = true;
        self.tooltip = Some(reason.into());
        self
    }
    /// Selected look (replaces the old accent `on` state). Also exposes toggle
    /// semantics unless [`Button::toggled`] was set explicitly.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Expose toggle semantics without changing the look.
    pub fn toggled(mut self, on: bool) -> Self {
        self.aria_toggled = Some(on);
        self
    }
    /// Report a partial state while `toggled(false)`.
    pub fn mixed(mut self, mixed: bool) -> Self {
        self.aria_mixed = mixed;
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
    fn is_square(&self) -> bool {
        self.square || self.size == ButtonSize::Icon
    }
    /// Disabled and busy buttons are inert: no tab stop, no click action.
    fn is_blocked(&self) -> bool {
        self.disabled || self.busy
    }
    /// Toggle state exposed to assistive technology; `selected` implies on.
    fn toggled_state(&self) -> Option<Toggled> {
        let on = self.aria_toggled.or(self.selected.then_some(true))?;
        Some(match (on, self.aria_mixed) {
            (true, _) => Toggled::True,
            (false, true) => Toggled::Mixed,
            (false, false) => Toggled::False,
        })
    }
    /// Name exposed to assistive technology.
    fn accessible_name(&self) -> SharedString {
        self.aria_name
            .clone()
            .or_else(|| self.tooltip.clone())
            .unwrap_or_else(|| self.label.clone())
    }
}

/// Palette used by a button: the dark palette when over artwork, else the theme's.
fn surface_palette(over_art: bool, themed: Palette) -> Palette {
    if over_art {
        let mut dark = Palette::dark();
        dark.secondary = dark.chip_on_art;
        dark
    } else {
        themed
    }
}

/// Disabled styling takes precedence over the variant and selected state.
fn disabled_colors(variant: ButtonVariant, p: &Palette) -> (Hsla, Hsla, Hsla) {
    let fill = match variant {
        ButtonVariant::Primary | ButtonVariant::Secondary => p.secondary,
        ButtonVariant::Ghost | ButtonVariant::Outline | ButtonVariant::Destructive => {
            transparent_black()
        }
    };
    let border = match variant {
        ButtonVariant::Ghost => transparent_black(),
        _ => p.border,
    };
    (fill, p.subtle_foreground, border)
}

fn busy_bar(ink: Hsla, cx: &App) -> AnyElement {
    let line = div().absolute().h_full();
    let line = if motion::reduced(cx) {
        line.w_full()
            .bg(palette(cx).subtle_foreground)
            .into_any_element()
    } else {
        line.w(relative(0.25)).bg(ink).animate_loop(
            "button-busy-sweep",
            timing::SPIN_PERIOD,
            cx,
            |el, t| el.left(relative(t * 1.25 - 0.25)),
        )
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
        let blocked = self.is_blocked();
        let square = self.is_square();
        let selected = self.selected && !self.disabled;
        let (rest, hover, pressed, ink, border) = match self.variant {
            ButtonVariant::Primary => (
                p.primary,
                p.primary_hover(),
                p.primary_pressed(),
                p.primary_foreground,
                transparent_black(),
            ),
            ButtonVariant::Secondary => (
                p.secondary,
                p.accent,
                p.pressed(p.secondary),
                // Square secondary buttons are quiet utility controls.
                if square {
                    p.muted_foreground
                } else {
                    p.foreground
                },
                p.border,
            ),
            ButtonVariant::Ghost => (
                transparent_black(),
                p.accent,
                p.pressed(p.secondary),
                p.muted_foreground,
                transparent_black(),
            ),
            ButtonVariant::Outline => (
                transparent_black(),
                p.accent,
                p.pressed(p.secondary),
                p.foreground,
                p.input,
            ),
            ButtonVariant::Destructive => (
                transparent_black(),
                p.accent,
                p.pressed(p.secondary),
                p.destructive,
                p.destructive.opacity(opacity::DESTRUCTIVE_EDGE),
            ),
        };
        let (rest, hover, ink, border) = match (selected, self.variant) {
            // Ghost keeps its hover fill and has no edge when selected.
            (true, ButtonVariant::Ghost) => (p.selection, hover, p.primary_text, border),
            (true, _) => selected_colors(&p),
            (false, _) => (rest, hover, ink, border),
        };
        let (rest, ink, border) = if self.disabled {
            disabled_colors(self.variant, &p)
        } else {
            (rest, ink, border)
        };
        // A keyed spring retains interpolated progress when hover reverses.
        let hovered = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let target = !blocked && *hovered.read(cx);
        let hover_state = hovered.clone();
        let name = self.accessible_name();
        let toggled = self.toggled_state();
        let tooltip = self
            .tooltip
            .clone()
            .or_else(|| square.then(|| self.label.clone()));
        let button = div()
            .id("button-face")
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .gap(if self.glyph.is_some() {
                space::S8
            } else {
                space::S6
            })
            .h(self.size.height())
            .px(space::S12)
            .rounded(radius::MD)
            .border(size::HAIRLINE)
            .border_color(border)
            .bg(rest)
            .text_color(ink)
            .text(TextSize::Base)
            .weight(Weight::Semibold)
            .when(square && self.badge.is_none(), |el| {
                el.w(size::ICON_TARGET).px(space::NONE)
            })
            .when(square && self.badge.is_some(), |el| {
                el.min_w(size::ICON_TARGET).px(space::S6).gap(space::S4)
            })
            .when(!blocked, |el| {
                el.group_active("button", move |s| s.bg(pressed))
            })
            .when_some(self.icon, |el, icon| el.child(Icon::new(icon).color(ink)))
            .children(self.glyph)
            .when(!square, |el| el.child(self.label.clone()))
            .when_some(self.sub.clone().filter(|_| !square), |el, sub| {
                el.child(
                    div()
                        .text(TextSize::Sm)
                        .text_color(if selected { ink } else { p.subtle_foreground })
                        .child(sub),
                )
            })
            .when_some(self.badge, |el, count| {
                el.child(
                    div()
                        .text(TextSize::Xs)
                        .weight(Weight::Semibold)
                        .tabular()
                        .text_color(if self.disabled {
                            p.subtle_foreground
                        } else {
                            p.primary_text
                        })
                        .child(count.to_string()),
                )
            })
            // Absolute placement preserves label metrics and keeps the sweep inside.
            .when(self.busy, |el| el.child(busy_bar(ink, cx)));
        let face = if blocked {
            button.into_any_element()
        } else {
            state_fill(button, "button-feedback", target, rest, hover, FEEDBACK, cx)
        };
        // Transparent, out-of-flow ring geometry avoids layout shifts on focus.
        // 2 px ring, 2 px gap, offset outwards by 4 px on each side.
        div()
            .id(self.id)
            .group("button")
            .relative()
            .flex_none()
            .border(size::FOCUS_RING)
            .border_color(transparent_black())
            .p(space::S2)
            .m(-space::S4)
            // Ring follows the host radius: face radius + gap + ring width.
            .rounded(radius::MD + space::S4)
            .role(Role::Button)
            .aria_label(name)
            .when_some(toggled, |el, toggled| el.aria_toggled(toggled))
            .when(!blocked, |el| {
                el.focusable()
                    .cursor_pointer()
                    .focus_visible(move |s| s.border_color(p.ring))
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
            .when_some(tooltip, |el, label| {
                el.tooltip(crate::ui::primitives::tooltip::tooltip(label))
            })
            .when_some(self.handler.filter(|_| !blocked), |el, handler| {
                el.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(face)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Button, ButtonSize, ButtonVariant, disabled_colors, selected_colors, surface_palette,
    };
    use crate::ui::primitives::IconName;
    use crate::ui::theme::Palette;
    use ::core::prelude::v1::test;

    #[test]
    fn over_art_uses_dark_surface_in_any_theme() {
        let dark = Palette::dark();
        let p = surface_palette(true, Palette::light());
        assert_eq!(p.foreground, dark.foreground);
        assert_eq!(p.secondary, dark.chip_on_art);
        assert_eq!(
            surface_palette(false, Palette::light()).foreground,
            Palette::light().foreground
        );
    }

    #[test]
    fn disabled_palette_overrides_every_variant_in_both_themes() {
        for p in [Palette::dark(), Palette::light()] {
            for variant in [
                ButtonVariant::Primary,
                ButtonVariant::Secondary,
                ButtonVariant::Ghost,
                ButtonVariant::Outline,
                ButtonVariant::Destructive,
            ] {
                let (fill, ink, border) = disabled_colors(variant, &p);
                assert_eq!(ink, p.subtle_foreground, "{variant:?}");
                assert_eq!(
                    fill,
                    if matches!(variant, ButtonVariant::Primary | ButtonVariant::Secondary) {
                        p.secondary
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
                        p.border
                    },
                    "{variant:?}"
                );
            }
        }
    }

    #[test]
    fn selected_state_uses_selection_tokens_in_both_themes() {
        for p in [Palette::dark(), Palette::light()] {
            let (rest, hover, ink, edge) = selected_colors(&p);
            assert_eq!(rest, p.selection);
            assert_eq!(ink, p.primary_text);
            assert!(hover.a > rest.a, "hover must deepen the tint");
            assert_eq!(
                (edge.h, edge.s, edge.l),
                (p.primary.h, p.primary.s, p.primary.l)
            );
        }
    }

    #[test]
    fn name_prefers_explicit_then_tooltip_then_label() {
        let plain = Button::new("b", "Play");
        assert_eq!(plain.accessible_name().as_ref(), "Play");
        let tip = Button::new("b", "Play").tooltip("Play trailer");
        assert_eq!(tip.accessible_name().as_ref(), "Play trailer");
        let named = Button::new("b", "Play").tooltip("tip").name("Play Dune");
        assert_eq!(named.accessible_name().as_ref(), "Play Dune");
        let icon = Button::icon_button("b", IconName::Play, "Play");
        assert_eq!(icon.accessible_name().as_ref(), "Play");
        assert!(icon.is_square());
        assert_eq!(icon.size, ButtonSize::Icon);
    }

    #[test]
    fn toggle_state_follows_selected_toggled_and_mixed() {
        use gpui::Toggled;
        assert_eq!(Button::new("b", "x").toggled_state(), None);
        assert_eq!(
            Button::new("b", "x").selected(true).toggled_state(),
            Some(Toggled::True)
        );
        assert_eq!(
            Button::new("b", "x").toggled(false).toggled_state(),
            Some(Toggled::False)
        );
        assert_eq!(
            Button::new("b", "x")
                .toggled(false)
                .mixed(true)
                .toggled_state(),
            Some(Toggled::Mixed)
        );
        // Selected is visual-only for the owner that never calls `selected`.
        assert_eq!(
            Button::new("b", "x")
                .toggled(true)
                .selected(false)
                .toggled_state(),
            Some(Toggled::True)
        );
    }

    #[test]
    fn disabled_and_busy_buttons_are_inert() {
        assert!(!Button::new("b", "x").is_blocked());
        assert!(Button::new("b", "x").disabled(true).is_blocked());
        assert!(Button::new("b", "x").busy(true).is_blocked());
        let reason = Button::new("b", "x").disabled_because("Needs an account");
        assert!(reason.is_blocked());
        assert_eq!(reason.accessible_name().as_ref(), "Needs an account");
    }

    #[test]
    fn zero_badge_is_absent() {
        assert_eq!(Button::new("b", "").badge(0).badge, None);
        assert_eq!(Button::new("b", "").badge(3).badge, Some(3));
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
            ButtonVariant::Outline,
            ButtonVariant::Destructive,
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
            cx.update(crate::ui::theme::init);
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
