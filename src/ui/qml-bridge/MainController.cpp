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
#include "ui/qml-bridge/BrowseViewModel.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/DiscoverViewModel.h"
#include "ui/qml-bridge/EpisodesListModel.h"
#include "ui/qml-bridge/KinemaImageProvider.h"
#include "ui/qml-bridge/MovieDetailViewModel.h"
#include "ui/qml-bridge/ResultsListModel.h"
#include "ui/qml-bridge/SearchViewModel.h"
#include "ui/qml-bridge/SeriesDetailViewModel.h"
#include "ui/qml-bridge/SettingsViewModels.h"
#include "ui/qml-bridge/StreamsListModel.h"
#include "ui/qml-bridge/SubtitlesViewModel.h"
#include "ui/qml-bridge/SubtitleResultsModel.h"
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
#include <QQmlEngine>
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

void MainController::requestSettings(const QString& category)
{
    if (m_settingsVm) {
        m_settingsVm->setInitialCategory(category);
    }
    Q_EMIT showSettingsRequested(category);
}

void MainController::pushSubtitlesPage(
    const api::PlaybackContext& ctx, bool fromPlayer)
{
    if (!m_subtitlesVm) {
        return;
    }
    m_subtitlesVm->setAttachOnDownload(fromPlayer);
    m_subtitlesVm->setMedia(ctx);
    Q_EMIT showSubtitlesRequested();
}

void MainController::requestAbout()
{
    Q_EMIT showAboutRequested();
}

void MainController::requestFocusSearch()
{
    Q_EMIT focusSearchRequested();
}

void MainController::applyBrowsePreset(int kind, int sort)
{
    if (!m_browseVm) {
        return;
    }
    m_browseVm->applyPreset(kind, sort);
    Q_EMIT navigateToBrowseRequested();
}

void MainController::openMovieDetail(const QString& imdbId,
    const QString& /*title*/)
{
    if (!m_movieDetailVm || imdbId.isEmpty()) {
        return;
    }
    m_movieDetailVm->load(imdbId);
    Q_EMIT showMovieDetailRequested();
}

void MainController::openMovieDetailByTmdb(int tmdbId, const QString& title)
{
    if (!m_movieDetailVm || tmdbId <= 0) {
        return;
    }
    m_movieDetailVm->loadByTmdbId(tmdbId, title);
    // The page push happens immediately so the user sees the loading
    // hero rather than a stuck Discover/Browse grid; the VM's own
    // "Looking up …" status message covers the resolution latency.
    Q_EMIT showMovieDetailRequested();
}

void MainController::openSeriesDetail(const QString& imdbId,
    const QString& /*title*/)
{
    if (!m_seriesDetailVm || imdbId.isEmpty()) {
        return;
    }
    m_seriesDetailVm->load(imdbId);
    Q_EMIT showSeriesDetailRequested();
}

void MainController::openSeriesDetailAt(const QString& imdbId,
    const QString& /*title*/, int season, int episode)
{
    if (!m_seriesDetailVm || imdbId.isEmpty()) {
        return;
    }
    m_seriesDetailVm->loadAt(imdbId, season, episode);
    Q_EMIT showSeriesDetailRequested();
}

