// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/shell/ShellViewModel.h"

#include "config/AppSettings.h"
#include "config/AppearanceSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/LibraryController.h"
#include "playback/resume/ResumeUseCase.h"
#include "playback/session/PlaybackSessionManager.h"
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

ShellViewModel::ShellViewModel(ShellDependencies dependencies, QObject* parent)
    : QObject(parent), m_dependencies(dependencies)
{
    wireNavigationRouting();
    wireStatusForwarding();

    // Fire the initial keyring reads (RD + TMDB + OpenSubtitles)
    // off the main thread of the engine. Does not block.
    m_dependencies.tokenController->loadAll();
}

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
    return m_dependencies.settings.appearance().closeToTray();
}

bool ShellViewModel::trayAvailable() const
{
    auto* tray = m_dependencies.tray;
    return tray && tray->available();
}

ShellViewModel::CloseDecision ShellViewModel::evaluateCloseRequest(bool reallyQuit,
                                                                   bool closeToTrayPref,
                                                                   bool trayAvail,
                                                                   bool toastShown)
{
    if (reallyQuit || !closeToTrayPref || !trayAvail) {
        // Genuine quit path: accept the close and let Qt tear the
        // app down normally.
        return {/*acceptClose=*/true,
                /*hideWindow=*/false,
                /*emitToast=*/false};
    }
    return {/*acceptClose=*/false,
            /*hideWindow=*/true,
            /*emitToast=*/!toastShown};
}

bool ShellViewModel::handleWindowCloseRequested()
{
    const auto decision = evaluateCloseRequest(m_reallyQuit,
                                               m_dependencies.settings.appearance().closeToTray(),
                                               trayAvailable(),
                                               m_hasShownTrayToast);

    if (decision.hideWindow && m_window) {
        m_window->setVisible(false);
        if (auto* tray = m_dependencies.tray) {
            tray->refreshMenu();
        }
    }
    if (decision.emitToast) {
        m_hasShownTrayToast = true;
        // Use KNotification for the first-time toast: it shows in
        // the user's notification history alongside other Kinema
        // events. Per-session status messages from controllers go
        // through `passiveMessage` instead.
        auto* n = new KNotification(QStringLiteral("trayMinimized"), KNotification::CloseOnTimeout);
        n->setTitle(i18nc("@title:window notification", "Kinema is still running"));
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
#ifdef KINEMA_HAVE_LIBMPV
    // Finalize playback before tearing down transfer engines so the
    // history layer can record the last reliable position.
    if (m_playerWindow) {
        m_playerWindow->close();
    }
#endif
    if (auto* lt = m_dependencies.libtorrent) {
        lt->stopAll();
    }
    QCoreApplication::quit();
}

void ShellViewModel::attachWindow(QQuickWindow* window)
{
    m_window = window;
    wireTray();
}

ShellViewModel::~ShellViewModel() = default;

} // namespace kinema::ui::qml
