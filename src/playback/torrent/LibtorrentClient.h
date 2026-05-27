// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "torrent/PiecePlanner.h"
#include "torrent/TorrentFileEntry.h"

#include <QByteArray>
#include <QCoro/QCoroTask>
#include <QHash>
#include <QObject>
#include <QString>
#include <QTimer>
#include <QVector>

#include <memory>

namespace libtorrent {
class session;
}

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::core {
class TorrentCache;
}

namespace kinema::playback::torrent {

/**
 * Per-asset metadata returned by `prepareSession()`.
 *
 * The token is the opaque handle that `ensureRange()` / `readRange()`
 * / `touchToken()` operate on; the rest is what playback / the
 * unified downloader needs to register the asset with the localhost
 * gateway and present a meaningful filename to the user.
 */
struct PreparedSession {
    QString token;
    QString fileName;
    qint64 fileSize = 0;
    QString infoHash;
};

/**
 * Controls how aggressively `prepareSession()` blocks before
 * returning. `Streaming` is the playback default: wait for the
 * startup buffer to land so the player can begin immediately.
 * `Background` returns as soon as metadata is fetched and the file
 * is selected; the swarm keeps pulling pieces afterwards. Used by
 * the unified downloader for `Save offline` enqueues.
 */
enum class PrepareMode {
    Streaming,
    Background,
};

/**
 * Libtorrent-backed engine. Owns the `lt::session`, alert pump,
 * stats timer, idle-stop timer, and the per-asset session map
 * (metadata fetch / file selection / piece-priority policy / byte
 * range I/O).
 *
 * Constructed dormant — the `lt::session` and the per-asset
 * bookkeeping are only built on the first `prepareSession()` call,
 * so RD-only / AD-only users (and the unit tests) don't pay for
 * libtorrent at boot.
 *
 * Per-asset API (`prepareSession`, `ensureRange`, `readRange`,
 * `touchToken`, `filesForInfoHash`, `setKeepAlive`, `pauseInfoHash`,
 * `resumeInfoHash`, `promoteToFull`, `stopInfoHash`,
 * `stopForContext`, `stopAll`) was relocated here from the
 * now-retired `kinema::torrent::TorrentStreamingService`. The
 * remaining facade in `src/torrent/` forwards to this class while
 * external consumers are being cut over.
 */
class LibtorrentClient : public QObject
{
    Q_OBJECT
public:
    LibtorrentClient(
        const config::TorrentStreamingSettings& settings,
        core::TorrentCache& cache,
        QObject* parent = nullptr);
    ~LibtorrentClient() override;

    LibtorrentClient(const LibtorrentClient&) = delete;
    LibtorrentClient& operator=(const LibtorrentClient&) = delete;

    /// Build the libtorrent session and arm the idle-stop timer on
    /// first call. Safe to call repeatedly. Returns true once the
    /// session is up.
    bool ensureStarted();
    bool isStarted() const noexcept { return static_cast<bool>(m_session); }

    /// Raw libtorrent session. Returns nullptr until
    /// `ensureStarted()` has been called.
    libtorrent::session* session() noexcept { return m_session.get(); }
    const libtorrent::session* session() const noexcept
    {
        return m_session.get();
    }

    /// Re-apply transfer rate limits from `settings`. Safe to call
    /// before `ensureStarted()`; no-ops until the session is built.
    void applyTransferSettings();

    /// Number of live torrent handles known to the client. Updated
    /// internally as sessions come and go; exposed so tests can
    /// drive the stats timer manually.
    void setActiveHandleCount(int n);

    // -----------------------------------------------------------------
    // Per-asset session pipeline.
    // -----------------------------------------------------------------

    /// Add the torrent (if missing), wait for metadata, pick the
    /// best file, and (for `Streaming`) pre-warm the head/tail
    /// piece windows. Returns the opaque token / filename / size
    /// callers need to wire the asset into a downstream stream
    /// gateway.
    QCoro::Task<PreparedSession> prepareSession(
        const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        PrepareMode mode = PrepareMode::Streaming);

    /// Wait until every piece backing `[range.start..range.endInclusive]`
    /// is on disk. Bumps the in-flight read-ahead window so the
    /// torrent prioritises subsequent reads. False on timeout / no
    /// such token / invalid range.
    QCoro::Task<bool> ensureRange(const QString& token,
        kinema::torrent::ByteRange range);

    /// Synchronous read against the local payload file. Bytes
    /// beyond `ensureRange()` are not guaranteed to be present.
    QByteArray readRange(const QString& token,
        kinema::torrent::ByteRange range) const;

    qint64  fileSizeForToken(const QString& token) const;
    QString fileNameForToken(const QString& token) const;

    /// Mark a session as recently active so the idle-stop timer
    /// doesn't reap it.
    void touchToken(const QString& token);

    /// File catalog for the (already-prepared) session backing
    /// `infoHash`. Empty when no session exists or metadata is not
    /// yet available.
    QVector<kinema::torrent::TorrentFileEntry> filesForInfoHash(
        const QString& infoHash) const;

    /// Exempt the session for `infoHash` from idle-stop (used by
    /// `Save offline` pins).
    void setKeepAlive(const QString& infoHash, bool on);

    /// Pause / resume the libtorrent handle behind a session.
    /// User-initiated; bypasses idle-stop bookkeeping.
    void pauseInfoHash(const QString& infoHash);
    void resumeInfoHash(const QString& infoHash);

    /// Promote a streaming session to a full background download:
    /// drops every `set_piece_deadline()` entry, sets the selected
    /// file to top priority, and exempts the session from
    /// idle-stop. Idempotent.
    void promoteToFull(const QString& infoHash);

    /// Stop and remove the session for `infoHash` (or for the
    /// stream identified by `ctx`). `stopAll` is the shutdown path.
    void stopInfoHash(const QString& infoHash);
    void stopForContext(const domain::PlaybackContext& ctx);
    void stopAll();

Q_SIGNALS:
    /// User-facing status notes raised during `prepareSession`
    /// (`Fetching torrent metadata…`, `Buffering torrent stream…`).
    /// Subscribers route these to a passive notification UI.
    void statusMessage(QString text, int timeoutMs = 3000);

    /// `state_update_alert` decoded into per-handle telemetry.
    /// `etaSeconds` is `-1` when the rate is zero or remaining
    /// bytes can't be computed.
    void statsUpdated(QString infoHash, qint64 doneBytes,
        qint64 ratePayloadBps, int peers, int seeds,
        int etaSeconds, bool finished);

    /// `torrent_finished_alert` for an info hash.
    void torrentFinished(QString infoHash);

    /// Fatal `torrent_error_alert`.
    void torrentFailed(QString infoHash, QString reason);

    /// `metadata_received_alert` for an info hash. Subscribers may
    /// now safely call `torrent_handle::torrent_file()`.
    void metadataReceived(QString infoHash);

private Q_SLOTS:
    void drainAlerts();
    void postTorrentUpdates();
    void stopIdleSessions();

private:
    struct Session;

    Session*       byToken(const QString& token);
    const Session* byToken(const QString& token) const;
    void           stopHash(const QString& hash, const char* reason);
    void           refreshStatsTimerRunning();

    const config::TorrentStreamingSettings& m_settings;
    core::TorrentCache&                     m_cache;
    std::unique_ptr<libtorrent::session>    m_session;
    QTimer                                  m_statsTimer;
    QTimer                                  m_idleTimer;
    QHash<QString, Session>                 m_sessions;
    QHash<QString, QString>                 m_tokenToHash;
    int                                     m_activeHandles = 0;
};

} // namespace kinema::playback::torrent
