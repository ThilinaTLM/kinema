//! Named icon buttons; nonzero counts appear inline beside the glyph.
use crate::{
    ui::primitives::IconName,
    ui::primitives::button::{Button, ButtonVariant},
};
use gpui::{App, ClickEvent, ElementId, IntoElement, RenderOnce, SharedString, Window};
#[derive(IntoElement)]
pub struct IconButton {
    button: Button,
    badge: Option<usize>,
}
impl IconButton {
    /// `label` is both the accessible name and tooltip; it must describe the action.
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        let mut button = Button::new(id, "").variant(ButtonVariant::Ghost).icon(icon);
        button.square = true;
        button.tooltip = Some(label.into());
        Self {
            button,
            badge: None,
        }
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.button = self.button.disabled(disabled);
        self
    }
    pub fn busy(mut self, busy: bool) -> Self {
        self.button = self.button.busy(busy);
        self
    }
    pub fn toggled(mut self, toggled: bool) -> Self {
        self.button.toggled = toggled;
        self
    }
    /// Dark-surface ink for dark artwork banners regardless of theme.
    pub fn over_art(mut self, over_art: bool) -> Self {
        self.button.over_art = over_art;
        self
    }
    pub fn badge(mut self, count: usize) -> Self {
        self.badge = (count > 0).then_some(count);
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.button = self.button.on_click(handler);
        self
    }
}
impl RenderOnce for IconButton {
    fn render(mut self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.button.badge = self.badge;
        self.button
    }
}
