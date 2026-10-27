//! Header Filters popover bodies for both Browse scopes. Choices apply
//! immediately; the host owns state and receives the next value.
//!
//! - Discover: provider-backed facets (sort, genres, release, rating) when TMDB
//!   Discover can serve them; otherwise only a sort of the loaded results, with
//!   a note saying why the other facets are unavailable.
//! - Library: watch status and sort of the local query.
use super::{LibraryFilters, Sort};
use crate::widgets::{
    choice_card::ChoiceChip,
    filter_panel::{FilterPanel, FilterSection},
};
use gpui::{prelude::*, *};
use kinema_core::{
    MediaKind,
    discover::{DiscoverFilters, DiscoverSort, YearRange, genres_for},
    library::{LibrarySort, WatchFilter},
};
use std::rc::Rc;

/// Below this window width header chips collapse to their icon.
pub(crate) const COMPACT_BELOW: Pixels = px(1100.);

type Apply<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// Server-side Discover sorts, in display order.
pub(crate) fn discover_sorts() -> [(DiscoverSort, &'static str); 4] {
    [
        (DiscoverSort::Popular, "Popular"),
        (DiscoverSort::TopRated, "Top rated"),
        (DiscoverSort::Newest, "Newest"),
        (DiscoverSort::Title, "Title A–Z"),
    ]
}

/// Release-period presets; `None` is "Any".
pub(crate) fn release_presets() -> [(Option<YearRange>, &'static str); 6] {
    let range = |from, to| Some(YearRange { from, to });
    [
        (None, "Any time"),
        (range(2020, 2029), "2020s"),
        (range(2010, 2019), "2010s"),
        (range(2000, 2009), "2000s"),
        (range(1990, 1999), "1990s"),
        (range(1900, 1989), "Classics"),
    ]
}

/// Minimum TMDB score presets; `None` is "Any".
pub(crate) fn rating_presets() -> [(Option<u8>, &'static str); 4] {
    [
        (None, "Any"),
        (Some(6), "6+"),
        (Some(7), "7+"),
        (Some(8), "8+"),
    ]
}

/// Why provider facets are unavailable, if they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiscoverMode {
    /// TMDB Discover serves every facet.
    Provider,
    /// A search is active; provider facets pause, loaded results can be sorted.
    Searching,
    /// The active provider has no faceted Discover endpoint.
    Unsupported,
}

/// Active count shown on the Discover chip for the current mode.
pub(crate) fn discover_active(mode: DiscoverMode, filters: &DiscoverFilters, local: Sort) -> usize {
    match mode {
        DiscoverMode::Provider => filters.active_count(),
        _ => usize::from(local != Sort::Provider),
    }
}

pub(crate) struct DiscoverPanel {
    pub mode: DiscoverMode,
    pub kind: MediaKind,
    pub filters: DiscoverFilters,
    /// Client-side order of loaded results, used when facets are unavailable.
    pub local: Sort,
    pub apply: Apply<DiscoverFilters>,
    pub apply_local: Apply<Sort>,
}

impl DiscoverPanel {
    pub(crate) fn render(self) -> FilterPanel {
        let Self {
            mode,
            kind,
            filters,
            local,
            apply,
            apply_local,
        } = self;
        if mode != DiscoverMode::Provider {
            let mut sort = FilterSection::new("Sort").hint("loaded results");
            for (value, label) in Sort::options() {
                let apply_local = apply_local.clone();
                sort = sort.child(
                    ChoiceChip::new(SharedString::from(format!("local-sort-{label}")), label)
                        .selected(local == value)
                        .on_click(move |window, cx| apply_local(value, window, cx)),
                );
            }
            let panel = FilterPanel::new("Filters")
                .section(sort)
                .footer(match mode {
                    DiscoverMode::Searching => {
                        "Genre, release and rating filters pause while searching."
                    }
                    _ => "Genre, release and rating filters need TMDB as the Discover provider.",
                });
            return if local == Sort::Provider {
                panel
            } else {
                panel.on_reset(move |window, cx| apply_local(Sort::Provider, window, cx))
            };
        }

        let choose = |id: String,
                      label: SharedString,
                      selected: bool,
                      next: DiscoverFilters,
                      apply: &Apply<DiscoverFilters>| {
            let apply = apply.clone();
            ChoiceChip::new(SharedString::from(id), label)
                .selected(selected)
                .on_click(move |window, cx| apply(next.clone(), window, cx))
        };

        let mut sort = FilterSection::new("Sort");
        for (value, label) in discover_sorts() {
            let mut next = filters.clone();
            next.sort = value;
            sort = sort.child(choose(
                format!("discover-sort-{label}"),
                label.into(),
                filters.sort == value,
                next,
                &apply,
            ));
        }

        let mut genres = FilterSection::new("Genres").hint("match all selected");
        for genre in genres_for(kind) {
            let selected = filters.genres.contains(&genre.id);
            let mut next = filters.clone();
            if selected {
                next.genres.remove(&genre.id);
            } else {
                next.genres.insert(genre.id);
            }
            genres = genres.child(
                choose(
                    format!("discover-genre-{}", genre.id),
                    genre.name.into(),
                    selected,
                    next,
                    &apply,
                )
                .multi(),
            );
        }

        let mut release = FilterSection::new("Released");
        for (value, label) in release_presets() {
            let mut next = filters.clone();
            next.years = value;
            release = release.child(choose(
                format!("discover-release-{label}"),
                label.into(),
                filters.years == value,
                next,
                &apply,
            ));
        }

        let mut rating = FilterSection::new("Rating").hint("TMDB score");
        for (value, label) in rating_presets() {
            let mut next = filters.clone();
            next.min_rating = value;
            rating = rating.child(choose(
                format!("discover-rating-{label}"),
                label.into(),
                filters.min_rating == value,
                next,
                &apply,
            ));
        }

        let panel = FilterPanel::new("Filters")
            .section(sort)
            .section(genres)
            .section(release)
            .section(rating);
        if filters.is_default() {
            panel
        } else {
            panel.on_reset(move |window, cx| apply(DiscoverFilters::default(), window, cx))
        }
    }
}

/// Number of non-default Library choices (kind lives in the toolbar).
pub(crate) fn library_active(filters: &LibraryFilters) -> usize {
    usize::from(filters.status != WatchFilter::Any)
        + usize::from(filters.sort != LibrarySort::RecentlyAdded)
}

pub(crate) fn library_panel(
    filters: LibraryFilters,
    apply: impl Fn(LibraryFilters, &mut Window, &mut App) + 'static,
) -> FilterPanel {
    let apply: Apply<LibraryFilters> = Rc::new(apply);
    let mut status = FilterSection::new("Show");
    for (value, label) in LibraryFilters::status_options(filters.kind) {
        let apply = apply.clone();
        let mut next = filters;
        next.status = value;
        status = status.child(
            ChoiceChip::new(SharedString::from(format!("library-status-{label}")), label)
                .selected(filters.status == value)
                .on_click(move |window, cx| apply(next, window, cx)),
        );
    }
    let mut sort = FilterSection::new("Sort");
    for (value, label) in LibraryFilters::sort_options() {
        let apply = apply.clone();
        let mut next = filters;
        next.sort = value;
        sort = sort.child(
            ChoiceChip::new(SharedString::from(format!("library-sort-{label}")), label)
                .selected(filters.sort == value)
                .on_click(move |window, cx| apply(next, window, cx)),
        );
    }
    let panel = FilterPanel::new("Filters").section(status).section(sort);
    if library_active(&filters) == 0 {
        panel
    } else {
        panel.on_reset(move |window, cx| {
            apply(
                LibraryFilters {
                    kind: filters.kind,
                    ..LibraryFilters::default()
                },
                window,
                cx,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn presets_start_with_any_and_are_valid() {
        assert_eq!(release_presets()[0].0, None);
        assert!(
            release_presets()
                .iter()
                .filter_map(|(r, _)| *r)
                .all(YearRange::is_valid)
        );
        assert_eq!(rating_presets()[0].0, None);
        assert_eq!(discover_sorts()[0].0, DiscoverSort::default());
    }

    #[::core::prelude::v1::test]
    fn active_count_follows_mode() {
        let mut filters = DiscoverFilters::default();
        filters.genres.insert(28);
        assert_eq!(
            discover_active(DiscoverMode::Provider, &filters, Sort::Title),
            1
        );
        // Provider facets are paused while searching; only the local sort counts.
        assert_eq!(
            discover_active(DiscoverMode::Searching, &filters, Sort::Title),
            1
        );
        assert_eq!(
            discover_active(DiscoverMode::Unsupported, &filters, Sort::Provider),
            0
        );
    }

    #[::core::prelude::v1::test]
    fn library_active_ignores_kind() {
        let mut filters = LibraryFilters::default();
        filters.set_kind(kinema_core::library::LibraryKind::Movies);
        assert_eq!(library_active(&filters), 0);
        filters.status = WatchFilter::Watched;
        filters.sort = LibrarySort::Title;
        assert_eq!(library_active(&filters), 2);
    }
}
