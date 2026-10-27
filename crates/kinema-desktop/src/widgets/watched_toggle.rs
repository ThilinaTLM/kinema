//! Tri-state watch toggle: `ActionButton` + `WatchMark` + a word + a muted count.
//!
//! The control is stateless and service-free. The caller passes a [`Tally`]
//! (watched / total) and receives the *desired* value: `true` = mark everything
//! watched (from none/partial), `false` = mark everything unwatched (from all).
//! Pass `unavailable(reason)` when tracking is not possible; the button is then
//! disabled and the reason is its tooltip. The All state uses the accent `on`
//! style. `compact` collapses it to an icon-only square that keeps its
//! tooltip and accessible name. Exposed as a checkbox-like toggle whose state
//! is `Mixed` for partial progress.
use super::{
    action_button::{ActionButton, ActionKind},
    watch_mark::{WatchMark, WatchMarkState},
};
use gpui::{prelude::*, *};
use kinema_core::watch_progress::{Tally, WatchedState};
use std::rc::Rc;

/// Words for each state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatchedLabels {
    pub none: &'static str,
    pub partial: &'static str,
    pub all: &'static str,
}

impl WatchedLabels {
    /// Season / generic use.
    pub const DEFAULT: Self = Self {
        none: "Not watched",
        partial: "Partly watched",
        all: "Watched",
    };
    /// Series hero: partial progress reads as "Watching".
    pub const SERIES_HERO: Self = Self {
        none: "Not watched",
        partial: "Watching",
        all: "Watched",
    };
    /// Movie hero: the off state is the action.
    pub const MOVIE: Self = Self {
        none: "Mark watched",
        partial: "Mark watched",
        all: "Watched",
    };
    fn word(self, state: WatchedState) -> &'static str {
        match state {
            WatchedState::None => self.none,
            WatchedState::Partial => self.partial,
            WatchedState::All => self.all,
        }
    }
}

/// `N of M` sub-label.
pub fn count_label(tally: Tally) -> String {
    format!("{} of {}", tally.watched.min(tally.total), tally.total)
}

/// Desired watched value after a click: anything short of "all" completes it.
pub fn desired_on_click(state: WatchedState) -> bool {
    state != WatchedState::All
}

/// Everything the button shows, derived purely from the inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct ToggleFace {
    pub word: String,
    pub count: Option<String>,
    pub mark: WatchMarkState,
    pub on: bool,
    pub mixed: bool,
    /// Why the button is inert, also its tooltip.
    pub blocked_reason: Option<String>,
    /// Tooltip and accessible action ("Mark series as watched").
    pub action: String,
    /// Desired value a click requests, if clicking is meaningful.
    pub intent: Option<bool>,
}

pub fn face(
    tally: Tally,
    labels: WatchedLabels,
    subject: &str,
    unavailable: Option<&str>,
    show_count: bool,
) -> ToggleFace {
    let state = tally.state();
    let all = state == Some(WatchedState::All) && unavailable.is_none();
    let action = match state {
        Some(WatchedState::All) => format!("Mark {subject} as unwatched"),
        Some(_) => format!("Mark {subject} as watched"),
        None => format!("No episodes in this {subject}"),
    };
    let (word, count, mark) = match (unavailable, state) {
        (Some(_), _) => (
            "Watched status unavailable".to_owned(),
            None,
            WatchMarkState::None,
        ),
        (None, Some(state)) => (
            labels.word(state).to_owned(),
            show_count.then(|| count_label(tally)),
            WatchMarkState::from_tally(tally),
        ),
        (None, None) => (
            labels.none.to_owned(),
            show_count.then(|| "No episodes".to_owned()),
            WatchMarkState::None,
        ),
    };
    ToggleFace {
        word,
        count,
        mark,
        on: all,
        mixed: unavailable.is_none() && state == Some(WatchedState::Partial),
        blocked_reason: unavailable
            .map(str::to_owned)
            .or_else(|| state.is_none().then(|| action.clone())),
        action,
        intent: unavailable
            .is_none()
            .then(|| state.map(desired_on_click))
            .flatten(),
    }
}

type Change = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct WatchedToggle {
    id: ElementId,
    tally: Tally,
    labels: WatchedLabels,
    unavailable: Option<SharedString>,
    busy: bool,
    subject: SharedString,
    show_count: bool,
    compact: bool,
    on_change: Option<Change>,
}

