// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "torrent/PiecePlanner.h"

#include <QVector>

namespace kinema::playback::policy {

/// Piece deadline batch: each piece gets `baseDeadlineMs + index * stepMs`.
struct PieceDeadlines {
    int first = -1;
    int last = -1;
    int baseDeadlineMs = 0;
    int stepMs = 0;

    bool isValid() const noexcept { return first >= 0 && last >= first; }
};

/// Inputs the streaming priority policy needs to plan piece work
/// for a torrent-backed playback session. All numeric byte values
/// are absolute (not MiB) so the policy can be exercised with
/// arbitrary numbers in tests.
struct StreamingPriorityInputs {
    kinema::torrent::FilePieceLayout layout;
    qint64 startupBufferBytes = 0;
    qint64 tailBufferBytes = 0;
    qint64 readaheadBytes = 0;
};

/// Build the startup pre-warm deadlines (head + tail windows).
QVector<PieceDeadlines> planStartupDeadlines(
    const StreamingPriorityInputs& in);

/// Build the readahead deadlines for one Range request.
PieceDeadlines planReadaheadDeadlines(
    const StreamingPriorityInputs& in,
    qint64 requestStart,
    qint64 requestEndInclusive);

/// Required-byte piece range that must be available before the
/// gateway can serve the Range response. Just a thin alias over
/// `pieceRangeForBytes()` to keep the call site in one place.
kinema::torrent::PieceRange requiredPiecesForRange(
    const kinema::torrent::FilePieceLayout& layout,
    qint64 requestStart,
    qint64 requestEndInclusive);

} // namespace kinema::playback::policy
