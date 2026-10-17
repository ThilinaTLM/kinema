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
}

impl<T: Styled + Sized> TypeExt for T {}
