// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"
#include "playback/ports/PlaybackHistoryRepository.h"

#include <QList>
#include <QObject>

#include <optional>

namespace kinema::core {
class HistoryStore;
}

namespace kinema::playback::history {

/**
 * Read-only query surface for the history layer. Wraps a
 * `PlaybackHistoryRepository` so callers (Continue Watching, detail
 * pages, library) don't need a reference to the full
 * `HistoryController` or to `core::HistoryStore`.
 *
 * Emits `changed()` whenever the underlying repository changes —
 * UI lists can connect once and re-query.
 */
class HistoryQueryService : public QObject
{
    Q_OBJECT
public:
    HistoryQueryService(ports::PlaybackHistoryRepository& repo,
        core::HistoryStore& storeForChangeSignal,
        QObject* parent = nullptr);
    ~HistoryQueryService() override;

    std::optional<domain::HistoryEntry> find(
        const domain::PlaybackKey& key) const;
    std::optional<domain::HistoryEntry> findLatestForMedia(
        domain::MediaKind kind, const QString& imdbId) const;
    QList<domain::HistoryEntry> continueWatching(int maxItems = 30) const;

Q_SIGNALS:
    void changed();

private:
    ports::PlaybackHistoryRepository& m_repo;
};

} // namespace kinema::playback::history