impl WatchedToggle {
    pub fn new(id: impl Into<ElementId>, tally: Tally) -> Self {
        Self {
            id: id.into(),
            tally,
            labels: WatchedLabels::DEFAULT,
            unavailable: None,
            busy: false,
            subject: "season".into(),
            show_count: true,
            compact: false,
            on_change: None,
        }
    }
    pub fn labels(mut self, labels: WatchedLabels) -> Self {
        self.labels = labels;
        self
    }
    /// What is being marked, for the accessible name and tooltip ("season", "series").
    pub fn subject(mut self, subject: impl Into<SharedString>) -> Self {
        self.subject = subject.into();
        self
    }
    /// Disable the pill; `reason` becomes its tooltip.
    pub fn unavailable(mut self, reason: impl Into<SharedString>) -> Self {
        self.unavailable = Some(reason.into());
        self
    }
    /// A write is in flight: the pill is inert until the outcome arrives.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }
    /// Show the muted `N of M` count (off for single titles).
    pub fn show_count(mut self, show: bool) -> Self {
        self.show_count = show;
        self
    }
    /// Icon-only square for narrow windows; tooltip and name are kept.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
    /// Receives the desired value (`true` = all watched).
    pub fn on_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for WatchedToggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let face = face(
            self.tally,
            self.labels,
            &self.subject,
            self.unavailable.as_deref(),
            self.show_count,
        );
        let kind = if self.compact {
            ActionKind::Icon
        } else {
            ActionKind::Secondary
        };
        let name = match &face.count {
            Some(count) => format!("{}, {count}. {}", face.word, face.action),
            None => format!("{}. {}", face.word, face.action),
        };
        let mut button = ActionButton::new(self.id, kind, face.word.clone())
            .glyph(WatchMark::new(face.mark))
            .on(face.on)
            .busy(self.busy)
            .toggled(face.on, face.mixed)
            .name(name)
            .tooltip(face.action.clone());
        if let Some(count) = face.count.clone() {
            button = button.trailing(count);
        }
        if let Some(reason) = face.blocked_reason.clone() {
            button = button.disabled(reason);
        }
        if let (Some(change), Some(desired)) = (self.on_change, face.intent) {
            button = button.on_click(move |_, window, cx| change(desired, window, cx));
        }
        button
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn click_completes_unless_already_all() {
        assert!(desired_on_click(WatchedState::None));
        assert!(desired_on_click(WatchedState::Partial));
        assert!(!desired_on_click(WatchedState::All));
    }

    #[test]
    fn words_and_counts() {
        assert_eq!(
            WatchedLabels::DEFAULT.word(WatchedState::Partial),
            "Partly watched"
        );
        assert_eq!(
            WatchedLabels::SERIES_HERO.word(WatchedState::Partial),
            "Watching"
        );
        assert_eq!(WatchedLabels::DEFAULT.word(WatchedState::All), "Watched");
        assert_eq!(
            WatchedLabels::MOVIE.word(WatchedState::None),
            "Mark watched"
        );
        assert_eq!(WatchedLabels::MOVIE.word(WatchedState::All), "Watched");
        assert_eq!(
            count_label(Tally {
                watched: 4,
                total: 22
            }),
            "4 of 22"
        );
        assert_eq!(
            count_label(Tally {
                watched: 9,
                total: 3
            }),
            "3 of 3"
        );
    }

    fn tally(watched: usize, total: usize) -> Tally {
        Tally { watched, total }
    }

    #[test]
    fn faces_follow_the_spec_states() {
        let f = |t, u: Option<&str>| face(t, WatchedLabels::SERIES_HERO, "series", u, true);
        let none = f(tally(0, 10), None);
        assert_eq!(
            (none.word.as_str(), none.on, none.mixed),
            ("Not watched", false, false)
        );
        assert_eq!(none.intent, Some(true));
        let part = f(tally(2, 10), None);
        assert_eq!(part.word, "Watching");
        assert_eq!(part.count.as_deref(), Some("2 of 10"));
        assert!(part.mixed && !part.on);
        assert_eq!(part.intent, Some(true));
        let all = f(tally(10, 10), None);
        assert!(all.on && !all.mixed);
        assert_eq!(all.action, "Mark series as unwatched");
        assert_eq!(all.intent, Some(false));
    }

    #[test]
    fn unavailable_and_empty_are_inert_with_a_reason() {
        let off = face(
            tally(3, 10),
            WatchedLabels::DEFAULT,
            "season",
            Some("No storage"),
            true,
        );
        assert_eq!(off.blocked_reason.as_deref(), Some("No storage"));
        assert_eq!(off.intent, None);
        assert!(!off.on && off.count.is_none());
        assert_eq!(off.mark, WatchMarkState::None);
        let empty = face(tally(0, 0), WatchedLabels::DEFAULT, "season", None, true);
        assert_eq!(empty.intent, None);
        assert!(empty.blocked_reason.is_some());
        assert!(!empty.on);
    }

    #[test]
    fn movie_face_has_no_count() {
        let f = face(tally(0, 1), WatchedLabels::MOVIE, "Dune", None, false);
        assert_eq!((f.word.as_str(), f.count), ("Mark watched", None));
        assert_eq!(f.action, "Mark Dune as watched");
        let f = face(tally(1, 1), WatchedLabels::MOVIE, "Dune", None, false);
        assert!(f.on);
        assert_eq!(f.word, "Watched");
    }
}
