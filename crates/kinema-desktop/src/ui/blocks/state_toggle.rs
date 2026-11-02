//! Independent personal-state icon toggles (Library membership, watched).
//!
//! The control never infers a state: callers pass the shared snapshot status and
//! receive the *desired* value, so unknown/failed reads are visibly neither
//! "off" nor writable. Pointer events are consumed here so a toggle placed inside
//! a card or episode row cannot also activate its host.
use crate::{
    ui::primitives::IconName,
    ui::primitives::button::{Button, ButtonVariant},
};
use gpui::{
    App, ClickEvent, ElementId, IntoElement, MouseButton, RenderOnce, SharedString, Window,
};
use gpui::{InteractiveElement, ParentElement, Styled};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleKind {
    /// Bookmark-off (not saved) / bookmark (saved).
    Membership,
    /// Circle (unwatched) / circle-check (watched).
    Watched,
}

/// Displayed state. `pending` holds the committed-or-requested value shown while
/// a write is in flight; the control is disabled until the outcome arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleStatus {
    Loading,
    Failed,
    Known { on: bool, pending: bool },
}

impl ToggleKind {
    pub const fn icon(self, on: bool) -> IconName {
        match (self, on) {
            (ToggleKind::Membership, false) => IconName::BookmarkOff,
            (ToggleKind::Membership, true) => IconName::Bookmark,
            (ToggleKind::Watched, false) => IconName::Circle,
            (ToggleKind::Watched, true) => IconName::CircleCheck,
        }
    }
    /// Names the action the next click performs.
    pub fn action_label(self, on: bool, subject: &str) -> String {
        match (self, on) {
            (ToggleKind::Membership, false) => format!("Add {subject} to Library"),
            (ToggleKind::Membership, true) => format!("Remove {subject} from Library"),
            (ToggleKind::Watched, false) => format!("Mark {subject} as watched"),
            (ToggleKind::Watched, true) => format!("Mark {subject} as unwatched"),
        }
    }
}

type Change = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type Retry = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct StateToggle {
    id: ElementId,
    kind: ToggleKind,
    status: ToggleStatus,
    subject: SharedString,
    over_art: bool,
    on_change: Option<Change>,
    on_retry: Option<Retry>,
}

impl StateToggle {
    /// `subject` completes the accessible name, e.g. a title or "Episode 3".
    pub fn new(
        id: impl Into<ElementId>,
        kind: ToggleKind,
        status: ToggleStatus,
        subject: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            status,
            subject: subject.into(),
            over_art: false,
            on_change: None,
            on_retry: None,
        }
    }
    pub fn over_art(mut self, over_art: bool) -> Self {
        self.over_art = over_art;
        self
    }
    /// Receives the desired value (not a blind toggle).
    pub fn on_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
    pub fn on_retry(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_retry = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for StateToggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let what = match self.kind {
            ToggleKind::Membership => "Library status",
            ToggleKind::Watched => "watched status",
        };
        let (icon, label, on, disabled, busy) = match self.status {
            ToggleStatus::Loading => (
                IconName::MoreHorizontal,
                format!("Loading {what}…"),
                false,
                true,
                false,
            ),
            ToggleStatus::Failed => (
                IconName::Warning,
                format!("{what} unavailable for {}. Retry", self.subject),
                false,
                false,
                false,
            ),
            ToggleStatus::Known { on, pending } => (
                self.kind.icon(on),
                self.kind.action_label(on, &self.subject),
                on,
                pending,
                pending,
            ),
        };
        let failed = self.status == ToggleStatus::Failed;
        let (change, retry) = (self.on_change, self.on_retry);
        // Over artwork the control keeps a persistent dark chip backing (Secondary
        // over-art face), so contrast never depends on the image beneath it.
        let mut button = Button::new(self.id, "")
            .variant(if self.over_art {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            })
            .icon(icon)
            .square(true)
            // Over artwork the dark chip stays neutral; the glyph carries the state.
            .selected(on && !self.over_art)
            .tooltip(label)
            .disabled(disabled && !busy)
            .busy(busy)
            .over_art(self.over_art)
            .on_click(move |_: &ClickEvent, window, cx| {
                if failed {
                    if let Some(retry) = &retry {
                        retry(window, cx);
                    }
                } else if let Some(change) = &change {
                    change(!on, window, cx);
                }
            });
        // Only a known state claims checked/unchecked; loading/failed expose none.
        if matches!(self.status, ToggleStatus::Known { .. }) {
            button = button.toggled(on);
        }
        // Consume pointer events so hosts (cards, rows) never see this activation.
        gpui::div()
            .flex_none()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icons_and_labels_follow_plan() {
        assert_eq!(ToggleKind::Membership.icon(false), IconName::BookmarkOff);
        assert_eq!(ToggleKind::Membership.icon(true), IconName::Bookmark);
        assert_eq!(ToggleKind::Watched.icon(false), IconName::Circle);
        assert_eq!(ToggleKind::Watched.icon(true), IconName::CircleCheck);
        assert_eq!(
            ToggleKind::Membership.action_label(false, "Dune"),
            "Add Dune to Library"
        );
        assert_eq!(
            ToggleKind::Watched.action_label(true, "Dune"),
            "Mark Dune as unwatched"
        );
    }
}
