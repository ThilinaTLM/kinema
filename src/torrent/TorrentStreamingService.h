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

/// Controls how aggressively `prepareSession()` blocks before
/// returning. `Streaming` is the legacy behaviour: wait for the
/// startup buffer to land so the player can begin playback
/// immediately. `Background` returns as soon as metadata is fetched
/// and the file is selected; the swarm keeps pulling pieces in the
/// background until the file is complete. Used by the unified
/// downloader for `Save offline` enqueues.
enum class PrepareMode {
    Streaming,
    Background,
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

    /// Marker tag used by the stub-only constructor below.
    struct StubTag {};

protected:
    /// Stubbing hook: leaves the libtorrent session uninitialised
    /// so test doubles can override every public virtual without
    /// paying the cost of starting a real session. Production code
    /// must use the cache + settings constructor.
    explicit TorrentStreamingService(StubTag, QObject* parent = nullptr);

public:

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
        const api::Stream& stream, const api::PlaybackContext& ctx,
        PrepareMode mode = PrepareMode::Streaming);

    QCoro::Task<bool> ensureRange(const QString& token, ByteRange range);
    QByteArray readRange(const QString& token, ByteRange range) const;
    qint64 fileSizeForToken(const QString& token) const;
    QString fileNameForToken(const QString& token) const;
    void touchToken(const QString& token);

    /// Mark the session for `infoHash` as exempt from idle-stop.
    /// The unified downloader calls this when the asset is pinned
    /// (Save offline) so background downloads keep running even
    /// after the user stops watching.
    virtual void setKeepAlive(const QString& infoHash, bool on);

    /// Pause / resume the libtorrent handle behind a session.
    /// User-initiated; bypasses idle-stop bookkeeping. Pausing
    /// keeps the session in `m_sessions` so it can resume without
    /// re-fetching metadata.
    virtual void pauseInfoHash(const QString& infoHash);
    virtual void resumeInfoHash(const QString& infoHash);

    /// Promote a streaming session to a full background download:
    /// drops every `set_piece_deadline()` entry so libtorrent picks
    /// pieces in normal order, sets the selected file to top
    /// priority, and exempts the session from idle-stop.
    /// Idempotent. Used by `DownloadManager::upgradeToFull` and by
    /// `TorrentBackend::changeMode(OnDemand\u2192Full)`.
    virtual void promoteToFull(const QString& infoHash);

public Q_SLOTS:
    void stopInfoHash(const QString& infoHash);
    void stopForContext(const api::PlaybackContext& ctx);
    void stopAll();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);

    /// Periodic per-session telemetry. Fires roughly every 2 s while
    /// the swarm is active. `etaSeconds` is `-1` when the rate is
    /// zero or the file size is unknown.
    void torrentStatsChanged(const QString& infoHash,
        qint64 doneBytes,
        qint64 ratePayloadBps,
        int peers,
        int seeds,
        int etaSeconds);

    /// Fired once when libtorrent reports `torrent_finished_alert`
    /// for the matching info hash. The asset's selected file is
    /// fully on disk by this point.
    void torrentFinished(const QString& infoHash);

    /// Fired on a fatal `torrent_error_alert` (storage error, etc.).
    /// Subscribers should mark the asset as failed.
    void torrentFailed(const QString& infoHash, const QString& reason);

private:
    Q_INVOKABLE void drainAlerts();
    Q_INVOKABLE void postTorrentUpdates();

    struct Private;
    std::unique_ptr<Private> d;
};

} // namespace kinema::torrent
