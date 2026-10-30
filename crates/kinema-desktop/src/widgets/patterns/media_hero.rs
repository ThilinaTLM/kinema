//! Cinematic detail header: blurred backdrop fading into the canvas, poster,
//! title, meta chips, overview and action slots.
//!
//! The art melts into the canvas through one long gradient (stacked twice in
//! light mode so dark ink always sits on a mostly-canvas surface). Dark art
//! stays behind the controls only in dark mode, so over-art styling follows
//! [`uses_over_art`]: the hero applies it to its own badges;
//! callers pass the same value to `Button`/`IconButton`/`Badge` slots.
use std::{sync::Arc, time::Instant};

use gpui::{prelude::*, *};

use super::{Badge, SkeletonText};

use crate::{
    artwork::ArtworkState,
    widgets::{
        TypeExt,
        motion::{self, Motion},
        tokens::{Mode, elevation, palette, radius, space, theme, typeface},
    },
};

/// Minimum hero height; content grows it when taller.
const MIN_HEIGHT: Pixels = px(340.0);
/// Footprint of the overview placeholder shown while the text loads.
const OVERVIEW_SKELETON_WIDTH: Pixels = px(480.0);
/// Poster frame width; screens that draw their own poster share this geometry.
pub const POSTER_WIDTH: Pixels = px(168.0);
/// Poster height / width (2:3).
pub const POSTER_ASPECT: f32 = 1.5;
const TEXT_MAX_WIDTH: Pixels = px(640.0);
/// Lines of the overview placeholder shown while the text loads.
const OVERVIEW_SKELETON_LINES: usize = 3;
/// Safety clamp for pathologically long overviews; normal ones (3-5 lines)
/// show in full.
const OVERVIEW_MAX_LINES: usize = 8;
/// Share of the hero height over which the art melts into the canvas.
const FADE_HEIGHT: f32 = 0.7;
/// Default strength of the even veil over the backdrop (times `backdrop_scrim`).
const DEFAULT_VEIL: f32 = 0.5;
/// Default opacity of the leading-edge scrim that keeps the title legible.
const DEFAULT_SIDE_SCRIM: f32 = 0.55;
/// Where the leading-edge scrim has fully faded out.
const SIDE_SCRIM_REACH: f32 = 0.7;

/// Whether controls over a backdrop should use dark over-art styling: only
/// when dark art really sits behind them (dark mode with a backdrop).
pub fn uses_over_art(has_backdrop: bool, mode: Mode) -> bool {
    has_backdrop && mode == Mode::Dark
}

pub enum MetaItem {
    Element(AnyElement),
    /// Restyled over artwork when the hero has a backdrop.
    Badge(Badge),
}

/// Backdrop crossfade bookkeeping, kept across renders.
#[derive(Default)]
struct BackdropState {
    current: Option<Arc<RenderImage>>,
    previous: Option<Arc<RenderImage>>,
    changed: Option<Instant>,
}

#[derive(IntoElement)]
pub struct MediaHero {
    id: ElementId,
    title: SharedString,
    eyebrow: Option<SharedString>,
    backdrop: Option<Arc<RenderImage>>,
    poster: Option<AnyElement>,
    meta: Vec<MetaItem>,
    overview: Option<SharedString>,
    tagline: Option<SharedString>,
    aside: Option<AnyElement>,
    aside_width: Pixels,
    stack_aside: bool,
    title_action: Option<AnyElement>,
    primary: Option<AnyElement>,
    secondary: Vec<AnyElement>,
    min_height: Pixels,
    fill: bool,
    fade_into: Option<Hsla>,
    veil: f32,
    side_scrim: f32,
    overview_loading: bool,
    gutter: Pixels,
    top_inset: Pixels,
    max_width: Option<Pixels>,
    poster_width: Pixels,
    text_max_width: Pixels,
}

