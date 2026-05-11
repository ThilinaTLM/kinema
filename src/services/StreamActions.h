// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"

#include <QCoro/QCoroTask>

#include <QObject>
#include <QUrl>

#include <optional>

namespace kinema::controllers {
class HistoryController;
}

namespace kinema::core {
class PlayerLauncher;
}

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::download {
class DownloadManager;
}

namespace kinema::services {

/**
 * Single entry point for every user-initiated action on an
 * domain::Stream: copy/open magnet, copy/open direct URL, play.
 *
 * UI widgets (StreamsPanel, context menus) call these slots directly.
 * The service centralises clipboard + KIO + launcher wiring so each
 * pane no longer needs five signals and MainWindow no longer needs
 * five handler slots.
 *
 * Status-bar messages flow out via statusMessage(); MainWindow
 * connects it once to its status bar.
 */
class StreamActions : public QObject
{
    Q_OBJECT
public:
    StreamActions(core::PlayerLauncher* launcher,
        torrent::TorrentStreamingService* torrentStreaming,
        QObject* parent = nullptr);

    /// Wire the unified downloader. When set, `play()` always asks
    /// the manager for a localhost URL; the legacy direct/torrent
    /// branch is bypassed.
    void setDownloadManager(download::DownloadManager* manager);

    /// Wire the history controller used to seed resume-from and
    /// record play-start entries. Two-phase init because the
    /// controller depends on `this` via resumeFromHistory.
    void setHistoryController(controllers::HistoryController* history);

public Q_SLOTS:
    void copyMagnet(const domain::Stream& stream);
    void openMagnet(const domain::Stream& stream);
    void copyDirectUrl(const domain::Stream& stream);
    void openDirectUrl(const domain::Stream& stream);

    /// Play `stream` with the identity/title information in `ctx`.
    /// Fills `ctx.streamRef` from the stream and asks the history
    /// controller (if wired) for a resume position before handing
    /// off to PlayerLauncher. Virtual so controller tests can record
    /// auto-next dispatches without spinning up a real launcher.
    virtual void play(const domain::Stream& stream,
        const domain::PlaybackContext& ctx);

    /// Same as `play()` but forces a specific download backend.
    /// Used by the per-stream override menu.
    void playWithBackend(const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadBackendKind backend);

    /// Background full-file download. Maps onto
    /// `DownloadManager::enqueueDownload` and never launches the
    /// player. Mode upgrade for already-streaming sessions is
    /// handled by the manager.
    void download(const domain::Stream& stream,
        const domain::PlaybackContext& ctx);
    void downloadWithBackend(const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadBackendKind backend);

Q_SIGNALS:
    /// Status-bar message. MainWindow connects this once.
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    void launchOpenUrlJob(const QUrl& url,
        const QString& successMsg,
        const QString& failurePrefix,
        const char* failureLogTag);

    QCoro::Task<void> playTorrentTask(domain::Stream stream,
        domain::PlaybackContext ctx, quint64 epoch);
    QCoro::Task<void> playLocalTask(domain::Stream stream,
        domain::PlaybackContext ctx, quint64 epoch,
        std::optional<domain::DownloadBackendKind> backendOverride);

    void playInternal(const domain::Stream& stream,
        const domain::PlaybackContext& ctxIn,
        std::optional<domain::DownloadBackendKind> backendOverride);

    core::PlayerLauncher* m_launcher;
    torrent::TorrentStreamingService* m_torrentStreaming;
    download::DownloadManager* m_downloadManager {};
    controllers::HistoryController* m_history {};
    quint64 m_playEpoch = 0;
};

} // namespace kinema::services
