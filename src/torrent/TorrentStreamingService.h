// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/torrent/LibtorrentClient.h"
#include "torrent/PiecePlanner.h"
#include "torrent/TorrentFileEntry.h"

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

/// Re-exports of the canonical per-asset types so existing consumers
/// in `kinema::torrent::` continue to compile during the
/// `playback::torrent::LibtorrentClient` cut-over. New code should
/// reference `kinema::playback::torrent::PreparedSession` /
/// `PrepareMode` directly.
using PreparedSession = kinema::playback::torrent::PreparedSession;
using PrepareMode = kinema::playback::torrent::PrepareMode;

/**
 * Thin facade over `playback::torrent::LibtorrentClient`.
 *
 * The per-asset state machine (prepareSession, ensureRange, file
 * selection, idle-stop, keepAlive, …) lives on `LibtorrentClient`
 * now. This class survives only as a forwarding shim plus the
 * legacy `prepare()` entry point that hands out a URL served by
 * the private `LocalStreamServer`. The legacy URL path is consumed
 * by `services::StreamActions` as a fallback when the unified
 * downloader isn't wired (some unit-test setups). Step 7 sub-commit
 * 3 deletes this class.
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
    /// and register the asset with the unified stream gateway.
    virtual QCoro::Task<QUrl> prepare(const domain::Stream& stream,
        const domain::PlaybackContext& ctx);

    /// Same per-asset preparation as `prepare()` but without the
    /// legacy `LocalStreamServer`. Forwards to
    /// `LibtorrentClient::prepareSession`.
    virtual QCoro::Task<PreparedSession> prepareSession(
        const domain::Stream& stream, const domain::PlaybackContext& ctx,
        PrepareMode mode = PrepareMode::Streaming);

    QCoro::Task<bool> ensureRange(const QString& token, ByteRange range);
    QByteArray readRange(const QString& token, ByteRange range) const;
    qint64 fileSizeForToken(const QString& token) const;
    QString fileNameForToken(const QString& token) const;
    void touchToken(const QString& token);

    virtual QVector<TorrentFileEntry> filesForInfoHash(
        const QString& infoHash) const;

    virtual void setKeepAlive(const QString& infoHash, bool on);
    virtual void pauseInfoHash(const QString& infoHash);
    virtual void resumeInfoHash(const QString& infoHash);
    virtual void promoteToFull(const QString& infoHash);

public Q_SLOTS:
    void stopInfoHash(const QString& infoHash);
    void stopForContext(const domain::PlaybackContext& ctx);
    void stopAll();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);

    /// Per-session telemetry forwarded from `LibtorrentClient`.
    /// `etaSeconds` is `-1` when the rate is zero or the file size
    /// is unknown.
    void torrentStatsChanged(const QString& infoHash,
        qint64 doneBytes,
        qint64 ratePayloadBps,
        int peers,
        int seeds,
        int etaSeconds);

    /// Fired once when libtorrent reports `torrent_finished_alert`
    /// for the matching info hash.
    void torrentFinished(const QString& infoHash);

    /// Fired on a fatal `torrent_error_alert` (storage error, etc.).
    void torrentFailed(const QString& infoHash, const QString& reason);

public:
    /// True once the underlying `LibtorrentClient` has built its
    /// `lt::session`. Test-only hook used by the lazy-start
    /// coverage to assert construction stays dormant.
    bool isStartedForTests() const noexcept
    {
        return m_client && m_client->isStarted();
    }

private:
    /// Owns the libtorrent session, alert pump, per-asset state,
    /// and the idle-stop timer. Constructed dormant for production
    /// instances; never built for `StubTag` instances. Parented to
    /// `this`.
    std::unique_ptr<kinema::playback::torrent::LibtorrentClient> m_client;

    /// Legacy localhost streaming server (only used by `prepare()`).
    /// Built lazily on the first `prepare()` call.
    std::unique_ptr<LocalStreamServer> m_legacyServer;
};

} // namespace kinema::torrent
