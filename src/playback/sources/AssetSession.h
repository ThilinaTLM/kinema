// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "playback/ports/ByteRangeSource.h"
#include "torrent/MediaFileSelector.h" // TorrentFileEntry
#include "torrent/PiecePlanner.h"

#include <QByteArray>
#include <QObject>
#include <QString>
#include <QVector>

#include <QCoro/QCoroTask>

namespace kinema::playback::sources {

using kinema::torrent::ByteRange;

/**
 * Abstract per-asset session owned by the transfer subsystem.
 *
 * Adds Qt signals (progress / completion / failure / live stats)
 * on top of the pure-read `playback::ports::ByteRangeSource` port,
 * plus mode / pause / resume / files() hooks needed by
 * `playback::transfer::TransferSession`.
 *
 * Concrete implementations live alongside in `playback::sources`:
 *
 *  - `TorrentAssetSession`         \u2014 libtorrent-backed; pieces are
 *    fetched from peers.
 *  - `HttpRangeAssetSession`       \u2014 debrid hoster URL fetched in
 *    chunks into a sparse local file.
 *
 * Each session represents a single playable file. `ensureRange()`
 * returns true when the requested byte range is available on disk;
 * false on timeout / fatal error. `readRange()` reads bytes from
 * the local payload. Both must remain thread-affine to the session's
 * owning thread (the GUI thread today).
 *
 * This is the relocated successor of the legacy
 * `download::AssetSession`. The transitional `token()` virtual was
 * dropped together with the legacy `LocalMediaServer`; the unified
 * `playback::streaming::LocalHttpStreamGateway` keys sources by
 * `assetId()` directly.
 */
class AssetSession : public QObject,
    public playback::ports::ByteRangeSource
{
    Q_OBJECT
public:
    using QObject::QObject;
    ~AssetSession() override = default;

    AssetSession(const AssetSession&) = delete;
    AssetSession& operator=(const AssetSession&) = delete;

    // ByteRangeSource: declarations from the port are inherited;
    // concrete sessions override the pure virtuals below.
    QString assetId() const override = 0;
    QString fileName() const override = 0;
    qint64 fileSize() const override = 0;
    QCoro::Task<bool> ensureRange(ByteRange range) override = 0;
    QByteArray readRange(ByteRange range) const override = 0;
    void touch() override = 0;
    qint64 cachedBytes() const override { return -1; }

    /// Full list of files inside the underlying torrent / magnet,
    /// 0-indexed in the order the source enumerated them. Empty
    /// when the session has not yet resolved metadata or when the
    /// source does not expose multi-file information. Consumed by
    /// series adjacency lookup. Pure read, safe to call from the
    /// GUI thread.
    virtual QVector<kinema::torrent::TorrentFileEntry> files() const
    {
        return {};
    }

    /// Current download mode. Concrete sessions persist this so a
    /// `MediaSourcePort::changeMode` can no-op when nothing changes.
    virtual domain::DownloadMode mode() const = 0;

    /// Update the in-memory mode flag. `MediaSourcePort::changeMode`
    /// calls this after applying the policy change to its own state
    /// (libtorrent priorities, prefetch loop, etc).
    virtual void setMode(domain::DownloadMode m) = 0;

    /// User-initiated pause. Default is a no-op so backends that
    /// don't yet honour pause keep working.
    virtual void pause() {}

    /// User-initiated resume; mirror of `pause()`.
    virtual void resume() {}

Q_SIGNALS:
    void cachedBytesChanged(qint64 bytes);
    void completed();
    void statusMessage(const QString& text, int timeoutMs);
    void failed(const QString& reason);

    /// Periodic transient telemetry (rate / peers / ETA). The
    /// torrent backend forwards engine-side stats, the HTTP backend
    /// computes a rate from byte deltas and reports `peers=0`.
    /// Subscribers must not assume a fixed cadence.
    void liveStatsChanged(qint64 ratePayloadBps,
        int peers,
        int seeds,
        int etaSeconds);
};

} // namespace kinema::playback::sources
