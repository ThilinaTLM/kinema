// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/history/SqlitePlaybackHistoryRepository.h"

#include "core/persistence/HistoryStore.h"

namespace kinema::playback::history {

namespace {

core::HistoryStore::SessionEndReason translateReason(PlaybackEndReason r)
{
    switch (r) {
    case PlaybackEndReason::NaturalEof:
        return core::HistoryStore::SessionEndReason::NaturalEof;
    case PlaybackEndReason::UserStop:
        return core::HistoryStore::SessionEndReason::UserStop;
    case PlaybackEndReason::ReplacedByNewSource:
    case PlaybackEndReason::PlayerError:
    case PlaybackEndReason::LoadTimeout:
        break;
    }
    return core::HistoryStore::SessionEndReason::Error;
}

} // namespace

SqlitePlaybackHistoryRepository::SqlitePlaybackHistoryRepository(
    core::HistoryStore& store)
    : m_store(store)
{
}

void SqlitePlaybackHistoryRepository::record(
    const domain::HistoryEntry& entry)
{
    m_store.record(entry);
}

void SqlitePlaybackHistoryRepository::recordSessionEnd(
    const domain::HistoryEntry& entry,
    PlaybackEndReason reason,
    std::optional<double> creditsStartSec)
{
    m_store.recordSessionEnd(entry, translateReason(reason),
        creditsStartSec);
}

void SqlitePlaybackHistoryRepository::remove(
    const domain::PlaybackKey& key)
{
    m_store.remove(key);
}

std::optional<domain::HistoryEntry>
SqlitePlaybackHistoryRepository::find(const domain::PlaybackKey& key) const
{
    return m_store.find(key);
}

std::optional<domain::HistoryEntry>
SqlitePlaybackHistoryRepository::findLatestForMedia(
    domain::MediaKind kind, const QString& imdbId) const
{
    return m_store.findLatestForMedia(kind, imdbId);
}

QList<domain::HistoryEntry>
SqlitePlaybackHistoryRepository::continueWatching(int maxItems) const
{
    return m_store.continueWatching(maxItems);
}

} // namespace kinema::playback::history
