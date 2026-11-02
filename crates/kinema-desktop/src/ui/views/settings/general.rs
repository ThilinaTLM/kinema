//! General section.
use super::*;

impl SettingsWindow {
    pub(super) fn general(&self, mut body: Div, p: theme::Palette) -> Div {
        body = body.child(text("Startup collection: Movies · provider order", p.foreground)).child(text("Search stays within the selected kind (Movies or Series). Provider collections are not configurable in this scaffold.", p.muted_foreground));
        body
    }
}
