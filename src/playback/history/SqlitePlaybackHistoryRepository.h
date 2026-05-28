// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/ports/PlaybackHistoryRepository.h"

namespace kinema::core {
class HistoryStore;
}

namespace kinema::playback::history {

/// SQLite-backed adapter for `PlaybackHistoryRepository`. Forwards
/// to `core::HistoryStore`. Owns no state of its own.
class SqlitePlaybackHistoryRepository final
    : public ports::PlaybackHistoryRepository
{
public:
    explicit SqlitePlaybackHistoryRepository(core::HistoryStore& store);
    ~SqlitePlaybackHistoryRepository() override = default;

    void record(const domain::HistoryEntry& entry) override;
    void recordSessionEnd(const domain::HistoryEntry& entry,
        PlaybackEndReason reason,
        std::optional<double> creditsStartSec) override;
    void remove(const domain::PlaybackKey& key) override;

    std::optional<domain::HistoryEntry> find(
        const domain::PlaybackKey& key) const override;
    std::optional<domain::HistoryEntry> findLatestForMedia(
        domain::MediaKind kind, const QString& imdbId) const override;
    QList<domain::HistoryEntry> continueWatching(
        int maxItems = 30) const override;

    core::HistoryStore& store() noexcept { return m_store; }

private:
    core::HistoryStore& m_store;
};

} // namespace kinema::playback::history
