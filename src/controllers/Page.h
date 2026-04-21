// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

namespace kinema::controllers {

/**
 * Canonical navigation surfaces visible in the main window.
 *
 * The Page enum is the single source of truth for "what's on screen":
 * NavigationController maps each value to the matching QStackedWidget
 * child, and every controller asks the navigation controller which
 * page is current rather than comparing widget pointers.
 */
enum class Page {
    DiscoverHome,     ///< Results stack showing the DiscoverPage
    SearchState,      ///< Results stack showing StateWidget (idle/loading/error)
    SearchResults,    ///< Results stack showing the grid of search results
    Browse,           ///< Results stack showing BrowsePage
    MovieDetail,      ///< DetailPane visible
    SeriesEpisodes,   ///< SeriesDetailPane, episodes sub-page
    SeriesStreams,    ///< SeriesDetailPane, streams sub-page
};

/// Human-readable tag for logs.
const char* toString(Page);

/// True when the page is rendered inside the inner "results" stack
/// (Discover / SearchState / SearchResults / Browse). Those pages
/// share the same back-target (DiscoverHome) and close on detail-open.
bool isResultsPage(Page);

/// True when the page is a detail surface (movie or series, any sub-page).
bool isDetailPage(Page);

} // namespace kinema::controllers
