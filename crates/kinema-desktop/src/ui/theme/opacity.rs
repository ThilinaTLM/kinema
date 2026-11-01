//! Named alpha / mix levels (0..=1). Gathered from the `.opacity(n)` literals and
//! module-local alpha constants in `ui/`; values are unchanged. Call sites
//! migrate in step 5, where nearby levels may be snapped together.

// --- State mixes (see `Palette::hover` & co) -----------------------------------
/// Neutral fill mixed toward `foreground` on hover.
pub const HOVER_MIX: f32 = 0.05;
/// Neutral fill mixed toward `foreground` while pressed.
pub const PRESSED_MIX: f32 = 0.10;
/// `primary` mixed toward `foreground` on hover.
pub const PRIMARY_HOVER_MIX: f32 = 0.15;
/// `primary` mixed toward black while pressed.
pub const PRIMARY_PRESSED_MIX: f32 = 0.20;

// --- Surfaces -------------------------------------------------------------------
/// Header glass fill: legible over bright art yet floating (was `GLASS_ALPHA`).
pub const GLASS: f32 = 0.78;
/// Hovered overlay-button fill over art (`overlay.rs`).
pub const OVERLAY_HOVER: f32 = 0.95;
/// Facts card surface over the hero backdrop (`facts.rs` 0.86).
pub const CARD_TRANSLUCENT: f32 = 0.86;
/// Soft list-row selected hover blend of `accent` (`list_row.rs` 0.5).
pub const SELECTED_HOVER: f32 = 0.5;
/// Status badge / alert wash over a surface (`badge.rs`, `notice.rs` 0.10).
pub const BADGE_WASH: f32 = 0.10;

// --- Edges ----------------------------------------------------------------------
/// Alert edge ink (`notice.rs` 0.28).
pub const ALERT_EDGE: f32 = 0.28;
/// Selected ("on") button edge of `primary` (`button.rs` 0.35).
pub const SELECTED_EDGE: f32 = 0.35;
/// Highlighted card edge of `primary` (`media_card.rs` ACCENT_EDGE_ALPHA).
pub const SELECTED_EDGE_STRONG: f32 = 0.55;
/// Destructive button edge (`button.rs` 0.4).
pub const DESTRUCTIVE_EDGE: f32 = 0.4;
/// Player slider track in `input` ink (`player_window.rs` 0.5).
pub const TRACK: f32 = 0.5;

// --- Ink ------------------------------------------------------------------------
/// Secondary ink on active segmented counts (`segmented.rs` 0.75).
pub const INK_SOFT: f32 = 0.75;
/// Slider thumb/knob ink (`player_window.rs` 0.6).
pub const INK_THUMB: f32 = 0.6;
/// Faint ring of an unwatched mark (`watch_mark.rs` 0.45).
pub const INK_FAINT: f32 = 0.45;
/// Refresh indicator ink (`resource_view.rs` 0.8).
pub const INK_REFRESH: f32 = 0.8;
/// Disabled over-art button (`overlay.rs` 0.6).
pub const DISABLED_OVER_ART: f32 = 0.6;
/// Scrollbar thumb at idle (`scroll_area.rs` IDLE_ALPHA).
pub const SCROLLBAR_IDLE: f32 = 0.35;

// --- Imagery --------------------------------------------------------------------
/// Darkening of a substitute image (`thumb.rs` FALLBACK_DIM).
pub const DIM_FALLBACK: f32 = 0.4;
/// Darkening of a substitute image while the real one loads (LOADING_DIM).
pub const DIM_LOADING: f32 = 0.55;
/// Extra darkening of a muted (e.g. watched) frame (MUTED_DIM).
pub const DIM_MUTED: f32 = 0.3;
/// Static sheen under reduced motion (`thumb.rs` STATIC_PULSE).
pub const STATIC_PULSE: f32 = 0.35;
/// Lowest alpha of the skeleton pulse (`skeleton.rs` PULSE_MIN).
pub const PULSE_MIN: f32 = 0.5;
/// Skeleton-card sheen at rest (`skeleton_card.rs` 0.35).
pub const SKELETON_SHEEN: f32 = 0.35;
/// Skeleton-card pulse floor and span (`skeleton_card.rs` 0.15 + 0.5 t).
pub const SKELETON_PULSE_MIN: f32 = 0.15;
pub const SKELETON_PULSE_SPAN: f32 = 0.5;
/// Poster-card scrim gradient peak (`poster_card.rs` 0.92).
pub const POSTER_SCRIM_PEAK: f32 = 0.92;
/// Player bottom text scrim (`player_window.rs` 0.7).
pub const PLAYER_SCRIM: f32 = 0.7;
/// Peak opacity of the main-window header scrim (`main_window.rs` SCRIM_ALPHA).
pub const HEADER_SCRIM: f32 = 0.92;
/// Hero veil and side scrim defaults (`media_hero.rs`).
pub const HERO_VEIL: f32 = 0.5;
pub const HERO_SIDE_SCRIM: f32 = 0.55;
/// Ambient artwork wash peak, dark/light (`ambient_background.rs`).
pub const AMBIENT_DARK: f32 = 0.14;
pub const AMBIENT_LIGHT: f32 = 0.07;
/// Secondary-color strength in the ambient wash.
pub const AMBIENT_SECONDARY: f32 = 0.45;
/// Artwork wash strength of `Palette::tinted`, dark/light.
pub const ART_WASH_DARK: f32 = 0.14;
pub const ART_WASH_LIGHT: f32 = 0.07;
/// Browse brand wash (amber, violet) per theme (`browse.rs`).
pub const BRAND_WASH_DARK: (f32, f32) = (0.16, 0.10);
pub const BRAND_WASH_LIGHT: (f32, f32) = (0.08, 0.05);
/// Initial fade-in floor of the main-window content (`main_window.rs` 0.5 + 0.5 t).
pub const CONTENT_FADE_MIN: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_valid_alphas() {
        for v in [
            HOVER_MIX,
            PRESSED_MIX,
            PRIMARY_HOVER_MIX,
            PRIMARY_PRESSED_MIX,
            GLASS,
            OVERLAY_HOVER,
            CARD_TRANSLUCENT,
            SELECTED_HOVER,
            BADGE_WASH,
            ALERT_EDGE,
            SELECTED_EDGE,
            SELECTED_EDGE_STRONG,
            DESTRUCTIVE_EDGE,
            TRACK,
            INK_SOFT,
            INK_THUMB,
            INK_FAINT,
            INK_REFRESH,
            DISABLED_OVER_ART,
            SCROLLBAR_IDLE,
            DIM_FALLBACK,
            DIM_LOADING,
            DIM_MUTED,
            STATIC_PULSE,
            PULSE_MIN,
            SKELETON_SHEEN,
            SKELETON_PULSE_MIN,
            POSTER_SCRIM_PEAK,
            PLAYER_SCRIM,
            HEADER_SCRIM,
            HERO_VEIL,
            HERO_SIDE_SCRIM,
            AMBIENT_DARK,
            AMBIENT_LIGHT,
            AMBIENT_SECONDARY,
            ART_WASH_DARK,
            ART_WASH_LIGHT,
            CONTENT_FADE_MIN,
        ] {
            assert!((0.0..=1.0).contains(&v));
        }
        const {
            assert!(HOVER_MIX < PRESSED_MIX && PRIMARY_HOVER_MIX < PRIMARY_PRESSED_MIX);
            assert!(SKELETON_PULSE_MIN + SKELETON_PULSE_SPAN <= 1.0);
        }
    }
}
