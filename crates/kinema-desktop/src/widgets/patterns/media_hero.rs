//! Cinematic detail header: blurred backdrop fading into the canvas, poster,
//! title, meta chips, clamped overview and action slots.
//!
//! With a backdrop the hero is a dark banner in either theme. Its own meta
//! [`Badge`]s and More toggle switch to over-art styling automatically; callers
//! should pass `.over_art(true)` to `Button`/`IconButton`/`Badge` slots
//! (`primary`, `secondary`, `meta`) whenever a backdrop is present.
use std::{sync::Arc, time::Instant};

use gpui::{prelude::*, *};

use super::Badge;

use crate::{
    artwork::ArtworkState,
    widgets::{
        IconName, TypeExt,
        button::{Button, ButtonVariant},
        motion::{self, Motion},
        tokens::{Mode, Palette, elevation, palette, radius, space, theme, typeface},
    },
};

/// Minimum hero height; content grows it when taller.
const MIN_HEIGHT: Pixels = px(340.0);
/// Poster frame width; screens that draw their own poster share this geometry.
pub const POSTER_WIDTH: Pixels = px(168.0);
/// Poster height / width (2:3).
pub const POSTER_ASPECT: f32 = 1.5;
const TEXT_MAX_WIDTH: Pixels = px(720.0);
/// Overview lines shown before expansion.
const OVERVIEW_LINES: usize = 3;
/// Overviews at most this long never need an expand toggle.
const OVERVIEW_COLLAPSE_CHARS: usize = 220;
/// Share of the hero (from the bottom) that fades into the canvas.
const FADE_HEIGHT: f32 = 0.95;

enum MetaItem {
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
    backdrop: Option<Arc<RenderImage>>,
    poster: Option<AnyElement>,
    meta: Vec<MetaItem>,
    overview: Option<SharedString>,
    primary: Option<AnyElement>,
    secondary: Vec<AnyElement>,
}

impl MediaHero {
    /// `id` must be stable across renders: it keys the overview expansion.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            backdrop: None,
            poster: None,
            meta: Vec::new(),
            overview: None,
            primary: None,
            secondary: Vec::new(),
        }
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
    pub fn overview(mut self, overview: impl Into<SharedString>) -> Self {
        self.overview = Some(overview.into());
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

/// Short overviews fit the clamp, so they get no toggle.
fn overview_collapsible(text: &str) -> bool {
    text.chars().count() > OVERVIEW_COLLAPSE_CHARS
}

impl RenderOnce for MediaHero {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let expanded = window.use_keyed_state((self.id.clone(), "expanded"), cx, |_, _| false);
        let is_expanded = *expanded.read(cx);
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
        // A light canvas under a dark backdrop would leave dark ink over dark
        // art mid-fade, so light mode keeps a dark banner with light ink instead.
        let banner = has_backdrop && theme(cx).mode == Mode::Light;
        let (ink, fade_to) = if banner {
            (Palette::dark(), p.scrim_max)
        } else {
            (p, p.bg_canvas)
        };

        let poster = self.poster.map(|poster| {
            div()
                .flex_none()
                .w(POSTER_WIDTH)
                .h(POSTER_WIDTH * POSTER_ASPECT)
                .rounded(radius::MD)
                .overflow_hidden()
                .bg(p.bg_placeholder)
                .border(space::HAIRLINE)
                .border_color(p.poster_edge)
                .shadow(elevation::overlay(&p))
                .child(div().size_full().child(poster))
        });

        let overview = self.overview.map(|text| {
            let collapsible = overview_collapsible(&text);
            let toggle = collapsible.then(|| {
                let expanded = expanded.clone();
                let (label, icon) = if is_expanded {
                    ("Less", IconName::ChevronDown)
                } else {
                    ("More", IconName::ChevronRight)
                };
                Button::new((self.id.clone(), "overview-toggle"), label)
                    .variant(ButtonVariant::Ghost)
                    .icon(icon)
                    .over_art(has_backdrop)
                    .on_click(move |_, _, cx| {
                        expanded.update(cx, |state, cx| {
                            *state = !*state;
                            cx.notify();
                        })
                    })
            });
            div()
                .flex()
                .flex_col()
                .gap(space::S4)
                .w_full()
                .max_w(TEXT_MAX_WIDTH)
                .child(
                    div()
                        .type_style(typeface::BODY_LONG)
                        .text_color(ink.text_secondary)
                        .when(!(collapsible && is_expanded), |el| {
                            el.line_clamp(OVERVIEW_LINES)
                        })
                        .child(text),
                )
                .children(toggle.map(|toggle| div().flex().child(toggle)))
        });

        let has_actions = self.primary.is_some() || !self.secondary.is_empty();
        div()
            .id(self.id)
            .relative()
            .w_full()
            .min_h(MIN_HEIGHT)
            .flex()
            .flex_col()
            .justify_end()
            .overflow_hidden()
            .bg(p.bg_canvas)
            .children(backdrop.into_iter().flatten())
            .when(has_backdrop, |el| {
                el.child(div().absolute().inset_0().bg(p.backdrop_scrim))
            })
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(relative(FADE_HEIGHT))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(fade_to.opacity(0.0), 0.),
                        linear_color_stop(fade_to, 1.),
                    )),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .items_end()
                    .gap(space::S24)
                    .px(space::GRID_INSET)
                    .pt(space::S48)
                    .pb(space::S20)
                    .children(poster)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(space::S8)
                            .child(
                                div()
                                    .type_style(typeface::DISPLAY)
                                    .text_color(ink.text_primary)
                                    .line_clamp(2)
                                    .child(self.title),
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
                                        .children(self.meta.into_iter().map(|item| match item {
                                            MetaItem::Element(el) => el,
                                            MetaItem::Badge(badge) => {
                                                badge.over_art(has_backdrop).into_any_element()
                                            }
                                        })),
                                )
                            })
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
    }
}

#[cfg(test)]
mod tests {
    use super::{OVERVIEW_COLLAPSE_CHARS, overview_collapsible};

    #[test]
    fn only_long_overviews_get_a_toggle() {
        assert!(!overview_collapsible(""));
        assert!(!overview_collapsible(&"a".repeat(OVERVIEW_COLLAPSE_CHARS)));
        assert!(overview_collapsible(
            &"a".repeat(OVERVIEW_COLLAPSE_CHARS + 1)
        ));
        // Counts characters, not bytes.
        assert!(!overview_collapsible(&"é".repeat(OVERVIEW_COLLAPSE_CHARS)));
    }
}
