//! Semantic Graphite palette and fixed metrics (docs/design/design-system.md §3-4).
//!
//! Values are opaque and resolved once per mode; components read `Tokens`, never
//! literal colours.
use gpui_kit::{Hsla, rgb, rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

/// Semantic colours. `Copy`, so consumers can take them out of the `App` borrow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub mode: Mode,
    pub canvas: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    /// `surface.hover`, also the active rail fill.
    pub hover: Hsla,
    /// Decorative separators and panel edges only.
    pub border: Hsla,
    /// Strong boundary of interactive controls (>= 3:1).
    pub control_border: Hsla,
    pub text: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub primary_pressed: Hsla,
    pub on_primary: Hsla,
    pub focus: Hsla,
    pub control: Hsla,
    pub control_hover: Hsla,
    pub control_pressed: Hsla,
    pub selection_subtle: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
    pub danger_fill: Hsla,
    /// IMDb label fill. Exclusive to the IMDb label.
    pub imdb: Hsla,
    pub imdb_ink: Hsla,
    pub poster_edge: Hsla,
}

impl Tokens {
    pub fn for_mode(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self::dark(),
            Mode::Light => Self::light(),
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: Mode::Dark,
            canvas: rgb(0x17191D).into(),
            surface: rgb(0x202227).into(),
            raised: rgb(0x292C33).into(),
            hover: rgb(0x31353D).into(),
            border: rgb(0x353942).into(),
            control_border: rgb(0x6E7788).into(),
            text: rgb(0xECEFF4).into(),
            secondary: rgb(0xAFB5C0).into(),
            muted: rgb(0x9CA5B4).into(),
            primary: rgb(0x3C66BA).into(),
            primary_hover: rgb(0x4673CC).into(),
            primary_pressed: rgb(0x335AA6).into(),
            on_primary: rgb(0xFFFFFF).into(),
            focus: rgb(0x85AFF5).into(),
            control: rgb(0x292C33).into(),
            control_hover: rgb(0x31353D).into(),
            control_pressed: rgb(0x3A3F49).into(),
            selection_subtle: rgba(0x85AFF529).into(),
            success: rgb(0x8CC9A8).into(),
            warning: rgb(0xE8BC72).into(),
            error: rgb(0xEF9A9A).into(),
            danger_fill: rgb(0xA93232).into(),
            imdb: rgb(0xF5C518).into(),
            imdb_ink: rgb(0x1F232B).into(),
            poster_edge: rgba(0xFFFFFF1A).into(),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: Mode::Light,
            canvas: rgb(0xF3F4F6).into(),
            surface: rgb(0xFFFFFF).into(),
            raised: rgb(0xE9ECF1).into(),
            hover: rgb(0xDEE3EA).into(),
            border: rgb(0xCDD3DD).into(),
            control_border: rgb(0x7B8495).into(),
            text: rgb(0x1F232B).into(),
            secondary: rgb(0x565F70).into(),
            muted: rgb(0x5F6879).into(),
            primary: rgb(0x315EB4).into(),
            primary_hover: rgb(0x2A52A0).into(),
            primary_pressed: rgb(0x234689).into(),
            on_primary: rgb(0xFFFFFF).into(),
            focus: rgb(0x315EB4).into(),
            control: rgb(0xFFFFFF).into(),
            control_hover: rgb(0xEEF1F5).into(),
            control_pressed: rgb(0xE2E6EC).into(),
            selection_subtle: rgba(0x315EB41F).into(),
            success: rgb(0x256441).into(),
            warning: rgb(0x77520D).into(),
            error: rgb(0xA93232).into(),
            danger_fill: rgb(0xA93232).into(),
            imdb: rgb(0xF5C518).into(),
            imdb_ink: rgb(0x1F232B).into(),
            poster_edge: rgba(0x00000024).into(),
        }
    }
}

/// Logical-pixel metrics shared by every view.
pub mod metrics {
    pub const RAIL_WIDTH: f32 = 56.0;
    pub const RAIL_TARGET: f32 = 36.0;
    pub const RAIL_GLYPH: f32 = 18.0;
    pub const TOOLBAR_HEIGHT: f32 = 44.0;
    pub const CONTROL_HEIGHT: f32 = 28.0;
    pub const CONTROL_COMPACT_HEIGHT: f32 = 24.0;
    pub const CONTROL_PROMINENT_HEIGHT: f32 = 32.0;
    pub const CONTROL_RADIUS: f32 = 4.0;
    pub const PANEL_RADIUS: f32 = 6.0;
    pub const WORKSPACE_PADDING: f32 = 16.0;
    pub const WORKSPACE_GAP: f32 = 12.0;
    pub const FONT_BODY: f32 = 13.0;
    pub const LINE_BODY: f32 = 18.0;
    pub const FONT_LABEL: f32 = 12.0;
    pub const FONT_META: f32 = 11.0;
    pub const LINE_META: f32 = 16.0;
    pub const FONT_HEADING: f32 = 18.0;
    pub const LINE_HEADING: f32 = 24.0;
    pub const FOCUS_RING: f32 = 2.0;
    pub const FOCUS_GAP: f32 = 2.0;
    pub const CARD_FOOTER: f32 = 76.0;
    pub const CARD_GAP_X: f32 = 12.0;
    pub const CARD_GAP_Y: f32 = 16.0;
    pub const POSTER_MIN: f32 = 132.0;
    pub const POSTER_MAX: f32 = 180.0;
    /// Poster height = width * 3 / 2.
    pub const POSTER_ASPECT: f32 = 1.5;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Hsla) -> f32 {
        let c = c.to_rgb();
        let f = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }

    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn documented_contrast_holds_in_both_modes() {
        for t in [Tokens::dark(), Tokens::light()] {
            assert!(contrast(t.muted, t.raised) >= 4.5, "{:?}", t.mode);
            assert!(contrast(t.on_primary, t.primary) >= 4.5);
            assert!(contrast(t.on_primary, t.primary_hover) >= 4.5);
            assert!(contrast(t.control_border, t.raised) >= 3.0);
            assert!(contrast(t.focus, t.canvas) >= 3.0);
            assert!(contrast(t.imdb_ink, t.imdb) >= 4.5);
            assert!(contrast(t.text, t.surface) >= 4.5);
        }
    }
}
