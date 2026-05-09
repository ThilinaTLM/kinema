// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "torrent/PiecePlanner.h"

#include <QCoro/QCoroTask>

#include <QObject>
#include <QUrl>

#include <memory>

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::core {
class TorrentCache;
}

namespace kinema::torrent {

class LocalStreamServer;

/// Per-asset metadata returned by `prepareSession()`. Used by the
/// unified-downloader's `TorrentAssetSession` to wire the asset into
/// `download::LocalMediaServer` without going through TSS's own
/// localhost server.
struct PreparedSession {
    QString token;
    QString fileName;
    qint64 fileSize = 0;
    QString infoHash;
};

/**
 * Libtorrent-backed engine. After the unified-downloader refactor
 * this class is the torrent **backend implementation**: it owns the
 * libtorrent session, picks the right file inside multi-file
 * torrents, and exposes per-asset byte-range fetches through
 * `prepareSession()` / `ensureRange()` / `readRange()`. Production
 * playback always reaches it through
 * `download::TorrentAssetSession` + `download::LocalMediaServer`.
 *
 * The legacy `prepare()` method (and its private `LocalStreamServer`)
 * are retained only for tests that haven't been ported to the
 * unified downloader. New code must use `prepareSession()`.
 */
class TorrentStreamingService : public QObject
{
    Q_OBJECT
public:
    TorrentStreamingService(core::TorrentCache& cache,
        const config::TorrentStreamingSettings& settings,
        QObject* parent = nullptr);
    ~TorrentStreamingService() override;

    /// LEGACY: returns a URL served by the engine's own private
    /// `LocalStreamServer`. New code must call `prepareSession()`
    /// and register the asset with `download::LocalMediaServer`.
    virtual QCoro::Task<QUrl> prepare(const api::Stream& stream,
        const api::PlaybackContext& ctx);

    /// Same per-asset preparation as `prepare()` (metadata fetch,
    /// file selection, startup buffering, piece prioritisation), but
    /// does not touch the legacy `LocalStreamServer`. Used by the
    /// new unified downloader pipeline.
    virtual QCoro::Task<PreparedSession> prepareSession(
        const api::Stream& stream, const api::PlaybackContext& ctx);

    QCoro::Task<bool> ensureRange(const QString& token, ByteRange range);
    QByteArray readRange(const QString& token, ByteRange range) const;
    qint64 fileSizeForToken(const QString& token) const;
    QString fileNameForToken(const QString& token) const;
    void touchToken(const QString& token);

public Q_SLOTS:
    void stopInfoHash(const QString& infoHash);
    void stopForContext(const api::PlaybackContext& ctx);
    void stopAll();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    struct Private;
    std::unique_ptr<Private> d;
};

} // namespace kinema::torrent
