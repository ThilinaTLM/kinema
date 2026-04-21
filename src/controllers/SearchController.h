// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

class QWidget;

namespace kinema::api {
class CinemetaClient;
}

namespace kinema::ui {
class ResultsModel;
class StateWidget;
}

namespace kinema::controllers {

class NavigationController;

/**
 * Runs Cinemeta search queries as coroutines and routes the result
 * into ResultsModel / StateWidget. Owns the stale-response epoch so
 * rapid retyping in the search bar doesn't render out-of-order
 * results.
 *
 * IMDB-id shortcut: queries matching `tt\\d{5,}` are treated as direct
 * lookups via cinemeta->meta() and synthesised into a single-item
 * result list.
 */
class SearchController : public QObject
{
    Q_OBJECT
public:
    SearchController(
        api::CinemetaClient* cinemeta,
        ui::ResultsModel* resultsModel,
        ui::StateWidget* resultsState,
        NavigationController* nav,
        QObject* parent = nullptr);

public Q_SLOTS:
    /// Fire-and-forget; each call supersedes the previous one.
    void runQuery(const QString& text, api::MediaKind kind);

Q_SIGNALS:
    /// Fired at the top of each query. Consumers that need to reset
    /// per-request state (e.g. delegate request trackers) listen here.
    void queryStarted();

    /// Fired after a query lands results. The caller can use this
    /// (for example) to move focus into the results view.
    void resultsReady(int count);

    /// Status-bar message; MainWindow routes to statusBar().
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    QCoro::Task<void> runSearchTask(QString text, api::MediaKind kind);

    api::CinemetaClient* m_cinemeta;
    ui::ResultsModel* m_resultsModel;
    ui::StateWidget* m_resultsState;
    NavigationController* m_nav;

    quint64 m_epoch = 0;
};

} // namespace kinema::controllers
