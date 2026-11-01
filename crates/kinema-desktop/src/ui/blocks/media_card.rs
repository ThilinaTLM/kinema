//! `MediaCard`: the raised, clipped container for media tiles.
//!
//! Raised fill, `MD` radius, `line_subtle` edge. On hover the card lifts 3 px,
//! gains the `card_hover` shadow and a brighter edge, driven by the same
//! [`CardHover`] tween as the poster card (reduced motion snaps). The card
//! fills its host, which owns identity, focus, clicks and context menus, so
//! the lift never moves the hit area.
use crate::{
    ui::motion::{self, CardHover, card_hover as hv},
    ui::theme::{self, palette, radius, space},
};
use gpui::{prelude::*, *};
use std::time::Instant;

/// Edge brightness at full lift, relative to the resting edge alpha.
pub const HOVER_EDGE_GAIN: f32 = 1.75;
/// Alpha of the accent edge on a highlighted (e.g. next-up) card.
pub const ACCENT_EDGE_ALPHA: f32 = 0.55;

/// Resting edge brightened by `lift` in `0..=1`.
pub fn lifted_edge(edge: Hsla, lift: f32) -> Hsla {
    let gain = 1. + (HOVER_EDGE_GAIN - 1.) * lift.clamp(0., 1.);
    Hsla {
        a: (edge.a * gain).min(1.),
        ..edge
    }
}

/// Soft shadow scaled by hover progress.
/// Shared by `PosterCard` and `MediaCard` so every hover lift casts the same
/// shadow (the Discover card's established strength).
pub fn lift_shadow(shadow: Vec<BoxShadow>, value: f32) -> Vec<BoxShadow> {
    shadow
        .into_iter()
        .map(|mut s| {
            s.color = s.color.opacity(s.color.a * value);
            s
        })
        .collect()
}

struct HoverState {
    hover: CardHover,
    origin: Instant,
    hovered: bool,
}

#[derive(IntoElement)]
pub struct MediaCard {
    id: ElementId,
    accent_edge: bool,
    children: Vec<AnyElement>,
}

impl MediaCard {
    /// `id` must be stable across renders: it keys the hover tween.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            accent_edge: false,
            children: Vec::new(),
        }
    }
    /// Accent edge, e.g. for the next-up episode.
    pub fn accent_edge(mut self, accent: bool) -> Self {
        self.accent_edge = accent;
        self
    }
}

impl ParentElement for MediaCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for MediaCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| HoverState {
            hover: CardHover::default(),
            origin: Instant::now(),
            hovered: false,
        });
        let reduced = cx.reduce_motion() || motion::reduced(cx);
        let (lift, animating) = state.update(cx, |s, _| {
            let now = s.origin.elapsed().as_secs_f32() * 1000.;
            s.hover.set_hovered(now, s.hovered, reduced);
            (s.hover.values(now).lift, !s.hover.settled(now))
        });
        if animating {
            window.request_animation_frame();
        }
        let edge = if self.accent_edge {
            p.accent.opacity(ACCENT_EDGE_ALPHA)
        } else {
            lifted_edge(p.line_subtle, lift)
        };
        div()
            .id(self.id)
            .size_full()
            .relative()
            .top(px(-hv::LIFT_PX * lift))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(radius::MD)
            .bg(p.bg_raised)
            .border(space::HAIRLINE)
            .border_color(edge)
            .shadow(lift_shadow(theme::elevation::card_hover(&p), lift))
            .on_hover(move |hovered, window, cx| {
                state.update(cx, |s, _| s.hovered = *hovered);
                window.refresh();
            })
            .children(self.children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Palette;
    use ::core::prelude::v1::test;

    #[test]
    fn edge_brightens_with_lift_and_stays_bounded() {
        let edge = Palette::dark().line_subtle;
        assert_eq!(lifted_edge(edge, 0.), edge);
        assert!(lifted_edge(edge, 1.).a > edge.a);
        assert!((lifted_edge(edge, 1.).a - edge.a * HOVER_EDGE_GAIN).abs() < 1e-6);
        assert_eq!(lifted_edge(edge, 9.).a, lifted_edge(edge, 1.).a);
        assert_eq!(lifted_edge(Hsla { a: 0.9, ..edge }, 1.).a, 1.);
    }

    #[test]
    fn shadow_fades_with_hover() {
        let shadow = theme::elevation::card_hover(&Palette::dark());
        let a = shadow[0].color.a;
        assert_eq!(lift_shadow(shadow.clone(), 0.)[0].color.a, 0.);
        // Matches the PosterCard lift exactly (alpha scaled by itself, then hover).
        assert!((lift_shadow(shadow, 1.)[0].color.a - a * a).abs() < 1e-6);
    }

    #[test]
    fn lift_distance_is_the_poster_card_lift() {
        assert_eq!(hv::LIFT_PX, 3.);
    }
}
