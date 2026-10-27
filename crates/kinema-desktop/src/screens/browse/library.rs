//! Pure Library-scope policy for Browse: scope, filters, per-scope view context,
//! personal-state projection and focus after a card leaves the result set.
use crate::{
    screens::coordinator::{PersonalLoad, PersonalView},
    widgets::state_toggle::ToggleStatus,
};
use gpui::SharedString;
use kinema_core::{
    MediaKind,
    library::{LibraryKind, LibraryQuery, LibrarySort, WatchFilter},
};

/// Rows requested per Library page.
pub(crate) const PAGE_SIZE: usize = 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    #[default]
    Discover,
    Library,
}
impl Scope {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Discover => 0,
            Self::Library => 1,
        }
    }
    pub(crate) fn search_placeholder(self, kind: MediaKind) -> &'static str {
        match (self, kind) {
            (Self::Library, _) => "Search your Library…",
            (Self::Discover, MediaKind::Movie) => "Search movies…",
            (Self::Discover, MediaKind::Series) => "Search series…",
        }
    }
}

/// Library controls; Discover keeps its own provider kind/sort in the coordinator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryFilters {
    pub kind: LibraryKind,
    pub status: WatchFilter,
    pub sort: LibrarySort,
}
impl LibraryFilters {
    pub(crate) fn kind_options() -> Vec<(LibraryKind, SharedString)> {
        vec![
            (LibraryKind::All, "All".into()),
            (LibraryKind::Movies, "Movies".into()),
            (LibraryKind::Series, "Series".into()),
        ]
    }
    pub(crate) fn status_options(kind: LibraryKind) -> Vec<(WatchFilter, SharedString)> {
        let mut options = vec![
            (WatchFilter::Any, "Any status".into()),
            (WatchFilter::Unwatched, "Unwatched".into()),
        ];
        if kind != LibraryKind::Movies {
            options.push((WatchFilter::InProgress, "In progress".into()));
        }
        options.push((WatchFilter::Watched, "Watched".into()));
        options
    }
    pub(crate) fn sort_options() -> Vec<(LibrarySort, SharedString)> {
        vec![
            (LibrarySort::RecentlyAdded, "Recently added".into()),
            (LibrarySort::Title, "Title A–Z".into()),
            (LibrarySort::NewestRelease, "Newest".into()),
        ]
    }
    /// In progress only exists for shows, so Movies resets it to Any.
    pub(crate) fn set_kind(&mut self, kind: LibraryKind) {
        self.kind = kind;
        if kind == LibraryKind::Movies && self.status == WatchFilter::InProgress {
            self.status = WatchFilter::Any;
        }
    }
    pub(crate) fn query(&self, search: &str) -> LibraryQuery {
        LibraryQuery {
            kind: self.kind,
            search: search.trim().to_owned(),
            status: self.status,
            sort: self.sort,
            offset: 0,
            limit: PAGE_SIZE,
        }
    }
    pub(crate) fn is_filtered(&self, search: &str) -> bool {
        self.kind != LibraryKind::All
            || self.status != WatchFilter::Any
            || !search.trim().is_empty()
    }
}

/// What a scope remembers while the other one (or Detail) is showing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ScopeContext {
    pub search: String,
    pub selected: Option<String>,
    /// First visible grid row.
    pub scroll_row: usize,
    /// A card had focus, so Back should return focus to it rather than the grid.
    pub card_focused: bool,
}

/// Shared membership state for a card; unknown is never presented as unsaved.
pub(crate) fn membership_status(view: &PersonalView) -> ToggleStatus {
    match &view.state {
        PersonalLoad::Loading => ToggleStatus::Loading,
        PersonalLoad::Failed(_) => ToggleStatus::Failed,
        PersonalLoad::Ready(title) => ToggleStatus::Known {
            on: title.added_at.is_some(),
            pending: view.pending,
        },
    }
}
/// Movie watched value; `None` while unknown or failed (never "unwatched").
pub(crate) fn movie_watched(view: &PersonalView) -> Option<bool> {
    match &view.state {
        PersonalLoad::Ready(title) if title.kind == MediaKind::Movie => {
            Some(title.movie_watch.is_some_and(|w| w.watched))
        }
        _ => None,
    }
}

/// Where focus goes when the focused card at `removed` leaves a list that now
/// holds `remaining` cards: the next card, else the previous, else the empty state.
pub(crate) fn focus_after_removal(removed: usize, remaining: usize) -> Option<usize> {
    match remaining {
        0 => None,
        n => Some(removed.min(n - 1)),
    }
}

