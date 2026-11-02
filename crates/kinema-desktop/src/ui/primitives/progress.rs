//! Thin rounded progress bars and a deliberately quiet busy indicator.
use crate::{
    ui::motion::{self, AnimateExt},
    ui::theme::motion::timing,
    ui::theme::{palette, space},
};
use gpui::{prelude::*, *};

/// Clamp untrusted provider progress; NaN means no progress.
pub fn clamp_progress(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}
#[derive(IntoElement)]
pub struct Progress {
    id: ElementId,
    value: Option<f32>,
}
impl Progress {
    pub fn new(id: impl Into<ElementId>, value: f32) -> Self {
        Self {
            id: id.into(),
            value: Some(clamp_progress(value)),
        }
    }
    pub fn indeterminate(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: None,
        }
    }
}
impl RenderOnce for Progress {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let fill = div().absolute().h_full().rounded_full().bg(p.primary);
        let fill = match self.value {
            Some(value) => fill.w(relative(value)).into_any_element(),
            None => {
                let fill = fill.w(relative(0.25));
                if motion::reduced(cx) {
                    fill.left(relative(0.375)).into_any_element()
                } else {
                    fill.animate_loop(self.id.clone(), timing::SPIN_PERIOD, cx, |el, t| {
                        el.left(relative(t * 1.25 - 0.25))
                    })
                }
            }
        };
        div()
            .id(self.id)
            .relative()
            .w_full()
            .h(space::S4)
            .rounded_full()
            .overflow_hidden()
            .bg(p.ctl_rest)
            .child(fill)
    }
}

/// Static, square busy glyph; no decorative looping under reduced motion.
#[derive(IntoElement)]
pub struct Spinner {
    id: ElementId,
    size: Pixels,
}
impl Spinner {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            size: space::S12,
        }
    }
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}
impl RenderOnce for Spinner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .size(self.size)
            .flex_none()
            .border(space::HAIRLINE)
            .border_color(palette(cx).text_muted)
            .child(
                div()
                    .size_full()
                    .border_l(space::S2)
                    .border_color(palette(cx).accent_text),
            )
    }
}
#[cfg(test)]
mod tests {
    use super::clamp_progress;
    #[test]
    fn progress_is_bounded() {
        for (input, expected) in [
            (-2.0, 0.0),
            (0.0, 0.0),
            (0.25, 0.25),
            (2.0, 1.0),
            (f32::NAN, 0.0),
            (f32::INFINITY, 1.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            assert_eq!(clamp_progress(input), expected);
        }
    }
}
