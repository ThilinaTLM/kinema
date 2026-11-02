//! Loading placeholders. They pulse in phase with each other and are fully
//! static when reduced motion is on.
use gpui::{prelude::*, *};

use crate::{
    ui::motion::{self, AnimateExt},
    ui::theme::motion::timing,
    ui::theme::{palette, radius, space},
};

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
    /// Text line (radius SM, full width).
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
            SkeletonShape::Block => base.rounded(radius::LG).w_full().h(space::S48),
            SkeletonShape::Line => base.rounded(radius::SM).w_full().h(LINE_HEIGHT),
            SkeletonShape::Circle => base.rounded(radius::FULL).size(CIRCLE_SIZE),
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

impl RenderOnce for Skeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let el = self.base.bg(palette(cx).bg_placeholder);
        if motion::reduced(cx) {
            return el.into_any_element();
        }
        let pulse = pulsating_between(PULSE_MIN, 1.0);
        el.animate_loop("skeleton-pulse", timing::PULSE_PERIOD, cx, move |el, t| {
            el.opacity(pulse(t))
        })
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