impl MediaHero {
    /// `id` must be stable across renders: it keys the overview expansion.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            eyebrow: None,
            backdrop: None,
            poster: None,
            meta: Vec::new(),
            overview: None,
            tagline: None,
            aside: None,
            aside_width: px(320.0),
            stack_aside: false,
            title_action: None,
            primary: None,
            secondary: Vec::new(),
            min_height: MIN_HEIGHT,
            fill: false,
            fade_into: None,
            veil: DEFAULT_VEIL,
            side_scrim: DEFAULT_SIDE_SCRIM,
            overview_loading: false,
            gutter: space::GRID_INSET,
            top_inset: px(0.0),
            max_width: None,
            poster_width: POSTER_WIDTH,
            text_max_width: TEXT_MAX_WIDTH,
        }
    }
    /// Small uppercase context line above the title (e.g. "Choose a stream").
    pub fn eyebrow(mut self, eyebrow: impl Into<SharedString>) -> Self {
        self.eyebrow = Some(eyebrow.into());
        self
    }
    /// Blurred, darkened image from `artwork::backdrop*`.
    pub fn backdrop(mut self, image: Option<Arc<RenderImage>>) -> Self {
        self.backdrop = image;
        self
    }
    /// Convenience for `artwork::backdrop_or_request`: only `Ready` shows.
    pub fn backdrop_state(self, state: ArtworkState) -> Self {
        self.backdrop(match state {
            ArtworkState::Ready(image) => Some(image),
            _ => None,
        })
    }
    /// Filled by the caller (usually an `img` with cover fit); the hero
    /// supplies the 2:3 frame, radius and elevation.
    pub fn poster(mut self, poster: impl IntoElement) -> Self {
        self.poster = Some(poster.into_any_element());
        self
    }
    /// One chip/badge/text in the meta line.
    pub fn meta(mut self, item: impl IntoElement) -> Self {
        self.meta.push(MetaItem::Element(item.into_any_element()));
        self
    }
    /// A pre-built meta item (plain element or over-art-aware badge).
    pub fn meta_item(mut self, item: MetaItem) -> Self {
        self.meta.push(item);
        self
    }
    pub fn metas(mut self, items: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.meta.extend(
            items
                .into_iter()
                .map(|item| MetaItem::Element(item.into_any_element())),
        );
        self
    }
    /// A badge in the meta line; it gets over-art styling when a backdrop is set.
    pub fn meta_badge(mut self, badge: Badge) -> Self {
        self.meta.push(MetaItem::Badge(badge));
        self
    }
    /// Minimum hero height (default 340 px); content grows it when taller.
    pub fn min_height(mut self, min_height: Pixels) -> Self {
        self.min_height = min_height;
        self
    }
    /// Grow into spare space of a flex-column parent (with `min_height` as the
    /// floor), so the hero absorbs whatever the siblings leave empty.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }
    /// Canvas color the art melts into (default: theme canvas); pass the
    /// artwork-tinted canvas so the seam is invisible.
    pub fn fade_into(mut self, canvas: Hsla) -> Self {
        self.fade_into = Some(canvas);
        self
    }
    /// Veil strength over the backdrop (0 = none, 1 = full `backdrop_scrim`)
    /// and opacity of the leading-edge scrim behind the text.
    pub fn scrims(mut self, veil: f32, side: f32) -> Self {
        self.veil = veil.clamp(0.0, 1.0);
        self.side_scrim = side.clamp(0.0, 1.0);
        self
    }
    /// Show an overview-shaped skeleton while the text is still loading.
    pub fn overview_loading(mut self, loading: bool) -> Self {
        self.overview_loading = loading;
        self
    }
    /// Reserve space at the top for a floating header overlaying the hero;
    /// the backdrop still extends to the top edge underneath it.
    pub fn top_inset(mut self, inset: Pixels) -> Self {
        self.top_inset = inset;
        self
    }
    /// Align the content with a page frame: side gutter and a maximum content
    /// width (the backdrop itself stays full-bleed).
    pub fn frame(mut self, gutter: Pixels, max_width: Pixels) -> Self {
        self.gutter = gutter;
        self.max_width = Some(max_width);
        self
    }
    /// Poster frame width and overview measure, scaled by the caller per
    /// window size.
    pub fn sizing(mut self, poster_width: Pixels, text_max_width: Pixels) -> Self {
        self.poster_width = poster_width;
        self.text_max_width = text_max_width;
        self
    }
    pub fn overview(mut self, overview: impl Into<SharedString>) -> Self {
        self.overview = Some(overview.into());
        self
    }
    /// Italic one-liner between the meta line and the overview.
    pub fn tagline(mut self, tagline: impl Into<SharedString>) -> Self {
        self.tagline = Some(tagline.into());
        self
    }
    /// Raised side panel (e.g. "At a glance") to the right of the story. With
    /// `stacked` it sits under the story at full width instead.
    pub fn aside(mut self, aside: impl IntoElement, width: Pixels, stacked: bool) -> Self {
        self.aside = Some(aside.into_any_element());
        self.aside_width = width;
        self.stack_aside = stacked;
        self
    }
    /// Small control right next to the title (e.g. an icon-only copy button).
    pub fn title_action(mut self, action: impl IntoElement) -> Self {
        self.title_action = Some(action.into_any_element());
        self
    }
    pub fn primary(mut self, action: impl IntoElement) -> Self {
        self.primary = Some(action.into_any_element());
        self
    }
    pub fn secondary(mut self, action: impl IntoElement) -> Self {
        self.secondary.push(action.into_any_element());
        self
    }
}

