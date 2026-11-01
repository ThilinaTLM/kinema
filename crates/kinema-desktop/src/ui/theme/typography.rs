//! Typography tokens: families, the [`TextSize`] scale, [`Weight`] and [`TypeExt`].
//!
//! Order matters when chaining: `.text(size)` sets size, line height, the UI
//! family and Regular weight, so apply `.weight(..)` / `.mono()` / `.tabular()`
//! afterwards.
//!
//! `.tabular()` is available: gpui-pre exposes `Styled::font_features`, and the
//! bundled Inter face supports `tnum`. It has no effect on fonts without it.

use std::sync::Arc;

use gpui::{FontFeatures, FontWeight, Pixels, Styled, px};

pub const UI_FAMILY: &str = "Inter";
pub const MONO_FAMILY: &str = "JetBrains Mono";

/// One step of the size scale: pixel size and line height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSize {
    /// 11 / 14 (captions).
    Xs,
    /// 12 / 16 (small, mono).
    Sm,
    /// 13 / 18 (body).
    Base,
    /// 14 / 21 (prose, synopsis).
    Md,
    /// 15 / 20 (headings).
    Lg,
    /// 18 / 24 (titles).
    Xl,
    /// 30 / 36 (display).
    Xxl,
}

impl TextSize {
    pub const ALL: [TextSize; 7] = [
        Self::Xs,
        Self::Sm,
        Self::Base,
        Self::Md,
        Self::Lg,
        Self::Xl,
        Self::Xxl,
    ];

    pub const fn size(self) -> Pixels {
        px(match self {
            Self::Xs => 11.0,
            Self::Sm => 12.0,
            Self::Base => 13.0,
            Self::Md => 14.0,
            Self::Lg => 15.0,
            Self::Xl => 18.0,
            Self::Xxl => 30.0,
        })
    }

    pub const fn line_height(self) -> Pixels {
        px(match self {
            Self::Xs => 14.0,
            Self::Sm => 16.0,
            Self::Base => 18.0,
            Self::Md => 21.0,
            Self::Lg => 20.0,
            Self::Xl => 24.0,
            Self::Xxl => 36.0,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
    Bold,
}

impl Weight {
    pub const fn value(self) -> FontWeight {
        FontWeight(match self {
            Self::Regular => 400.0,
            Self::Medium => 500.0,
            Self::Semibold => 600.0,
            Self::Bold => 700.0,
        })
    }
}

/// TEMPORARY (removed in step 5): one step of the old type scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    pub size: Pixels,
    pub line_height: Pixels,
    pub weight: FontWeight,
    pub mono: bool,
}

const fn ts(size: TextSize, weight: Weight, mono: bool) -> TypeStyle {
    TypeStyle {
        size: size.size(),
        line_height: size.line_height(),
        weight: weight.value(),
        mono,
    }
}

/// TEMPORARY (removed in step 5): old named styles, expressed on the new scale.
pub mod typeface {
    use super::{TextSize as S, TypeStyle, Weight as W, ts};

    pub use super::{MONO_FAMILY, UI_FAMILY};

    pub const DISPLAY: TypeStyle = ts(S::Xxl, W::Bold, false);
    pub const TITLE: TypeStyle = ts(S::Xl, W::Semibold, false);
    pub const HEADING: TypeStyle = ts(S::Lg, W::Semibold, false);
    pub const BODY: TypeStyle = ts(S::Base, W::Regular, false);
    pub const BODY_LONG: TypeStyle = ts(S::Md, W::Regular, false);
    pub const BODY_STRONG: TypeStyle = ts(S::Base, W::Semibold, false);
    pub const SMALL: TypeStyle = ts(S::Sm, W::Regular, false);
    pub const SMALL_MEDIUM: TypeStyle = ts(S::Sm, W::Medium, false);
    pub const CAPTION: TypeStyle = ts(S::Xs, W::Semibold, false);
    pub const MONO: TypeStyle = ts(S::Sm, W::Regular, true);
}

pub trait TypeExt: Styled + Sized {
    /// Size and line height from the scale, with the UI family and Regular weight.
    fn text(self, size: TextSize) -> Self {
        self.font_family(UI_FAMILY)
            .text_size(size.size())
            .line_height(size.line_height())
            .font_weight(Weight::Regular.value())
    }
    fn weight(self, weight: Weight) -> Self {
        self.font_weight(weight.value())
    }
    /// Monospace family.
    fn mono(self) -> Self {
        self.font_family(MONO_FAMILY)
    }
    /// Tabular figures (`tnum`) so changing numbers do not jitter.
    fn tabular(self) -> Self {
        self.font_features(FontFeatures(Arc::new(vec![("tnum".to_string(), 1)])))
    }
    /// TEMPORARY (removed in step 5): apply an old [`TypeStyle`].
    fn type_style(self, style: TypeStyle) -> Self {
        let family = if style.mono { MONO_FAMILY } else { UI_FAMILY };
        self.font_family(family)
            .text_size(style.size)
            .line_height(style.line_height)
            .font_weight(style.weight)
    }
    /// Wrap to at most `lines` lines and end the last with an ellipsis. Plain
    /// `line_clamp` without an ellipsis leaves the remainder of the text on
    /// the final line, unwrapped and clipped mid-word at the container edge.
    fn clamp_lines(self, lines: usize) -> Self {
        self.line_clamp(lines).text_ellipsis()
    }
}

impl<T: Styled + Sized> TypeExt for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::div;

    #[test]
    fn clamped_text_always_ends_with_an_ellipsis() {
        let mut el = div().clamp_lines(3);
        let text = &el.style().text;
        assert_eq!(text.line_clamp, Some(3));
        assert!(text.text_overflow.is_some());
    }

    #[test]
    fn scale_matches_the_spec() {
        let want = [
            (TextSize::Xs, 11.0, 14.0),
            (TextSize::Sm, 12.0, 16.0),
            (TextSize::Base, 13.0, 18.0),
            (TextSize::Md, 14.0, 21.0),
            (TextSize::Lg, 15.0, 20.0),
            (TextSize::Xl, 18.0, 24.0),
            (TextSize::Xxl, 30.0, 36.0),
        ];
        for (size, px_size, line) in want {
            assert_eq!(size.size(), px(px_size));
            assert_eq!(size.line_height(), px(line));
        }
        assert_eq!(Weight::Semibold.value(), FontWeight(600.0));
    }

    #[test]
    fn old_styles_equal_the_new_scale() {
        assert_eq!(typeface::BODY.size, TextSize::Base.size());
        assert_eq!(typeface::DISPLAY.weight, Weight::Bold.value());
        const {
            assert!(typeface::MONO.mono);
        }
    }

    #[test]
    fn tabular_sets_tnum() {
        let mut el = div().tabular();
        let Some(features) = el.style().text.font_features.clone() else {
            panic!("tabular() must set font features");
        };
        assert_eq!(features.tag_value_list(), &[("tnum".to_string(), 1)]);
    }
}
