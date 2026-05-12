// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/WatchedController.h"

#include "controllers/HistoryController.h"
#include "core/persistence/WatchedStore.h"

namespace kinema::controllers {

WatchedController::WatchedController(core::WatchedStore& store,
    HistoryController* history, QObject* parent)
    : QObject(parent)
    , m_store(store)
    , m_history(history)
{
    connect(&m_store, &core::WatchedStore::changed,
        this, &WatchedController::changed);
    if (m_history) {
        connect(m_history, &HistoryController::changed,
            this, &WatchedController::changed);
    }
}

WatchedController::~WatchedController() = default;

bool WatchedController::isWatched(const domain::PlaybackKey& key) const
{
    if (!key.isValid()) {
        return false;
    }
    const auto state = m_store.override(key);
    if (state == core::WatchedStore::Override::Watched) {
        return true;
    }
    if (state == core::WatchedStore::Override::Unwatched) {
        return false;
    }
    if (!m_history) {
        return false;
    }
    const auto entry = m_history->find(key);
    return entry && entry->finished;
}

bool WatchedController::isMovieWatched(const QString& imdbId) const
{
    return isWatched(movieKey(imdbId));
}

bool WatchedController::isEpisodeWatched(const QString& imdbId,
    int season, int episode) const
{
    return isWatched(episodeKey(imdbId, season, episode));
}

double WatchedController::progressFor(const domain::PlaybackKey& key) const
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

double WatchedController::movieProgress(const QString& imdbId) const
{
    return progressFor(movieKey(imdbId));
}

double WatchedController::episodeProgress(const QString& imdbId,
    int season, int episode) const
{
    return progressFor(episodeKey(imdbId, season, episode));
}

std::optional<domain::HistoryEntry> WatchedController::resumeEntryFor(
    const domain::PlaybackKey& key) const
{
    if (!m_history) {
        return std::nullopt;
    }
    const auto entry = m_history->find(key);
    if (!entry || entry->finished || entry->progressFraction() <= 0.0) {
        return std::nullopt;
    }
    return entry;
}

std::optional<domain::HistoryEntry> WatchedController::resumeEntryForMovie(
    const QString& imdbId) const
{
    return resumeEntryFor(movieKey(imdbId));
}

std::optional<domain::HistoryEntry> WatchedController::resumeEntryForEpisode(
    const QString& imdbId, int season, int episode) const
{
    return resumeEntryFor(episodeKey(imdbId, season, episode));
}

void WatchedController::setMovieWatched(const QString& imdbId, bool watched)
{
    m_store.setOverride(movieKey(imdbId), watched
            ? core::WatchedStore::Override::Watched
            : core::WatchedStore::Override::Unwatched);
}

void WatchedController::setEpisodeWatched(const QString& imdbId,
    int season, int episode, bool watched)
{
    m_store.setOverride(episodeKey(imdbId, season, episode), watched
            ? core::WatchedStore::Override::Watched
            : core::WatchedStore::Override::Unwatched);
}

void WatchedController::setEpisodesWatched(const QString& imdbId,
    const QList<QPair<int, int>>& seasonEpisode, bool watched)
{
    if (imdbId.isEmpty()) {
        return;
    }
    const auto state = watched
        ? core::WatchedStore::Override::Watched
        : core::WatchedStore::Override::Unwatched;
    for (const auto& [season, episode] : seasonEpisode) {
        m_store.setOverride(episodeKey(imdbId, season, episode), state);
    }
}

void WatchedController::clearOverride(const domain::PlaybackKey& key)
{
    m_store.clearOverride(key);
}

domain::PlaybackKey WatchedController::movieKey(const QString& imdbId) const
{
    domain::PlaybackKey key;
    key.kind = domain::MediaKind::Movie;
    key.imdbId = imdbId;
    return key;
}

domain::PlaybackKey WatchedController::episodeKey(const QString& imdbId,
    int season, int episode) const
{
    domain::PlaybackKey key;
    key.kind = domain::MediaKind::Series;
    key.imdbId = imdbId;
    key.season = season;
    key.episode = episode;
    return key;
}

} // namespace kinema::controllers
