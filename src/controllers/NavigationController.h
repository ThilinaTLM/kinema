// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "controllers/Page.h"

#include <QObject>

class QStackedWidget;
class QWidget;

namespace kinema::controllers {

/**
 * Owns the two-level QStackedWidget tree and the Page enum that names
 * every visible surface. Other controllers ask `current()` and call
 * `goTo(Page)` / `goBack()`; they never touch the stacks directly.
 *
 * NavigationController only toggles top-level widgets. Sub-page
 * transitions inside a detail pane (series streams \u2194 episodes)
 * are handled by SeriesDetailController listening to
 * `currentChanged(Page)`.
 *
 * Back rules (hierarchical, not history-based):
 *   SeriesStreams      \u2192 SeriesEpisodes
 *   MovieDetail        \u2192 results page that was visible when detail opened
 *   SeriesEpisodes     \u2192 results page that was visible when detail opened
 *   SearchState, SearchResults, Browse \u2192 DiscoverHome
 *   DiscoverHome       \u2192 no-op (canGoBack() is false)
 */
class NavigationController : public QObject
{
    Q_OBJECT
public:
    NavigationController(
        QStackedWidget* centerStack,
        QStackedWidget* resultsStack,
        QWidget* discover,
        QWidget* browse,
        QWidget* searchState,
        QWidget* searchResults,
        QWidget* movieDetail,
        QWidget* seriesDetail,
        QObject* parent = nullptr);

    Page current() const noexcept { return m_current; }
    bool canGoBack() const noexcept;

    /// For logs.
    Page resultsPageBeforeDetail() const noexcept
    {
        return m_resultsPageBeforeDetail;
    }

public Q_SLOTS:
    /// Switch to `page`. If `page` is a detail surface, the current
    /// results-stack page is remembered so Back returns to it.
    void goTo(Page page);

    /// Walk one level up the hierarchy. No-op at DiscoverHome.
    void goBack();

Q_SIGNALS:
    void currentChanged(Page);

private:
    void applyPage(Page);

    QStackedWidget* m_centerStack;
    QStackedWidget* m_resultsStack;
    QWidget* m_discover;
    QWidget* m_browse;
    QWidget* m_searchState;
    QWidget* m_searchResults;
    QWidget* m_movieDetail;
    QWidget* m_seriesDetail;

    Page m_current = Page::DiscoverHome;

    /// Which results-stack page was visible when the user opened a
    /// detail surface. Back from a detail returns here so e.g. opening
    /// a movie from Browse lands you back on Browse, not on Discover.
    Page m_resultsPageBeforeDetail = Page::DiscoverHome;
};

} // namespace kinema::controllers
