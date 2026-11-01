//! Compact technical-metadata badges (quality, HDR, Dolby Vision, codec).
use gpui::{prelude::*, *};

use crate::{
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::TypeExt,
    ui::theme::{Palette, palette, radius, space, typeface},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeTone {
    #[default]
    Neutral,
    Accent,
    Info,
    Success,
    Warn,
}

/// `(fill, ink)` over artwork: the tone's dark-palette ink on the dark chip.
fn over_art_colors(tone: BadgeTone) -> (Hsla, Hsla) {
    let dark = Palette::dark();
    (dark.chip_over_art, colors(tone, &dark).1)
}

/// `(fill, ink)` for a tone. Fills are translucent so they sit on any surface.
fn colors(tone: BadgeTone, p: &Palette) -> (Hsla, Hsla) {
    match tone {
        BadgeTone::Neutral => (p.ctl_rest, p.text_secondary),
        BadgeTone::Accent => (p.accent_tint, p.accent_text),
        BadgeTone::Info => (p.ctl_hover, p.text_primary),
        BadgeTone::Success => (p.status_ok.opacity(0.10), p.status_ok),
        BadgeTone::Warn => (p.status_warn.opacity(0.10), p.status_warn),
    }
}

#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    tone: BadgeTone,
    icon: Option<IconName>,
    uppercase: bool,
    over_art: bool,
}

impl Badge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            tone: BadgeTone::Neutral,
            icon: None,
            uppercase: false,
            over_art: false,
        }
    }
    pub fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
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
        let (fill, ink) = if self.over_art {
            over_art_colors(self.tone)
        } else {
            colors(self.tone, &p)
        };
        let label = if self.uppercase {
            self.label.to_uppercase().into()
        } else {
            self.label
        };
        div()
            .h(space::CHIP_HEIGHT)
            .flex_none()
            .flex()
            .items_center()
            .gap(space::S4)
            .px(space::S6)
            .rounded(radius::SM)
            .bg(fill)
            .text_color(ink)
            .type_style(typeface::CAPTION)
            .when_some(self.icon, |el, icon| {
                el.child(Icon::new(icon).size(space::S12).color(ink))
            })
            .child(label)
    }
}

#[cfg(test)]
mod tests {
    use super::{BadgeTone, Hsla, Palette, Rgba, colors, over_art_colors};

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
            for surface in [p.bg_canvas, p.bg_surface, p.bg_raised] {
                for tone in [
                    BadgeTone::Neutral,
                    BadgeTone::Accent,
                    BadgeTone::Info,
                    BadgeTone::Success,
                    BadgeTone::Warn,
                ] {
                    let (fill, ink) = colors(tone, &p);
                    let ratio = contrast(surface.blend(fill), ink);
                    assert!(ratio >= 4.5, "{tone:?} {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn over_art_badges_are_legible_on_dark_art_in_any_theme() {
        let canvas = Palette::dark().bg_canvas;
        for tone in [
            BadgeTone::Neutral,
            BadgeTone::Accent,
            BadgeTone::Info,
            BadgeTone::Success,
            BadgeTone::Warn,
        ] {
            let (fill, ink) = over_art_colors(tone);
            assert!(contrast(canvas.blend(fill), ink) >= 4.5, "{tone:?}");
        }
    }
}
