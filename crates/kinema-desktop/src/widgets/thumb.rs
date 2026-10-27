//! `Thumb`: an aspect-ratio media frame with a radius and four artwork states.
//!
//! The widget is presentation only. Hosts resolve artwork elsewhere and pass a
//! [`ThumbState`]; [`ThumbState::layers`] is the pure rule for what is drawn.
//! GPUI has no image blur, so a Loading frame with a fallback shows the
//! fallback dimmed under the pulsing skeleton. Reduced motion makes the pulse
//! static. Overlays (badges, buttons) are added with [`Thumb::overlay`].
use super::{
    motion,
    text::TypeExt,
    tokens::{Palette, palette, space, typeface},
};
use gpui::{prelude::*, *};
use std::{sync::Arc, time::Duration};

const PULSE: Duration = Duration::from_millis(1600);

/// What a thumb shows. Generic over the image handle so the rules are testable.
#[derive(Clone, Debug, PartialEq)]
pub enum ThumbState<T = Arc<RenderImage>> {
    /// The requested artwork.
    Ready(T),
    /// Waiting for artwork; an already-decoded substitute may sit underneath.
    Loading { fallback: Option<T> },
    /// A substitute for artwork that is not available.
    Fallback(T),
    /// No artwork anywhere: a placeholder with this label (may be empty).
    Missing(SharedString),
    /// Nothing yet and nothing to wait for visibly (placeholder only), e.g. a
    /// load that has not been pending long enough to deserve a skeleton.
    Blank,
}

/// How strongly an image is darkened (0 = untouched).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dim(pub f32);

/// The layers of a frame, bottom to top: image, dim, skeleton, label.
#[derive(Clone, Debug, PartialEq)]
pub struct ThumbLayers<'a, T> {
    pub image: Option<&'a T>,
    pub dim: Dim,
    pub skeleton: bool,
    pub label: Option<&'a str>,
}

/// Darkening applied to a substitute image.
pub const FALLBACK_DIM: f32 = 0.4;
/// Darkening applied to a substitute image while the real one loads.
pub const LOADING_DIM: f32 = 0.55;
/// Extra darkening for a muted (e.g. watched) frame.
pub const MUTED_DIM: f32 = 0.3;

impl<T> ThumbState<T> {
    pub fn layers(&self) -> ThumbLayers<'_, T> {
        let (image, dim, skeleton, label) = match self {
            Self::Ready(image) => (Some(image), 0., false, None),
            Self::Loading { fallback } => (fallback.as_ref(), LOADING_DIM, true, None),
            Self::Fallback(image) => (Some(image), FALLBACK_DIM, false, None),
            Self::Missing(label) => (
                None,
                0.,
                false,
                Some(label.as_ref()).filter(|l| !l.is_empty()),
            ),
            Self::Blank => (None, 0., false, None),
        };
        ThumbLayers {
            image,
            dim: Dim(dim),
            skeleton,
            label,
        }
    }
}

/// Pulse strength for phase `t` in `0..=1`: a smooth triangle wave.
pub fn pulse_opacity(t: f32) -> f32 {
    let w = 1. - (2. * t.clamp(0., 1.) - 1.).abs();
    0.15 + 0.5 * w * w * (3. - 2. * w)
}

/// Opacity of the skeleton sheen when motion is reduced.
pub const STATIC_PULSE: f32 = 0.35;

#[derive(IntoElement)]
pub struct Thumb {
    state: ThumbState,
    ratio: f32,
    radius: Pixels,
    width: Option<Pixels>,
    muted: bool,
    overlays: Vec<AnyElement>,
}

