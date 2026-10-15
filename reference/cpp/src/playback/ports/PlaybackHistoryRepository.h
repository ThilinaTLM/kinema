// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QList>

#include <optional>

namespace kinema::playback::ports {

/**
 * Storage port for playback history. The SQLite adapter wraps
 * `core::HistoryStore`; tests can substitute an in-memory fake.
 */
class PlaybackHistoryRepository
{
public:
    virtual ~PlaybackHistoryRepository() = default;

    /// Upsert by `entry.key`. Empty-stream rows preserve the prior
    /// stream ref (mirrors `HistoryStore::record`).
    virtual void record(const domain::HistoryEntry& entry) = 0;

    /// Apply session-end policy (NaturalEof / UserStop / Error).
    virtual void recordSessionEnd(const domain::HistoryEntry& entry,
        PlaybackEndReason reason,
        std::optional<double> creditsStartSec)
        = 0;

    /// Forget a row.
    virtual void remove(const domain::PlaybackKey& key) = 0;

    virtual std::optional<domain::HistoryEntry> find(
        const domain::PlaybackKey& key) const
        = 0;

    virtual std::optional<domain::HistoryEntry> findLatestForMedia(
        domain::MediaKind kind, const QString& imdbId) const
        = 0;

    virtual QList<domain::HistoryEntry> continueWatching(
        int maxItems = 30) const
        = 0;
};

} // namespace kinema::playback::ports
