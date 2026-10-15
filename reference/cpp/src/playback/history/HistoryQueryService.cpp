// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/history/HistoryQueryService.h"

#include "core/persistence/HistoryStore.h"

namespace kinema::playback::history {

HistoryQueryService::HistoryQueryService(
    ports::PlaybackHistoryRepository& repo,
    core::HistoryStore& storeForChangeSignal,
    QObject* parent)
    : QObject(parent)
    , m_repo(repo)
{
    connect(&storeForChangeSignal, &core::HistoryStore::changed,
        this, &HistoryQueryService::changed);
}

HistoryQueryService::~HistoryQueryService() = default;

std::optional<domain::HistoryEntry> HistoryQueryService::find(
    const domain::PlaybackKey& key) const
{
    return m_repo.find(key);
}

std::optional<domain::HistoryEntry> HistoryQueryService::findLatestForMedia(
    domain::MediaKind kind, const QString& imdbId) const
{
    return m_repo.findLatestForMedia(kind, imdbId);
}

QList<domain::HistoryEntry> HistoryQueryService::continueWatching(
    int maxItems) const
{
    return m_repo.continueWatching(maxItems);
}

} // namespace kinema::playback::history
