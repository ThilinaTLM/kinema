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

#ifdef KINEMA_HAVE_LIBMPV

ui::player::PlayerWindow* ShellViewModel::ensurePlayerWindow()
{
    if (m_playerWindow) {
        return m_playerWindow;
    }

    auto& settings = m_dependencies.settings;
    m_playerWindow = new ui::player::PlayerWindow(settings.appearance(), settings.player());

    auto* embeddedAdapter = m_dependencies.embeddedPlayer;
    auto* seriesSession = m_dependencies.seriesSession;
    auto* subtitlesVm = m_dependencies.subtitles;
    auto* subtitleCtrl = m_dependencies.subtitleController;
    auto* sessions = m_dependencies.playbackSessions;

    // The window persists across successive plays now — closing it
    // via the X button hides it and clears playback state, but
    // keeps the libmpv context alive for the next launch. We only
    // react to `destroyed()` for the application-shutdown case.
    connect(m_playerWindow, &QObject::destroyed, this, [this, embeddedAdapter](QObject* obj) {
        if (obj != m_playerWindow) {
            return;
        }
        m_playerWindow = nullptr;
        if (auto* tray = m_dependencies.tray) {
            tray->setPlayerWindow(nullptr);
        }
        if (embeddedAdapter) {
            embeddedAdapter->setPlayerWindow(nullptr);
        }
    });

    if (auto* tray = m_dependencies.tray) {
        tray->setPlayerWindow(m_playerWindow);
    }
    if (embeddedAdapter) {
        // The embedded adapter is the sole owner of the player
        // window now: it drives play / transport, owns the load
        // watchdog, and publishes typed events on the
        // PlaybackEventStream. Subscribe to its visibilityChanged
        // re-emit so the tray menu refreshes on show / hide.
        embeddedAdapter->setPlayerWindow(m_playerWindow);
        connect(embeddedAdapter,
                &ui::player::EmbeddedMpvPlayerAdapter::visibilityChanged,
                this,
                &ShellViewModel::onPlayerVisibilityChanged,
                Qt::UniqueConnection);
    }

    auto* playerVm = m_playerWindow->viewModel();
    if (playerVm && seriesSession) {
        const auto refreshEpisodeNavigation = [seriesSession, playerVm] {
            playerVm->setEpisodeNavigationState(seriesSession->navigationVisible(),
                                                seriesSession->canGoPrevious(),
                                                seriesSession->canGoNext());
        };

        refreshEpisodeNavigation();
        connect(seriesSession,
                &playback::series::SeriesSessionService::navigationChanged,
                playerVm,
                refreshEpisodeNavigation);
        if (sessions) {
            connect(m_playerWindow,
                    &ui::player::PlayerWindow::previousRequested,
                    sessions,
                    &playback::session::PlaybackSessionManager::playPreviousEpisode);
            connect(m_playerWindow,
                    &ui::player::PlayerWindow::nextRequested,
                    sessions,
                    &playback::session::PlaybackSessionManager::playNextEpisode);
        }
    }

    // Player chrome's `SubtitlePicker → Download…` lands on the
    // main window. We restore + raise the main window first so the
    // user can see the pushed page; the player keeps its separate
    // window visible behind it.
    if (playerVm) {
        connect(playerVm,
                &ui::player::PlayerViewModel::subtitlesDialogRequested,
                this,
                [this, embeddedAdapter] {
                    if (m_window) {
                        m_window->setVisible(true);
                        m_window->raise();
                        m_window->requestActivate();
                    }
                    if (embeddedAdapter) {
                        pushSubtitlesPage(embeddedAdapter->activeContext(),
                                          /*fromPlayer=*/true);
                    }
                });
    }

    // Subtitles VM → player. Sideload downloaded / picked local
    // files into mpv via the player's view-model. The key filter
    // protects against subtitles that arrive after a fresh play
    // attempt has superseded the one the user issued the search
    // from.
    if (subtitlesVm && playerVm) {
        connect(subtitlesVm,
                &SubtitlesViewModel::downloadCompleted,
                playerVm,
                [this, subtitlesVm, embeddedAdapter, playerVm, sessions](domain::PlaybackKey key,
                                                                         const QString& fileId,
                                                                         const QString& localPath,
                                                                         const QString& lang,
                                                                         const QString& langName) {
                    Q_UNUSED(fileId);
                    if (!subtitlesVm->attachOnDownload()) {
                        return;
                    }
                    if (embeddedAdapter && embeddedAdapter->activeContext().key != key) {
                        return;
                    }
                    if (sessions) {
                        sessions->attachSubtitle(localPath, lang);
                    } else {
                        playerVm->attachExternalSubtitle(
                            localPath, langName, lang, /*select=*/true);
                    }
                });
        connect(subtitlesVm,
                &SubtitlesViewModel::localFileChosen,
                playerVm,
                [this, subtitlesVm, embeddedAdapter, playerVm, sessions](domain::PlaybackKey key,
                                                                         const QString& path) {
                    if (!subtitlesVm->attachOnDownload()) {
                        return;
                    }
                    if (embeddedAdapter && embeddedAdapter->activeContext().key != key) {
                        return;
                    }
                    if (sessions) {
                        sessions->attachSubtitle(path, QString{});
                    } else {
                        playerVm->attachExternalSubtitle(path,
                                                         QString{},
                                                         QString{},
                                                         /*select=*/true);
                    }
                });
    }

    // Mirror the subtitle-controller gate onto the player VM so
    // its `Download…` picker entry can disable itself when
    // OpenSubtitles isn't configured.
    if (subtitleCtrl && playerVm) {
        playerVm->setSubtitleDownloadEnabled(subtitleCtrl->downloadEnabled());
        connect(subtitleCtrl,
                &controllers::SubtitleController::downloadEnabledChanged,
                playerVm,
                &ui::player::PlayerViewModel::setSubtitleDownloadEnabled);
    }

    return m_playerWindow;
}

void ShellViewModel::openEmbeddedPlayer(const QUrl& url, const domain::PlaybackContext& ctx)
{
    ensurePlayerWindow();
    if (auto* adapter = m_dependencies.embeddedPlayer) {
        // The adapter's `play()` consumes `resumeSeconds` either
        // as a direct seek (below the prompt threshold) or as a
        // deferred prompt (above the threshold). Forward whatever
        // the caller put on the ctx; the adapter applies policy.
        adapter->play(url, ctx, ctx.resumeSeconds);
    }
}

void ShellViewModel::onPlayerVisibilityChanged(bool /*visible*/)
{
    if (auto* tray = m_dependencies.tray) {
        tray->refreshMenu();
    }
}

#endif

} // namespace kinema::ui::qml
