//! Interaction helpers shared by primitives: hover/press spring fills, shadow
//! interpolation and the focus ring.

use std::time::Duration;

use gpui::{prelude::*, *};

/// Critically damped spring used by hover/press fills (same curve as `Button`).
pub(crate) fn spring_config(duration: Duration) -> SpringConfig {
    SpringConfig::new(
        (6.0 / duration.as_secs_f32()).powi(2),
        12.0 / duration.as_secs_f32(),
        1.0,
    )
}

/// Fade a fill between `rest` and `active` with a spring so reversing a hover
/// keeps its progress. Zero duration (reduced motion) snaps. Apply last: the
/// returned element can no longer be styled.
pub(crate) fn fade_fill(
    face: Stateful<Div>,
    key: impl Into<ElementId>,
    active: bool,
    rest: Hsla,
    hover: Hsla,
    duration: Duration,
) -> AnyElement {
    if duration.is_zero() {
        face.bg(if active { hover } else { rest })
            .into_any_element()
    } else {
        face.with_spring(
            key,
            SpringAnimation::new(spring_config(duration)).to(if active { 1.0_f32 } else { 0.0 }),
            move |el, t| el.bg(rest.blend(hover.opacity(t.clamp(0.0, 1.0)))),
        )
        .into_any_element()
    }
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
pub(crate) fn focus_ring(color: Hsla) -> BoxShadow {
    BoxShadow::new(px(0.0), px(0.0), color)
        .spread_radius(crate::ui::theme::space::FOCUS_RING)
        .inset()
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
