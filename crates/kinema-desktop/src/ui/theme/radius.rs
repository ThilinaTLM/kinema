//! Corner geometry. Focus rings follow their host's radius.

use gpui::{Pixels, px};

/// shadcn base radius; the scale below is derived around it.
pub const RADIUS: Pixels = px(8.0);

/// Chips and badges.
pub const SM: Pixels = px(4.0);
/// Controls, inputs, menu items and list-row hover.
pub const MD: Pixels = px(6.0);
/// Cards and posters.
pub const LG: Pixels = RADIUS;
/// Sheets, popovers and menus.
pub const XL: Pixels = px(12.0);
/// Pills and circles (replaces `rounded_full()`); clamped by GPUI to half the
/// shorter side.
pub const FULL: Pixels = px(9999.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_scale_is_ordered() {
        assert_eq!([SM, MD, LG, XL], [px(4.0), px(6.0), px(8.0), px(12.0)]);
        assert!(SM < MD && MD < LG && LG < XL && XL < FULL);
        assert_eq!(LG, RADIUS);
    }
}
