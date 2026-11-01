//! Loading placeholders. They pulse in phase with each other and are fully
//! static when reduced motion is on.
use std::time::Duration;

use gpui::{prelude::*, *};

use crate::{
    ui::motion::{self, Motion},
    ui::theme::{palette, radius, space},
};

/// Pulse cycle length is this many Crossfade durations (1.2 s nominal).
const PULSE_CYCLES: u32 = 5;
const PULSE_MIN: f32 = 0.5;
/// Default footprint of a [`SkeletonShape::Line`] / circle.
const LINE_HEIGHT: Pixels = space::S12;
const CIRCLE_SIZE: Pixels = space::S32;
/// Width of the last line of a [`SkeletonText`] paragraph.
const LAST_LINE_FRACTION: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkeletonShape {
    /// Card/poster-like surface (radius MD).
    Block,
    /// Text line (radius XS, full width).
    Line,
    /// Avatar or icon placeholder.
    Circle,
}

/// Size it with the usual `w`/`h`/`flex_*` styling.
#[derive(IntoElement)]
pub struct Skeleton {
    base: Div,
}

impl Skeleton {
    pub fn new(shape: SkeletonShape) -> Self {
        let base = div().flex_none();
        let base = match shape {
            SkeletonShape::Block => base.rounded(radius::MD).w_full().h(space::S48),
            SkeletonShape::Line => base.rounded(radius::XS).w_full().h(LINE_HEIGHT),
            SkeletonShape::Circle => base.rounded_full().size(CIRCLE_SIZE),
        };
        Self { base }
    }
    pub fn block() -> Self {
        Self::new(SkeletonShape::Block)
    }
    pub fn line() -> Self {
        Self::new(SkeletonShape::Line)
    }
    pub fn circle() -> Self {
        Self::new(SkeletonShape::Circle)
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

/// `None` means static (reduced motion).
fn pulse_period(cx: &App) -> Option<Duration> {
    let base = motion::duration(Motion::Crossfade, cx);
    (!base.is_zero()).then(|| base * PULSE_CYCLES)
}

impl RenderOnce for Skeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let el = self.base.bg(palette(cx).bg_placeholder);
        match pulse_period(cx) {
            None => el.into_any_element(),
            Some(period) => el
                .with_animation(
                    "skeleton-pulse",
                    Animation::new(period)
                        .repeat_synced()
                        .with_easing(pulsating_between(PULSE_MIN, 1.0)),
                    |el, value| el.opacity(value),
                )
                .into_any_element(),
        }
    }
}

/// Paragraph placeholder: full lines with a shorter last line.
#[derive(IntoElement)]
pub struct SkeletonText {
    lines: usize,
    base: Div,
}

impl SkeletonText {
    pub fn new(lines: usize) -> Self {
        Self {
            lines,
            base: div().flex().flex_col().gap(space::S8).w_full(),
        }
    }

    /// Width of each line as a fraction of the container.
    pub fn line_widths(lines: usize) -> Vec<f32> {
        (0..lines)
            .map(|i| {
                if lines > 1 && i + 1 == lines {
                    LAST_LINE_FRACTION
                } else {
                    1.0
                }
            })
            .collect()
    }
}

impl Styled for SkeletonText {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for SkeletonText {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.base.children(
            Self::line_widths(self.lines)
                .into_iter()
                .map(|fraction| Skeleton::line().w(relative(fraction))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{LAST_LINE_FRACTION, SkeletonText};

    #[test]
    fn last_line_is_ragged_only_for_paragraphs() {
        assert!(SkeletonText::line_widths(0).is_empty());
        assert_eq!(SkeletonText::line_widths(1), vec![1.0]);
        assert_eq!(
            SkeletonText::line_widths(3),
            vec![1.0, 1.0, LAST_LINE_FRACTION]
        );
    }
}
