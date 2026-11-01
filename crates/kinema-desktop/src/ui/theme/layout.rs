//! View metrics grouped per view. Values are the module-local constants of
//! `ui/views/**` and `ui/blocks/**`, unchanged; call sites migrate in step 5
//! (until then the module-local originals remain next to their users).

/// Window-width breakpoints shared by several views.
pub mod breakpoints {
    use gpui::{Pixels, px};
    /// Below this, action rows compact (`detail/layout.rs` COMPACT_ACTIONS_BELOW).
    pub const COMPACT: Pixels = px(900.);
    /// Below this, toolbars/filters/glance panels collapse
    /// (`browse/filters.rs` COMPACT_BELOW, `detail/layout.rs` GLANCE_STACK_BELOW,
    /// `toolbar.rs` TRANSFER_DETAIL_MIN_WIDTH).
    pub const WIDE: Pixels = px(1100.);
    /// Frame width from which the Streams inspector sits beside the list.
    pub const INSPECTOR: Pixels = px(1040.);
}

pub mod detail {
    use gpui::{Pixels, px};
    pub const PAGE_MAX_WIDTH: Pixels = px(2400.);
    pub const EPISODE_CARD_MIN: Pixels = px(280.);
    pub const EPISODE_CARD_MAX: Pixels = px(360.);
    pub const EPISODE_GRID_GAP: Pixels = px(16.);
    pub const SIMILAR_MIN: Pixels = px(140.);
    pub const SIMILAR_MAX: Pixels = px(200.);
    pub const SIMILAR_GAP: Pixels = px(14.);
    pub const CAST_MIN: Pixels = px(112.);
    pub const CAST_MAX: Pixels = px(160.);
    pub const FACT_LABEL_WIDTH: Pixels = px(120.);
    pub const FACT_CARD_MIN: Pixels = px(260.);
    pub const SEASON_TILE_MIN: Pixels = px(184.);
    pub const SEASON_TILE_MAX: Pixels = px(300.);
    pub const SEASON_TILE_GAP: Pixels = px(12.);
    pub const SEASON_POSTER_W: Pixels = px(40.);
    pub const SEASON_HEADER_POSTER_W: Pixels = px(72.);
    pub const STREAMS_POSTER_WIDTH: Pixels = px(104.);
    pub const STREAMS_POSTER_WIDTH_NARROW: Pixels = px(80.);
    pub const STREAMS_DOCK_CLEARANCE: Pixels = px(96.);
    pub const STREAMS_DOCK_SESSION_WIDTH: Pixels = px(340.);
    pub const STREAMS_INSPECTOR_MIN_FRAME: Pixels = super::breakpoints::INSPECTOR;
    // Hero and rows (blocks).
    pub const HERO_MIN_HEIGHT: Pixels = px(340.);
    pub const HERO_OVERVIEW_SKELETON_WIDTH: Pixels = px(480.);
    pub const HERO_POSTER_WIDTH: Pixels = px(168.);
    pub const HERO_TEXT_MAX_WIDTH: Pixels = px(640.);
    pub const STREAM_SIZE_WIDTH: Pixels = px(84.);
    pub const STREAM_SEEDERS_WIDTH: Pixels = px(72.);
    pub const QUALITY_TILE_WIDTH: Pixels = px(56.);
    pub const QUALITY_TILE_HEIGHT: Pixels = px(40.);
    /// Cast rows shown before "show all" (count, not pixels).
    pub const CAST_ROWS_COLLAPSED: usize = 2;
}

pub mod browse {
    use gpui::{Pixels, px};
    pub const SEARCH_WIDTH: Pixels = px(440.);
    pub const SEARCH_MIN_WIDTH: Pixels = px(180.);
    /// Height of the header scrim ramp (`main_window.rs` SCRIM_RAMP).
    pub const SCRIM_RAMP: Pixels = px(24.);
    pub const COMPACT_BELOW: Pixels = super::breakpoints::WIDE;
    pub const OVERLAY_BUTTON_SIZE: Pixels = px(28.);
}

pub mod transfers {
    use gpui::{Pixels, px};
    pub const PANEL_WIDTH: Pixels = px(352.);
    pub const LIST_MAX_HEIGHT: Pixels = px(336.);
}

pub mod player {
    use gpui::{Pixels, px};
    pub const DOCK_SIDE_INSET: Pixels = px(28.);
    pub const DOCK_BOTTOM_INSET: Pixels = px(23.);
    pub const DOCK_FADE: Pixels = px(72.);
    pub const DOCK_ESTIMATE: Pixels = px(148.);
    pub const TOP_FADE: Pixels = px(48.);
    pub const TOP_BAR_HEIGHT: Pixels = px(48.);
    pub const HELP_COLUMN: Pixels = px(300.);
    pub const VOLUME_PANEL_EXTRA: Pixels = px(84.);
    pub const VOLUME_TRACK: Pixels = px(12.);
    pub const VOLUME_THUMB: Pixels = px(18.);
}

/// Primitive-level metrics that are not component sizes.
pub mod controls {
    use gpui::{Pixels, px};
    /// Choice tile width (`choice_card.rs` TILE_WIDTH).
    pub const CHOICE_TILE_WIDTH: Pixels = px(196.);
    /// Carousel edge slop (`carousel.rs` EDGE_SLOP).
    pub const CAROUSEL_EDGE_SLOP: Pixels = px(1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn breakpoints_are_ordered_and_ranges_sane() {
        assert!(breakpoints::COMPACT < breakpoints::INSPECTOR);
        assert!(breakpoints::INSPECTOR < breakpoints::WIDE);
        assert!(detail::EPISODE_CARD_MIN < detail::EPISODE_CARD_MAX);
        assert!(detail::SIMILAR_MIN < detail::SIMILAR_MAX);
        assert!(detail::CAST_MIN < detail::CAST_MAX);
        assert!(detail::SEASON_TILE_MIN < detail::SEASON_TILE_MAX);
        assert!(browse::SEARCH_MIN_WIDTH < browse::SEARCH_WIDTH);
        assert!(player::VOLUME_TRACK < player::VOLUME_THUMB);
        assert_eq!(breakpoints::WIDE, px(1100.));
    }
}
