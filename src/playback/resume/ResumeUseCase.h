// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QCoro/QCoroTask>

#include <QObject>

#include <optional>

namespace kinema::playback::history {
class HistoryQueryService;
}

namespace kinema::playback::ports {
class PlaybackHistoryRepository;
class StreamIndexerPort;
}

namespace kinema::playback::progress {
class PlaybackProgressProjector;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::playback::resume {

/**
 * One-click resume of a Continue-Watching entry.
 *
 * Owns the indexer fetch + match logic that previously lived in
 * `HistoryController::resumeFromHistory`: re-resolves the saved
 * release against the active indexer, matches by
 * `lastStream.matches()`, and dispatches the matching stream
 * through `services::StreamActions::play` (transitional; the
 * end-state routes to `PlaybackSessionManager::play`).
 */
class ResumeUseCase : public QObject
{
    Q_OBJECT
public:
    ResumeUseCase(playback::history::HistoryQueryService& queryService,
        playback::progress::PlaybackProgressProjector& projector,
        ports::StreamIndexerPort& indexer,
        ports::PlaybackHistoryRepository& historyRepo,
        services::StreamActions& actions,
        QObject* parent = nullptr);
    ~ResumeUseCase() override;

    /// Resume position to seed into `PlaybackContext.resumeSeconds`
    /// for `key`. Returns the live in-memory position of the
    /// active session when the keys match (mid-session swap), the
    /// stored row position otherwise (clamped by ResumePolicy), or
    /// nullopt when nothing to resume.
    std::optional<qint64> resumeSecondsFor(
        const domain::PlaybackKey& key) const;

public Q_SLOTS:
    void resume(const domain::HistoryEntry& entry);
    /// Forget a history row.
    void removeEntry(const domain::HistoryEntry& entry);

Q_SIGNALS:
    /// The stored release is no longer available in the active
    /// indexer's response. The shell opens the matching detail
    /// page so the user can pick another stream.
    void resumeFallbackRequested(const domain::HistoryEntry& entry);
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    QCoro::Task<void> resumeTask(domain::HistoryEntry entry);

    playback::history::HistoryQueryService& m_queryService;
    playback::progress::PlaybackProgressProjector& m_projector;
    ports::StreamIndexerPort& m_indexer;
    ports::PlaybackHistoryRepository& m_historyRepo;
    services::StreamActions& m_actions;
    quint64 m_resumeEpoch = 0;
};

} // namespace kinema::playback::resume
