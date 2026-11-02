//! Spacing scale (padding, gaps, insets). Component and page dimensions live in
//! [`super::size`], corner geometry in [`super::radius`].

use gpui::{Pixels, px};

pub const NONE: Pixels = px(0.0);
pub const S2: Pixels = px(2.0);
pub const S4: Pixels = px(4.0);
pub const S6: Pixels = px(6.0);
pub const S8: Pixels = px(8.0);
pub const S12: Pixels = px(12.0);
pub const S16: Pixels = px(16.0);
pub const S20: Pixels = px(20.0);
pub const S24: Pixels = px(24.0);
pub const S32: Pixels = px(32.0);
pub const S48: Pixels = px(48.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_strictly_increasing() {
        let all = [NONE, S2, S4, S6, S8, S12, S16, S20, S24, S32, S48];
        assert!(all.windows(2).all(|w| w[0] < w[1]));
    }
}
