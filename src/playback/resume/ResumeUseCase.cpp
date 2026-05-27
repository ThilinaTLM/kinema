// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/resume/ResumeUseCase.h"

#include "controllers/HistoryController.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/policy/ResumePolicy.h"
#include "playback/progress/PlaybackProgressProjector.h"

namespace kinema::playback::resume {

ResumeUseCase::ResumeUseCase(controllers::HistoryController& history,
    playback::history::HistoryQueryService& queryService,
    playback::progress::PlaybackProgressProjector& projector,
    QObject* parent)
    : QObject(parent)
    , m_history(history)
    , m_queryService(queryService)
    , m_projector(projector)
{
    connect(&m_history,
        &controllers::HistoryController::resumeFallbackRequested,
        this, &ResumeUseCase::resumeFallbackRequested);
    connect(&m_history,
        &controllers::HistoryController::statusMessage,
        this, &ResumeUseCase::statusMessage);
}

std::optional<qint64> ResumeUseCase::resumeSecondsFor(
    const domain::PlaybackKey& key) const
{
    if (!key.isValid()) {
        return std::nullopt;
    }
    policy::ResumeInputs in;
    in.key = key;
    // Live in-memory position is authoritative when the projector
    // is still tracking the same key (mid-session stream swap).
    if (m_projector.hasActiveContext()) {
        const auto& ctx = m_projector.activeContext();
        if (ctx.has_value() && ctx->key == key) {
            in.activeSessionMatches = true;
            in.activeSessionPositionSec = m_projector.lastPosition();
        }
    }
    in.storedEntry = m_queryService.find(key);
    return policy::resumeSecondsFor(in);
}

ResumeUseCase::~ResumeUseCase() = default;

void ResumeUseCase::resume(const domain::HistoryEntry& entry)
{
    m_history.resumeFromHistory(entry);
}

void ResumeUseCase::removeEntry(const domain::HistoryEntry& entry)
{
    m_history.removeEntry(entry);
}

} // namespace kinema::playback::resume
