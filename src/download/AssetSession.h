// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "torrent/PiecePlanner.h"

#include <QByteArray>
#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::download {

using torrent::ByteRange;

/**
 * Abstract per-asset session served by the unified
 * `LocalMediaServer`. Concrete implementations:
 *
 *  - `download::TorrentAssetSession` \u2014 libtorrent-backed; pieces are
 *    fetched from peers.
 *  - `download::HttpAssetSession` \u2014 RD hoster URL fetched in chunks
 *    into a sparse local file.
 *
 * Each session represents a single playable file. The opaque
 * `token()` ties the session to localhost URLs the server hands out.
 *
 * `ensureRange()` returns true when the requested byte range is
 * available on disk; false on timeout / fatal error.
 * `readRange()` reads bytes from the local payload. Both must remain
 * thread-affine to the session's owning thread (the GUI thread today).
 */
class AssetSession : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;
    ~AssetSession() override = default;

    AssetSession(const AssetSession&) = delete;
    AssetSession& operator=(const AssetSession&) = delete;

    /// Opaque, server-routed token used inside localhost URLs.
    /// Stable for the lifetime of the session.
    virtual QString token() const = 0;

    /// Stable identity in the persistent download store.
    virtual QString assetId() const = 0;

    /// Display-friendly file name used for the localhost URL path
    /// (and for the player's `media-title` heuristic).
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

    /// Update the LRU timestamp; called by the server on every
    /// inbound request and by the manager on user activity.
    virtual void touch() = 0;

    /// Bytes currently available on disk. Used for progress UI.
    virtual qint64 cachedBytes() const { return -1; }

Q_SIGNALS:
    void cachedBytesChanged(qint64 bytes);
    void completed();
    void statusMessage(const QString& text, int timeoutMs);
    void failed(const QString& reason);
};

} // namespace kinema::download
