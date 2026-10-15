// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/ResumePolicy.h"

#include <QtGlobal>

namespace kinema::playback::policy {

std::optional<qint64> resumeSecondsFor(const ResumeInputs& in)
{
    if (!in.key.isValid()) {
        return std::nullopt;
    }

    if (in.activeSessionMatches && in.activeSessionPositionSec
        && *in.activeSessionPositionSec > 1.0) {
        return static_cast<qint64>(*in.activeSessionPositionSec);
    }

    if (!in.storedEntry) {
        return std::nullopt;
    }
    const auto& stored = *in.storedEntry;
    if (stored.finished) {
        return std::nullopt;
    }
    if (stored.positionSec < 1.0) {
        return std::nullopt;
    }
    double resume = stored.positionSec;
    if (stored.durationSec > 10.0) {
        resume = qMin(resume, stored.durationSec - 5.0);
    }
    return static_cast<qint64>(qMax(0.0, resume));
}

bool shouldShowResumePrompt(qint64 resumeSeconds, qint64 thresholdSec)
{
    if (resumeSeconds <= 0) {
        return false;
    }
    return resumeSeconds > thresholdSec;
}

} // namespace kinema::playback::policy
