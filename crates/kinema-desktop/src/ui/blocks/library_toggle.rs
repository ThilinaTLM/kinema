//! Library membership toggle built on [`ActionButton`].
//!
//! Off reads "+ Add to Library"; on reads "In Library" with a filled bookmark
//! and the accent `on` style, and its tooltip offers "Remove from Library".
//! Like [`super::state_toggle::StateToggle`] it never infers state: callers pass
//! the shared [`ToggleStatus`] and receive the *desired* value. Loading is inert,
//! a write in flight is busy (width kept), and a failed read is a warning
//! button that requests a retry. `compact` makes it an icon-only square.
use crate::{
    ui::blocks::state_toggle::ToggleStatus,
    ui::primitives::IconName,
    ui::primitives::action_button::{ActionButton, ActionKind},
};
use gpui::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use std::rc::Rc;

/// What the button shows for a status; pure so it can be tested.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryFace {
    pub label: &'static str,
    pub icon: IconName,
    pub on: bool,
    pub busy: bool,
    /// Inert, with this reason as its tooltip.
    pub disabled: Option<String>,
    pub tooltip: String,
    /// Known state exposed to assistive tech; `None` while loading or failed.
    pub toggled: Option<bool>,
    pub retry: bool,
}

pub fn face(status: ToggleStatus, subject: &str) -> LibraryFace {
    match status {
        ToggleStatus::Loading => LibraryFace {
            label: "Library",
            icon: IconName::MoreHorizontal,
            on: false,
            busy: false,
            disabled: Some("Loading Library status…".into()),
            tooltip: "Loading Library status…".into(),
            toggled: None,
            retry: false,
        },
        ToggleStatus::Failed => LibraryFace {
            label: "Library unavailable",
            icon: IconName::Warning,
            on: false,
            busy: false,
            disabled: None,
            tooltip: format!("Library status unavailable for {subject}. Retry"),
            toggled: None,
            retry: true,
        },
        ToggleStatus::Known { on, pending } => LibraryFace {
            label: if on { "In Library" } else { "Add to Library" },
            icon: if on {
                IconName::BookmarkFilled
            } else {
                IconName::Plus
            },
            on,
            busy: pending,
            disabled: None,
            tooltip: if on {
                "Remove from Library".into()
            } else {
                format!("Add {subject} to Library")
            },
            toggled: Some(on),
            retry: false,
        },
    }
}

/// Desired membership after a click on a known state.
pub fn desired_on_click(on: bool) -> bool {
    !on
}

type Change = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type Retry = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct LibraryToggle {
    id: ElementId,
    status: ToggleStatus,
    subject: SharedString,
    compact: bool,
    on_change: Option<Change>,
    on_retry: Option<Retry>,
}

impl LibraryToggle {
    /// `subject` completes tooltips and names, e.g. the title.
    pub fn new(
        id: impl Into<ElementId>,
        status: ToggleStatus,
        subject: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            status,
            subject: subject.into(),
            compact: false,
            on_change: None,
            on_retry: None,
        }
    }
    /// Icon-only square for narrow windows.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
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

impl RenderOnce for LibraryToggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let face = face(self.status, &self.subject);
        let kind = if self.compact {
            ActionKind::Icon
        } else {
            ActionKind::Secondary
        };
        let mut button = ActionButton::new(self.id, kind, face.label)
            .icon(face.icon)
            .on(face.on)
            .busy(face.busy)
            .tooltip(face.tooltip.clone())
            .name(format!("{}: {}", face.label, face.tooltip));
        if let Some(on) = face.toggled {
            button = button.toggled(on, false);
        }
        if let Some(reason) = face.disabled.clone() {
            button = button.disabled(reason);
        }
        let (change, retry) = (self.on_change, self.on_retry);
        let (known, retrying) = (face.toggled, face.retry);
        button.on_click(move |_, window, cx| {
            if retrying {
                if let Some(retry) = &retry {
                    retry(window, cx);
                }
            } else if let (Some(on), Some(change)) = (known, &change) {
                change(desired_on_click(on), window, cx);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn off_and_on_faces() {
        let off = face(
            ToggleStatus::Known {
                on: false,
                pending: false,
            },
            "Dune",
        );
        assert_eq!(
            (off.label, off.icon, off.on),
            ("Add to Library", IconName::Plus, false)
        );
        assert_eq!(off.tooltip, "Add Dune to Library");
        let on = face(
            ToggleStatus::Known {
                on: true,
                pending: false,
            },
            "Dune",
        );
        assert_eq!(
            (on.label, on.icon, on.on),
            ("In Library", IconName::BookmarkFilled, true)
        );
        assert_eq!(on.tooltip, "Remove from Library");
        assert_eq!(on.toggled, Some(true));
        assert!(desired_on_click(false) && !desired_on_click(true));
    }

    #[test]
    fn pending_is_busy_but_keeps_the_known_face() {
        let f = face(
            ToggleStatus::Known {
                on: true,
                pending: true,
            },
            "Dune",
        );
        assert!(f.busy && f.on && f.disabled.is_none());
    }

    #[test]
    fn loading_is_inert_and_failure_asks_for_retry() {
        let loading = face(ToggleStatus::Loading, "Dune");
        assert!(loading.disabled.is_some() && loading.toggled.is_none() && !loading.on);
        let failed = face(ToggleStatus::Failed, "Dune");
        assert!(failed.retry && failed.disabled.is_none() && failed.toggled.is_none());
        assert!(!failed.on);
    }
}
