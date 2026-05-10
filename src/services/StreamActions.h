// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

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
 * api::Stream: copy/open magnet, copy/open direct URL, play.
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
    void copyMagnet(const api::Stream& stream);
    void openMagnet(const api::Stream& stream);
    void copyDirectUrl(const api::Stream& stream);
    void openDirectUrl(const api::Stream& stream);

    /// Play `stream` with the identity/title information in `ctx`.
    /// Fills `ctx.streamRef` from the stream and asks the history
    /// controller (if wired) for a resume position before handing
    /// off to PlayerLauncher. Virtual so controller tests can record
    /// auto-next dispatches without spinning up a real launcher.
    virtual void play(const api::Stream& stream,
        const api::PlaybackContext& ctx);

    /// Same as `play()` but forces a specific download backend.
    /// Used by the per-stream override menu.
    void playWithBackend(const api::Stream& stream,
        const api::PlaybackContext& ctx,
        api::DownloadBackendKind backend);

    /// Background full-file download. Maps onto
    /// `DownloadManager::enqueueDownload` and never launches the
    /// player. Mode upgrade for already-streaming sessions is
    /// handled by the manager.
    void download(const api::Stream& stream,
        const api::PlaybackContext& ctx);
    void downloadWithBackend(const api::Stream& stream,
        const api::PlaybackContext& ctx,
        api::DownloadBackendKind backend);

Q_SIGNALS:
    /// Status-bar message. MainWindow connects this once.
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    void launchOpenUrlJob(const QUrl& url,
        const QString& successMsg,
        const QString& failurePrefix,
        const char* failureLogTag);

    QCoro::Task<void> playTorrentTask(api::Stream stream,
        api::PlaybackContext ctx, quint64 epoch);
    QCoro::Task<void> playLocalTask(api::Stream stream,
        api::PlaybackContext ctx, quint64 epoch,
        std::optional<api::DownloadBackendKind> backendOverride);

    void playInternal(const api::Stream& stream,
        const api::PlaybackContext& ctxIn,
        std::optional<api::DownloadBackendKind> backendOverride);

    core::PlayerLauncher* m_launcher;
    torrent::TorrentStreamingService* m_torrentStreaming;
    download::DownloadManager* m_downloadManager {};
    controllers::HistoryController* m_history {};
    quint64 m_playEpoch = 0;
};

} // namespace kinema::services
