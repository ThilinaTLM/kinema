// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/ShellViewModel.h"

#include "app/ServiceContainer.h"
#include "config/AppSettings.h"
#include "config/AppearanceSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/HistoryController.h"
#include "controllers/LibraryController.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "controllers/MprisController.h"
#include "controllers/PlaybackController.h"
#include "controllers/SeriesPlaybackSessionController.h"
#endif
#include "controllers/SubtitleController.h"
#include "controllers/TokenController.h"
#include "controllers/TrayController.h"
#include "core/mpv/PlayerLauncher.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "kinema_log_app.h"
#include "services/StreamActions.h"
#include "torrent/TorrentStreamingService.h"
#include "ui/qml-bridge/BrowseViewModel.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/DiscoverViewModel.h"
#include "ui/qml-bridge/LibraryViewModel.h"
#include "ui/qml-bridge/MovieDetailViewModel.h"
#include "ui/qml-bridge/SearchViewModel.h"
#include "ui/qml-bridge/SeriesDetailViewModel.h"
#include "ui/qml-bridge/SubtitlesViewModel.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/PlayerViewModel.h"
#include "ui/player/PlayerWindow.h"
#endif

#include <KAboutData>
#include <KLocalizedString>
#include <KNotification>

#include <QCoreApplication>
#include <QQuickWindow>