/// Position the focused key now occupies; falls back per [`focus_after_removal`]
/// using its old index when the key was filtered out.
pub(crate) fn refocus(old_keys: &[String], new_keys: &[String], focused: &str) -> Option<usize> {
    if let Some(i) = new_keys.iter().position(|k| k == focused) {
        return Some(i);
    }
    let old = old_keys.iter().position(|k| k == focused)?;
    // Prefer the first surviving successor so "next card" is stable across removals.
    old_keys[old + 1..]
        .iter()
        .find_map(|k| new_keys.iter().position(|n| n == k))
        .or_else(|| focus_after_removal(old, new_keys.len()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::screens::coordinator::PersonalTitle;
    use kinema_core::library::{TitleId, WatchState};

    fn view(state: PersonalLoad<PersonalTitle>, pending: bool) -> PersonalView {
        PersonalView { state, pending }
    }
    fn title(added: Option<i64>, kind: MediaKind, watched: Option<bool>) -> PersonalTitle {
        PersonalTitle {
            title_id: TitleId::new(1).unwrap(),
            kind,
            added_at: added,
            movie_watch: watched.map(|watched| WatchState {
                watched,
                updated_at: 1,
            }),
            show_summary: None,
        }
    }
    fn keys(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn unknown_is_not_unsaved_or_unwatched() {
        let loading = view(PersonalLoad::Loading, false);
        assert_eq!(membership_status(&loading), ToggleStatus::Loading);
        assert_eq!(movie_watched(&loading), None);
        let failed = view(PersonalLoad::Failed("db".into()), false);
        assert_eq!(membership_status(&failed), ToggleStatus::Failed);
        assert_eq!(movie_watched(&failed), None);
    }

    #[test]
    fn ready_state_projects_membership_and_watch() {
        let saved = view(
            PersonalLoad::Ready(title(Some(5), MediaKind::Movie, Some(true))),
            true,
        );
        assert_eq!(
            membership_status(&saved),
            ToggleStatus::Known {
                on: true,
                pending: true
            }
        );
        assert_eq!(movie_watched(&saved), Some(true));
        let absent = view(
            PersonalLoad::Ready(title(None, MediaKind::Movie, None)),
            false,
        );
        assert_eq!(
            membership_status(&absent),
            ToggleStatus::Known {
                on: false,
                pending: false
            }
        );
        assert_eq!(movie_watched(&absent), Some(false));
        let show = view(
            PersonalLoad::Ready(title(Some(1), MediaKind::Series, None)),
            false,
        );
        assert_eq!(movie_watched(&show), None);
    }

    #[test]
    fn movies_reset_in_progress_and_hide_it() {
        let mut f = LibraryFilters {
            status: WatchFilter::InProgress,
            ..Default::default()
        };
        f.set_kind(LibraryKind::Series);
        assert_eq!(f.status, WatchFilter::InProgress);
        f.set_kind(LibraryKind::Movies);
        assert_eq!(f.status, WatchFilter::Any);
        assert!(
            !LibraryFilters::status_options(LibraryKind::Movies)
                .iter()
                .any(|(s, _)| *s == WatchFilter::InProgress)
        );
        assert!(
            LibraryFilters::status_options(LibraryKind::All)
                .iter()
                .any(|(s, _)| *s == WatchFilter::InProgress)
        );
    }

    #[test]
    fn query_defaults_and_trims_search() {
        let q = LibraryFilters::default().query("  dune ");
        assert_eq!(q.search, "dune");
        assert_eq!(q.sort, LibrarySort::RecentlyAdded);
        assert_eq!((q.offset, q.limit), (0, PAGE_SIZE));
        assert!(!LibraryFilters::default().is_filtered("  "));
        assert!(LibraryFilters::default().is_filtered("x"));
    }

    #[test]
    fn focus_prefers_next_then_previous_then_empty() {
        assert_eq!(focus_after_removal(1, 3), Some(1));
        assert_eq!(focus_after_removal(2, 2), Some(1));
        assert_eq!(focus_after_removal(0, 0), None);
        let old = keys(&["a", "b", "c"]);
        assert_eq!(refocus(&old, &keys(&["a", "c"]), "b"), Some(1));
        assert_eq!(refocus(&old, &keys(&["a", "b"]), "c"), Some(1));
        assert_eq!(refocus(&old, &keys(&["a", "b", "c"]), "b"), Some(1));
        assert_eq!(refocus(&old, &[], "b"), None);
        assert_eq!(refocus(&old, &keys(&["a"]), "zzz"), None);
    }
}
