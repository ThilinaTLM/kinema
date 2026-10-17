//! Attributable IMDb score label. Known `0.0` is a value; unknown is a dash.
use gpui_kit::{App, Window};
use gpui_kit::{
    FontWeight, IntoElement, ParentElement as _, RenderOnce, SharedString, Styled as _, div, px,
};

use super::{theme::tokens, tokens::metrics};

/// Valid IMDb scores are finite and within 0..=10; anything else is unknown.
pub fn valid_score(score: Option<f32>) -> Option<f32> {
    score.filter(|s| s.is_finite() && (0.0..=10.0).contains(s))
}

/// `"8.1/10"`, `"0.0/10"` or `"—"`.
pub fn score_text(score: Option<f32>) -> SharedString {
    match valid_score(score) {
        Some(value) => format!("{value:.1}/10").into(),
        None => "—".into(),
    }
}

/// Screen-reader reading, never substituting another provider's score.
pub fn score_description(score: Option<f32>) -> String {
    match valid_score(score) {
        Some(value) => format!("IMDb rating {value:.1} out of 10"),
        None => "IMDb rating unavailable".to_owned(),
    }
}

/// Yellow `IMDb` label followed by a neutral value.
#[derive(IntoElement)]
pub struct ImdbBadge {
    score: Option<f32>,
}

impl ImdbBadge {
    pub fn new(score: Option<f32>) -> Self {
        Self { score }
    }
}

impl RenderOnce for ImdbBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens(cx);
        let known = valid_score(self.score).is_some();
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(metrics::LINE_META))
            .text_size(px(metrics::FONT_META))
            .child(
                div()
                    .px(px(4.))
                    .h(px(14.))
                    .flex()
                    .items_center()
                    .rounded(px(2.))
                    .bg(t.imdb)
                    .text_color(t.imdb_ink)
                    .font_weight(FontWeight::BOLD)
                    .child("IMDb"),
            )
            .child(
                div()
                    .text_color(if known { t.text } else { t.muted })
                    .child(score_text(self.score)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_known_and_unknown_is_dash() {
        assert_eq!(score_text(Some(0.0)), "0.0/10");
        assert_eq!(score_text(Some(8.14)), "8.1/10");
        assert_eq!(score_text(None), "—");
        assert_eq!(score_text(Some(f32::NAN)), "—");
        assert_eq!(score_text(Some(11.0)), "—");
        assert_eq!(score_text(Some(-0.5)), "—");
        assert_eq!(score_description(Some(0.0)), "IMDb rating 0.0 out of 10");
        assert_eq!(score_description(None), "IMDb rating unavailable");
    }
}
