// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MainController.h"

#include "api/CinemetaClient.h"
#include "api/OpenSubtitlesClient.h"
#include "api/PlaybackContext.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "controllers/PlaybackController.h"
#endif
#include "controllers/SubtitleController.h"
#include "controllers/TokenController.h"
#include "controllers/TrayController.h"
#include "core/Database.h"
#include "core/HistoryStore.h"
#include "core/HttpClient.h"
#include "core/PlayerLauncher.h"
#include "core/SubtitleCacheStore.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"
#include "services/StreamActions.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/KinemaImageProvider.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/PlayerViewModel.h"
#include "ui/player/PlayerWindow.h"
#endif

#include <KAboutData>
#include <KLocalizedContext>
#include <KLocalizedString>
#include <KNotification>

#include <QCoreApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QTimer>
#include <QUrl>

namespace kinema::ui::qml {

MainController::MainController(config::AppSettings& settings,
    QQmlApplicationEngine& engine, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
    buildCoreServices();
    wireStatusForwarding();
    registerImageProvider(engine);
    installLocalizedContext(engine);
    exposeContextProperties(engine);

    // Fire the initial keyring reads (RD + TMDB + OpenSubtitles)
    // off the main thread of the engine. Does not block.
    m_tokenCtrl->loadAll();
}

MainController::~MainController() = default;

QString MainController::applicationName() const
{
    const auto about = KAboutData::applicationData();
    if (!about.displayName().isEmpty()) {
        return about.displayName();
    }
    return QStringLiteral("Kinema");
}

KAboutData MainController::aboutData() const
{
    return KAboutData::applicationData();
}

bool MainController::hideToTrayEnabled() const
{
    return m_settings.appearance().closeToTray();
}

bool MainController::trayAvailable() const
{
    return m_tray && m_tray->available();
}

MainController::CloseDecision MainController::evaluateCloseRequest(
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

bool MainController::handleWindowCloseRequested()
{
    const auto decision = evaluateCloseRequest(m_reallyQuit,
        m_settings.appearance().closeToTray(), trayAvailable(),
        m_hasShownTrayToast);

    if (decision.hideWindow && m_window) {
        m_window->setVisible(false);
        if (m_tray) {
            m_tray->refreshMenu();
        }
    }
    if (decision.emitToast) {
        m_hasShownTrayToast = true;
        // Use KNotification for the first-time toast (carryover
        // from MainWindow): it shows in the user's notification
        // history alongside other Kinema events. Per-session
        // status messages from controllers go through
        // `passiveMessage` instead.
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

void MainController::requestQuit()
{
    m_reallyQuit = true;
#ifdef KINEMA_HAVE_LIBMPV
    // Take the player down explicitly so its closeEvent persists
    // geometry / volume and stops playback before exit.
    if (m_playerWindow) {
        m_playerWindow->close();
    }
#endif
    QCoreApplication::quit();
}

void MainController::requestSettings()
{
    Q_EMIT showSettingsRequested();
}

void MainController::requestAbout()
{
    Q_EMIT showAboutRequested();
}

void MainController::requestFocusSearch()
{
    Q_EMIT focusSearchRequested();
}

void MainController::attachWindow(QQuickWindow* window)
{
    m_window = window;
    wireTray();
}

void MainController::buildCoreServices()
{
    // Identical service graph to the legacy `MainWindow::buildCoreServices`.
    // The widget-coupled controllers (`Search`, `MovieDetail`,
    // `SeriesDetail`, `Navigation`) land in this controller in
    // phases 04/05 once their pages exist on the QML side.
    m_http = std::make_unique<core::HttpClient>(this);
    m_tokens = std::make_unique<core::TokenStore>(this);
    m_player = std::make_unique<core::PlayerLauncher>(
        m_settings.player(), this);
    m_cinemeta = new api::CinemetaClient(m_http.get(), this);
    m_torrentio = new api::TorrentioClient(m_http.get(), this);
    m_tmdb = new api::TmdbClient(m_http.get(), this);
    m_imageLoader = new ImageLoader(m_http.get(), this);
    m_streamActions = new services::StreamActions(m_player.get(), this);

    // History DB. Open is best-effort: a broken DB means history
    // is disabled for this session, not that the app refuses to
    // start. HistoryStore handles `!isOpen()` gracefully.
    m_db = std::make_unique<core::Database>(this);
    if (!m_db->open()) {
        qCWarning(KINEMA)
            << "MainController: history database unavailable; "
               "history / resume features are disabled this session";
    }
    m_history = std::make_unique<core::HistoryStore>(*m_db, this);
    m_history->runRetentionPass();
    m_subtitleCache
        = std::make_unique<core::SubtitleCacheStore>(*m_db, this);

    // Tokens — built before the OpenSubtitles client + history
    // controller because both reference its live `const QString&`
    // aliases.
    m_tokenCtrl = new controllers::TokenController(
        m_tokens.get(), m_tmdb, m_settings.realDebrid(), this);

    // OpenSubtitles + subtitle controller.
    m_openSubtitles = new api::OpenSubtitlesClient(m_http.get(),
        m_tokenCtrl->openSubtitlesApiKey(),
        m_tokenCtrl->openSubtitlesUsername(),
        m_tokenCtrl->openSubtitlesPassword(), this);
    m_subtitleCtrl = new controllers::SubtitleController(
        m_openSubtitles, m_subtitleCache.get(),
        m_settings.subtitle(), m_settings.cache(), this);
    QTimer::singleShot(0, this,
        [this] { m_subtitleCtrl->reconcileCacheOnStartup(); });
    // Credentials change → drop JWT so the next request re-logs
    // in, and tell the controller to re-evaluate downloadEnabled.
    connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesApiKeyChanged,
        m_openSubtitles, [this](const QString&) {
            m_openSubtitles->clearJwt();
            m_subtitleCtrl->notifyAuthChanged();
        });
    connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesUsernameChanged,
        m_openSubtitles, [this](const QString&) {
            m_openSubtitles->clearJwt();
            m_subtitleCtrl->notifyAuthChanged();
        });
    connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesPasswordChanged,
        m_openSubtitles, [this](const QString&) {
            m_openSubtitles->clearJwt();
            m_subtitleCtrl->notifyAuthChanged();
        });

    // History controller. Two-phase: StreamActions is wired now;
    // `setPlayerWindow` lands in `openEmbeddedPlayer`.
    m_historyCtrl = new controllers::HistoryController(*m_history,
        m_torrentio, m_settings, m_tokenCtrl->realDebridToken(),
        this);
    m_streamActions->setHistoryController(m_historyCtrl);
    m_historyCtrl->setStreamActions(m_streamActions);

#ifdef KINEMA_HAVE_LIBMPV
    m_playbackCtrl = new controllers::PlaybackController(
        *m_cinemeta, *m_torrentio, *m_streamActions, *m_historyCtrl,
        m_settings, m_tokenCtrl->realDebridToken(), m_http.get(),
        this);
    if (m_subtitleCtrl) {
        connect(m_playbackCtrl,
            &controllers::PlaybackController::moviehashComputed,
            m_subtitleCtrl,
            &controllers::SubtitleController::setMoviehash);
        connect(m_playbackCtrl,
            &controllers::PlaybackController::streamCleared,
            m_subtitleCtrl,
            &controllers::SubtitleController::clearMoviehash);
    }

    connect(m_player.get(),
        &core::PlayerLauncher::embeddedRequested, this,
        &MainController::openEmbeddedPlayer);
#endif

    if (!m_player->preferredPlayerAvailable()) {
        qCInfo(KINEMA) << "preferred media player not found on $PATH";
    }
}

void MainController::wireStatusForwarding()
{
    // Every controller / service that emits user-facing status
    // messages funnels through `passiveMessage` to a single
    // `Kirigami.PassiveNotification` in QML. The legacy
    // `statusBar()->showMessage` calls in `MainWindow` collapse
    // into this one signal.
    const auto forward = [this](const QString& text, int ms) {
        Q_EMIT passiveMessage(text, ms);
    };

    connect(m_player.get(), &core::PlayerLauncher::launched, this,
        [this](core::player::Kind, const QString& title) {
            Q_EMIT passiveMessage(
                i18nc("@info:status", "Playing: %1", title), 4000);
        });
    connect(m_player.get(), &core::PlayerLauncher::launchFailed,
        this, [this](core::player::Kind, const QString& reason) {
            Q_EMIT passiveMessage(reason, 6000);
        });
    connect(m_streamActions,
        &services::StreamActions::statusMessage, this, forward);
    connect(m_subtitleCtrl,
        &controllers::SubtitleController::statusMessage, this,
        forward);
    connect(m_historyCtrl,
        &controllers::HistoryController::statusMessage, this,
        forward);
#ifdef KINEMA_HAVE_LIBMPV
    connect(m_playbackCtrl,
        &controllers::PlaybackController::statusMessage, this,
        forward);
#endif
}

void MainController::wireTray()
{
    // No-op on desktops without a tray host. TrayController
    // self-detects QSystemTrayIcon::isSystemTrayAvailable() and
    // skips menu construction when missing.
    m_tray = new controllers::TrayController(m_window, this);
    connect(m_tray,
        &controllers::TrayController::toggleMainWindowRequested,
        this, [this] {
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
            m_tray->refreshMenu();
        });
    connect(m_tray,
        &controllers::TrayController::showPlayerWindowRequested,
        this, [this] {
#ifdef KINEMA_HAVE_LIBMPV
            if (m_playerWindow) {
                m_playerWindow->show();
                m_playerWindow->raise();
                m_playerWindow->requestActivate();
                m_tray->refreshMenu();
            }
#endif
        });
    connect(m_tray,
        &controllers::TrayController::quitRequested, this,
        &MainController::requestQuit);
}

#ifdef KINEMA_HAVE_LIBMPV
void MainController::openEmbeddedPlayer(const QUrl& url,
    const api::PlaybackContext& ctx)
{
    // Lazy construction. The MpvVideoItem inside `PlayerWindow`
    // initialises libmpv + an OpenGL render context in its ctor,
    // so we pay nothing until the user actually picks Embedded.
    if (!m_playerWindow) {
        m_playerWindow = new ui::player::PlayerWindow(
            m_settings.appearance(), m_settings.player(), m_window);
        if (m_tray) {
            m_tray->setPlayerWindow(m_playerWindow);
        }
        if (m_historyCtrl) {
            m_historyCtrl->setPlayerWindow(m_playerWindow);
        }
        if (m_playbackCtrl) {
            m_playbackCtrl->setPlayerWindow(m_playerWindow);
            connect(m_playbackCtrl,
                &controllers::PlaybackController::visibilityChanged,
                this, [this](bool) {
                    if (m_tray) {
                        m_tray->refreshMenu();
                    }
                });
        }

        // Phase 02 leaves the player → SubtitlesDialog routing
        // unwired: with `MainWindow` gone there's no QWidget to
        // own the dialog, and the SubtitlesDialog widget itself
        // is being replaced by a Kirigami page in phase 06. The
        // QML player chrome's `subtitlesDialogRequested` signal
        // therefore fires into the void here; phase 06 hooks it
        // up to the new pushed `SubtitlesPage`.
    }

    if (m_playbackCtrl) {
        m_playbackCtrl->play(url, ctx);
    }
}
#endif

void MainController::registerImageProvider(QQmlApplicationEngine& engine)
{
    // QQmlEngine takes ownership of the provider. Provider id is
    // referenced from QML as `image://kinema/<id>`.
    engine.addImageProvider(QStringLiteral("kinema"),
        new KinemaImageProvider(m_imageLoader));
}

void MainController::installLocalizedContext(
    QQmlApplicationEngine& engine)
{
    // Wire `KLocalizedContext` so QML can call `i18n(...)` /
    // `i18nc(...)` and have them route through the same KCatalog
    // that `KLocalizedString` uses on the C++ side. Without this,
    // the calls fall through to the literal English string and
    // translations never load. Owned by the engine's root context.
    auto* ctx = new KLocalizedContext(&engine);
    engine.rootContext()->setContextObject(ctx);
}

void MainController::exposeContextProperties(
    QQmlApplicationEngine& engine)
{
    engine.rootContext()->setContextProperty(
        QStringLiteral("mainController"), this);
}

} // namespace kinema::ui::qml
