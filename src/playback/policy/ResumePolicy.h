// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <optional>

namespace kinema::playback::policy {

/// Inputs needed to compute a resume position. The caller provides
/// optional in-memory and on-disk history snapshots; the policy
/// merges them according to the rules below.
struct ResumeInputs {
    domain::PlaybackKey key;
    /// In-memory position of the currently-active session, if any,
    /// when the active session's key matches `key`. Used for
    /// mid-session stream swaps so resume picks up where the
    /// in-memory tick stream stopped, not where disk lags.
    std::optional<double> activeSessionPositionSec;
    /// Whether the active-session key matches `key`.
    bool activeSessionMatches = false;
    /// On-disk history row, if any. `nullopt` means no row.
    std::optional<domain::HistoryEntry> storedEntry;
};

/**
 * Pure resume policy. Implements the rules previously expressed in
 * `HistoryController::resumeSecondsFor`:
 *   - prefer the live in-memory position when the active session's
 *     key matches and position > 1s;
 *   - ignore finished rows;
 *   - ignore trivial (< 1s) positions;
 *   - clamp to `duration - 5s` when duration is known.
 */
std::optional<qint64> resumeSecondsFor(const ResumeInputs& in);

/// Whether the resume position is high enough to warrant a prompt
/// rather than an automatic seek. `thresholdSec` is the user
/// preference (see `PlayerSettings::resumePromptThresholdSec`).
bool shouldShowResumePrompt(qint64 resumeSeconds, qint64 thresholdSec);

} // namespace kinema::playback::policy
