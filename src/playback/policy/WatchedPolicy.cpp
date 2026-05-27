// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/WatchedPolicy.h"

namespace kinema::playback::policy {

double defaultStopThreshold(domain::MediaKind kind) noexcept
{
    return kind == domain::MediaKind::Movie ? 0.85 : 0.90;
}

WatchedDecision decideWatched(const WatchedInputs& in)
{
    if (in.callerFinishedHint) {
        return { true, "caller" };
    }

    // Errors / load timeouts never auto-finish.
    if (in.reason == PlaybackEndReason::PlayerError
        || in.reason == PlaybackEndReason::LoadTimeout) {
        return { false, "error" };
    }
    // Replaced source: don't finish; the user's new attempt owns
    // the outcome.
    if (in.reason == PlaybackEndReason::ReplacedByNewSource) {
        return { false, "replaced" };
    }

    if (in.reason == PlaybackEndReason::NaturalEof) {
        return { true, "natural-eof" };
    }

    // UserStop:
    if (in.creditsStartSec && in.positionSec >= *in.creditsStartSec - 2.0) {
        return { true, "chapter" };
    }
    if (in.durationSec > 0.0
        && in.positionSec / in.durationSec >= in.stopThreshold) {
        return { true, "stop-threshold" };
    }
    if (in.durationSec > 0.0
        && in.positionSec / in.durationSec >= in.passiveThreshold) {
        return { true, "tick-threshold" };
    }
    return { false, "below-threshold" };
}

} // namespace kinema::playback::policy
