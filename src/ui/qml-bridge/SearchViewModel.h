// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QObject>
#include <QString>
#include <QStringList>
#include <QTimer>

#include <QCoro/QCoroTask>

namespace kinema::api {
class CinemetaClient;
}

namespace kinema::config {
class SearchSettings;
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
 * Live search-as-you-type: `setQuery` debounces submission through
 * `m_debounce` (250 ms, minimum 2 trimmed chars). IMDB ids are
 * detected eagerly and submitted without waiting; `submit()` (Enter
 * or `useRecent`) also bypasses the timer. Successful searches
 * push the query into `SearchSettings::addRecentQuery`, exposed
 * back to QML through `recentQueries`.
 */
class SearchViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    Q_PROPERTY(int kind READ kind WRITE setKind NOTIFY kindChanged)
    Q_PROPERTY(ResultsListModel* results READ results CONSTANT)
    Q_PROPERTY(QStringList recentQueries READ recentQueries
        NOTIFY recentQueriesChanged)

public:
    SearchViewModel(api::CinemetaClient* cinemeta,
        config::SearchSettings& settings,
        QObject* parent = nullptr);

    QString query() const { return m_query; }
    void setQuery(const QString& q);

    /// `int` (not `api::MediaKind`) for QML binding ergonomics.
    /// Values mirror `api::MediaKind`: 0 = Movie, 1 = Series.
    int kind() const noexcept { return static_cast<int>(m_kind); }
    void setKind(int kind);

    ResultsListModel* results() const noexcept { return m_results; }

    /// MRU list of recently-submitted queries (newest first).
    /// Forwards to `SearchSettings::recentQueries()`.
    QStringList recentQueries() const;

public Q_SLOTS:
    /// Run the current `query` + `kind` against Cinemeta. No-op on
    /// empty query (the page leaves the Idle placeholder up).
    /// Stops the debounce timer so Enter / IMDB-id paths submit
    /// immediately.
    void submit();

    /// Reset query to empty, results model to `Idle`. Does not
    /// cancel an in-flight request explicitly — the epoch guard
    /// drops its result on arrival.
    void clear();

    /// QML hook for clicking a result. Routes to `openMovieRequested`
    /// or `openSeriesRequested` based on the row's stored kind.
    void activate(int row);

    /// Re-run a recent search. Sets `query` and submits without
    /// waiting for the debounce window.
    Q_INVOKABLE void useRecent(const QString& q);

    /// Wipe the recent-queries list (forwards to settings).
    Q_INVOKABLE void clearRecent();

Q_SIGNALS:
    void queryChanged();
    void kindChanged();
    void recentQueriesChanged();

    /// Routed by `MainController` to a passive notification (phase
    /// 04) and, in phase 05, to the real detail page push.
    void openMovieRequested(const QString& imdbId, const QString& title);
    void openSeriesRequested(const QString& imdbId, const QString& title);

    /// Forwarded into `MainController::passiveMessage` so the user
    /// sees "Searching for X…", "No matches", "Search failed".
    void statusMessage(const QString& text, int durationMs);

private:
    QCoro::Task<void> runSearchTask(QString text, api::MediaKind kind);

    api::CinemetaClient* m_cinemeta;
    config::SearchSettings* m_settings;
    ResultsListModel* m_results;
    QTimer m_debounce;

    QString m_query;
    api::MediaKind m_kind = api::MediaKind::Movie;
    quint64 m_epoch = 0;
};

} // namespace kinema::ui::qml
