// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppSettings.h"
#include "config/AppearanceSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/LibraryController.h"
#include "playback/resume/ResumeUseCase.h"
#include "playback/session/PlaybackSessionManager.h"
#include "ui/qml-bridge/shell/ShellViewModel.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "playback/desktop/MprisPlaybackProjection.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/series/SeriesSessionService.h"
#include "ui/player/EmbeddedMpvPlayerAdapter.h"
#endif
#include "controllers/SubtitleController.h"
#include "controllers/TokenController.h"
#include "controllers/TrayController.h"
#include "core/io/OpenUrl.h"
#include "core/mpv/PlayerLauncher.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "kinema_log_app.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/EmbeddedMpvPlayerAdapter.h"
#endif
#include "playback/torrent/LibtorrentClient.h"
#include "services/StreamActions.h"
#include "ui/qml-bridge/browse/BrowseViewModel.h"
#include "ui/qml-bridge/details/MovieDetailViewModel.h"
#include "ui/qml-bridge/details/SeriesDetailViewModel.h"
#include "ui/qml-bridge/discover/DiscoverViewModel.h"
#include "ui/qml-bridge/downloads/DownloadsViewModel.h"
#include "ui/qml-bridge/library/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/library/LibraryViewModel.h"
#include "ui/qml-bridge/search/SearchViewModel.h"
#include "ui/qml-bridge/subtitles/SubtitlesViewModel.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/PlayerViewModel.h"
#include "ui/player/PlayerWindow.h"
#endif

#include <QClipboard>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QQuickWindow>
#include <QRegularExpression>
#include <QUrl>

#include <KAboutData>
#include <KLocalizedString>
#include <KNotification>

