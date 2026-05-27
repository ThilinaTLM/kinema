// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QTimer>

#include <memory>

namespace libtorrent {
class session;
}

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::playback::torrent {

/**
 * Thin wrapper around `libtorrent::session`.
 *
 * Owns the libtorrent session, its settings, the alert-pump, and
 * the periodic stats timer. Lazy-starts so RD-only / AD-only users
 * (and the unit tests) don't pay the cost of constructing a real
 * session at boot.
 *
 * Higher-level callers (`TorrentMediaSource`, the soon-to-retire
 * `torrent::TorrentStreamingService`) use `session()` to add /
 * remove torrents and manipulate handles directly; the client
 * decodes the resulting alerts and re-emits them as typed Qt
 * signals.
 */
class LibtorrentClient : public QObject
{
    Q_OBJECT
public:
    explicit LibtorrentClient(
        const config::TorrentStreamingSettings& settings,
        QObject* parent = nullptr);
    ~LibtorrentClient() override;

    LibtorrentClient(const LibtorrentClient&) = delete;
    LibtorrentClient& operator=(const LibtorrentClient&) = delete;

    /// Build the libtorrent session on first call. Safe to call
    /// repeatedly. Returns true once the session is up.
    bool ensureStarted();
    bool isStarted() const noexcept { return static_cast<bool>(m_session); }

    /// Raw libtorrent session. Returns nullptr until
    /// `ensureStarted()` has been called. Callers operate on
    /// handles directly; the client only adds value at session /
    /// settings / alert boundaries.
    libtorrent::session* session() noexcept { return m_session.get(); }
    const libtorrent::session* session() const noexcept
    {
        return m_session.get();
    }

    /// Re-apply transfer rate limits from `settings`. Safe to call
    /// before `ensureStarted()`; no-ops until the session is built.
    void applyTransferSettings();

    /// Number of live torrent handles known to the client. Updated
    /// from alerts; used by `TorrentStreamingService` to decide
    /// whether to keep the stats timer running.
    void setActiveHandleCount(int n);

Q_SIGNALS:
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

private:
    const config::TorrentStreamingSettings& m_settings;
    std::unique_ptr<libtorrent::session> m_session;
    QTimer m_statsTimer;
    int m_activeHandles = 0;
};

} // namespace kinema::playback::torrent
