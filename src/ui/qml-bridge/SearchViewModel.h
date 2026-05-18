// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::api {
class CinemetaClient;
}

namespace kinema::config {
class SearchSettings;
}

namespace kinema::controllers {
class LibraryController;
class WatchedController;
}

namespace kinema::ui::qml {

class ResultsListModel;

/**
 * Search page view-model. Replaces the widget-coupled
 * `controllers::SearchController` outright: same coroutine + epoch
 * pattern, same IMDB-id shortcut, but exposes its state through
 * Q_PROPERTY signals instead of writing into a `ResultsModel` /
 * `StateWidget` / `NavigationController` triplet.
 *
 * Bound to the `searchVm` context property; the matching results
 * grid binds to `searchVm.results` (a `ResultsListModel*`).
 *
 * Submission is explicit: the Search page binds `submit()` to the
 * SearchField's `Enter` and to the page's Refresh action. `setQuery`
 * only mutates the text and never fires a request. IMDB-id detection
 * inside `runSearchTask` still picks between `CinemetaClient::meta`
 * and `::search`, but it no longer changes *when* submission happens.
 */
class SearchViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    Q_PROPERTY(int kind READ kind WRITE setKind NOTIFY kindChanged)
    Q_PROPERTY(ResultsListModel* results READ results CONSTANT)

public:
    SearchViewModel(api::CinemetaClient* cinemeta,
        config::SearchSettings& settings,
        QObject* parent = nullptr);

    /// Optional injection for the PosterCard row context menu's
    /// title-level actions.
    void setLibraryController(controllers::LibraryController* lib);
    void setWatchedController(controllers::WatchedController* watched);

    QString query() const { return m_query; }
    void setQuery(const QString& q);

    /// `int` (not `domain::MediaKind`) for QML binding ergonomics.
    /// Values mirror `domain::MediaKind`: 0 = Movie, 1 = Series.
    int kind() const noexcept { return static_cast<int>(m_kind); }
    void setKind(int kind);

    ResultsListModel* results() const noexcept { return m_results; }

public Q_SLOTS:
    /// Run the current `query` + `kind` against Cinemeta. No-op on
    /// empty query (the page leaves the Idle placeholder up).
    void submit();

    /// Reset query to empty, results model to `Idle`. Does not
    /// cancel an in-flight request explicitly — the epoch guard
    /// drops its result on arrival.
    void clear();

    /// QML hook for clicking a result. Routes to `openMovieRequested`
    /// or `openSeriesRequested` based on the row's stored kind.
    void activate(int row);

    /// PosterCard context-menu hooks. Search rows already carry an
    /// IMDb id, so these dispatch directly to the controllers
    /// without the TMDB→IMDb resolution hop that Discover / Browse
    /// need.
    void addRowToLibrary(int row);
    void markRowWatched(int row);
    void findStreamsForRow(int row);

Q_SIGNALS:
    void queryChanged();
    void kindChanged();

    /// Routed by `MainController` to a passive notification (phase
    /// 04) and, in phase 05, to the real detail page push.
    void openMovieRequested(const QString& imdbId, const QString& title);
    void openSeriesRequested(const QString& imdbId, const QString& title);

    /// Context-menu "Find Streams" route, handled by ShellViewModel.
    void findMovieStreamsByImdbRequested(const QString& imdbId,
        const QString& title);
    void findSeriesStreamsByImdbRequested(const QString& imdbId,
        const QString& title);

    /// Forwarded into `MainController::passiveMessage` so the user
    /// sees "Searching for X…", "No matches", "Search failed".
    void statusMessage(const QString& text, int durationMs);

private:
    QCoro::Task<void> runSearchTask(QString text, domain::MediaKind kind);

    api::CinemetaClient* m_cinemeta;
    config::SearchSettings* m_settings;
    ResultsListModel* m_results;
    controllers::LibraryController* m_library {};
    controllers::WatchedController* m_watched {};

    QString m_query;
    domain::MediaKind m_kind = domain::MediaKind::Movie;
    quint64 m_epoch = 0;
};

} // namespace kinema::ui::qml
