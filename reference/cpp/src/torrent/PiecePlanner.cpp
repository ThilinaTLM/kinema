// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "torrent/PiecePlanner.h"

#include <QtGlobal>

namespace kinema::torrent {

ByteRange clampRange(qint64 start, qint64 length, qint64 fileSize)
{
    if (fileSize <= 0 || length <= 0 || start >= fileSize) {
        return {};
    }
    start = qMax<qint64>(0, start);
    const qint64 end = qMin(fileSize - 1, start + length - 1);
    return { start, end };
}

PieceRange pieceRangeForBytes(const FilePieceLayout& layout, ByteRange range)
{
    if (!range.isValid() || layout.pieceSize <= 0
        || layout.pieceCount <= 0 || layout.fileSize <= 0) {
        return {};
    }
    const qint64 clampedEnd = qMin(range.endInclusive, layout.fileSize - 1);
    if (clampedEnd < range.start) {
        return {};
    }

    const qint64 torrentStart = layout.fileOffset + range.start;
    const qint64 torrentEnd = layout.fileOffset + clampedEnd;
    int first = static_cast<int>(torrentStart / layout.pieceSize);
    int last = static_cast<int>(torrentEnd / layout.pieceSize);
    first = qBound(0, first, layout.pieceCount - 1);
    last = qBound(0, last, layout.pieceCount - 1);
    return { first, last };
}

QVector<PieceRange> startupPieceWindows(const FilePieceLayout& layout,
    qint64 startupBytes, qint64 tailBytes)
{
    QVector<PieceRange> out;
    const auto head = pieceRangeForBytes(layout,
        clampRange(0, startupBytes, layout.fileSize));
    if (head.isValid()) {
        out.append(head);
    }
    if (tailBytes > 0 && layout.fileSize > startupBytes) {
        const qint64 tailStart = qMax<qint64>(0, layout.fileSize - tailBytes);
        const auto tail = pieceRangeForBytes(layout,
            { tailStart, layout.fileSize - 1 });
        if (tail.isValid()
            && (out.isEmpty() || tail.first > out.last().last)) {
            out.append(tail);
        }
    }
    return out;
}

ByteRange readaheadRange(qint64 requestStart, qint64 requestEndInclusive,
    qint64 readaheadBytes, qint64 fileSize)
{
    if (fileSize <= 0 || requestStart >= fileSize) {
        return {};
    }
    requestStart = qMax<qint64>(0, requestStart);
    requestEndInclusive = qMax(requestStart, requestEndInclusive);
    const qint64 end = qMin(fileSize - 1,
        qMax(requestEndInclusive, requestStart + readaheadBytes - 1));
    return { requestStart, end };
}

} // namespace kinema::torrent
