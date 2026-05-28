// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "playback/events/PlaybackEvent.h"

#include <optional>

namespace kinema::playback::policy {

/// Bucket the decision into a single discriminated outcome.
struct WatchedDecision {
    bool finished = false;
    /// Diagnostic tag for the reason the policy returned `finished`.
    /// Use values like "natural-eof", "chapter", "stop-threshold",
    /// "tick-threshold", "caller", "below-threshold", "error".
    const char* via = "below-threshold";
};

struct WatchedInputs {
    domain::MediaKind kind = domain::MediaKind::Movie;
    PlaybackEndReason reason = PlaybackEndReason::UserStop;
    double positionSec = 0.0;
    double durationSec = 0.0;
    /// Credits-start hint derived from chapter analysis, in seconds.
    std::optional<double> creditsStartSec;
    /// Already-finished flag passed by the caller (e.g. a previously
    /// persisted row). Honoured: if true, output is always true.
    bool callerFinishedHint = false;
    /// Threshold for stop-completion. Defaults: 0.85 movies / 0.90
    /// episodes.
    double stopThreshold = 0.85;
    /// Passive tick threshold (defence-in-depth). Defaults 0.9.
    double passiveThreshold = 0.9;
};

/// Compute whether a playback session ended in "watched" state.
/// Replicates `HistoryStore::recordSessionEnd` semantics.
WatchedDecision decideWatched(const WatchedInputs& in);

/// Default per-kind stop threshold used by `WatchedInputs`.
double defaultStopThreshold(domain::MediaKind kind) noexcept;

} // namespace kinema::playback::policy
