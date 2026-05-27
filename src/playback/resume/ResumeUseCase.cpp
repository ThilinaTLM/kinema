// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/resume/ResumeUseCase.h"

#include "controllers/HistoryController.h"

namespace kinema::playback::resume {

ResumeUseCase::ResumeUseCase(controllers::HistoryController& history,
    QObject* parent)
    : QObject(parent)
    , m_history(history)
{
    connect(&m_history,
        &controllers::HistoryController::resumeFallbackRequested,
        this, &ResumeUseCase::resumeFallbackRequested);
    connect(&m_history,
        &controllers::HistoryController::statusMessage,
        this, &ResumeUseCase::statusMessage);
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
