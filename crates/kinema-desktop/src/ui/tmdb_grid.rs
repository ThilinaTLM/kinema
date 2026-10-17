//! TMDB browse grid: pure layout and focus math plus the card adapter.
//!
//! The grid is virtualized by rows (`uniform_list`); every row holds `columns`
//! equally sized cards. All sizes are fixed, so hover/focus facts never move
//! the layout. [`card`] is the single place that draws a title; it is meant to
//! be replaced by the shared design-library `MediaCard` once that is published.

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

pub(crate) const CARD_WIDTH: f32 = 148.0;
pub(crate) const GAP: f32 = 12.0;
/// 2:3 poster.
pub(crate) const POSTER_HEIGHT: f32 = 222.0;
const CAPTION_HEIGHT: f32 = 40.0;
/// Height of one virtual grid row, including the gap below it.
pub(crate) const ROW_HEIGHT: f32 = POSTER_HEIGHT + CAPTION_HEIGHT + GAP;

/// Columns that fit `width` (the scrolling area), at least one.
pub(crate) fn columns(width: f32) -> usize {
    if !width.is_finite() || width <= CARD_WIDTH {
        return 1;
    }
    (((width + GAP) / (CARD_WIDTH + GAP)).floor() as usize).max(1)
}

pub(crate) fn row_count(len: usize, columns: usize) -> usize {
    len.div_ceil(columns.max(1))
}

pub(crate) fn row_of(index: usize, columns: usize) -> usize {
    index / columns.max(1)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// Where keyboard focus lands after `step`. No focus yet lands on the first
/// card; movement clamps at the edges (it never wraps or leaves the grid), and
/// Down from a partial last row lands on the last card.
pub(crate) fn step(focus: Option<usize>, step: Step, columns: usize, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let columns = columns.max(1);
    let Some(at) = focus.filter(|at| *at < len) else {
        return Some(0);
    };
    Some(match step {
        Step::Left => at.saturating_sub(1),
        Step::Right => (at + 1).min(len - 1),
        Step::Up if at >= columns => at - columns,
        Step::Up => at,
        Step::Down => (at + columns).min(len - 1),
    })
}

/// Whether Down was already on the last row (the caller may load more).
pub(crate) fn at_last_row(focus: Option<usize>, columns: usize, len: usize) -> bool {
    match (focus, len) {
        (Some(at), len) if len > 0 => row_of(at, columns) + 1 >= row_count(len, columns),
        _ => false,
    }
}

/// IMDb rating in tenths (`77` = 7.7). `None` is unknown and is shown as a
/// dash; a genuine `0` is shown as `0.0`.
pub(crate) fn imdb_label(tenths: Option<u16>) -> String {
    match tenths {
        Some(t) => format!("{}.{}", t / 10, t % 10),
        None => "–".to_owned(),
    }
}

/// What a card shows. Owned presentation data only; no I/O in render.
pub(crate) struct CardProps {
    pub title: SharedString,
    pub year: Option<SharedString>,
    pub overview: Option<SharedString>,
    pub imdb_tenths: Option<u16>,
    pub focused: bool,
}

/// One title card: a poster tile (placeholder until artwork exists), an IMDb
/// badge visible at rest, and an opaque facts panel inside the poster that
/// appears on hover or keyboard focus.
pub(crate) fn card(id: impl Into<ElementId>, props: CardProps, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (border, ring, surface, fg, muted) = (
        theme.border,
        theme.primary,
        theme.secondary,
        theme.foreground,
        theme.muted_foreground,
    );
    let group = SharedString::from("tmdb-card");
    let mut facts = div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .p_2()
        .flex()
        .flex_col()
        .gap_1()
        .bg(theme.popover)
        .text_size(px(12.0))
        .text_color(fg)
        .children(props.year.clone().map(|year| div().text_color(muted).child(year)))
        .child(
            div()
                .line_clamp(4)
                .child(props.overview.clone().unwrap_or_else(|| "No overview.".into())),
        );
    if !props.focused {
        facts = facts
            .invisible()
            .group_hover(group.clone(), |style| style.visible());
    }
    let poster = div()
        .relative()
        .w(px(CARD_WIDTH))
        .h(px(POSTER_HEIGHT))
        .flex_none()
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(if props.focused { ring } else { border })
        .bg(surface)
        .flex()
        .items_center()
        .justify_center()
        .p_2()
        .text_size(px(13.0))
        .text_color(muted)
        .child(div().line_clamp(3).text_center().child(props.title.clone()))
        .child(
            div()
                .absolute()
                .top_1()
                .right_1()
                .px_1()
                .rounded_sm()
                .bg(theme.popover)
                .text_size(px(11.0))
                .text_color(fg)
                .child(format!("IMDb {}", imdb_label(props.imdb_tenths))),
        )
        .child(facts);
    div()
        .id(id)
        .group(group)
        .w(px(CARD_WIDTH))
        .flex_none()
        .flex()
        .flex_col()
        .gap_1()
        .cursor_pointer()
        .child(poster)
        .child(
            div()
                .h(px(CAPTION_HEIGHT - 4.0))
                .overflow_hidden()
                .text_size(px(13.0))
                .text_color(fg)
                .line_clamp(2)
                .child(props.title),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn columns_fit_width_and_never_reach_zero() {
        assert_eq!(columns(0.0), 1);
        assert_eq!(columns(f32::NAN), 1);
        assert_eq!(columns(CARD_WIDTH), 1);
        assert_eq!(columns(2.0 * CARD_WIDTH + GAP), 2);
        assert_eq!(columns(2.0 * CARD_WIDTH + GAP - 1.0), 1);
        assert_eq!(columns(1200.0), 7);
    }

    #[test]
    fn rows_round_up() {
        assert_eq!(row_count(0, 5), 0);
        assert_eq!(row_count(5, 5), 1);
        assert_eq!(row_count(6, 5), 2);
        assert_eq!(row_count(6, 0), 6);
        assert_eq!(row_of(7, 5), 1);
    }

    #[test]
    fn focus_moves_clamp_and_stay_inside() {
        // 7 items, 3 columns: rows [0 1 2] [3 4 5] [6].
        assert_eq!(step(None, Step::Down, 3, 7), Some(0));
        assert_eq!(step(None, Step::Left, 3, 0), None);
        assert_eq!(step(Some(0), Step::Left, 3, 7), Some(0));
        assert_eq!(step(Some(6), Step::Right, 3, 7), Some(6));
        assert_eq!(step(Some(1), Step::Up, 3, 7), Some(1));
        assert_eq!(step(Some(4), Step::Up, 3, 7), Some(1));
        assert_eq!(step(Some(5), Step::Down, 3, 7), Some(6));
        assert_eq!(step(Some(6), Step::Down, 3, 7), Some(6));
        // A stale index after the list shrank restarts at the first card.
        assert_eq!(step(Some(40), Step::Right, 3, 7), Some(0));
    }

    #[test]
    fn last_row_detection_drives_load_more() {
        assert!(!at_last_row(None, 3, 7));
        assert!(!at_last_row(Some(2), 3, 7));
        assert!(at_last_row(Some(3), 3, 6));
        assert!(at_last_row(Some(6), 3, 7));
        assert!(!at_last_row(Some(0), 3, 0));
    }

    #[test]
    fn imdb_is_genuine_or_unknown_never_substituted() {
        assert_eq!(imdb_label(None), "–");
        assert_eq!(imdb_label(Some(0)), "0.0");
        assert_eq!(imdb_label(Some(77)), "7.7");
        assert_eq!(imdb_label(Some(100)), "10.0");
    }
}