impl Thumb {
    /// `ratio` is width / height, e.g. `16. / 9.` or `2. / 3.`.
    pub fn new(state: ThumbState, ratio: f32, radius: Pixels) -> Self {
        Self {
            state,
            ratio,
            radius,
            width: None,
            muted: false,
            overlays: Vec::new(),
        }
    }
    /// Fixed width; otherwise the frame fills its parent.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }
    /// Desaturate and darken the image (watched episodes).
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }
    /// An element placed over the artwork; position it absolutely.
    pub fn overlay(mut self, overlay: impl IntoElement) -> Self {
        self.overlays.push(overlay.into_any_element());
        self
    }
}

impl RenderOnce for Thumb {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let ink = Palette::dark();
        let layers = self.state.layers();
        let dim = (layers.dim.0
            + if self.muted && layers.image.is_some() {
                MUTED_DIM
            } else {
                0.
            })
        .min(0.85);
        let mut frame = div()
            .relative()
            .flex_none()
            .overflow_hidden()
            .rounded(self.radius)
            .bg(p.bg_placeholder)
            .aspect_ratio(self.ratio);
        frame = match self.width {
            Some(width) => frame.w(width),
            None => frame.w_full(),
        };
        if let Some(image) = layers.image {
            let image = img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .grayscale(self.muted);
            frame = frame.child(
                div()
                    .absolute()
                    .inset_0()
                    .size_full()
                    .overflow_hidden()
                    .child(image),
            );
        }
        if dim > 0. {
            frame = frame.child(div().absolute().inset_0().bg(ink.bg_canvas.opacity(dim)));
        }
        if layers.skeleton {
            let sheen = div().absolute().inset_0().bg(p.ctl_hover);
            frame = frame.child(if motion::reduced(cx) {
                sheen.opacity(STATIC_PULSE).into_any_element()
            } else {
                sheen
                    .with_animation("thumb-pulse", Animation::new(PULSE).repeat(), |el, t| {
                        el.opacity(pulse_opacity(t))
                    })
                    .into_any_element()
            });
        }
        if let Some(label) = layers.label {
            frame = frame.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(space::S8)
                    .type_style(typeface::BODY_STRONG)
                    .text_color(p.text_muted)
                    .child(label.to_owned()),
            );
        }
        frame.children(self.overlays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    type S = ThumbState<u8>;

    #[test]
    fn ready_shows_the_image_untouched() {
        let l = S::Ready(1).layers();
        assert_eq!(
            (l.image, l.dim, l.skeleton, l.label),
            (Some(&1), Dim(0.), false, None)
        );
    }

    #[test]
    fn loading_with_a_fallback_dims_it_under_the_skeleton() {
        let l = S::Loading { fallback: Some(2) }.layers();
        assert_eq!(l.image, Some(&2));
        assert_eq!(l.dim, Dim(LOADING_DIM));
        assert!(l.skeleton && l.label.is_none());
    }

    #[test]
    fn loading_without_a_fallback_is_a_plain_skeleton() {
        let l = S::Loading { fallback: None }.layers();
        assert_eq!((l.image, l.skeleton, l.dim), (None, true, Dim(LOADING_DIM)));
    }

    #[test]
    fn fallback_is_slightly_dimmed_and_never_a_skeleton() {
        let l = S::Fallback(3).layers();
        assert_eq!(
            (l.image, l.dim, l.skeleton),
            (Some(&3), Dim(FALLBACK_DIM), false)
        );
    }

    #[test]
    fn missing_shows_its_label_and_blank_shows_nothing() {
        let missing = S::Missing("E03".into());
        let l = missing.layers();
        assert_eq!((l.image, l.skeleton, l.label), (None, false, Some("E03")));
        assert_eq!(S::Missing("".into()).layers().label, None);
        let b = S::Blank.layers();
        assert_eq!((b.image, b.skeleton, b.label), (None, false, None));
    }

    #[test]
    fn pulse_stays_within_its_band() {
        for step in 0..=20 {
            let v = pulse_opacity(step as f32 / 20.);
            assert!((0.15..=0.65).contains(&v), "{v}");
        }
        assert!(pulse_opacity(0.5) > pulse_opacity(0.));
    }
}