void MainController::openSeriesDetailByTmdb(int tmdbId,
    const QString& title)
{
    if (!m_seriesDetailVm || tmdbId <= 0) {
        return;
    }
    m_seriesDetailVm->loadByTmdbId(tmdbId, title);
    Q_EMIT showSeriesDetailRequested();
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

    // Discover / Search / Browse surface VMs. They sit on top of
    // the existing service graph; action signals route back into
    // `MainController` either for direct controller forwarding
    // (resume / remove) or for navigation events the QML shell
    // listens for.
    m_discoverVm = new DiscoverViewModel(m_tmdb, m_tokenCtrl, this);
    m_continueWatchingVm
        = new ContinueWatchingViewModel(m_historyCtrl, this);
    m_searchVm = new SearchViewModel(m_cinemeta,
        m_settings.search(), this);
    m_browseVm = new BrowseViewModel(m_tmdb, m_settings.browse(), this);
    m_movieDetailVm = new MovieDetailViewModel(m_cinemeta,
        m_torrentio, m_tmdb, m_streamActions, m_tokenCtrl,
        m_settings, m_tokenCtrl->realDebridToken(), this);
    m_seriesDetailVm = new SeriesDetailViewModel(m_cinemeta,
        m_torrentio, m_tmdb, m_streamActions, m_tokenCtrl,
        m_settings, m_tokenCtrl->realDebridToken(), this);

    // TMDB token gain/loss propagates from `TokenController` to
    // both the Discover VM (which already handles it internally)
    // and the Browse VM. We refresh on token gain and clear on
    // loss; the Browse VM's own `tmdbConfigured` property toggles
    // the placeholder visibility from QML.
    connect(m_tokenCtrl,
        &controllers::TokenController::tmdbTokenChanged, this,
        [this](const QString& token) {
            if (!m_browseVm) {
                return;
            }
            if (token.isEmpty()) {
                m_browseVm->results()->setItems({});
            } else {
                m_browseVm->refresh();
            }
        });

    // Continue Watching action routing. Resume / remove go straight
    // to the controller; detail / resume-fallback push the matching
    // detail page. Series entries thread the saved season + episode
    // through `openSeriesDetailAt` so the page lands pre-selected.
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::resumeRequested, m_historyCtrl,
        &controllers::HistoryController::resumeFromHistory);
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::removeRequested, m_historyCtrl,
        &controllers::HistoryController::removeEntry);
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::detailRequested, this,
        [this](const api::HistoryEntry& entry) {
            if (entry.key.kind == api::MediaKind::Movie) {
                openMovieDetail(entry.key.imdbId, entry.title);
                return;
            }
            const auto title = entry.seriesTitle.isEmpty()
                ? entry.title : entry.seriesTitle;
            openSeriesDetailAt(entry.key.imdbId, title,
                entry.key.season.value_or(-1),
                entry.key.episode.value_or(-1));
        });
    // Resume-from-history fallback: the saved release is gone, so
    // open the matching detail page so the user can pick another
    // stream. Same season / episode threading for series entries.
    connect(m_historyCtrl,
        &controllers::HistoryController::resumeFallbackRequested,
        this, [this](const api::HistoryEntry& entry) {
            if (entry.key.kind == api::MediaKind::Movie) {
                openMovieDetail(entry.key.imdbId, entry.title);
                return;
            }
            const auto title = entry.seriesTitle.isEmpty()
                ? entry.title : entry.seriesTitle;
            openSeriesDetailAt(entry.key.imdbId, title,
                entry.key.season.value_or(-1),
                entry.key.episode.value_or(-1));
        });

    // Discover navigation routing. "Show all" forwards into the
    // Browse VM via a typed (kind, sort) preset and asks the shell
    // to navigate. Poster activation routes movies and series into
    // their respective detail VMs.
    connect(m_discoverVm, &DiscoverViewModel::browseRequested,
        this, &MainController::applyBrowsePreset);
    connect(m_discoverVm, &DiscoverViewModel::openMovieRequested,
        this, &MainController::openMovieDetailByTmdb);
    connect(m_discoverVm, &DiscoverViewModel::openSeriesRequested,
        this, &MainController::openSeriesDetailByTmdb);

    // Search VM action routing. Both movie and series activations
    // now push their respective detail page directly.
    connect(m_searchVm, &SearchViewModel::statusMessage, this,
        &MainController::passiveMessage);
    connect(m_searchVm, &SearchViewModel::openMovieRequested,
        this, &MainController::openMovieDetail);
    connect(m_searchVm, &SearchViewModel::openSeriesRequested,
        this, &MainController::openSeriesDetail);

    // Browse VM action routing. Movies and series both route into
    // their TMDB-id detail flow (which resolves the IMDB id
    // internally) before pushing the matching detail page.
    connect(m_browseVm, &BrowseViewModel::statusMessage, this,
        &MainController::passiveMessage);
    connect(m_browseVm, &BrowseViewModel::openMovieRequested,
        this, &MainController::openMovieDetailByTmdb);
    connect(m_browseVm, &BrowseViewModel::openSeriesRequested,
        this, &MainController::openSeriesDetailByTmdb);

    // Detail VM → host. Status messages forward, similar carousels
    // re-push a fresh detail page (recursing through the TMDB
    // resolvers so movie↔series transitions Just Work), and
    // `subtitlesRequested` pushes the Kirigami subtitles page on
    // top of the current detail surface.
    const auto pushSubtitlesFromDetail
        = [this](const api::PlaybackContext& ctx) {
              pushSubtitlesPage(ctx, /*fromPlayer=*/false);
          };
    connect(m_movieDetailVm,
        &MovieDetailViewModel::statusMessage, this,
        &MainController::passiveMessage);
    connect(m_movieDetailVm,
        &MovieDetailViewModel::openMovieByTmdbRequested, this,
        &MainController::openMovieDetailByTmdb);
    connect(m_movieDetailVm,
        &MovieDetailViewModel::openSeriesByTmdbRequested, this,
        &MainController::openSeriesDetailByTmdb);
    connect(m_movieDetailVm,
        &MovieDetailViewModel::subtitlesRequested, this,
        pushSubtitlesFromDetail);
    connect(m_movieDetailVm,
        &MovieDetailViewModel::streamsRequested, this, [this] {
            Q_EMIT showStreamsRequested(m_movieDetailVm);
        });

    connect(m_seriesDetailVm,
        &SeriesDetailViewModel::statusMessage, this,
        &MainController::passiveMessage);
    connect(m_seriesDetailVm,
        &SeriesDetailViewModel::openMovieByTmdbRequested, this,
        &MainController::openMovieDetailByTmdb);
    connect(m_seriesDetailVm,
        &SeriesDetailViewModel::openSeriesByTmdbRequested, this,
        &MainController::openSeriesDetailByTmdb);
    connect(m_seriesDetailVm,
        &SeriesDetailViewModel::subtitlesRequested, this,
        pushSubtitlesFromDetail);
    connect(m_seriesDetailVm,
        &SeriesDetailViewModel::streamsRequested, this, [this] {
            Q_EMIT showStreamsRequested(m_seriesDetailVm);
        });

    // Subtitles VM. Wraps `SubtitleController` and routes
    // download / local-file / settings requests back through this
    // controller. The settings request lands the Subtitles
    // sub-page directly so the user can fix credentials and
    // bounce straight back to the search.
    m_subtitlesVm = new SubtitlesViewModel(m_subtitleCtrl,
        m_settings.subtitle(), this);
    connect(m_subtitlesVm,
        &SubtitlesViewModel::settingsRequested, this, [this] {
            requestSettings(QStringLiteral("subtitles"));
        });
    connect(m_subtitlesVm,
        &SubtitlesViewModel::closeRequested, this,
        [this] { Q_EMIT popPageRequested(); });

    // Settings root + token routing back to the live
    // TokenController. RD / TMDB / OS credential changes refresh
    // their respective in-memory aliases without a keyring
    // round-trip, matching the legacy SettingsDialog -> MainWindow
    // wiring.
    m_settingsVm = new SettingsRootViewModel(m_http.get(),
        m_tokens.get(), m_settings, m_subtitleCache.get(), this);
    connect(m_settingsVm,
        &SettingsRootViewModel::tmdbTokenChanged, m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshTmdb(); });
    connect(m_settingsVm,
        &SettingsRootViewModel::realDebridTokenChanged,
        m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshRealDebrid(); });
    connect(m_settingsVm,
        &SettingsRootViewModel::subtitleCredentialsChanged,
        m_tokenCtrl, [this] {
            m_tokenCtrl->refreshOpenSubtitlesCredentials();
        });

