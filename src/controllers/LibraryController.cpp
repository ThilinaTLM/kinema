// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/LibraryController.h"

#include "api/CinemetaClient.h"
#include "api/Media.h"
#include "core/LibraryStore.h"
#include "kinema_log_app.h"

#include <KLocalizedString>

#include <QDateTime>

#include <algorithm>
#include <exception>

namespace kinema::controllers {

namespace {

/// Cap concurrent Cinemeta backfill requests so a large library
/// doesn't open dozens of HTTP connections at once. Cinemeta is
/// rate-tolerant but the user's network and CPU are not.
constexpr int kBackfillConcurrency = 4;

bool needsBackfill(const api::LibraryTitle& t)
{
    return t.genres.isEmpty()
        && !t.imdbRating.has_value()
        && !t.runtimeMinutes.has_value()
        && t.cast.isEmpty();
}

api::LibraryTitle titleFromMeta(const api::MetaDetail& meta,
    api::MediaKind kind)
{
    const auto& s = meta.summary;
    api::LibraryTitle t;
    t.kind = kind;
    t.imdbId = s.imdbId;
    t.title = s.title;
    t.year = s.year;
    t.poster = s.poster;
    t.backdrop = meta.background;
    t.overview = s.description;
    t.releaseDate = s.released;
    t.updatedAt = QDateTime::currentDateTimeUtc();
    t.genres = meta.genres;
    t.imdbRating = s.imdbRating;
    t.runtimeMinutes = meta.runtimeMinutes;
    t.cast = meta.cast;
    return t;
}

api::LibraryEpisode episodeFromApi(const QString& seriesImdbId,
    const api::Episode& ep)
{
    api::LibraryEpisode out;
    out.seriesImdbId = seriesImdbId;
    out.season = ep.season;
    out.episode = ep.number;
    out.title = ep.title;
    out.overview = ep.description;
    out.thumbnail = ep.thumbnail;
    out.releaseDate = ep.released;
    out.updatedAt = QDateTime::currentDateTimeUtc();
    return out;
}

} // namespace

LibraryController::LibraryController(core::LibraryStore& store,
    api::CinemetaClient* cinemeta, QObject* parent)
    : QObject(parent)
    , m_store(store)
    , m_cinemeta(cinemeta)
{
    connect(&m_store, &core::LibraryStore::changed,
        this, &LibraryController::changed);
}

LibraryController::~LibraryController() = default;

bool LibraryController::isInLibrary(api::MediaKind kind,
    const QString& imdbId) const
{
    return m_store.contains(kind, imdbId);
}

std::optional<api::LibraryTitle> LibraryController::title(
    api::MediaKind kind, const QString& imdbId) const
{
    return m_store.find(kind, imdbId);
}

QList<api::LibraryTitle> LibraryController::titles() const
{
    return m_store.titles();
}

QList<api::LibraryEpisode> LibraryController::episodesForSeries(
    const QString& imdbId) const
{
    return m_store.episodesForSeries(imdbId);
}

void LibraryController::saveMovie(const api::MetaDetail& meta)
{
    auto t = titleFromMeta(meta, api::MediaKind::Movie);
    if (t.imdbId.isEmpty() || t.title.isEmpty()) {
        return;
    }
    m_store.upsertTitle(t);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Added \u201c%1\u201d to Library.", t.title), 3000);
}

void LibraryController::saveSeries(const api::SeriesDetail& detail)
{
    auto t = titleFromMeta(detail.meta, api::MediaKind::Series);
    if (t.imdbId.isEmpty() || t.title.isEmpty()) {
        return;
    }
    m_store.upsertTitle(t);

    QList<api::LibraryEpisode> rows;
    rows.reserve(detail.episodes.size());
    for (const auto& ep : detail.episodes) {
        rows.append(episodeFromApi(t.imdbId, ep));
    }
    m_store.upsertEpisodes(t.imdbId, rows);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Added \u201c%1\u201d to Library.", t.title), 3000);
}

void LibraryController::removeFromLibrary(api::MediaKind kind,
    const QString& imdbId)
{
    const auto t = m_store.find(kind, imdbId);
    m_store.remove(kind, imdbId);
    if (t) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Removed \u201c%1\u201d from Library.", t->title),
            3000);
    }
}

void LibraryController::backfillMetadata()
{
    if (!m_cinemeta) {
        return;
    }
    // Re-scan whenever this is invoked. Already-backfilled rows are
    // filtered out by `needsBackfill`; the queue stays bounded.
    for (const auto& t : m_store.titles()) {
        if (!needsBackfill(t)) {
            continue;
        }
        // Skip rows already enqueued (a re-call before the first
        // batch finished). O(n) scan is fine — backfill queues are
        // small and run once per launch.
        const bool dup = std::any_of(
            m_backfillQueue.begin(), m_backfillQueue.end(),
            [&](const api::LibraryTitle& q) {
                return q.kind == t.kind && q.imdbId == t.imdbId;
            });
        if (!dup) {
            m_backfillQueue.enqueue(t);
        }
    }
    pumpBackfill();
}

void LibraryController::pumpBackfill()
{
    while (m_backfillInFlight < kBackfillConcurrency
        && !m_backfillQueue.isEmpty()) {
        auto seed = m_backfillQueue.dequeue();
        ++m_backfillInFlight;
        // Fire-and-forget: the coroutine is owned by its own task
        // frame; we just drop the handle.
        auto task = runBackfillOne(std::move(seed));
        Q_UNUSED(task);
    }
}

QCoro::Task<void> LibraryController::runBackfillOne(
    api::LibraryTitle seed)
{
    try {
        api::LibraryTitle refreshed;
        refreshed.kind = seed.kind;
        refreshed.imdbId = seed.imdbId;
        if (seed.kind == api::MediaKind::Movie) {
            auto m = co_await m_cinemeta->meta(
                api::MediaKind::Movie, seed.imdbId);
            refreshed.title = m.summary.title;
            refreshed.year = m.summary.year;
            refreshed.poster = m.summary.poster;
            refreshed.backdrop = m.background;
            refreshed.overview = m.summary.description;
            refreshed.releaseDate = m.summary.released;
            refreshed.genres = m.genres;
            refreshed.imdbRating = m.summary.imdbRating;
            refreshed.runtimeMinutes = m.runtimeMinutes;
            refreshed.cast = m.cast;
        } else {
            auto sd = co_await m_cinemeta->seriesMeta(seed.imdbId);
            refreshed.title = sd.meta.summary.title;
            refreshed.year = sd.meta.summary.year;
            refreshed.poster = sd.meta.summary.poster;
            refreshed.backdrop = sd.meta.background;
            refreshed.overview = sd.meta.summary.description;
            refreshed.releaseDate = sd.meta.summary.released;
            refreshed.genres = sd.meta.genres;
            refreshed.imdbRating = sd.meta.summary.imdbRating;
            refreshed.runtimeMinutes = sd.meta.runtimeMinutes;
            refreshed.cast = sd.meta.cast;
        }
        if (!refreshed.title.isEmpty() && !refreshed.imdbId.isEmpty()) {
            // Preserve the original addedAt; bump updatedAt.
            refreshed.addedAt = seed.addedAt;
            refreshed.updatedAt = QDateTime::currentDateTimeUtc();
            m_store.upsertTitle(refreshed);
        }
    } catch (const std::exception& e) {
        qCDebug(KINEMA_APP)
            << "LibraryController: backfill failed for"
            << seed.imdbId << "—" << e.what();
    }
    --m_backfillInFlight;
    pumpBackfill();
    co_return;
}

} // namespace kinema::controllers
