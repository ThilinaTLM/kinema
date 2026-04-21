// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/NavigationController.h"

#include <QStackedWidget>
#include <QWidget>

namespace kinema::controllers {

const char* toString(Page p)
{
    switch (p) {
    case Page::DiscoverHome:   return "DiscoverHome";
    case Page::SearchState:    return "SearchState";
    case Page::SearchResults:  return "SearchResults";
    case Page::Browse:         return "Browse";
    case Page::MovieDetail:    return "MovieDetail";
    case Page::SeriesEpisodes: return "SeriesEpisodes";
    case Page::SeriesStreams:  return "SeriesStreams";
    }
    return "?";
}

bool isResultsPage(Page p)
{
    return p == Page::DiscoverHome || p == Page::SearchState
        || p == Page::SearchResults || p == Page::Browse;
}

bool isDetailPage(Page p)
{
    return p == Page::MovieDetail || p == Page::SeriesEpisodes
        || p == Page::SeriesStreams;
}

NavigationController::NavigationController(
    QStackedWidget* centerStack,
    QStackedWidget* resultsStack,
    QWidget* discover,
    QWidget* browse,
    QWidget* searchState,
    QWidget* searchResults,
    QWidget* movieDetail,
    QWidget* seriesDetail,
    QObject* parent)
    : QObject(parent)
    , m_centerStack(centerStack)
    , m_resultsStack(resultsStack)
    , m_discover(discover)
    , m_browse(browse)
    , m_searchState(searchState)
    , m_searchResults(searchResults)
    , m_movieDetail(movieDetail)
    , m_seriesDetail(seriesDetail)
{
}

bool NavigationController::canGoBack() const noexcept
{
    return m_current != Page::DiscoverHome;
}

void NavigationController::goTo(Page page)
{
    if (page == m_current) {
        return;
    }

    // Entering a detail surface from a results-stack page: remember
    // which results page to return to on Back.
    if (!isDetailPage(m_current) && isDetailPage(page)
        && isResultsPage(m_current)) {
        m_resultsPageBeforeDetail = m_current;
    }

    applyPage(page);
    m_current = page;
    Q_EMIT currentChanged(page);
}

void NavigationController::goBack()
{
    switch (m_current) {
    case Page::DiscoverHome:
        return;
    case Page::SearchState:
    case Page::SearchResults:
    case Page::Browse:
        goTo(Page::DiscoverHome);
        return;
    case Page::MovieDetail:
    case Page::SeriesEpisodes:
        goTo(m_resultsPageBeforeDetail);
        return;
    case Page::SeriesStreams:
        goTo(Page::SeriesEpisodes);
        return;
    }
}

void NavigationController::applyPage(Page page)
{
    switch (page) {
    case Page::DiscoverHome:
        m_centerStack->setCurrentWidget(m_resultsStack);
        m_resultsStack->setCurrentWidget(m_discover);
        break;
    case Page::SearchState:
        m_centerStack->setCurrentWidget(m_resultsStack);
        m_resultsStack->setCurrentWidget(m_searchState);
        break;
    case Page::SearchResults:
        m_centerStack->setCurrentWidget(m_resultsStack);
        m_resultsStack->setCurrentWidget(m_searchResults);
        break;
    case Page::Browse:
        m_centerStack->setCurrentWidget(m_resultsStack);
        m_resultsStack->setCurrentWidget(m_browse);
        break;
    case Page::MovieDetail:
        m_centerStack->setCurrentWidget(m_movieDetail);
        break;
    case Page::SeriesEpisodes:
    case Page::SeriesStreams:
        // Sub-page toggle belongs to SeriesDetailController, which
        // listens to currentChanged(Page) and flips the pane's
        // internal stack. We just ensure the pane is visible.
        m_centerStack->setCurrentWidget(m_seriesDetail);
        break;
    }
}

} // namespace kinema::controllers