#ifdef KINEMA_HAVE_LIBMPV
    m_playbackCtrl = new controllers::PlaybackController(
        *m_historyCtrl, m_settings, m_http.get(), this);
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
    // Each playback gets its own `PlayerWindow` (and therefore its
    // own libmpv context). Tear down any prior instance first so
    // there is no carry-over of mpv state, picker selections, or
    // chrome view-model values between sessions.
    if (m_playerWindow) {
        // Detach all subscribers from the doomed window. Tray /
        // history hold raw pointers, so they must be cleared
        // explicitly; PlaybackController's own destroyed-handler
        // would clear it eventually, but we drive the cleanup here
        // synchronously so the new window's wiring lands on a
        // fully detached graph.
        if (m_tray) {
            m_tray->setPlayerWindow(nullptr);
        }
        if (m_historyCtrl) {
            m_historyCtrl->setPlayerWindow(nullptr);
        }
        if (m_playbackCtrl) {
            m_playbackCtrl->setPlayerWindow(nullptr);
        }
        auto* doomed = m_playerWindow;
        m_playerWindow = nullptr;
        doomed->stopAndHide();
        doomed->deleteLater();
    }

    m_playerWindow = new ui::player::PlayerWindow(
        m_settings.appearance(), m_settings.player(), m_window);

    // The window self-destructs from `closeEvent` so the user X
    // button drops the libmpv context for real. Clear our caches
    // when that happens; the next `openEmbeddedPlayer` call will
    // build a fresh window from scratch.
    connect(m_playerWindow, &QObject::destroyed, this,
        [this](QObject* obj) {
            if (obj != m_playerWindow) {
                return;
            }
            m_playerWindow = nullptr;
            if (m_tray) {
                m_tray->setPlayerWindow(nullptr);
            }
            if (m_historyCtrl) {
                m_historyCtrl->setPlayerWindow(nullptr);
            }
            // PlaybackController auto-clears via its own destroyed
            // hook; nothing to do here.
        });

    if (m_tray) {
        m_tray->setPlayerWindow(m_playerWindow);
    }
    if (m_historyCtrl) {
        m_historyCtrl->setPlayerWindow(m_playerWindow);
    }
    if (m_playbackCtrl) {
        m_playbackCtrl->setPlayerWindow(m_playerWindow);
        // The visibilityChanged forwarding only needs to be wired
        // once for the lifetime of the controller; PlaybackController
        // re-emits the new window's signal as `visibilityChanged`.
        // Use a unique connection so repeated openEmbeddedPlayer
        // calls don't stack duplicate handlers.
        connect(m_playbackCtrl,
            &controllers::PlaybackController::visibilityChanged,
            this, [this](bool) {
                if (m_tray) {
                    m_tray->refreshMenu();
                }
            }, Qt::UniqueConnection);
    }

    // Player chrome's `SubtitlePicker → Download…` lands on the
    // main window. We restore + raise the main window first so the
    // user can see the pushed page; the player keeps its separate
    // window visible behind it. The pushed Subtitles page tracks
    // the live `PlaybackController` context so its header reflects
    // whatever is currently playing.
    connect(m_playerWindow->viewModel(),
        &ui::player::PlayerViewModel::subtitlesDialogRequested,
        this, [this] {
            if (m_window) {
                m_window->setVisible(true);
                m_window->raise();
                m_window->requestActivate();
            }
            if (m_playbackCtrl) {
                pushSubtitlesPage(m_playbackCtrl->currentContext(),
                    /*fromPlayer=*/true);
            }
        });

    // Subtitles VM → player. On a successful download with
    // attach-on-download semantics, sideload the file into mpv via
    // the player's view-model. Local-file picks follow the same
    // path. We bind to the freshly-built `m_playerWindow` and
    // automatically lose the connection when it's destroyed (the
    // sender is `m_subtitlesVm`, but the receiver context object is
    // the player VM, so Qt drops the slot when that VM dies).
    if (m_subtitlesVm) {
        auto* playerVm = m_playerWindow->viewModel();
        connect(m_subtitlesVm,
            &SubtitlesViewModel::downloadCompleted, playerVm,
            [this, playerVm](api::PlaybackKey key, const QString& fileId,
                const QString& localPath, const QString& lang,
                const QString& langName) {
                Q_UNUSED(fileId);
                if (!m_subtitlesVm->attachOnDownload()) {
                    return;
                }
                if (m_playbackCtrl
                    && m_playbackCtrl->currentKey() != key) {
                    return;
                }
                playerVm->attachExternalSubtitle(
                    localPath, langName, lang, /*select=*/true);
            });
        connect(m_subtitlesVm,
            &SubtitlesViewModel::localFileChosen, playerVm,
            [this, playerVm](api::PlaybackKey key, const QString& path) {
                if (!m_subtitlesVm->attachOnDownload()) {
                    return;
                }
                if (m_playbackCtrl
                    && m_playbackCtrl->currentKey() != key) {
                    return;
                }
                playerVm->attachExternalSubtitle(
                    path, QString {}, QString {},
                    /*select=*/true);
            });
    }

    // Mirror the subtitle-controller gate onto the player VM so its
    // `Download…` picker entry can disable itself when
    // OpenSubtitles isn't configured.
    if (m_subtitleCtrl) {
        auto* playerVm = m_playerWindow->viewModel();
        playerVm->setSubtitleDownloadEnabled(
            m_subtitleCtrl->downloadEnabled());
        connect(m_subtitleCtrl,
            &controllers::SubtitleController::downloadEnabledChanged,
            playerVm,
            &ui::player::PlayerViewModel::setSubtitleDownloadEnabled);
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
    // Register the section model as an uncreatable QML type so
    // QML can read its `State` enum by name (e.g.
    // `DiscoverSectionModel.Loading`). This is a runtime
    // registration scoped to the engine's type system; it does
    // NOT route through `QML_ELEMENT` and therefore stays
    // compatible with the kinema_core / kinema_qml_app target
    // split. Calling it once per engine is fine; Qt deduplicates.
    qmlRegisterUncreatableType<DiscoverSectionModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "DiscoverSectionModel",
        QStringLiteral("DiscoverSectionModel is owned by C++."));
    qmlRegisterUncreatableType<ResultsListModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "ResultsListModel",
        QStringLiteral("ResultsListModel is owned by C++."));
    qmlRegisterUncreatableType<StreamsListModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "StreamsListModel",
        QStringLiteral("StreamsListModel is owned by C++."));
    qmlRegisterUncreatableType<MovieDetailViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "MovieDetailViewModel",
        QStringLiteral("MovieDetailViewModel is owned by C++."));
    qmlRegisterUncreatableType<SeriesDetailViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "SeriesDetailViewModel",
        QStringLiteral("SeriesDetailViewModel is owned by C++."));
    qmlRegisterUncreatableType<EpisodesListModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "EpisodesListModel",
        QStringLiteral("EpisodesListModel is owned by C++."));
    qmlRegisterUncreatableType<SubtitlesViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "SubtitlesViewModel",
        QStringLiteral("SubtitlesViewModel is owned by C++."));
    qmlRegisterUncreatableType<SubtitleResultsModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "SubtitleResultsModel",
        QStringLiteral("SubtitleResultsModel is owned by C++."));
    qmlRegisterUncreatableType<SettingsRootViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "SettingsRootViewModel",
        QStringLiteral("SettingsRootViewModel is owned by C++."));
    qmlRegisterUncreatableType<GeneralSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "GeneralSettingsViewModel",
        QStringLiteral("GeneralSettingsViewModel is owned by C++."));
    qmlRegisterUncreatableType<TmdbSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "TmdbSettingsViewModel",
        QStringLiteral("TmdbSettingsViewModel is owned by C++."));
    qmlRegisterUncreatableType<RealDebridSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "RealDebridSettingsViewModel",
        QStringLiteral("RealDebridSettingsViewModel is owned by C++."));
    qmlRegisterUncreatableType<FiltersSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "FiltersSettingsViewModel",
        QStringLiteral("FiltersSettingsViewModel is owned by C++."));
    qmlRegisterUncreatableType<PlayerSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "PlayerSettingsViewModel",
        QStringLiteral("PlayerSettingsViewModel is owned by C++."));
    qmlRegisterUncreatableType<SubtitlesSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "SubtitlesSettingsViewModel",
        QStringLiteral("SubtitlesSettingsViewModel is owned by C++."));
    qmlRegisterUncreatableType<AppearanceSettingsViewModel>(
        "dev.tlmtech.kinema.app", 1, 0,
        "AppearanceSettingsViewModel",
        QStringLiteral("AppearanceSettingsViewModel is owned by C++."));

    engine.rootContext()->setContextProperty(
        QStringLiteral("mainController"), this);
    engine.rootContext()->setContextProperty(
        QStringLiteral("discoverVm"), m_discoverVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("continueWatchingVm"), m_continueWatchingVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("searchVm"), m_searchVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("browseVm"), m_browseVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("movieDetailVm"), m_movieDetailVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("seriesDetailVm"), m_seriesDetailVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("subtitlesVm"), m_subtitlesVm);
    engine.rootContext()->setContextProperty(
        QStringLiteral("settingsVm"), m_settingsVm);

    // Kick the initial Discover + Browse fetches once everything's
    // wired so each page lands populated rather than empty-spinner.
    // Token resolution may finish later via `loadAll()`; both VMs
    // listen on `tmdbTokenChanged` for a delayed-arrival refresh.
    m_discoverVm->refresh();
    m_browseVm->refresh();
}

} // namespace kinema::ui::qml
