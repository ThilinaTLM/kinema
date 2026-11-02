//! Interaction helpers shared by primitives: hover/press spring fills, shadow
//! interpolation, the focus ring and the state-style helper.

use std::time::Duration;

use gpui::{prelude::*, *};

use super::{resolve, transition::Transition};
use crate::ui::theme::{Palette, size};

/// Critically damped spring used by hover/press fills (same curve as `Button`).
pub(crate) fn spring(duration: Duration) -> SpringConfig {
    SpringConfig::new(
        (6.0 / duration.as_secs_f32()).powi(2),
        12.0 / duration.as_secs_f32(),
        1.0,
    )
}

/// Drive `apply(el, t)` with a keyed spring toward `active` (1) or rest (0), so
/// reversing mid-flight keeps its progress. The spring settles in about the
/// preset's duration; zero duration (reduced motion) snaps to the target.
pub(crate) fn state_spring<E: IntoElement + 'static>(
    el: E,
    key: impl Into<ElementId>,
    active: bool,
    preset: Transition,
    cx: &App,
    apply: impl FnOnce(E, f32) -> E + 'static,
) -> AnyElement {
    let target = if active { 1.0_f32 } else { 0.0 };
    let duration = resolve(preset, cx).duration;
    if duration.is_zero() {
        apply(el, target).into_any_element()
    } else {
        el.with_spring(
            key,
            SpringAnimation::new(spring(duration)).to(target),
            move |el, t| apply(el, t.clamp(0.0, 1.0)),
        )
        .into_any_element()
    }
}

/// Fade a fill between `rest` and `hover` while `active`. Apply last: the
/// returned element can no longer be styled.
pub(crate) fn state_fill(
    face: Stateful<Div>,
    key: impl Into<ElementId>,
    active: bool,
    rest: Hsla,
    hover: Hsla,
    preset: Transition,
    cx: &App,
) -> AnyElement {
    state_spring(face, key, active, preset, cx, move |el, t| {
        el.bg(rest.blend(hover.opacity(t)))
    })
}

/// Linear blend of two shadow stacks (e.g. `raised` -> `card_hover`).
/// Layers are paired by index; surplus layers are kept unchanged.
pub(crate) fn lerp_shadows(from: &[BoxShadow], to: &[BoxShadow], t: f32) -> Vec<BoxShadow> {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a + (b - a) * t;
    from.iter()
        .zip(to)
        .map(|(a, b)| BoxShadow {
            color: Hsla {
                a: mix(a.color.a, b.color.a),
                ..a.color
            },
            offset: point(
                px(mix(f32::from(a.offset.x), f32::from(b.offset.x))),
                px(mix(f32::from(a.offset.y), f32::from(b.offset.y))),
            ),
            blur_radius: px(mix(f32::from(a.blur_radius), f32::from(b.blur_radius))),
            spread_radius: px(mix(f32::from(a.spread_radius), f32::from(b.spread_radius))),
            inset: a.inset,
        })
        .collect()
}

/// Inset focus ring that follows the host's radius and never shifts layout.
pub(crate) fn focus_ring(palette: &Palette) -> BoxShadow {
    BoxShadow::new(px(0.0), px(0.0), palette.focus_ring)
        .spread_radius(size::FOCUS_RING)
        .inset()
}

/// Rest/hover/pressed/focus-visible/disabled styling shared by interactive
/// surfaces. Disabled keeps the fill and only dims the ink to the muted text
/// color (no opacity fade).
#[derive(Clone, Debug)]
pub struct StateStyle {
    pub rest: Hsla,
    pub hover: Hsla,
    pub pressed: Hsla,
    pub ring: BoxShadow,
    pub ink: Hsla,
    pub disabled_ink: Hsla,
}

impl StateStyle {
    pub fn new(palette: &Palette, rest: Hsla, hover: Hsla, pressed: Hsla, ink: Hsla) -> Self {
        Self {
            rest,
            hover,
            pressed,
            ring: focus_ring(palette),
            ink,
            disabled_ink: palette.text_muted,
        }
    }

    /// Ink for the settled state.
    pub fn ink(&self, disabled: bool) -> Hsla {
        if disabled {
            self.disabled_ink
        } else {
            self.ink
        }
    }

    /// Visible focus ring that never shifts layout.
    pub fn focus<E: StatefulInteractiveElement>(&self, el: E) -> E {
        let ring = self.ring.clone();
        el.focus_visible(move |s| s.shadow(vec![ring.clone()]))
    }

    /// Pressed fill.
    pub fn press<E: StatefulInteractiveElement>(&self, el: E) -> E {
        let pressed = self.pressed;
        el.active(move |s| s.bg(pressed))
    }
}

#[cfg(test)]
mod tests {
    use super::lerp_shadows;
    use crate::ui::theme::{Palette, elevation};

    #[test]
    fn shadow_lerp_hits_both_ends() {
        let p = Palette::dark();
        let (a, b) = (elevation::sm(&p), elevation::md(&p));
        let start = lerp_shadows(&a, &b, 0.0);
        let end = lerp_shadows(&a, &b, 1.0);
        assert_eq!(start[0].offset, a[0].offset);
        assert_eq!(end[0].blur_radius, b[0].blur_radius);
        let mid = lerp_shadows(&a, &b, 0.5);
        assert!(mid[0].blur_radius > a[0].blur_radius && mid[0].blur_radius < b[0].blur_radius);
    }
}
