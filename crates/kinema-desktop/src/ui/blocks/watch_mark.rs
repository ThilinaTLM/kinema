//! `WatchMark`: the one watched glyph used by hero, season tiles and episodes.
//!
//! None is a muted ring, Partial a faint ring with an accent arc from 12
//! o'clock, All a filled accent circle with an `on_accent` check. Watched is
//! never green. The glyph is purely presentational; hosts supply the name.
use crate::{
    ui::primitives::icon::{Icon, IconName},
    ui::theme::{Palette, palette, radius},
};
use gpui::{prelude::*, *};
use kinema_core::watch_progress::{Tally, WatchedState};

/// What the mark shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WatchMarkState {
    None,
    /// Fraction in `(0, 1)`; use [`WatchMarkState::from_fraction`] to normalise.
    Partial(f32),
    All,
}

impl WatchMarkState {
    /// Normalise a progress fraction: non-finite or ≤ 0 is `None`, ≥ 1 is `All`.
    pub fn from_fraction(fraction: f32) -> Self {
        if !fraction.is_finite() || fraction <= 0. {
            Self::None
        } else if fraction >= 1. {
            Self::All
        } else {
            Self::Partial(fraction)
        }
    }

    /// An empty tally (no episodes) reads as `None`, never as complete.
    pub fn from_tally(tally: Tally) -> Self {
        match tally.state() {
            Some(WatchedState::All) => Self::All,
            Some(WatchedState::Partial) => Self::from_fraction(tally.fraction()),
            Some(WatchedState::None) | None => Self::None,
        }
    }

    /// Complete / not complete for binary hosts (episode marks).
    pub fn from_watched(watched: bool) -> Self {
        if watched { Self::All } else { Self::None }
    }
}

/// Supported glyph sizes (px).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WatchMarkSize {
    #[default]
    S16,
    S18,
    S20,
}

impl WatchMarkSize {
    pub const fn px(self) -> f32 {
        match self {
            Self::S16 => 16.,
            Self::S18 => 18.,
            Self::S20 => 20.,
        }
    }
}

/// Derived geometry for one size, in px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkGeometry {
    pub size: f32,
    pub ring_stroke: f32,
    pub arc_stroke: f32,
    /// Radius of the stroked ring's centre line.
    pub radius: f32,
    /// Check icon edge inside the filled circle.
    pub check: f32,
}

pub fn geometry(size: WatchMarkSize) -> MarkGeometry {
    let size = size.px();
    let arc_stroke = 2.;
    MarkGeometry {
        size,
        ring_stroke: 1.5,
        arc_stroke,
        radius: size / 2. - arc_stroke / 2.,
        check: (size * 0.7).round(),
    }
}

/// Point on a circle at `fraction` of a turn clockwise from 12 o'clock.
pub fn arc_point(center: (f32, f32), radius: f32, fraction: f32) -> (f32, f32) {
    let theta = fraction.clamp(0., 1.) * std::f32::consts::TAU;
    (
        center.0 + radius * theta.sin(),
        center.1 - radius * theta.cos(),
    )
}

#[derive(IntoElement)]
pub struct WatchMark {
    state: WatchMarkState,
    size: WatchMarkSize,
    over_art: bool,
}

impl WatchMark {
    pub fn new(state: WatchMarkState) -> Self {
        Self {
            state,
            size: WatchMarkSize::S16,
            over_art: false,
        }
    }
    pub fn size(mut self, size: WatchMarkSize) -> Self {
        self.size = size;
        self
    }
    /// Draw with the dark palette, for the dark backing of overlay buttons.
    pub fn over_art(mut self, over_art: bool) -> Self {
        self.over_art = over_art;
        self
    }
}