namespace kinema::ui::qml {

void ShellViewModel::wireStatusForwarding()
{
    // Every controller / service that emits user-facing status
    // messages funnels through `passiveMessage` to a single
    // `Kirigami.PassiveNotification` in QML.
    auto* player = m_dependencies.player;
    connect(player,
            &core::PlayerLauncher::launched,
            this,
            [this](core::player::Kind, const QString& title) {
                Q_EMIT passiveMessage(i18nc("@info:status", "Playing: %1", title), 4000);
            });
    connect(
        player,
        &core::PlayerLauncher::launchFailed,
        this,
        [this](core::player::Kind, const QString& reason) { Q_EMIT passiveMessage(reason, 6000); });
    connect(m_dependencies.streamActions,
            &services::StreamActions::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
    connect(m_dependencies.playbackSessions,
            &playback::session::PlaybackSessionManager::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
    connect(m_dependencies.libtorrent,
            &playback::torrent::LibtorrentClient::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
    connect(m_dependencies.subtitleController,
            &controllers::SubtitleController::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
    if (auto* downloadsVm = m_dependencies.downloads) {
        connect(
            downloadsVm, &DownloadsViewModel::statusMessage, this, &ShellViewModel::passiveMessage);
    }
    connect(m_dependencies.resume,
            &playback::resume::ResumeUseCase::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
    connect(m_dependencies.libraryController,
            &controllers::LibraryController::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
#ifdef KINEMA_HAVE_LIBMPV
    auto* seriesSession = m_dependencies.seriesSession;
    auto* libtorrentClient = m_dependencies.libtorrent;
    auto* embeddedAdapter = m_dependencies.embeddedPlayer;
    // Series adjacency + auto-next live entirely in
    // SeriesSessionService now (event-stream driven). The shell
    // only handles cross-subsystem residue: stopping the torrent
    // engine and detaching the download row when playback ends.
    // PlaybackEnded carries the typed end reason so we can pick
    // the right action without parsing mpv's stringly-typed reason.
    if (auto* eventStream = m_dependencies.playbackEvents) {
        connect(eventStream,
                &playback::events::PlaybackEventStream::eventPublished,
                this,
                [this, libtorrentClient](const playback::events::PlaybackEvent& e) {
                    if (!std::holds_alternative<playback::events::PlaybackEnded>(e)) {
                        return;
                    }
                    const auto& ended = std::get<playback::events::PlaybackEnded>(e);
                    // ReplacedByNewSource is the supersede signal
                    // emitted by `PlaybackSessionManager` itself;
                    // we don't want to stop the engine in that
                    // case — a fresh play is already in flight.
                    if (ended.reason == playback::PlaybackEndReason::ReplacedByNewSource) {
                        return;
                    }
                    if (libtorrentClient) {
                        libtorrentClient->stopForContext(ended.ctx);
                    }
                    // Sharpen `hasPlayerAttached` for the embedded
                    // player on user-stop paths (user closed window /
                    // MPRIS Stop): the Downloads page stops showing
                    // `Streaming` / `Downloading + Playing` chips
                    // while no consumer is actually reading bytes.
                    // External-player launches stay sticky — we
                    // don't observe their lifetime — and the engine's
                    // idle-stop timer eventually quiesces them via
                    // `TransferUseCase::detachPlayer`. NaturalEof
                    // also detaches: nothing is reading bytes once
                    // mpv has signalled EOF.
                    if (ended.reason == playback::PlaybackEndReason::UserStop
                        || ended.reason == playback::PlaybackEndReason::NaturalEof
                        || ended.reason == playback::PlaybackEndReason::LoadTimeout
                        || ended.reason == playback::PlaybackEndReason::PlayerError) {
                        if (auto* dc = m_dependencies.downloadController) {
                            if (const auto row = dc->findForKey(ended.ctx.key)) {
                                dc->detachPlayer(row->assetId);
                            }
                        }
                    }
                });
    }
    if (seriesSession) {
        connect(seriesSession,
                &playback::series::SeriesSessionService::windowCloseRequested,
                this,
                [this] {
                    if (m_playerWindow) {
                        m_playerWindow->stopAndHide();
                    }
                });
        // Close the loop between the picker's "Season pack" chip
        // and runtime behaviour: emit a one-shot passive status
        // message once adjacency has actually resolved against the
        // session's file catalog. The service de-dups per
        // `(infoHash, season, episode)` so seeks / resumes do not
        // re-trigger this.
        connect(seriesSession,
                &playback::series::SeriesSessionService::packAdjacencyResolved,
                this,
                [this](bool nextAvailable, int nextSeason, int nextEpisode) {
                    if (nextAvailable) {
                        const QString code = QStringLiteral("S%1E%2")
                                                 .arg(nextSeason, 2, 10, QLatin1Char('0'))
                                                 .arg(nextEpisode, 2, 10, QLatin1Char('0'));
                        Q_EMIT passiveMessage(i18nc("@info:status auto-next episode queued. "
                                                    "%1 is an episode code like 'S01E03'",
                                                    "Auto-play queued for %1.",
                                                    code),
                                              4000);
                    } else {
                        Q_EMIT passiveMessage(
                            i18nc("@info:status pack does not include next episode",
                                  "Next episode is not in this pack."),
                            4000);
                    }
                });

        // Hydrate the picker row's size cell once the playback
        // pipeline learns the authoritative byte count. Only the
        // series detail page has a relevant streams model in scope
        // here \u2014 the service doesn't fire for movies.
        connect(seriesSession,
                &playback::series::SeriesSessionService::currentStreamSizeResolved,
                this,
                [this](const QString& infoHash, int fileIndex, qint64 size) {
                    if (auto* vm = m_dependencies.seriesDetail) {
                        if (auto* model = vm->streams()) {
                            model->hydrateSize(infoHash, fileIndex, size);
                        }
                    }
                });
    }
    if (embeddedAdapter) {
        connect(embeddedAdapter,
                &ui::player::EmbeddedMpvPlayerAdapter::statusMessage,
                this,
                &ShellViewModel::passiveMessage);
    }
#endif
}

void ShellViewModel::wireTray()
{
    // No-op on desktops without a tray host. TrayController
    // self-detects QSystemTrayIcon::isSystemTrayAvailable() and
    // skips menu construction when missing. The container owns
    // the controller so its lifetime tracks the rest of the
    // services.
    auto* tray = new controllers::TrayController(m_window, this);
    m_dependencies.tray = tray;
    connect(tray, &controllers::TrayController::toggleMainWindowRequested, this, [this, tray] {
        if (!m_window) {
            return;
        }
        const bool shown =
            m_window->isVisible() && (m_window->windowState() != Qt::WindowMinimized);
        if (shown) {
            m_window->setVisible(false);
        } else {
            m_window->setVisible(true);
            if (m_window->windowState() == Qt::WindowMinimized) {
                m_window->setWindowState(Qt::WindowNoState);
            }
            m_window->raise();
            m_window->requestActivate();
        }
        tray->refreshMenu();
    });
    connect(tray, &controllers::TrayController::showPlayerWindowRequested, this, [this, tray] {
#ifdef KINEMA_HAVE_LIBMPV
        if (m_playerWindow) {
            m_playerWindow->show();
            m_playerWindow->raise();
            m_playerWindow->requestActivate();
            tray->refreshMenu();
        }
#else
            Q_UNUSED(tray);
#endif
    });
    connect(tray, &controllers::TrayController::quitRequested, this, &ShellViewModel::requestQuit);
}

} // namespace kinema::ui::qml
