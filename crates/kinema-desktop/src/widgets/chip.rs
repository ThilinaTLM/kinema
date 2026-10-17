//! Non-interactive metadata chips; absent ratings produce no element.
use super::{
    Icon, IconName, TypeExt,
    tokens::{Palette, palette, space, typeface},
};
use gpui::{prelude::*, *};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
}
impl StatusKind {
    pub(crate) fn ink(self, p: Palette) -> Hsla {
        match self {
            Self::Info => p.text_secondary,
            Self::Success => p.status_ok,
            Self::Warning => p.status_warn,
            Self::Error => p.status_error,
        }
    }
}
#[derive(Clone, Debug)]
pub enum ChipKind {
    Rating { score: Option<f32> },
    Series,
    Quality(SharedString),
    Status(StatusKind, SharedString),
}
/// Invalid or unknown scores are absent, but a known zero is displayed.
pub fn format_rating(score: Option<f32>) -> Option<String> {
    score
        .filter(|s| s.is_finite())
        .map(|s| format!("{:.1}", s.clamp(0.0, 10.0)))
}
#[derive(IntoElement)]
pub struct Chip {
    kind: ChipKind,
    over_art: bool,
}
impl Chip {
    pub fn new(kind: ChipKind) -> Self {
        Self {
            kind,
            over_art: false,
        }
    }
    pub fn rating(score: Option<f32>) -> Self {
        Self::new(ChipKind::Rating { score })
    }
    pub fn series() -> Self {
        Self::new(ChipKind::Series)
    }
    pub fn quality(text: impl Into<SharedString>) -> Self {
        Self::new(ChipKind::Quality(text.into()))
    }
    pub fn status(kind: StatusKind, text: impl Into<SharedString>) -> Self {
        Self::new(ChipKind::Status(kind, text.into()))
    }
    pub fn over_art(mut self, over_art: bool) -> Self {
        self.over_art = over_art;
        self
    }
}
impl RenderOnce for Chip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let art = Palette::dark();
        let ink = if self.over_art {
            art.text_secondary
        } else {
            p.text_secondary
        };
        let mut rating = false;
        let (label, ink): (SharedString, Hsla) = match self.kind {
            ChipKind::Rating { score } => {
                let Some(score) = format_rating(score) else {
                    return Empty.into_any_element();
                };
                rating = true;
                (
                    score.into(),
                    if self.over_art {
                        art.text_primary
                    } else {
                        p.text_primary
                    },
                )
            }
            ChipKind::Series => ("SERIES".into(), ink),
            ChipKind::Quality(text) => (text.to_uppercase().into(), ink),
            ChipKind::Status(kind, text) => (text, kind.ink(if self.over_art { art } else { p })),
        };
        div()
            .h(space::CHIP_HEIGHT)
            .px(space::S6)
            .flex()
            .items_center()
            .gap(space::S4)
            .flex_none()
            .type_style(typeface::CAPTION)
            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
            .bg(if self.over_art {
                p.chip_over_art
            } else {
                p.ctl_rest
            })
            .text_color(ink)
            .when(rating, |el| {
                el.child(
                    Icon::new(IconName::StarFilled)
                        .size(space::S12)
                        .color(if self.over_art {
                            p.rating_over_art()
                        } else {
                            p.rating
                        }),
                )
            })
            .child(label)
            .into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use super::format_rating;
    #[test]
    fn rating_distinguishes_zero_from_unknown() {
        assert_eq!(format_rating(None), None);
        assert_eq!(format_rating(Some(0.0)).as_deref(), Some("0.0"));
        assert_eq!(format_rating(Some(8.14)).as_deref(), Some("8.1"));
        assert_eq!(format_rating(Some(f32::NAN)), None);
        assert_eq!(format_rating(Some(99.0)).as_deref(), Some("10.0"));
    }
}
