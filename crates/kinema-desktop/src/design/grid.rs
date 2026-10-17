//! Pure card-grid sizing shared by every virtualized grid. No GPUI types.
//!
//! A row is `card_height + CARD_GAP_Y` tall (`row_height`), so a fixed-height
//! `uniform_list` can render one row per list item; items are laid out with
//! `CARD_GAP_X` between columns.
use std::ops::Range;

use super::tokens::metrics::{
    CARD_FOOTER, CARD_GAP_X, CARD_GAP_Y, POSTER_ASPECT, POSTER_MAX, POSTER_MIN,
};

/// Rows kept mounted beyond the viewport on each side.
pub const OVERSCAN_ROWS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridMetrics {
    pub columns: usize,
    pub card_width: f32,
    pub poster_height: f32,
    pub card_height: f32,
    /// Row stride: card height plus the row gap.
    pub row_height: f32,
    pub total_rows: usize,
    pub item_count: usize,
}

impl GridMetrics {
    /// Equal columns within `available_width`, card width in
    /// `POSTER_MIN..=POSTER_MAX` whenever the width allows.
    pub fn new(available_width: f32, item_count: usize) -> Self {
        Self::with_range(available_width, item_count, POSTER_MIN, POSTER_MAX)
    }

    pub fn with_range(available_width: f32, item_count: usize, min: f32, max: f32) -> Self {
        let width = if available_width.is_finite() {
            available_width.max(0.0)
        } else {
            0.0
        };
        let columns = (((width + CARD_GAP_X) / (min + CARD_GAP_X)).floor() as usize).max(1);
        let fit = (width - CARD_GAP_X * (columns as f32 - 1.0)) / columns as f32;
        let card_width = fit.clamp(min.min(max), max);
        let poster_height = (card_width * POSTER_ASPECT).round();
        let card_height = poster_height + CARD_FOOTER;
        Self {
            columns,
            card_width,
            poster_height,
            card_height,
            row_height: card_height + CARD_GAP_Y,
            total_rows: item_count.div_ceil(columns),
            item_count,
        }
    }

    /// Item indices of one row, clipped to the data.
    pub fn row_items(&self, row: usize) -> Range<usize> {
        let start = (row * self.columns).min(self.item_count);
        start..(start + self.columns).min(self.item_count)
    }

    /// Rows to mount for a scroll offset: visible rows plus overscan each side.
    pub fn mounted_rows(&self, scroll_top: f32, viewport_height: f32) -> Range<usize> {
        if self.total_rows == 0 {
            return 0..0;
        }
        let top = scroll_top.max(0.0);
        let first = (top / self.row_height).floor() as usize;
        let last = ((top + viewport_height.max(0.0)) / self.row_height).ceil() as usize;
        first.saturating_sub(OVERSCAN_ROWS).min(self.total_rows)
            ..(last + OVERSCAN_ROWS).min(self.total_rows)
    }

    /// Row containing an item (for scroll-into-view on keyboard navigation).
    pub fn row_of(&self, item: usize) -> usize {
        item / self.columns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_and_sizes_follow_width() {
        let g = GridMetrics::new(912.0, 10);
        assert_eq!(g.columns, 6);
        assert!((POSTER_MIN..=POSTER_MAX).contains(&g.card_width));
        assert_eq!(g.card_height, g.poster_height + CARD_FOOTER);
        assert_eq!(g.row_height, g.card_height + CARD_GAP_Y);
        assert_eq!(g.total_rows, 2);
        assert_eq!(g.row_items(1), 6..10);
        assert_eq!(g.row_items(5), 10..10);
    }

    #[test]
    fn narrow_and_degenerate_widths_keep_one_column() {
        assert_eq!(GridMetrics::new(40.0, 3).columns, 1);
        assert_eq!(GridMetrics::new(f32::NAN, 3).columns, 1);
        assert_eq!(GridMetrics::new(500.0, 0).total_rows, 0);
        assert_eq!(GridMetrics::new(500.0, 0).mounted_rows(0.0, 400.0), 0..0);
    }

    #[test]
    fn mounted_rows_are_viewport_plus_two_overscan() {
        let g = GridMetrics::new(300.0, 1000);
        let rows = g.mounted_rows(g.row_height * 50.0, g.row_height * 3.0);
        assert_eq!(rows, 48..55);
        assert_eq!(g.mounted_rows(0.0, g.row_height * 3.0), 0..5);
        assert_eq!(g.row_of(g.columns * 7 + 1), 7);
    }
}
