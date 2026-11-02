//! Soft shadows only for raised surfaces; pass directly to `Styled::shadow`.

use gpui::{BoxShadow, px};

use super::Palette;

fn soft(palette: &Palette, offset: f32, blur: f32) -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.0), px(offset), palette.shadow).blur_radius(px(blur))]
}

/// Raised surface at rest (0/2/8).
pub fn sm(palette: &Palette) -> Vec<BoxShadow> {
    soft(palette, 2.0, 8.0)
}

/// Card hover (0/4/14).
pub fn md(palette: &Palette) -> Vec<BoxShadow> {
    soft(palette, 4.0, 14.0)
}

/// Overlays: menus and popovers (0/6/20).
pub fn lg(palette: &Palette) -> Vec<BoxShadow> {
    soft(palette, 6.0, 20.0)
}

/// Sheets (0/12/36).
pub fn xl(palette: &Palette) -> Vec<BoxShadow> {
    soft(palette, 12.0, 36.0)
}
