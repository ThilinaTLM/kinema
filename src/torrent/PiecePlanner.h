// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina.lakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/util/ByteRange.h"

#include <QVector>

#include <optional>

namespace kinema::torrent {

using core::ByteRange;

struct PieceRange
{
    int first = 0;
    int last = -1;

    bool isValid() const noexcept { return first >= 0 && last >= first; }
};

struct FilePieceLayout
{
    qint64 fileOffset = 0; ///< byte offset of selected file in torrent
    qint64 fileSize = 0;
    int pieceSize = 0;
    int pieceCount = 0;
};

PieceRange pieceRangeForBytes(const FilePieceLayout& layout, ByteRange range);
QVector<PieceRange>
startupPieceWindows(const FilePieceLayout& layout, qint64 startupBytes, qint64 tailBytes);
ByteRange clampRange(qint64 start, qint64 length, qint64 fileSize);
ByteRange readaheadRange(qint64 requestStart,
                         qint64 requestEndInclusive,
                         qint64 readaheadBytes,
                         qint64 fileSize);

} // namespace kinema::torrent
