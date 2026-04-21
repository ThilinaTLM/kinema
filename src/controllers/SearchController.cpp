// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/SearchController.h"

#include "api/CinemetaClient.h"
#include "controllers/NavigationController.h"
#include "controllers/Page.h"
#include "core/HttpErrorPresenter.h"
#include "ui/ResultsModel.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QRegularExpression>

namespace kinema::controllers {

namespace {

bool looksLikeImdbId(const QString& s)
{
    static const QRegularExpression re(QStringLiteral("^tt\\d{5,}$"));
    return re.match(s).hasMatch();
}

} // namespace

SearchController::SearchController(
    api::CinemetaClient* cinemeta,
    ui::ResultsModel* resultsModel,
    ui::StateWidget* resultsState,
    NavigationController* nav,
    QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_resultsModel(resultsModel)
    , m_resultsState(resultsState)
    , m_nav(nav)
{
}

void SearchController::runQuery(const QString& text, api::MediaKind kind)
{
    // Fire-and-forget; each invocation is its own coroutine. Stale
    // ones are filtered out inside runSearchTask via the epoch.
    auto t = runSearchTask(text, kind);
    Q_UNUSED(t);
}

QCoro::Task<void> SearchController::runSearchTask(
    QString text, api::MediaKind kind)
{
    const auto myEpoch = ++m_epoch;

    Q_EMIT queryStarted();
    m_resultsState->showLoading(i18nc("@info:status", "Searching\u2026"));
    m_nav->goTo(Page::SearchState);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Searching for \"%1\"\u2026", text), 0);

    try {
        QList<api::MetaSummary> results;
        if (looksLikeImdbId(text)) {
            auto detail = co_await m_cinemeta->meta(kind, text);
            if (myEpoch != m_epoch) {
                co_return;
            }
            results.append(detail.summary);
        } else {
            results = co_await m_cinemeta->search(kind, text);
            if (myEpoch != m_epoch) {
                co_return;
            }
        }

        if (results.isEmpty()) {
            m_resultsModel->reset({});
            m_resultsState->showIdle(
                i18nc("@label", "No matches"),
                i18nc("@info", "Nothing found for \"%1\".", text));
            m_nav->goTo(Page::SearchState);
            Q_EMIT statusMessage(
                i18nc("@info:status", "No matches"), 4000);
            co_return;
        }

        m_resultsModel->reset(results);
        m_nav->goTo(Page::SearchResults);
        Q_EMIT statusMessage(
            i18ncp("@info:status", "%1 result", "%1 results",
                results.size()),
            3000);
        Q_EMIT resultsReady(results.size());

    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        const auto msg = core::describeError(e, "search");
        m_resultsState->showError(msg, /*retryable=*/false);
        m_nav->goTo(Page::SearchState);
        Q_EMIT statusMessage(
            i18nc("@info:status", "Search failed"), 4000);
    }
}

} // namespace kinema::controllers
