// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/search/SearchViewModel.h"

#include "api/cinemeta/CinemetaClient.h"
#include "api/common/MetadataQuery.h"
#include "api/tmdb/TmdbClient.h"
#include "config/SearchSettings.h"
#include "controllers/LibraryController.h"
#include "controllers/WatchedController.h"
#include "core/io/HttpErrorPresenter.h"
#include "ui/qml-bridge/search/ResultsListModel.h"

#include <KLocalizedString>

#include <algorithm>
#include <exception>

namespace kinema::ui::qml {

namespace {

constexpr int kTmdbFallbackLimit = 10;

domain::MediaKind oppositeKind(domain::MediaKind kind)
{
    return kind == domain::MediaKind::Movie ? domain::MediaKind::Series : domain::MediaKind::Movie;
}

bool hasUsableSummary(const domain::MetaSummary& summary)
{
    return !summary.imdbId.isEmpty() && !summary.title.isEmpty();
}

} // namespace

SearchViewModel::SearchViewModel(api::CinemetaClient* cinemeta,
                                 api::TmdbClient* tmdb,
                                 config::SearchSettings& settings,
                                 QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_tmdb(tmdb)
    , m_settings(&settings)
    , m_results(new ResultsListModel(this))
    , m_kind(settings.kind())
{ }

void SearchViewModel::setQuery(const QString& q)
{
    if (m_query == q) {
        return;
    }
    m_query = q;
    Q_EMIT queryChanged();
}

void SearchViewModel::setKind(int kind)
{
    const auto k = (kind == static_cast<int>(domain::MediaKind::Series)) ? domain::MediaKind::Series
                                                                         : domain::MediaKind::Movie;
    if (m_kind == k) {
        return;
    }
    m_kind = k;
    m_settings->setKind(k);
    Q_EMIT kindChanged();
}

void SearchViewModel::submit()
{
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
    if (!m_query.isEmpty()) {
        m_query.clear();
        Q_EMIT queryChanged();
    }
    // Bumping the epoch makes any in-flight response a no-op when
    // it lands.
    ++m_epoch;
    m_results->setIdle();
}

void SearchViewModel::activate(int row)
{
    const auto* item = m_results->at(row);
    if (!item) {
        return;
    }
    if (item->kind == domain::MediaKind::Series) {
        Q_EMIT openSeriesRequested(item->imdbId, item->title);
    } else {
        Q_EMIT openMovieRequested(item->imdbId, item->title);
    }
}

void SearchViewModel::setLibraryController(controllers::LibraryController* lib)
{
    m_library = lib;
}

void SearchViewModel::setWatchedController(controllers::WatchedController* watched)
{
    m_watched = watched;
}

void SearchViewModel::addRowToLibrary(int row)
{
    const auto* item = m_results->at(row);
    if (!item || !m_library || item->imdbId.isEmpty()) {
        return;
    }
    auto task = m_library->saveByImdbId(item->imdbId, item->kind);
    Q_UNUSED(task);
}

void SearchViewModel::markRowWatched(int row)
{
    const auto* item = m_results->at(row);
    if (!item || !m_watched || item->imdbId.isEmpty()) {
        return;
    }
    if (item->kind == domain::MediaKind::Series) {
        // Mirror the Discover / Browse behaviour: marking an entire
        // series watched from a list is too coarse; nudge the user
        // toward the detail page instead.
        Q_EMIT statusMessage(i18nc("@info:status",
                                   "Open \u201c%1\u201d to mark individual episodes "
                                   "as watched.",
                                   item->title),
                             4000);
        return;
    }
    m_watched->setMovieWatched(item->imdbId, /*watched=*/true);
    Q_EMIT statusMessage(i18nc("@info:status", "Marked \u201c%1\u201d as watched.", item->title),
                         3000);
}

void SearchViewModel::findStreamsForRow(int row)
{
    const auto* item = m_results->at(row);
    if (!item || item->imdbId.isEmpty()) {
        return;
    }
    if (item->kind == domain::MediaKind::Series) {
        Q_EMIT findSeriesStreamsByImdbRequested(item->imdbId, item->title);
    } else {
        Q_EMIT findMovieStreamsByImdbRequested(item->imdbId, item->title);
    }
}

QCoro::Task<void> SearchViewModel::runSearchTask(QString text, domain::MediaKind kind)
{
    const auto myEpoch = ++m_epoch;
    m_results->setLoading();
    Q_EMIT statusMessage(i18nc("@info:status", "Searching for \u201c%1\u201d\u2026", text), 0);

    try {
        QList<domain::MetaSummary> results;
        if (const auto imdbId = api::metadata_query::extractImdbTitleId(text)) {
            const auto summary = co_await lookupExactImdb(*imdbId, kind);
            if (myEpoch != m_epoch) {
                co_return;
            }
            if (summary) {
                results.append(*summary);
            }
        } else {
            results = co_await m_cinemeta->search(kind, text);
            if (myEpoch != m_epoch) {
                co_return;
            }
            if (results.isEmpty()) {
                results = co_await searchTmdbFallback(kind, text);
                if (myEpoch != m_epoch) {
                    co_return;
                }
            }
        }

        if (results.isEmpty()) {
            m_results->setResults({});
            Q_EMIT statusMessage(i18nc("@info:status", "No matches"), 4000);
            co_return;
        }

        m_results->setResults(std::move(results));
        Q_EMIT statusMessage(
            i18ncp("@info:status", "%1 result", "%1 results", m_results->rowCount()), 3000);

    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        const auto msg = core::describeError(e, "search");
        m_results->setError(msg);
        Q_EMIT statusMessage(i18nc("@info:status", "Search failed"), 4000);
    }
}

QCoro::Task<std::optional<domain::MetaSummary>>
SearchViewModel::lookupExactImdb(QString imdbId, domain::MediaKind preferredKind)
{
    std::exception_ptr firstException;

    try {
        auto detail = co_await m_cinemeta->meta(preferredKind, imdbId);
        if (hasUsableSummary(detail.summary)) {
            co_return detail.summary;
        }
    } catch (...) {
        firstException = std::current_exception();
    }

    try {
        auto detail = co_await m_cinemeta->meta(oppositeKind(preferredKind), imdbId);
        if (hasUsableSummary(detail.summary)) {
            co_return detail.summary;
        }
    } catch (...) {
        if (firstException) {
            std::rethrow_exception(firstException);
        }
        throw;
    }

    if (firstException) {
        std::rethrow_exception(firstException);
    }
    co_return std::nullopt;
}

QCoro::Task<QList<domain::MetaSummary>> SearchViewModel::searchTmdbFallback(domain::MediaKind kind,
                                                                            QString text)
{
    QList<domain::MetaSummary> out;
    if (!m_tmdb || !m_tmdb->hasToken()) {
        co_return out;
    }

    domain::DiscoverPageResult page;
    try {
        page = co_await m_tmdb->search(kind, std::move(text), 1);
    } catch (...) {
        co_return out;
    }

    const int limit = std::min(kTmdbFallbackLimit, static_cast<int>(page.items.size()));
    out.reserve(limit);
    for (int i = 0; i < limit; ++i) {
        const auto& item = page.items.at(i);
        QString imdbId;
        try {
            imdbId = item.kind == domain::MediaKind::Series
                         ? co_await m_tmdb->imdbIdForTmdbSeries(item.tmdbId)
                         : co_await m_tmdb->imdbIdForTmdbMovie(item.tmdbId);
        } catch (...) {
            continue;
        }
        if (imdbId.isEmpty()) {
            continue;
        }

        domain::MetaSummary summary;
        summary.imdbId = std::move(imdbId);
        summary.kind = item.kind;
        summary.title = item.title;
        summary.year = item.year;
        summary.poster = item.poster;
        summary.description = item.overview;
        out.append(std::move(summary));
    }

    co_return out;
}

} // namespace kinema::ui::qml
