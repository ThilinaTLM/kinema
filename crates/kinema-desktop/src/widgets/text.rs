//! Typography helper: apply a [`TypeStyle`] token to any styled element.

use gpui::Styled;

use super::tokens::{TypeStyle, typeface};

pub trait TypeExt: Styled + Sized {
    /// Apply family, size, line height and weight from a type token.
    fn type_style(self, style: TypeStyle) -> Self {
        let family = if style.mono {
            typeface::MONO_FAMILY
        } else {
            typeface::UI_FAMILY
        };
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
    use super::TypeExt;
    use gpui::{Styled, div};

    #[test]
    fn clamped_text_always_ends_with_an_ellipsis() {
        // A bare line clamp leaves the unwrapped remainder on the last line,
        // clipped mid-word at the container edge.
        let mut el = div().clamp_lines(3);
        let text = &el.style().text;
        assert_eq!(text.line_clamp, Some(3));
        assert!(text.text_overflow.is_some());
    }
}
