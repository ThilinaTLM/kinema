//! About section.
use super::*;

impl SettingsWindow {
    pub(super) fn about(&self, mut body: Div, p: theme::Palette) -> Div {
        body = body.child(div().text(TextSize::Lg).weight(Weight::Semibold).child(concat!("Kinema ", env!("CARGO_PKG_VERSION"))))
                    .child(text("Pre-1.0 scaffold — not the complete MVP. Provider, playback and OS support are not implied by this interface.", p.muted_foreground))
                    .child(text("Inter — SIL Open Font License 1.1\nJetBrains Mono — SIL Open Font License 1.1\nLucide — ISC License\nGPUI — Apache License 2.0", p.muted_foreground))
                    .child(img("kinema/tmdb-logo.svg").w(s::S48 * 3.).h(s::S32).object_fit(ObjectFit::Contain))
                    .child(text("This product uses the TMDB API but is not endorsed or certified by TMDB.", p.muted_foreground));
        body
    }
}
