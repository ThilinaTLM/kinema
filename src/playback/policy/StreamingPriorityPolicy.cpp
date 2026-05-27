// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/policy/StreamingPriorityPolicy.h"

namespace kinema::playback::policy {

namespace {

constexpr int kStartupStepMs = 10;
constexpr int kReadaheadStepMs = 5;

} // namespace

QVector<PieceDeadlines> planStartupDeadlines(
    const StreamingPriorityInputs& in)
{
    QVector<PieceDeadlines> out;
    const auto windows = kinema::torrent::startupPieceWindows(in.layout,
        in.startupBufferBytes, in.tailBufferBytes);
    int deadline = 0;
    for (const auto& w : windows) {
        if (!w.isValid()) {
            continue;
        }
        PieceDeadlines d;
        d.first = w.first;
        d.last = w.last;
        d.baseDeadlineMs = deadline;
        d.stepMs = kStartupStepMs;
        out.append(d);
        deadline += kStartupStepMs * (w.last - w.first + 1);
    }
    return out;
}

PieceDeadlines planReadaheadDeadlines(
    const StreamingPriorityInputs& in,
    qint64 requestStart,
    qint64 requestEndInclusive)
{
    const auto urgent = kinema::torrent::readaheadRange(requestStart,
        requestEndInclusive, in.readaheadBytes, in.layout.fileSize);
    const auto pieces = kinema::torrent::pieceRangeForBytes(in.layout, urgent);
    if (!pieces.isValid()) {
        return {};
    }
    PieceDeadlines d;
    d.first = pieces.first;
    d.last = pieces.last;
    d.baseDeadlineMs = 0;
    d.stepMs = kReadaheadStepMs;
    return d;
}

kinema::torrent::PieceRange requiredPiecesForRange(
    const kinema::torrent::FilePieceLayout& layout,
    qint64 requestStart,
    qint64 requestEndInclusive)
{
    return kinema::torrent::pieceRangeForBytes(layout,
        { requestStart, requestEndInclusive });
}

} // namespace kinema::playback::policy