namespace kinema::ui::qml {

ShellViewModel::ShellViewModel(app::ServiceContainer& services,
    QObject* parent)
    : QObject(parent)
    , m_services(services)
{
    wireNavigationRouting();
    wireStatusForwarding();

    // Fire the initial keyring reads (RD + TMDB + OpenSubtitles)
    // off the main thread of the engine. Does not block.
    m_services.tokenController()->loadAll();
}

ShellViewModel::~ShellViewModel() = default;

QString ShellViewModel::applicationName() const
{
    const auto about = KAboutData::applicationData();
    if (!about.displayName().isEmpty()) {
        return about.displayName();
    }
    return QStringLiteral("Kinema");
}

KAboutData ShellViewModel::aboutData() const
{
    return KAboutData::applicationData();
}

bool ShellViewModel::hideToTrayEnabled() const
{
    return m_services.settings().appearance().closeToTray();
}

bool ShellViewModel::trayAvailable() const
{
    auto* tray = m_services.tray();
    return tray && tray->available();
}

ShellViewModel::CloseDecision ShellViewModel::evaluateCloseRequest(
    bool reallyQuit, bool closeToTrayPref, bool trayAvail,
    bool toastShown)
{
    if (reallyQuit || !closeToTrayPref || !trayAvail) {
        // Genuine quit path: accept the close and let Qt tear the
        // app down normally.
        return { /*acceptClose=*/true, /*hideWindow=*/false,
            /*emitToast=*/false };
    }
    return { /*acceptClose=*/false, /*hideWindow=*/true,
        /*emitToast=*/!toastShown };
}

bool ShellViewModel::handleWindowCloseRequested()
{
    const auto decision = evaluateCloseRequest(m_reallyQuit,
        m_services.settings().appearance().closeToTray(),
        trayAvailable(), m_hasShownTrayToast);

    if (decision.hideWindow && m_window) {
        m_window->setVisible(false);
        if (auto* tray = m_services.tray()) {
            tray->refreshMenu();
        }
    }
    if (decision.emitToast) {
        m_hasShownTrayToast = true;
        // Use KNotification for the first-time toast: it shows in
        // the user's notification history alongside other Kinema
        // events. Per-session status messages from controllers go
        // through `passiveMessage` instead.
        auto* n = new KNotification(
            QStringLiteral("trayMinimized"),
            KNotification::CloseOnTimeout);
        n->setTitle(i18nc("@title:window notification",
            "Kinema is still running"));
        n->setText(i18nc("@info notification",
            "Closing the main window hid Kinema to the system "
            "tray. Right-click the tray icon to show or quit."));
        n->setIconName(QStringLiteral("dev.tlmtech.kinema"));
        n->sendEvent();
    }
    return decision.acceptClose;
}

void ShellViewModel::requestQuit()
{
    m_reallyQuit = true;
    if (auto* ts = m_services.torrentStreaming()) {
        ts->stopAll();
    }
#ifdef KINEMA_HAVE_LIBMPV
    // Take the player down explicitly so its closeEvent persists
    // geometry / volume and stops playback before exit.
    if (m_playerWindow) {
        m_playerWindow->close();
    }
#endif
    QCoreApplication::quit();
}

void ShellViewModel::requestSettings(const QString& category)
{
    Q_EMIT showSettingsRequested(category);
}

void ShellViewModel::pushSubtitlesPage(
    const domain::PlaybackContext& ctx, bool fromPlayer)
{
    auto* vm = m_services.subtitlesVm();
    if (!vm) {
        return;
    }
    vm->setAttachOnDownload(fromPlayer);
    vm->setMedia(ctx);
    Q_EMIT showSubtitlesRequested();
}

void ShellViewModel::requestAbout()
{
    Q_EMIT showAboutRequested();
}

void ShellViewModel::requestFocusSearch()
{
    Q_EMIT focusSearchRequested();
}

void ShellViewModel::applyBrowsePreset(int kind, int sort)
{
    auto* vm = m_services.browseVm();
    if (!vm) {
        return;
    }
    vm->applyPreset(kind, sort);
    Q_EMIT navigateToBrowseRequested();
}

void ShellViewModel::openMovieDetail(const QString& imdbId,
    const QString& /*title*/)
{
    auto* vm = m_services.movieDetailVm();
    if (!vm || imdbId.isEmpty()) {
        return;
    }
    vm->load(imdbId);
    Q_EMIT showMovieDetailRequested();
}

void ShellViewModel::openMovieDetailByTmdb(int tmdbId,
    const QString& title)
{
    auto* vm = m_services.movieDetailVm();
    if (!vm || tmdbId <= 0) {
        return;
    }
    vm->loadByTmdbId(tmdbId, title);
    Q_EMIT showMovieDetailRequested();
}

void ShellViewModel::openSeriesDetail(const QString& imdbId,
    const QString& /*title*/)
{
    auto* vm = m_services.seriesDetailVm();
    if (!vm || imdbId.isEmpty()) {
        return;
    }
    vm->load(imdbId);
    Q_EMIT showSeriesDetailRequested();
}

void ShellViewModel::openSeriesDetailAt(const QString& imdbId,
    const QString& /*title*/, int season, int episode)
{
    auto* vm = m_services.seriesDetailVm();
    if (!vm || imdbId.isEmpty()) {
        return;
    }
    vm->loadAt(imdbId, season, episode);
    Q_EMIT showSeriesDetailRequested();
}

void ShellViewModel::openSeriesDetailByTmdb(int tmdbId,
    const QString& title)
{
    auto* vm = m_services.seriesDetailVm();
    if (!vm || tmdbId <= 0) {
        return;
    }
    vm->loadByTmdbId(tmdbId, title);
    Q_EMIT showSeriesDetailRequested();
}

void ShellViewModel::attachWindow(QQuickWindow* window)
{
    m_window = window;
    wireTray();
}

void ShellViewModel::wireNavigationRouting()
{
    auto* continueWatchingVm = m_services.continueWatchingVm();
    auto* historyCtrl = m_services.historyController();
    auto* libraryVm = m_services.libraryVm();
    auto* discoverVm = m_services.discoverVm();
    auto* searchVm = m_services.searchVm();
    auto* browseVm = m_services.browseVm();
    auto* movieDetailVm = m_services.movieDetailVm();
    auto* seriesDetailVm = m_services.seriesDetailVm();
    auto* subtitlesVm = m_services.subtitlesVm();

    // Continue Watching action routing. Resume / remove go straight
    // to the history controller; Details pushes the matching detail
    // page; Streams reuses the detail VMs directly and asks the
    // shell to push `StreamsPage` without an intermediate detail-page
    // push. Series entries thread the saved season + episode through
    // the detail VM so both routes land on the remembered episode.
    connect(continueWatchingVm,
        &ContinueWatchingViewModel::resumeRequested, historyCtrl,
        &controllers::HistoryController::resumeFromHistory);
    connect(continueWatchingVm,
        &ContinueWatchingViewModel::removeRequested, historyCtrl,
        &controllers::HistoryController::removeEntry);
    const auto openHistoryDetail = [this](const domain::HistoryEntry& entry) {
        if (entry.key.kind == domain::MediaKind::Movie) {
            openMovieDetail(entry.key.imdbId, entry.title);
            return;
        }
        const auto title = entry.seriesTitle.isEmpty()
            ? entry.title : entry.seriesTitle;
        openSeriesDetailAt(entry.key.imdbId, title,
            entry.key.season.value_or(-1),
            entry.key.episode.value_or(-1));
    };
    const auto openHistoryStreams = [this, openHistoryDetail](
                                        const domain::HistoryEntry& entry) {
        if (entry.key.kind == domain::MediaKind::Movie) {
            auto* mv = m_services.movieDetailVm();
            if (!mv || entry.key.imdbId.isEmpty()) {
                return;
            }
            mv->clear();
            mv->load(entry.key.imdbId);
            Q_EMIT showStreamsRequested(mv);
            return;
        }

        auto* sv = m_services.seriesDetailVm();
        if (!sv || entry.key.imdbId.isEmpty()
            || !entry.key.season || !entry.key.episode) {
            openHistoryDetail(entry);
            return;
        }
        sv->clear();
        sv->loadAt(entry.key.imdbId,
            *entry.key.season, *entry.key.episode);
        Q_EMIT showStreamsRequested(sv);
    };
    connect(continueWatchingVm,
        &ContinueWatchingViewModel::detailRequested, this,
        openHistoryDetail);
    connect(continueWatchingVm,
        &ContinueWatchingViewModel::streamsRequested, this,
        openHistoryStreams);
    // Resume-from-history fallback: the saved release is gone, so
    // open the matching detail page so the user can pick another
    // stream.
    connect(historyCtrl,
        &controllers::HistoryController::resumeFallbackRequested,
        this, openHistoryDetail);

    connect(libraryVm, &LibraryViewModel::resumeRequested,
        historyCtrl, &controllers::HistoryController::resumeFromHistory);
    connect(libraryVm, &LibraryViewModel::openMovieRequested,
        this, &ShellViewModel::openMovieDetail);
    connect(libraryVm, &LibraryViewModel::openSeriesRequested,
        this, &ShellViewModel::openSeriesDetail);
    connect(libraryVm, &LibraryViewModel::openSeriesEpisodeRequested,
        this, &ShellViewModel::openSeriesDetailAt);
    // Resume-rail activations: load the matching detail VM as the
    // streams backing context, then push `StreamsPage` directly.
    // Mirrors the Continue Watching "Streams" action in shape.
    connect(libraryVm, &LibraryViewModel::openMovieStreamsRequested,
        this, [this](const QString& imdbId, const QString& /*title*/) {
            auto* mv = m_services.movieDetailVm();
            if (!mv || imdbId.isEmpty()) {
                return;
            }
            mv->clear();
            mv->load(imdbId);
            Q_EMIT showStreamsRequested(mv);
        });
    connect(libraryVm,
        &LibraryViewModel::openSeriesEpisodeStreamsRequested, this,
        [this](const QString& imdbId, const QString& title,
            int season, int episode) {
            auto* sv = m_services.seriesDetailVm();
            if (!sv || imdbId.isEmpty() || season <= 0
                || episode <= 0) {
                // Fall back to the detail page if anything is
                // missing -- safer than dropping the click.
                openSeriesDetailAt(imdbId, title, season, episode);
                return;
            }
            sv->clear();
            sv->loadAt(imdbId, season, episode);
            Q_EMIT showStreamsRequested(sv);
        });

    // Discover navigation routing. "Show all" forwards into the
    // Browse VM via a typed (kind, sort) preset and asks the shell
    // to navigate. Poster activation routes movies and series into
    // their respective detail VMs.
    connect(discoverVm, &DiscoverViewModel::browseRequested,
        this, &ShellViewModel::applyBrowsePreset);
    connect(discoverVm, &DiscoverViewModel::openMovieRequested,
        this, &ShellViewModel::openMovieDetailByTmdb);
    connect(discoverVm, &DiscoverViewModel::openSeriesRequested,
        this, &ShellViewModel::openSeriesDetailByTmdb);

    // Search VM action routing. Both movie and series activations
    // now push their respective detail page directly.
    connect(searchVm, &SearchViewModel::statusMessage, this,
        &ShellViewModel::passiveMessage);
    connect(searchVm, &SearchViewModel::openMovieRequested,
        this, &ShellViewModel::openMovieDetail);
    connect(searchVm, &SearchViewModel::openSeriesRequested,
        this, &ShellViewModel::openSeriesDetail);

    // Browse VM action routing.
    connect(browseVm, &BrowseViewModel::statusMessage, this,
        &ShellViewModel::passiveMessage);
    connect(browseVm, &BrowseViewModel::openMovieRequested,
        this, &ShellViewModel::openMovieDetailByTmdb);
    connect(browseVm, &BrowseViewModel::openSeriesRequested,
        this, &ShellViewModel::openSeriesDetailByTmdb);

    // Detail VM → shell.
    const auto pushSubtitlesFromDetail
        = [this](const domain::PlaybackContext& ctx) {
              pushSubtitlesPage(ctx, /*fromPlayer=*/false);
          };
    connect(movieDetailVm,
        &MovieDetailViewModel::statusMessage, this,
        &ShellViewModel::passiveMessage);
    connect(movieDetailVm,
        &MovieDetailViewModel::openMovieByTmdbRequested, this,
        &ShellViewModel::openMovieDetailByTmdb);
    connect(movieDetailVm,
        &MovieDetailViewModel::openSeriesByTmdbRequested, this,
        &ShellViewModel::openSeriesDetailByTmdb);
    connect(movieDetailVm,
        &MovieDetailViewModel::subtitlesRequested, this,
        pushSubtitlesFromDetail);
    connect(movieDetailVm,
        &MovieDetailViewModel::streamsRequested, this, [this] {
            Q_EMIT showStreamsRequested(m_services.movieDetailVm());
        });

    connect(seriesDetailVm,
        &SeriesDetailViewModel::statusMessage, this,
        &ShellViewModel::passiveMessage);
    connect(seriesDetailVm,
        &SeriesDetailViewModel::openMovieByTmdbRequested, this,
        &ShellViewModel::openMovieDetailByTmdb);
    connect(seriesDetailVm,
        &SeriesDetailViewModel::openSeriesByTmdbRequested, this,
        &ShellViewModel::openSeriesDetailByTmdb);
    connect(seriesDetailVm,
        &SeriesDetailViewModel::subtitlesRequested, this,
        pushSubtitlesFromDetail);
    connect(seriesDetailVm,
        &SeriesDetailViewModel::streamsRequested, this, [this] {
            Q_EMIT showStreamsRequested(m_services.seriesDetailVm());
        });

    // Subtitles VM. Routes download / local-file / settings
    // requests back through this shell.
    connect(subtitlesVm,
        &SubtitlesViewModel::settingsRequested, this, [this] {
            requestSettings(QStringLiteral("subtitles"));
        });
    connect(subtitlesVm,
        &SubtitlesViewModel::closeRequested, this,
        [this] { Q_EMIT popPageRequested(); });

#ifdef KINEMA_HAVE_LIBMPV
    auto* mprisCtrl = m_services.mprisController();
    auto* playerLauncher = m_services.player();
    if (mprisCtrl) {
        connect(mprisCtrl, &controllers::MprisController::raiseRequested,
            this, [this] {
                if (m_playerWindow && m_playerWindow->hasEverLoaded()) {
                    m_playerWindow->show();
                    m_playerWindow->raise();
                    m_playerWindow->requestActivate();
                    return;
                }
                if (m_window) {
                    m_window->setVisible(true);
                    m_window->raise();
                    m_window->requestActivate();
                }
            });
        connect(mprisCtrl, &controllers::MprisController::quitRequested,
            this, &ShellViewModel::requestQuit);
    }

    connect(playerLauncher,
        &core::PlayerLauncher::embeddedRequested, this,
        &ShellViewModel::openEmbeddedPlayer);
#endif
}

void ShellViewModel::wireStatusForwarding()
{
    // Every controller / service that emits user-facing status
    // messages funnels through `passiveMessage` to a single
    // `Kirigami.PassiveNotification` in QML.
    auto* player = m_services.player();
    connect(player, &core::PlayerLauncher::launched, this,
        [this](core::player::Kind, const QString& title) {
            Q_EMIT passiveMessage(
                i18nc("@info:status", "Playing: %1", title), 4000);
        });
    connect(player, &core::PlayerLauncher::launchFailed,
        this, [this](core::player::Kind, const QString& reason) {
            Q_EMIT passiveMessage(reason, 6000);
        });
    connect(m_services.streamActions(),
        &services::StreamActions::statusMessage,
        this, &ShellViewModel::passiveMessage);
    connect(m_services.torrentStreaming(),
        &torrent::TorrentStreamingService::statusMessage,
        this, &ShellViewModel::passiveMessage);
    connect(m_services.subtitleController(),
        &controllers::SubtitleController::statusMessage, this,
        &ShellViewModel::passiveMessage);
    connect(m_services.historyController(),
        &controllers::HistoryController::statusMessage, this,
        &ShellViewModel::passiveMessage);
    connect(m_services.libraryController(),
        &controllers::LibraryController::statusMessage, this,
        &ShellViewModel::passiveMessage);
#ifdef KINEMA_HAVE_LIBMPV
    auto* playbackCtrl = m_services.playbackController();
    auto* seriesSessionCtrl = m_services.seriesSessionController();
    auto* torrentStreaming = m_services.torrentStreaming();
    // Embedded series-pack navigation + auto-next. The controller
    // derives prev/next strictly from the active torrent's file list
    // and requests window close when playback reaches a terminal EOF.
    if (playbackCtrl && seriesSessionCtrl) {
        connect(playbackCtrl,
            &controllers::PlaybackController::activeSessionChanged,
            seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::refreshFromPlayback);
        connect(playbackCtrl,
            &controllers::PlaybackController::endOfFile,
            seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::onPlayerEndOfFile);
        connect(playbackCtrl,
            &controllers::PlaybackController::endOfFile,
            this,
            [this, torrentStreaming, playbackCtrl](const QString&) {
                if (torrentStreaming && playbackCtrl) {
                    torrentStreaming->stopForContext(
                        playbackCtrl->currentContext());
                }
            });
        connect(playbackCtrl,
            &controllers::PlaybackController::userClosedWindow,
            seriesSessionCtrl,
            [this, torrentStreaming, seriesSessionCtrl]
            (const domain::PlaybackContext& ctx) {
                if (torrentStreaming) {
                    torrentStreaming->stopForContext(ctx);
                }
                seriesSessionCtrl->onPlayerUserClosed(ctx);
            });
        connect(seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::windowCloseRequested,
            this, [this] {
                if (m_playerWindow) {
                    m_playerWindow->stopAndHide();
                }
            });
    }
    if (playbackCtrl) {
        connect(playbackCtrl,
            &controllers::PlaybackController::statusMessage, this,
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
    m_services.setTray(tray);
    connect(tray,
        &controllers::TrayController::toggleMainWindowRequested,
        this, [this, tray] {
            if (!m_window) {
                return;
            }
            const bool shown = m_window->isVisible()
                && (m_window->windowState() != Qt::WindowMinimized);
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
    connect(tray,
        &controllers::TrayController::showPlayerWindowRequested,
        this, [this, tray] {
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
    connect(tray,
        &controllers::TrayController::quitRequested, this,
        &ShellViewModel::requestQuit);
}

#ifdef KINEMA_HAVE_LIBMPV
ui::player::PlayerWindow* ShellViewModel::ensurePlayerWindow()
{
    if (m_playerWindow) {
        return m_playerWindow;
    }

    auto& settings = m_services.settings();
    m_playerWindow = new ui::player::PlayerWindow(
        settings.appearance(), settings.player(), m_window);

    auto* historyCtrl = m_services.historyController();
    auto* playbackCtrl = m_services.playbackController();
    auto* seriesSessionCtrl = m_services.seriesSessionController();
    auto* subtitlesVm = m_services.subtitlesVm();
    auto* subtitleCtrl = m_services.subtitleController();

    // The window persists across successive plays now — closing it
    // via the X button hides it and clears playback state, but
    // keeps the libmpv context alive for the next launch. We only
    // react to `destroyed()` for the application-shutdown case.
    connect(m_playerWindow, &QObject::destroyed, this,
        [this, historyCtrl](QObject* obj) {
            if (obj != m_playerWindow) {
                return;
            }
            m_playerWindow = nullptr;
            if (auto* tray = m_services.tray()) {
                tray->setPlayerWindow(nullptr);
            }
            if (historyCtrl) {
                historyCtrl->setPlayerWindow(nullptr);
            }
        });

    if (auto* tray = m_services.tray()) {
        tray->setPlayerWindow(m_playerWindow);
    }
    if (historyCtrl) {
        historyCtrl->setPlayerWindow(m_playerWindow);
    }
    if (playbackCtrl) {
        playbackCtrl->setPlayerWindow(m_playerWindow);
        // The visibilityChanged forwarding only needs to be wired
        // once for the lifetime of this VM; `Qt::UniqueConnection`
        // requires PMF on both ends.
        connect(playbackCtrl,
            &controllers::PlaybackController::visibilityChanged,
            this, &ShellViewModel::onPlayerVisibilityChanged,
            Qt::UniqueConnection);
    }

    auto* playerVm = m_playerWindow->viewModel();
    if (playerVm && seriesSessionCtrl) {
        const auto refreshEpisodeNavigation = [seriesSessionCtrl, playerVm] {
            playerVm->setEpisodeNavigationState(
                seriesSessionCtrl->navigationVisible(),
                seriesSessionCtrl->canGoPrevious(),
                seriesSessionCtrl->canGoNext());
        };

        refreshEpisodeNavigation();
        connect(seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::navigationChanged,
            playerVm, refreshEpisodeNavigation);
        connect(m_playerWindow, &ui::player::PlayerWindow::previousRequested,
            seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::playPreviousEpisode);
        connect(m_playerWindow, &ui::player::PlayerWindow::nextRequested,
            seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::playNextEpisode);
    }

    // Player chrome's `SubtitlePicker → Download…` lands on the
    // main window. We restore + raise the main window first so the
    // user can see the pushed page; the player keeps its separate
    // window visible behind it.
    if (playerVm) {
        connect(playerVm,
            &ui::player::PlayerViewModel::subtitlesDialogRequested,
            this, [this, playbackCtrl] {
                if (m_window) {
                    m_window->setVisible(true);
                    m_window->raise();
                    m_window->requestActivate();
                }
                if (playbackCtrl) {
                    pushSubtitlesPage(playbackCtrl->currentContext(),
                        /*fromPlayer=*/true);
                }
            });
    }

    // Subtitles VM → player. Sideload downloaded / picked local
    // files into mpv via the player's view-model.
    if (subtitlesVm && playerVm) {
        connect(subtitlesVm,
            &SubtitlesViewModel::downloadCompleted, playerVm,
            [this, subtitlesVm, playbackCtrl, playerVm]
            (domain::PlaybackKey key, const QString& fileId,
                const QString& localPath, const QString& lang,
                const QString& langName) {
                Q_UNUSED(fileId);
                if (!subtitlesVm->attachOnDownload()) {
                    return;
                }
                if (playbackCtrl
                    && playbackCtrl->currentKey() != key) {
                    return;
                }
                playerVm->attachExternalSubtitle(
                    localPath, langName, lang, /*select=*/true);
            });
        connect(subtitlesVm,
            &SubtitlesViewModel::localFileChosen, playerVm,
            [this, subtitlesVm, playbackCtrl, playerVm]
            (domain::PlaybackKey key, const QString& path) {
                if (!subtitlesVm->attachOnDownload()) {
                    return;
                }
                if (playbackCtrl
                    && playbackCtrl->currentKey() != key) {
                    return;
                }
                playerVm->attachExternalSubtitle(
                    path, QString {}, QString {},
                    /*select=*/true);
            });
    }

    // Mirror the subtitle-controller gate onto the player VM so
    // its `Download…` picker entry can disable itself when
    // OpenSubtitles isn't configured.
    if (subtitleCtrl && playerVm) {
        playerVm->setSubtitleDownloadEnabled(
            subtitleCtrl->downloadEnabled());
        connect(subtitleCtrl,
            &controllers::SubtitleController::downloadEnabledChanged,
            playerVm,
            &ui::player::PlayerViewModel::setSubtitleDownloadEnabled);
    }

    return m_playerWindow;
}

void ShellViewModel::openEmbeddedPlayer(const QUrl& url,
    const domain::PlaybackContext& ctx)
{
    ensurePlayerWindow();
    if (auto* playbackCtrl = m_services.playbackController()) {
        playbackCtrl->play(url, ctx);
    }
}

void ShellViewModel::onPlayerVisibilityChanged(bool /*visible*/)
{
    if (auto* tray = m_services.tray()) {
        tray->refreshMenu();
    }
}
#endif

} // namespace kinema::ui::qml
