// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QObject>

#include <optional>

namespace kinema::controllers {
class HistoryController;
}

namespace kinema::playback::history {
class HistoryQueryService;
}

namespace kinema::playback::progress {
class PlaybackProgressProjector;
}

namespace kinema::playback::resume {

/**
 * One-click resume of a Continue-Watching entry.
 *
 * During the refactor this is a facade over
 * `controllers::HistoryController::resumeFromHistory`; Phase 7 will
 * move the indexer fetch + match logic here over a
 * `StreamIndexerPort`.
 */
class ResumeUseCase : public QObject
{
    Q_OBJECT
public:
    ResumeUseCase(controllers::HistoryController& history,
        playback::history::HistoryQueryService& queryService,
        playback::progress::PlaybackProgressProjector& projector,
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
    /// Forget a history row. During the refactor this forwards to
    /// `HistoryController::removeEntry`; the long-term home is a
    /// dedicated history mutation port.
    void removeEntry(const domain::HistoryEntry& entry);

Q_SIGNALS:
    /// Forwarded from `HistoryController::resumeFallbackRequested`.
    void resumeFallbackRequested(const domain::HistoryEntry& entry);
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    controllers::HistoryController& m_history;
    playback::history::HistoryQueryService& m_queryService;
    playback::progress::PlaybackProgressProjector& m_projector;
};

} // namespace kinema::playback::resume
