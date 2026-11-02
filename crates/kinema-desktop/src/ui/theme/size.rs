//! Component and page sizes: controls, icons, rows, chips, lines, toolbar, panels.

use gpui::{Pixels, px};

// Controls
pub const CONTROL_SM: Pixels = px(28.0);
pub const CONTROL: Pixels = px(32.0);
/// Every floating-header control (chips, segmented, search, icon groups).
pub const TOOLBAR_CONTROL: Pixels = px(32.0);

// Icons
pub const ICON: Pixels = px(16.0);
/// Compact glyphs (player chrome).
pub const ICON_SM: Pixels = px(14.0);
pub const ICON_TARGET: Pixels = px(28.0);

// Rows and chips
pub const ROW: Pixels = px(32.0);
pub const ROW_TALL: Pixels = px(56.0);
pub const CHIP: Pixels = px(20.0);

// Lines
pub const HAIRLINE: Pixels = px(1.0);
pub const FOCUS_RING: Pixels = px(2.0);

/// Tolerance for comparing laid-out offsets, and the overlap that hides seams
/// between stacked paint strips.
pub const SUBPIXEL: Pixels = px(0.5);

// Toolbar and page
/// Side, top and bottom inset of every main-window page.
pub const PAGE_INSET: Pixels = px(20.0);
/// Space above the floating-header controls; matches the gap below them to
/// page content ([`PAGE_INSET`]) so the header row reads balanced.
pub const TOOLBAR_TOP: Pixels = PAGE_INSET;
/// Floating header: top padding plus one control row.
pub const TOOLBAR_HEIGHT: Pixels = px(52.0);
/// Where page content starts beneath the floating header.
pub const PAGE_TOP: Pixels = px(72.0);

// Panels
pub const SHEET_WIDTH: Pixels = px(560.0);
pub const POPOVER_WIDTH: Pixels = px(380.0);
pub const ACTION_COLUMN: Pixels = px(400.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_row_is_balanced_against_the_page_inset() {
        assert_eq!(TOOLBAR_HEIGHT, TOOLBAR_TOP + TOOLBAR_CONTROL);
        assert_eq!(PAGE_TOP, TOOLBAR_HEIGHT + PAGE_INSET);
        // Same space above the header controls as between them and content.
        assert_eq!(TOOLBAR_TOP, PAGE_TOP - TOOLBAR_HEIGHT);
    }

    #[test]
    fn control_sizes_are_ordered() {
        assert!(CONTROL_SM < CONTROL && ICON_SM < ICON && ICON < ICON_TARGET);
        assert!(CHIP < ROW && ROW < ROW_TALL);
    }
}