fn ring(fraction: f32, track: Hsla, arc: Hsla, g: MarkGeometry) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let c = bounds.center();
            let center = (f32::from(c.x), f32::from(c.y));
            let r = px(g.radius);
            let circle = |stroke: f32, color: Hsla, window: &mut Window| {
                let mut b = PathBuilder::stroke(px(stroke));
                let (top, bottom) = (point(c.x, c.y - r), point(c.x, c.y + r));
                b.move_to(top);
                b.arc_to(point(r, r), px(0.), false, true, bottom);
                b.arc_to(point(r, r), px(0.), false, true, top);
                b.close();
                if let Ok(path) = b.build() {
                    window.paint_path(path, color);
                }
            };
            circle(
                if fraction > 0. {
                    g.arc_stroke
                } else {
                    g.ring_stroke
                },
                track,
                window,
            );
            if fraction > 0. {
                let (x, y) = arc_point(center, g.radius, fraction);
                let mut b = PathBuilder::stroke(px(g.arc_stroke));
                b.move_to(point(c.x, c.y - r));
                b.arc_to(
                    point(r, r),
                    px(0.),
                    fraction > 0.5,
                    true,
                    point(px(x), px(y)),
                );
                if let Ok(path) = b.build() {
                    window.paint_path(path, arc);
                }
            }
        },
    )
    .size_full()
}

impl RenderOnce for WatchMark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p: Palette = if self.over_art {
            Palette::dark()
        } else {
            *palette(cx)
        };
        let g = geometry(self.size);
        let frame = div().relative().flex_none().size(px(g.size));
        match self.state {
            WatchMarkState::All => frame
                .rounded(radius::FULL)
                .bg(p.primary)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(IconName::Check)
                        .size(px(g.check))
                        .color(p.on_accent),
                ),
            WatchMarkState::None => frame.child(ring(0., p.text_muted, p.primary, g)),
            WatchMarkState::Partial(f) => frame.child(ring(
                f.clamp(0.01, 0.99),
                p.text_muted.opacity(0.45),
                p.primary,
                g,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn fractions_normalise_to_states() {
        assert_eq!(WatchMarkState::from_fraction(0.), WatchMarkState::None);
        assert_eq!(
            WatchMarkState::from_fraction(f32::NAN),
            WatchMarkState::None
        );
        assert_eq!(WatchMarkState::from_fraction(-1.), WatchMarkState::None);
        assert_eq!(
            WatchMarkState::from_fraction(0.3),
            WatchMarkState::Partial(0.3)
        );
        assert_eq!(WatchMarkState::from_fraction(1.), WatchMarkState::All);
        assert_eq!(WatchMarkState::from_fraction(7.), WatchMarkState::All);
    }

    #[test]
    fn tallies_never_read_complete_without_episodes() {
        let t = |watched, total| WatchMarkState::from_tally(Tally { watched, total });
        assert_eq!(t(0, 0), WatchMarkState::None);
        assert_eq!(t(0, 10), WatchMarkState::None);
        assert_eq!(t(5, 10), WatchMarkState::Partial(0.5));
        assert_eq!(t(10, 10), WatchMarkState::All);
        assert_eq!(WatchMarkState::from_watched(true), WatchMarkState::All);
        assert_eq!(WatchMarkState::from_watched(false), WatchMarkState::None);
    }

    #[test]
    fn geometry_fits_every_size() {
        for size in [WatchMarkSize::S16, WatchMarkSize::S18, WatchMarkSize::S20] {
            let g = geometry(size);
            assert!(g.radius + g.arc_stroke / 2. <= g.size / 2. + f32::EPSILON);
            assert!(g.check < g.size);
        }
        assert_eq!(WatchMarkSize::S18.px(), 18.);
    }

    #[test]
    fn arc_starts_at_twelve_and_runs_clockwise() {
        let close =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
        assert!(close(arc_point((10., 10.), 8., 0.), (10., 2.)));
        assert!(close(arc_point((10., 10.), 8., 0.25), (18., 10.)));
        assert!(close(arc_point((10., 10.), 8., 0.5), (10., 18.)));
        assert!(close(arc_point((10., 10.), 8., 0.75), (2., 10.)));
    }
}
