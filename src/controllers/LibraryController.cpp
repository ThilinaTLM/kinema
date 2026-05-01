// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/LibraryController.h"

#include "api/Media.h"
#include "controllers/HistoryController.h"
#include "core/LibraryStore.h"

#include <KLocalizedString>

#include <QDateTime>

namespace kinema::controllers {

namespace {

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
    t.active = true;
    t.updatedAt = QDateTime::currentDateTimeUtc();
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
    HistoryController* history,
    QObject* parent)
    : QObject(parent)
    , m_store(store)
    , m_history(history)
{
    connect(&m_store, &core::LibraryStore::changed,
        this, &LibraryController::changed);
    if (m_history) {
        connect(m_history, &HistoryController::changed,
            this, &LibraryController::changed);
    }
}

LibraryController::~LibraryController() = default;

bool LibraryController::isInLibrary(api::MediaKind kind,
    const QString& imdbId) const
{
    return m_store.isActive(kind, imdbId);
}

std::optional<api::LibraryTitle> LibraryController::title(
    api::MediaKind kind, const QString& imdbId) const
{
    return m_store.find(kind, imdbId);
}

QList<api::LibraryTitle> LibraryController::activeTitles() const
{
    return m_store.activeTitles();
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
        i18nc("@info:status", "Added “%1” to Library.", t.title), 3000);
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
        i18nc("@info:status", "Added “%1” to Library.", t.title), 3000);
}

void LibraryController::softRemove(api::MediaKind kind,
    const QString& imdbId)
{
    const auto t = m_store.find(kind, imdbId);
    m_store.setActive(kind, imdbId, false);
    if (t) {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Hid “%1” from Library.", t->title), 3000);
    }
}

void LibraryController::hardDelete(api::MediaKind kind,
    const QString& imdbId)
{
    const auto t = m_store.find(kind, imdbId);
    m_store.hardDelete(kind, imdbId);
    if (t) {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Deleted Library data for “%1”.", t->title),
            4000);
    }
}

void LibraryController::setMovieWatched(const QString& imdbId, bool watched)
{
    m_store.setWatchOverride(movieKey(imdbId), watched
            ? api::LibraryWatchOverride::Watched
            : api::LibraryWatchOverride::Unwatched);
}

void LibraryController::setEpisodeWatched(const QString& imdbId,
    int season, int episode, bool watched)
{
    m_store.setWatchOverride(episodeKey(imdbId, season, episode), watched
            ? api::LibraryWatchOverride::Watched
            : api::LibraryWatchOverride::Unwatched);
}

void LibraryController::clearWatchOverride(const api::PlaybackKey& key)
{
    m_store.clearWatchOverride(key);
}

bool LibraryController::isMovieWatched(const QString& imdbId) const
{
    return isWatched(movieKey(imdbId));
}

bool LibraryController::isEpisodeWatched(const QString& imdbId,
    int season, int episode) const
{
    return isWatched(episodeKey(imdbId, season, episode));
}

double LibraryController::movieProgress(const QString& imdbId) const
{
    return progressFor(movieKey(imdbId));
}

double LibraryController::episodeProgress(const QString& imdbId,
    int season, int episode) const
{
    return progressFor(episodeKey(imdbId, season, episode));
}

std::optional<api::HistoryEntry> LibraryController::resumeEntryForMovie(
    const QString& imdbId) const
{
    if (!m_history) {
        return std::nullopt;
    }
    const auto entry = m_history->find(movieKey(imdbId));
    if (!entry || entry->finished || entry->progressFraction() <= 0.0) {
        return std::nullopt;
    }
    return entry;
}

std::optional<api::HistoryEntry> LibraryController::resumeEntryForEpisode(
    const QString& imdbId, int season, int episode) const
{
    if (!m_history) {
        return std::nullopt;
    }
    const auto entry = m_history->find(episodeKey(imdbId, season, episode));
    if (!entry || entry->finished || entry->progressFraction() <= 0.0) {
        return std::nullopt;
    }
    return entry;
}

api::PlaybackKey LibraryController::movieKey(const QString& imdbId) const
{
    api::PlaybackKey key;
    key.kind = api::MediaKind::Movie;
    key.imdbId = imdbId;
    return key;
}

api::PlaybackKey LibraryController::episodeKey(const QString& imdbId,
    int season, int episode) const
{
    api::PlaybackKey key;
    key.kind = api::MediaKind::Series;
    key.imdbId = imdbId;
    key.season = season;
    key.episode = episode;
    return key;
}

bool LibraryController::isWatched(const api::PlaybackKey& key) const
{
    const auto override = m_store.watchOverride(key);
    if (override == api::LibraryWatchOverride::Watched) {
        return true;
    }
    if (override == api::LibraryWatchOverride::Unwatched) {
        return false;
    }
    if (!m_history) {
        return false;
    }
    const auto entry = m_history->find(key);
    return entry && entry->finished;
}

double LibraryController::progressFor(const api::PlaybackKey& key) const
{
    if (!m_history) {
        return -1.0;
    }
    const auto entry = m_history->find(key);
    if (!entry || entry->finished) {
        return -1.0;
    }
    return entry->progressFraction();
}

} // namespace kinema::controllers