impl RenderOnce for MediaHero {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let fade = motion::duration(Motion::Crossfade, cx);

        // Remember the previous image so a change crossfades image-to-image.
        let state = window.use_keyed_state((self.id.clone(), "backdrop"), cx, |_, _| {
            BackdropState::default()
        });
        let now = Instant::now();
        state.update(cx, |s, _| {
            let same = match (&s.current, &self.backdrop) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                s.previous = s.current.take().filter(|_| self.backdrop.is_some());
                s.current = self.backdrop.clone();
                s.changed = Some(now);
            } else if s.previous.is_some() && s.changed.is_none_or(|at| now - at >= fade) {
                s.previous = None;
            }
        });
        let previous = state.read(cx).previous.clone();
        let image_layer = |image: Arc<RenderImage>| {
            div()
                .absolute()
                .inset_0()
                .child(img(image).object_fit(ObjectFit::Cover).size_full())
        };
        let backdrop = self.backdrop.clone().map(|image| {
            let key = Arc::as_ptr(&image) as *const () as usize;
            let layer = image_layer(image);
            let layer = if fade.is_zero() {
                layer.into_any_element()
            } else {
                layer
                    .with_animation(
                        ("hero-backdrop", key),
                        Animation::new(fade).with_easing(motion::ease_out_cubic),
                        |el, t| el.opacity(t),
                    )
                    .into_any_element()
            };
            (!fade.is_zero())
                .then_some(previous)
                .flatten()
                .map(image_layer)
                .into_iter()
                .map(IntoElement::into_any_element)
                .chain([layer])
                .collect::<Vec<_>>()
        });
        let has_backdrop = backdrop.is_some();
        let mode = theme(cx).mode;
        let over_art = uses_over_art(has_backdrop, mode);
        let canvas = self.fade_into.unwrap_or(p.bg_canvas);
        let ink = p;
        let ramp = |fraction: f32| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(relative(fraction))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(canvas.opacity(0.0), 0.),
                    linear_color_stop(canvas, 1.),
                ))
        };

        let poster = self.poster.map(|poster| {
            div()
                .flex_none()
                .w(self.poster_width)
                .h(self.poster_width * POSTER_ASPECT)
                .rounded(radius::MD)
                .overflow_hidden()
                .bg(p.bg_placeholder)
                .border(space::HAIRLINE)
                .border_color(p.poster_edge)
                .shadow(elevation::overlay(&p))
                .child(div().size_full().child(poster))
        });

        let loading = self.overview_loading && self.overview.is_none();
        let overview = self.overview.map(|text| {
            div()
                .w_full()
                .min_w_0()
                .max_w(self.text_max_width)
                .type_style(typeface::BODY_LONG)
                .text_color(ink.text_secondary)
                .clamp_lines(OVERVIEW_MAX_LINES)
                .child(text)
        });

        let overview = overview.or_else(|| {
            loading.then(|| {
                div()
                    .w(OVERVIEW_SKELETON_WIDTH)
                    .max_w_full()
                    .child(SkeletonText::new(OVERVIEW_SKELETON_LINES))
            })
        });
        let has_actions = self.primary.is_some() || !self.secondary.is_empty();
        div()
            .id(self.id)
            .relative()
            .w_full()
            .min_h(self.min_height)
            .when(self.fill, |el| el.flex_grow_1().flex_shrink_0())
            .flex()
            .flex_col()
            .justify_end()
            .overflow_hidden()
            .bg(canvas)
            .children(backdrop.into_iter().flatten())
            // Even veil keeps the art readable but present; the side scrim
            // protects the title.
            .when(has_backdrop, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(p.backdrop_scrim.opacity(self.veil)),
                )
                .child(div().absolute().inset_0().bg(linear_gradient(
                    90.,
                    linear_color_stop(canvas.opacity(self.side_scrim), 0.),
                    linear_color_stop(canvas.opacity(0.0), SIDE_SCRIM_REACH),
                )))
                .child(ramp(FADE_HEIGHT))
            })
            .when(has_backdrop && mode == Mode::Light, |el| {
                el.child(ramp(FADE_HEIGHT))
            })
            .child(
                div().relative().w_full().flex().justify_center().child(
                    div()
                        .relative()
                        .w_full()
                        .when_some(self.max_width, |el, w| el.max_w(w))
                        .flex()
                        .gap(space::S24)
                        .px(self.gutter)
                        .pt(space::S24 + self.top_inset)
                        .pb(space::S24)
                        .when(self.stack_aside, |el| el.flex_col())
                        .when(!self.stack_aside, |el| el.items_end())
                        .child(
                            div()
                                .flex()
                                .items_end()
                                .gap(space::S24)
                                .flex_1()
                                .min_w_0()
                                .children(poster)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(space::S8)
                                        .children(self.eyebrow.map(|eyebrow| {
                                            div()
                                                .mb(-space::S4)
                                                .type_style(typeface::CAPTION)
                                                .text_color(ink.text_muted)
                                                .child(eyebrow.to_uppercase())
                                        }))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(space::S8)
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .type_style(typeface::DISPLAY)
                                                        .text_color(ink.text_primary)
                                                        .clamp_lines(2)
                                                        .child(self.title),
                                                )
                                                .children(
                                                    self.title_action
                                                        .map(|a| div().flex_none().child(a)),
                                                ),
                                        )
                                        .when(!self.meta.is_empty(), |el| {
                                            el.child(
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .items_center()
                                                    .gap(space::S8)
                                                    .text_color(ink.text_secondary)
                                                    .type_style(typeface::SMALL_MEDIUM)
                                                    .children(self.meta.into_iter().map(|item| {
                                                        match item {
                                                            MetaItem::Element(el) => el,
                                                            MetaItem::Badge(badge) => badge
                                                                .over_art(over_art)
                                                                .into_any_element(),
                                                        }
                                                    })),
                                            )
                                        })
                                        .children(self.tagline.map(|text| {
                                            div()
                                                .w_full()
                                                .min_w_0()
                                                .max_w(self.text_max_width)
                                                .type_style(typeface::BODY)
                                                .italic()
                                                .text_color(ink.text_secondary)
                                                .child(text)
                                        }))
                                        .children(overview)
                                        .when(has_actions, |el| {
                                            el.child(
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .items_center()
                                                    .gap(space::S8)
                                                    .pt(space::S4)
                                                    .children(self.primary)
                                                    .children(self.secondary),
                                            )
                                        }),
                                ),
                        )
                        .children(self.aside.map(|aside| {
                            div()
                                .flex_none()
                                .when(!self.stack_aside, |el| el.w(self.aside_width))
                                .when(self.stack_aside, |el| el.w_full())
                                .child(aside)
                        })),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::uses_over_art;
    use crate::widgets::tokens::Mode;

    #[test]
    fn over_art_only_for_dark_mode_with_a_backdrop() {
        assert!(uses_over_art(true, Mode::Dark));
        assert!(!uses_over_art(true, Mode::Light));
        assert!(!uses_over_art(false, Mode::Dark));
    }
}
