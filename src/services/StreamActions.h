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

namespace kinema::playback::resume {
class ResumeUseCase;
}

namespace kinema::core {
class PlayerLauncher;
}

namespace kinema::playback::transfer {
class TransferUseCase;
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
    explicit StreamActions(core::PlayerLauncher* launcher,
        QObject* parent = nullptr);

    /// Wire the unified downloader. When set, `play()` always asks
    /// the use-case for a localhost URL; the legacy direct/torrent
    /// branch is bypassed.
    void setTransferUseCase(playback::transfer::TransferUseCase* useCase);

    /// Wire the resume use-case used to seed `ctx.resumeSeconds`.
    /// Optional: when null, `play()` does not seed a resume
    /// position.
    void setResumeUseCase(playback::resume::ResumeUseCase* useCase);

public Q_SLOTS:
    void copyMagnet(const domain::Stream& stream);
    void openMagnet(const domain::Stream& stream);
    void copyDirectUrl(const domain::Stream& stream);
    void openDirectUrl(const domain::Stream& stream);

    /// Copy `stream.releaseName` (or a short fallback when the
    /// release is missing) to the system clipboard and surface a
    /// passive notification. Per `AGENTS.md`, every action on a
    /// `domain::Stream` flows through this service, so the menu
    /// item in `StreamRowActions.qml` dispatches here via the per-
    /// detail-VM `copyReleaseName(int row)` trampolines.
    void copyReleaseName(const domain::Stream& stream);

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
    /// `TransferUseCase::saveOffline` and never launches the
    /// player. Mode upgrade for already-streaming sessions is
    /// handled by the use-case.
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

    QCoro::Task<void> playLocalTask(domain::Stream stream,
        domain::PlaybackContext ctx, quint64 epoch,
        std::optional<domain::DownloadBackendKind> backendOverride);

    void playInternal(const domain::Stream& stream,
        const domain::PlaybackContext& ctxIn,
        std::optional<domain::DownloadBackendKind> backendOverride);

    core::PlayerLauncher* m_launcher;
    playback::transfer::TransferUseCase* m_transferUseCase {};
    playback::resume::ResumeUseCase* m_resume {};
    quint64 m_playEpoch = 0;
};

} // namespace kinema::services
