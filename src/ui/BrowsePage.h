// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"

#include <QModelIndex>
#include <QWidget>

#include <QCoro/QCoroTask>

class QLabel;
class QPushButton;
class QScrollArea;
class QStackedWidget;

namespace kinema::api {
class TmdbClient;
}

namespace kinema::config {
class BrowseSettings;
}

namespace kinema::ui {

class BrowseFilterBar;
class DiscoverCardDelegate;
class DiscoverGridView;
class DiscoverRowModel;
class ImageLoader;
class StateWidget;

/**
 * Filterable catalog surface backed by TMDB /discover.
 *
 * Layout:
 *
 *   +------------------------------------------------------------+
 *   |  BrowseFilterBar  (sticky)                                 |
 *   +------------------------------------------------------------+
 *   |  QScrollArea                                               |
 *   |    [ DiscoverGridView ]     \u2014 wraps, no row cap           |
 *   |    [ StateWidget overlay ]  \u2014 loading/empty/error         |
 *   |    [ Load-more footer ]     \u2014 button + inline status       |
 *   +------------------------------------------------------------+
 *
 * Page-level states (via m_pageStack):
 *   idx 0 = content (filter bar + scrolling grid)
 *   idx 1 = CTA    (no-token / auth-failed)
 *
 * Paging:
 *   - `m_page` is the next page to request.
 *   - `refresh()` resets page/model and fetches page 1.
 *   - Load-more fetches the next page and appends into the model.
 *   - Stale responses are filtered out via `m_epoch`.
 *
 * Parent (MainWindow) wires itemActivated() to the existing
 * openFromDiscover pipeline and settingsRequested() to the Settings
 * dialog. The Browse page itself does not know about IMDB.
 */
class BrowsePage : public QWidget
{
    Q_OBJECT
public:
    BrowsePage(api::TmdbClient* tmdb,
        ImageLoader* images,
        config::BrowseSettings& settings,
        QWidget* parent = nullptr);

    /// Re-fetch page 1 using the current filter-bar state. Used on
    /// first show, whenever filters change, and from the CTA/empty
    /// retry paths.
    void refresh();

    /// Swap to the "set up TMDB to enable Browse" CTA. Called by
    /// MainWindow when the effective TMDB token is empty.
    void showNotConfigured();

Q_SIGNALS:
    /// The user activated a grid card. MainWindow resolves the TMDB
    /// id to an IMDB id and opens the existing detail flow.
    void itemActivated(const api::DiscoverItem& item);

    /// The "Open Settings" button on the page-level CTA was pressed.
    void settingsRequested();

private:
    QCoro::Task<void> loadFirstPage();
    QCoro::Task<void> loadNextPage();
    QCoro::Task<void> loadGenresFor(api::MediaKind kind);

    void scheduleRefresh();            // debounce wrapper
    void showPageCta(const QString& title, const QString& body);
    void showGridOrState();            // centralised state routing
    void updateLoadMoreVisibility();

    void onCardActivated(const QModelIndex& idx);

    api::TmdbClient* m_tmdb;
    ImageLoader* m_images;
    config::BrowseSettings& m_settings;

    // Page-level stack (idx 0 = content, idx 1 = CTA).
    QStackedWidget* m_pageStack {};
    QWidget* m_pageCta {};
    QLabel* m_pageCtaTitle {};
    QLabel* m_pageCtaBody {};

    // Content column.
    BrowseFilterBar* m_filterBar {};
    QScrollArea* m_scroll {};
    QWidget* m_scrollContents {};
    QStackedWidget* m_gridStack {};     // [0] state, [1] grid
    StateWidget* m_state {};
    DiscoverGridView* m_grid {};
    DiscoverRowModel* m_model {};
    DiscoverCardDelegate* m_delegate {};
    QWidget* m_loadMoreBar {};
    QPushButton* m_loadMoreBtn {};
    QLabel* m_loadMoreStatus {};

    // Paging / request-tracking state.
    int m_page = 1;                     // next page to request
    int m_totalPages = 0;               // 0 means "unknown yet"
    bool m_loading = false;             // any in-flight request
    quint64 m_epoch = 0;

    // Page-level CTA latch so one section's 401 doesn't churn back
    // into a retryable error while the page-CTA is already up.
    bool m_pageAuthFailed = false;

    // Genre-cache tracker. We fetch the genre list lazily per kind
    // and remember which kinds have already been fetched (or are in
    // flight) so successive refreshes don't duplicate the fetch.
    bool m_genresLoadedMovie = false;
    bool m_genresLoadedSeries = false;
    bool m_genresLoadingMovie = false;
    bool m_genresLoadingSeries = false;

    // Debounce flag for filter-bar signal coalescing.
    bool m_refreshQueued = false;
};

} // namespace kinema::ui
