//! Compact, non-interactive labels: technical metadata (quality, HDR, codec),
//! status and the rating chip.
//!
//! One `Badge` with tones `Neutral | Primary | Success | Warning | Destructive |
//! OnArt`. `strong(true)` raises a neutral badge to the hover fill with
//! foreground ink (the former `Info` tone). Over artwork a badge keeps its
//! tone's dark-palette ink on the dark `chip_on_art` backing. Absent ratings
//! produce no element.
//!
//! Accessibility: badges are static text, not focusable and not interactive.
//! Their text is the accessible content; tone is never the only carrier of
//! meaning, so pair status tones with a meaningful label (or an icon).
use gpui::{prelude::*, *};

use crate::{
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{Palette, opacity, palette, radius, size, space},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeTone {
    #[default]
    Neutral,
    Primary,
    Success,
    Warning,
    Destructive,
    /// Neutral ink on the dark artwork chip, whatever the theme.
    OnArt,
}

/// Invalid or unknown scores are absent, but a known zero is displayed.
pub fn format_rating(score: Option<f32>) -> Option<String> {
    score
        .filter(|s| s.is_finite())
        .map(|s| format!("{:.1}", s.clamp(0.0, 10.0)))
}

/// `(fill, ink)` over artwork: the tone's dark-palette ink on the dark chip.
fn over_art_colors(tone: BadgeTone, strong: bool) -> (Hsla, Hsla) {
    let dark = Palette::dark();
    (dark.chip_on_art, colors(tone, strong, &dark).1)
}

/// `(fill, ink)` for a tone. Fills are translucent so they sit on any surface.
fn colors(tone: BadgeTone, strong: bool, p: &Palette) -> (Hsla, Hsla) {
    match tone {
        BadgeTone::Neutral | BadgeTone::OnArt if strong => (p.accent, p.foreground),
        BadgeTone::Neutral | BadgeTone::OnArt => (p.secondary, p.muted_foreground),
        BadgeTone::Primary => (p.selection, p.primary_text),
        BadgeTone::Success => (p.success.opacity(opacity::BADGE_WASH), p.success),
        BadgeTone::Warning => (p.warning.opacity(opacity::BADGE_WASH), p.warning),
        BadgeTone::Destructive => (p.destructive.opacity(opacity::BADGE_WASH), p.destructive),
    }
}

#[derive(Clone, Debug)]
enum Content {
    Text,
    /// `None` renders nothing.
    Rating(Option<String>),
}

#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    tone: BadgeTone,
    icon: Option<IconName>,
    uppercase: bool,
    over_art: bool,
    strong: bool,
    content: Content,
}

impl Badge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            tone: BadgeTone::Neutral,
            icon: None,
            uppercase: false,
            over_art: false,
            strong: false,
            content: Content::Text,
        }
    }
    /// Star and score; absent for an invalid or unknown score.
    pub fn rating(score: Option<f32>) -> Self {
        let mut badge = Self::new("");
        badge.content = Content::Rating(format_rating(score));
        badge
    }
    /// The "SERIES" marker.
    pub fn series() -> Self {
        Self::new("SERIES")
    }
    /// Upper-cased quality label such as `1080P`.
    pub fn quality(text: impl Into<SharedString>) -> Self {
        Self::new(text).uppercase(true)
    }
    pub fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self.over_art |= tone == BadgeTone::OnArt;
        self
    }
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    /// Raised neutral look: hover fill with foreground ink.
    pub fn strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }
    /// Dark backing chip and dark-palette ink regardless of theme, for badges
    /// drawn over artwork.
    pub fn over_art(mut self, over_art: bool) -> Self {
        self.over_art = over_art;
        self
    }
    /// Shouty labels such as `HDR10` or `4K`.
    pub fn uppercase(mut self, uppercase: bool) -> Self {
        self.uppercase = uppercase;
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let art = Palette::dark();
        let (fill, mut ink) = if self.over_art {
            over_art_colors(self.tone, self.strong)
        } else {
            colors(self.tone, self.strong, &p)
        };
        let (label, rating): (SharedString, bool) = match self.content {
            Content::Text if self.uppercase => (self.label.to_uppercase().into(), false),
            Content::Text => (self.label, false),
            Content::Rating(None) => return Empty.into_any_element(),
            Content::Rating(Some(score)) => {
                ink = if self.over_art {
                    art.foreground
                } else {
                    p.foreground
                };
                (score.into(), true)
            }
        };
        div()
            .h(size::CHIP)
            .flex_none()
            .flex()
            .items_center()
            .gap(space::S4)
            .px(space::S6)
            .rounded(radius::SM)
            .bg(fill)
            .text_color(ink)
            .text(TextSize::Xs)
            .weight(Weight::Semibold)
            .when(rating, |el| el.tabular())
            .when(rating, |el| {
                el.child(
                    Icon::new(IconName::StarFilled)
                        .size(space::S12)
                        .color(if self.over_art {
                            p.rating_on_art
                        } else {
                            p.rating
                        }),
                )
            })
            .when_some(self.icon, |el, icon| {
                el.child(Icon::new(icon).size(space::S12).color(ink))
            })
            .child(label)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{BadgeTone, Hsla, Palette, Rgba, colors, format_rating, over_art_colors};
    use ::core::prelude::v1::test;

    const TONES: [BadgeTone; 6] = [
        BadgeTone::Neutral,
        BadgeTone::Primary,
        BadgeTone::Success,
        BadgeTone::Warning,
        BadgeTone::Destructive,
        BadgeTone::OnArt,
    ];

    fn luminance(c: Hsla) -> f32 {
        let c: Rgba = c.into();
        let lin = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn every_tone_is_legible_on_surfaces_in_both_themes() {
        for p in [Palette::dark(), Palette::light()] {
            for surface in [p.background, p.surface, p.card] {
                for tone in TONES {
                    for strong in [false, true] {
                        let (fill, ink) = colors(tone, strong, &p);
                        let ratio = contrast(surface.blend(fill), ink);
                        assert!(ratio >= 4.5, "{tone:?} strong={strong} {ratio:.2}");
                    }
                }
            }
        }
    }

    #[test]
    fn over_art_badges_are_legible_on_dark_art_in_any_theme() {
        let canvas = Palette::dark().background;
        for tone in TONES {
            for strong in [false, true] {
                let (fill, ink) = over_art_colors(tone, strong);
                assert!(contrast(canvas.blend(fill), ink) >= 4.5, "{tone:?}");
            }
        }
    }

    #[test]
    fn rating_distinguishes_zero_from_unknown() {
        assert_eq!(format_rating(None), None);
        assert_eq!(format_rating(Some(0.0)).as_deref(), Some("0.0"));
        assert_eq!(format_rating(Some(8.14)).as_deref(), Some("8.1"));
        assert_eq!(format_rating(Some(f32::NAN)), None);
        assert_eq!(format_rating(Some(99.0)).as_deref(), Some("10.0"));
    }

    #[test]
    fn on_art_tone_implies_the_dark_chip() {
        assert!(super::Badge::new("x").tone(BadgeTone::OnArt).over_art);
        assert!(!super::Badge::new("x").tone(BadgeTone::Primary).over_art);
    }
}
