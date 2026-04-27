// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SearchViewModel.h"

#include "api/CinemetaClient.h"
#include "config/SearchSettings.h"
#include "core/HttpErrorPresenter.h"
#include "ui/qml-bridge/ResultsListModel.h"

#include <KLocalizedString>

#include <QRegularExpression>

namespace kinema::ui::qml {

namespace {

constexpr int kDebounceMs = 250;
constexpr int kMinChars = 2;

bool looksLikeImdbId(const QString& s)
{
    static const QRegularExpression re(QStringLiteral("^tt\\d{5,}$"));
    return re.match(s).hasMatch();
}

} // namespace

SearchViewModel::SearchViewModel(api::CinemetaClient* cinemeta,
    config::SearchSettings& settings,
    QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_settings(&settings)
    , m_results(new ResultsListModel(this))
{
    m_debounce.setSingleShot(true);
    m_debounce.setInterval(kDebounceMs);
    connect(&m_debounce, &QTimer::timeout, this,
        &SearchViewModel::submit);

    connect(m_settings, &config::SearchSettings::recentQueriesChanged,
        this, &SearchViewModel::recentQueriesChanged);
}

void SearchViewModel::setQuery(const QString& q)
{
    if (m_query == q) {
        return;
    }
    m_query = q;
    Q_EMIT queryChanged();

    // Reset the debounce window every time the user edits.
    m_debounce.stop();

    const auto trimmed = m_query.trimmed();
    if (trimmed.isEmpty()) {
        // Cancel any in-flight result and drop back to Idle so the
        // recent-searches strip / placeholder show through.
        ++m_epoch;
        m_results->setIdle();
        return;
    }

    if (looksLikeImdbId(trimmed)) {
        // IMDB ids are deterministic — no point waiting.
        submit();
        return;
    }

    if (trimmed.size() < kMinChars) {
        // Don't pummel Cinemeta with single-character requests;
        // leave the model in its current state until the user
        // types more.
        return;
    }

    m_debounce.start();
}

void SearchViewModel::setKind(int kind)
{
    const auto k = (kind == static_cast<int>(api::MediaKind::Series))
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
    if (m_kind == k) {
        return;
    }
    m_kind = k;
    Q_EMIT kindChanged();
}

void SearchViewModel::submit()
{
    // Enter / IMDB-id / useRecent all converge here — cancel any
    // pending debounce so we don't fire a duplicate request.
    m_debounce.stop();

    const auto trimmed = m_query.trimmed();
    if (trimmed.isEmpty()) {
        // Empty submit lands us back on the Idle placeholder so the
        // Search-page binding doesn't get stuck on a stale result
        // grid after the user clears the field and presses Enter.
        m_results->setIdle();
        return;
    }
    auto t = runSearchTask(trimmed, m_kind);
    Q_UNUSED(t);
}

void SearchViewModel::clear()
{
    m_debounce.stop();
    if (!m_query.isEmpty()) {
        m_query.clear();
        Q_EMIT queryChanged();
    }
    // Bumping the epoch makes any in-flight response a no-op when
    // it lands.
    ++m_epoch;
    m_results->setIdle();
}

QStringList SearchViewModel::recentQueries() const
{
    return m_settings->recentQueries();
}

void SearchViewModel::useRecent(const QString& q)
{
    if (m_query != q) {
        m_query = q;
        Q_EMIT queryChanged();
    }
    submit();
}

void SearchViewModel::clearRecent()
{
    m_settings->clearRecentQueries();
}

void SearchViewModel::activate(int row)
{
    const auto* item = m_results->at(row);
    if (!item) {
        return;
    }
    if (item->kind == api::MediaKind::Series) {
        Q_EMIT openSeriesRequested(item->imdbId, item->title);
    } else {
        Q_EMIT openMovieRequested(item->imdbId, item->title);
    }
}

QCoro::Task<void> SearchViewModel::runSearchTask(QString text,
    api::MediaKind kind)
{
    const auto myEpoch = ++m_epoch;
    m_results->setLoading();
    Q_EMIT statusMessage(
        i18nc("@info:status",
            "Searching for \u201c%1\u201d\u2026", text),
        0);

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
            m_results->setResults({});
            Q_EMIT statusMessage(
                i18nc("@info:status", "No matches"), 4000);
            co_return;
        }

        m_results->setResults(std::move(results));
        // Successful result — record the query in MRU history so
        // the idle state can surface it next time.
        m_settings->addRecentQuery(text);
        Q_EMIT statusMessage(
            i18ncp("@info:status", "%1 result", "%1 results",
                m_results->rowCount()),
            3000);

    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        const auto msg = core::describeError(e, "search");
        m_results->setError(msg);
        Q_EMIT statusMessage(
            i18nc("@info:status", "Search failed"), 4000);
    }
}

} // namespace kinema::ui::qml
