//! View metrics grouped per view. Values are the former module-local constants of
//! `ui/views/**`, `ui/blocks/**` and `ui/primitives/**`, unchanged.

/// Gap of poster grids and carousels.
pub const GRID_GAP: gpui::Pixels = super::space::S12;

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
    pub const OVERVIEW_MAX_WIDTH: Pixels = px(880.);
    pub const HERO_ASIDE_WIDTH: Pixels = px(320.);
    /// Fraction of the hero height the backdrop fade covers.
    pub const HERO_FADE_HEIGHT: f32 = 0.7;
    /// Fraction of the hero width the side scrim reaches.
    pub const HERO_SIDE_SCRIM_REACH: f32 = 0.7;
    /// Poster and glance panel scale with the window, within these bounds.
    pub const HERO_POSTER_FRACTION: f32 = 0.14;
    pub const HERO_POSTER_MIN: Pixels = px(160.);
    pub const HERO_POSTER_MAX: Pixels = px(240.);
    pub const GLANCE_FRACTION: f32 = 0.24;
    pub const GLANCE_MIN: Pixels = px(280.);
    pub const GLANCE_MAX: Pixels = px(380.);
    pub const GLANCE_RATINGS_WIDTH: Pixels = px(300.);
    pub const FACT_GLANCE_LABEL_WIDTH: Pixels = px(84.);
    pub const FACT_WIDE_LABEL_WIDTH: Pixels = px(112.);
    /// Streams inspector: a fraction of the frame within bounds.
    pub const STREAMS_INSPECTOR_FRACTION: f32 = 0.27;
    pub const STREAMS_INSPECTOR_MIN: Pixels = px(340.);
    pub const STREAMS_INSPECTOR_MAX: Pixels = px(400.);
    pub const STREAMS_OVERVIEW_MAX_WIDTH: Pixels = px(920.);
    pub const STREAMS_SEARCH_WIDTH: Pixels = px(240.);
    pub const STREAMS_SEARCH_WIDTH_NARROW: Pixels = px(180.);
    pub const STREAMS_BANNER_WIDTH: Pixels = px(260.);
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
    /// Fraction of the header scrim ramp that stays at full strength.
    pub const SCRIM_HOLD: f32 = 0.6;
}

pub mod settings {
    use gpui::{Pixels, px};
    pub const WINDOW_WIDTH: Pixels = px(860.);
    pub const WINDOW_HEIGHT: Pixels = px(600.);
    pub const MIN_WIDTH: Pixels = px(720.);
    pub const MIN_HEIGHT: Pixels = px(480.);
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
    pub const WINDOW_WIDTH: Pixels = px(1100.);
    pub const WINDOW_HEIGHT: Pixels = px(680.);
    pub const MIN_WIDTH: Pixels = px(720.);
    pub const MIN_HEIGHT: Pixels = px(440.);
    /// Timeline track thickness and its thumb at rest / while active.
    pub const TIMELINE_TRACK: Pixels = px(12.);
    pub const TIMELINE_THUMB: Pixels = px(14.);
    pub const TIMELINE_THUMB_ACTIVE: Pixels = px(18.);
    pub const HELP_HEADER_ICON: Pixels = px(18.);
    pub const HELP_ROW_MIN_HEIGHT: Pixels = px(24.);
    pub const HELP_MOUSE_LABEL_MAX: Pixels = px(132.);
    pub const DIAGNOSTICS_TOP: Pixels = px(72.);
    pub const DIAGNOSTICS_MAX_WIDTH: Pixels = px(360.);
}

/// Primitive-level metrics that are not component sizes.
pub mod controls {
    use gpui::{Pixels, px};
    /// Choice tile width (`choice_card.rs` TILE_WIDTH).
    pub const CHOICE_TILE_WIDTH: Pixels = px(196.);
    /// Carousel edge slop (`carousel.rs` EDGE_SLOP).
    pub const CAROUSEL_EDGE_SLOP: Pixels = px(1.0);
    /// Scrollbar hit area while hovered (rest is `space::S6`).
    pub const SCROLLBAR_HOVER_WIDTH: Pixels = px(10.);
    /// Minimum width of a menu panel.
    pub const MENU_MIN_WIDTH: Pixels = px(200.);
    /// Active-tab underline: distance from the bottom edge and thickness.
    pub const INDICATOR_INSET: Pixels = px(3.);
    pub const INDICATOR_THICKNESS: Pixels = px(2.);
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
