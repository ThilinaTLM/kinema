//! Restrained artwork-led background only. Semantic text/control colors stay intact.
use crate::artwork::ArtworkPalette;
use crate::ui::theme::{Mode, Palette, opacity};
use gpui::{prelude::*, *};

const WASH_ANGLE: f32 = 135.0;

/// Full-size composable surface; place Detail content inside using `.child(...)`.
/// `None` is the unchanged theme canvas. It does not sample images or recolor ink.
#[derive(IntoElement)]
pub struct AmbientBackground {
    palette: Option<ArtworkPalette>,
    mode: Mode,
    children: Vec<AnyElement>,
}
impl AmbientBackground {
    pub fn new(palette: Option<ArtworkPalette>, mode: Mode) -> Self {
        Self {
            palette,
            mode,
            children: Vec::new(),
        }
    }
}
impl ParentElement for AmbientBackground {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
fn wash_colors(palette: Option<ArtworkPalette>, mode: Mode) -> [Rgba; 2] {
    let (base, opacity) = match mode {
        Mode::Dark => (Palette::dark().background, opacity::AMBIENT_DARK),
        Mode::Light => (Palette::light().background, opacity::AMBIENT_LIGHT),
    };
    let base: Rgba = base.into();
    let Some(palette) = palette else {
        return [base; 2];
    };
    let blend = |color: [u8; 3], alpha: f32| Rgba {
        r: base.r * (1.0 - alpha) + f32::from(color[0]) / 255.0 * alpha,
        g: base.g * (1.0 - alpha) + f32::from(color[1]) / 255.0 * alpha,
        b: base.b * (1.0 - alpha) + f32::from(color[2]) / 255.0 * alpha,
        a: 1.0,
    };
    [
        blend(palette.primary, opacity),
        blend(palette.secondary, opacity * opacity::AMBIENT_SECONDARY),
    ]
}
impl RenderOnce for AmbientBackground {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let [primary, secondary] = wash_colors(self.palette, self.mode);
        div()
            .size_full()
            .bg(linear_gradient(
                WASH_ANGLE,
                linear_color_stop(primary, 0.0),
                linear_color_stop(secondary, 1.0),
            ))
            .children(self.children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn washes_are_finite_bounded_and_missing_art_is_canvas() {
        for mode in [Mode::Dark, Mode::Light] {
            let base = wash_colors(None, mode);
            assert_eq!(base[0], base[1]);
            for palette in [
                ArtworkPalette::NEUTRAL,
                ArtworkPalette {
                    primary: [255; 3],
                    secondary: [0; 3],
                },
            ] {
                let colors = wash_colors(Some(palette), mode);
                for color in colors {
                    for channel in [color.r, color.g, color.b, color.a] {
                        assert!(channel.is_finite() && (0.0..=1.0).contains(&channel));
                    }
                    assert!((color.r - base[0].r).abs() <= opacity::AMBIENT_DARK);
                    assert!((color.g - base[0].g).abs() <= opacity::AMBIENT_DARK);
                    assert!((color.b - base[0].b).abs() <= opacity::AMBIENT_DARK);
                    assert_eq!(color.a, 1.0);
                }
            }
        }
    }
}
