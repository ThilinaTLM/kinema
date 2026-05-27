// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "torrent/PiecePlanner.h" // ByteRange

#include <QByteArray>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::playback::ports {

using kinema::torrent::ByteRange;

/**
 * Minimal byte-range read interface required by the local HTTP
 * stream gateway. Concrete implementations (torrent / debrid
 * sessions) own bytes; the gateway never sees lifecycle, mode,
 * pause, or telemetry concerns — those live on a richer
 * `TransferSession` type.
 *
 * Methods are GUI-thread affine and pair with QCoro coroutine
 * tasks for asynchronous waits.
 */
class ByteRangeSource
{
public:
    virtual ~ByteRangeSource() = default;

    /// Stable identity used in the persistent download store and
    /// in the gateway's localhost URL path.
    virtual QString assetId() const = 0;

    /// Display-friendly file name used for the localhost URL path
    /// and `Content-Type` heuristics.
    virtual QString fileName() const = 0;

    /// Total file size in bytes. Returns -1 when not yet known
    /// (e.g. magnet metadata still resolving).
    virtual qint64 fileSize() const = 0;

    /// Block until `range` is fully available locally. Returns
    /// `true` on success, `false` on timeout / unrecoverable error.
    virtual QCoro::Task<bool> ensureRange(ByteRange range) = 0;

    /// Read bytes already stored locally. Caller is responsible for
    /// having previously co_awaited `ensureRange`.
    virtual QByteArray readRange(ByteRange range) const = 0;

    /// LRU activity tick.
    virtual void touch() = 0;

    /// Bytes currently available on disk. Used for progress UI.
    /// Returns -1 when not known.
    virtual qint64 cachedBytes() const { return -1; }
};

} // namespace kinema::playback::ports
