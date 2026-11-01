//! Spacing scale (padding, gaps, insets). Component and page dimensions live in
//! [`super::size`], corner geometry in [`super::radius`].

use gpui::{Pixels, px};

pub const NONE: Pixels = px(0.0);
pub const S2: Pixels = px(2.0);
pub const S4: Pixels = px(4.0);
pub const S6: Pixels = px(6.0);
pub const S8: Pixels = px(8.0);
pub const S12: Pixels = px(12.0);
pub const S16: Pixels = px(16.0);
pub const S20: Pixels = px(20.0);
pub const S24: Pixels = px(24.0);
pub const S32: Pixels = px(32.0);
pub const S48: Pixels = px(48.0);

// --- TEMP: removed in step 5 (old names; use `theme::size` instead) -----------
use super::size;
pub const PAGE_INSET: Pixels = size::PAGE_INSET; // TEMP step-5
pub const TOOLBAR_TOP: Pixels = size::TOOLBAR_TOP; // TEMP step-5
pub const TOOLBAR_HEIGHT: Pixels = size::TOOLBAR_HEIGHT; // TEMP step-5
pub const PAGE_TOP: Pixels = size::PAGE_TOP; // TEMP step-5
pub const CONTROL_HEIGHT: Pixels = size::CONTROL_SM; // TEMP step-5
pub const CONTROL_HEIGHT_LARGE: Pixels = size::CONTROL; // TEMP step-5
pub const TOOLBAR_CONTROL: Pixels = size::TOOLBAR_CONTROL; // TEMP step-5
pub const ICON: Pixels = size::ICON; // TEMP step-5
pub const ICON_TARGET: Pixels = size::ICON_TARGET; // TEMP step-5
pub const ROW: Pixels = size::ROW; // TEMP step-5
pub const ROW_TWO_LINE: Pixels = size::ROW_TALL; // TEMP step-5
pub const CHIP_HEIGHT: Pixels = size::CHIP; // TEMP step-5
pub const GRID_INSET: Pixels = size::PAGE_INSET; // TEMP step-5
pub const GRID_GAP: Pixels = S12; // TEMP step-5 (becomes layout::browse)
pub const HAIRLINE: Pixels = size::HAIRLINE; // TEMP step-5
pub const FOCUS_RING: Pixels = size::FOCUS_RING; // TEMP step-5
pub const ACTION_COLUMN: Pixels = size::ACTION_COLUMN; // TEMP step-5
pub const SHEET_WIDTH: Pixels = size::SHEET_WIDTH; // TEMP step-5
pub const POPOVER_WIDTH: Pixels = size::POPOVER_WIDTH; // TEMP step-5

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_strictly_increasing() {
        let all = [NONE, S2, S4, S6, S8, S12, S16, S20, S24, S32, S48];
        assert!(all.windows(2).all(|w| w[0] < w[1]));
    }
}
