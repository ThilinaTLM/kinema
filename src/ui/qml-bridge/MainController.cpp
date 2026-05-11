// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MainController.h"

#include "api/CinemetaClient.h"
#include "api/OpenSubtitlesClient.h"
#include "api/PlaybackContext.h"
#include "api/AllDebridClient.h"
#include "api/RealDebridClient.h"
#include "api/TmdbClient.h"
#include "api/IndexerSelector.h"
#include "api/PeerflixIndexer.h"
#include "api/TorrentioIndexer.h"
#include "config/AppSettings.h"
#include "config/DebridSettings.h"
#include "config/DownloadSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/HistoryController.h"
#include "controllers/LibraryController.h"
#include "controllers/WatchedController.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "controllers/MprisController.h"
#include "controllers/PlaybackController.h"
#include "controllers/SeriesPlaybackSessionController.h"
#endif
#include "controllers/SubtitleController.h"
#include "controllers/TokenController.h"
#include "controllers/TrayController.h"
#include "core/Database.h"
#include "core/DownloadStore.h"
#include "core/HistoryStore.h"
#include "core/HttpClient.h"
#include "core/LibraryStore.h"
#include "core/MediaCache.h"
#include "core/WatchedStore.h"
#include "core/PlayerLauncher.h"
#include "core/SubtitleCacheStore.h"
#include "core/TokenStore.h"
#include "core/TorrentCache.h"
#include "download/DownloadManager.h"
#include "kinema_log_app.h"
#include "services/StreamActions.h"
#include "torrent/TorrentStreamingService.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/BrowseViewModel.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/DiscoverViewModel.h"
#include "ui/qml-bridge/DownloadsListModel.h"
#include "ui/qml-bridge/DownloadsViewModel.h"
#include "ui/qml-bridge/EpisodesListModel.h"
#include "ui/qml-bridge/AppIconResolver.h"
#include "ui/qml-bridge/KinemaImageProvider.h"
#include "ui/qml-bridge/LibraryListModel.h"
#include "ui/qml-bridge/LibraryRailModel.h"
#include "ui/qml-bridge/LibraryViewModel.h"
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
    if (m_torrentStreaming) {
        m_torrentStreaming->stopAll();
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

void MainController::requestSettings(const QString& category)
{
    // QML-side `KirigamiSettings.ConfigurationView` consumes the
    // category string as its `defaultModule` argument.
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
    // Indexer abstraction: a selector owns one or more concrete
    // indexers (Torrentio + Peerflix today) and view-models call
    // `selector->active()->streams()`. The active indexer tracks
    // `IndexerSettings`.
    m_indexers = new api::IndexerSelector(m_settings.indexers(), this);
    m_indexers->registerIndexer(std::make_unique<api::TorrentioIndexer>(
        m_http.get(), m_settings.torrentio(), m_settings.filter()));
    m_indexers->registerIndexer(std::make_unique<api::PeerflixIndexer>(
        m_http.get(), m_settings.peerflix()));
    m_tmdb = new api::TmdbClient(m_http.get(), this);
    m_imageLoader = new ImageLoader(m_http.get(), this);
    m_torrentCache = std::make_unique<core::TorrentCache>(
        m_settings.torrentStreaming(), this);
    m_torrentStreaming = new torrent::TorrentStreamingService(
        *m_torrentCache, m_settings.torrentStreaming(), this);
    m_streamActions = new services::StreamActions(
        m_player.get(), m_torrentStreaming, this);

    // Real-Debrid client + unified downloader settings/cache. The
    // RD client picks up its token from the keyring once the
    // TokenController has fired its initial reads. Routing between
    // RD and the libtorrent backend is decided inside
    // `download::BackendSelector` at enqueue time — if RD is
    // configured every stream goes through it; otherwise libtorrent
    // takes over.
    m_rd = std::make_unique<api::RealDebridClient>(m_http.get(), this);
    m_ad = std::make_unique<api::AllDebridClient>(m_http.get(), this);
    m_mediaCache = std::make_unique<core::MediaCache>(
        m_settings.download(), this);

    // History DB. Open is best-effort: a broken DB means history
    // is disabled for this session, not that the app refuses to
    // start. HistoryStore handles `!isOpen()` gracefully.
    m_db = std::make_unique<core::Database>(this);
    if (!m_db->open()) {
        qCWarning(KINEMA_APP)
            << "MainController: history database unavailable; "
               "history / resume features are disabled this session";
    }
    m_history = std::make_unique<core::HistoryStore>(*m_db, this);
    m_history->runRetentionPass();
    m_library = std::make_unique<core::LibraryStore>(*m_db, this);
    m_watched = std::make_unique<core::WatchedStore>(*m_db, this);
    m_subtitleCache
        = std::make_unique<core::SubtitleCacheStore>(*m_db, this);
    m_downloadStore = std::make_unique<core::DownloadStore>(*m_db, this);

    // The unified downloader. Wires the localhost media server,
    // RD-resolution pipeline, torrent engine, and persistent store.
    m_downloadManager = new download::DownloadManager(*m_http, *m_rd,
        *m_ad, *m_torrentStreaming, *m_downloadStore, *m_mediaCache,
        m_settings.download(), this);
    m_streamActions->setDownloadManager(m_downloadManager);
    m_downloadCtrl = new controllers::DownloadController(
        *m_downloadManager, *m_downloadStore, this);

    // Re-attach Full background downloads from the previous run.
    // OnDemand sessions are intentionally skipped: by definition
    // they only do work while a player is consuming them, and
    // there is no consumer at startup.
    m_downloadManager->resumePersisted();

    // Tokens — built before the OpenSubtitles client + history
    // controller because both reference its live `const QString&`
    // aliases.
    m_tokenCtrl = new controllers::TokenController(
        m_tokens.get(), m_tmdb, m_settings.debrid(), this);

    // Keep the RD / AD clients' in-memory tokens in sync with the
    // keyring-backed `TokenController`. Both backends check
    // `client.token()` / `client.apiKey()` for `canHandle`; the
    // active-provider gate lives in `BackendSelector`.
    m_rd->setToken(m_tokenCtrl->realDebridToken());
    m_ad->setApiKey(m_tokenCtrl->allDebridApiKey());
    connect(m_tokenCtrl,
        &controllers::TokenController::realDebridTokenChanged,
        m_rd.get(), [this](const QString& tok) {
            m_rd->setToken(tok);
        });
    connect(m_tokenCtrl,
        &controllers::TokenController::allDebridApiKeyChanged,
        m_ad.get(), [this](const QString& key) {
            m_ad->setApiKey(key);
        });

    // Forward the active-debrid-provider radio to the download
    // manager's selector. Initial value + future changes both go
    // through the same path.
    m_downloadManager->setActiveDebridProvider(
        m_settings.debrid().activeProvider());
    connect(&m_settings.debrid(),
        &config::DebridSettings::activeProviderChanged,
        m_downloadManager,
        [this](api::DebridProvider p) {
            m_downloadManager->setActiveDebridProvider(p);
        });

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
    const auto onOsCredentialChanged = [this](const QString&) {
        m_openSubtitles->clearJwt();
        m_subtitleCtrl->notifyAuthChanged();
    };
    connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesApiKeyChanged,
        m_openSubtitles, onOsCredentialChanged);
    connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesUsernameChanged,
        m_openSubtitles, onOsCredentialChanged);
    connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesPasswordChanged,
        m_openSubtitles, onOsCredentialChanged);

    // History controller. Two-phase: StreamActions is wired now;
    // `setPlayerWindow` lands in `openEmbeddedPlayer`.
    m_historyCtrl = new controllers::HistoryController(*m_history,
        m_indexers, m_tokenCtrl->realDebridToken(), this);
    m_streamActions->setHistoryController(m_historyCtrl);
    m_historyCtrl->setStreamActions(m_streamActions);

    m_libraryCtrl = new controllers::LibraryController(
        *m_library, m_cinemeta, this);
    // Lazy backfill of v7 schema columns (genres / rating / runtime
    // / cast) for titles saved before that migration. Queued so the
    // first event-loop tick boots the UI cleanly; backfill itself is
    // capped + silent on failure.
    QMetaObject::invokeMethod(m_libraryCtrl,
        &controllers::LibraryController::backfillMetadata,
        Qt::QueuedConnection);
    m_watchedCtrl = new controllers::WatchedController(
        *m_watched, m_historyCtrl, this);

    // Downloads page VM. Lives over the entire app lifetime so the
    // drawer's downloads entry can show counts even before the
    // first navigation to the page.
    m_downloadsVm = new DownloadsViewModel(*m_downloadCtrl,
        *m_downloadManager, m_streamActions, this);

    // Discover / Search / Browse surface VMs. They sit on top of
    // the existing service graph; action signals route back into
    // `MainController` either for direct controller forwarding
    // (resume / remove) or for navigation events the QML shell
    // listens for.
    m_discoverVm = new DiscoverViewModel(m_tmdb, m_tokenCtrl, this);
    m_continueWatchingVm
        = new ContinueWatchingViewModel(m_historyCtrl, this);
    m_libraryVm = new LibraryViewModel(m_libraryCtrl, m_watchedCtrl,
        m_settings.library(), this);
    m_searchVm = new SearchViewModel(m_cinemeta,
        m_settings.search(), this);
    m_browseVm = new BrowseViewModel(m_tmdb, m_settings.browse(), this);
    m_movieDetailVm = new MovieDetailViewModel(m_cinemeta,
        m_indexers, m_tmdb, m_streamActions, m_libraryCtrl,
        m_watchedCtrl, m_tokenCtrl, m_settings,
        m_tokenCtrl->realDebridToken(),
        m_tokenCtrl->allDebridApiKey(), this);
    m_movieDetailVm->setDownloadController(m_downloadCtrl);
    m_seriesDetailVm = new SeriesDetailViewModel(m_cinemeta,
        m_indexers, m_tmdb, m_streamActions, m_libraryCtrl,
        m_watchedCtrl, m_tokenCtrl, m_settings,
        m_tokenCtrl->realDebridToken(),
        m_tokenCtrl->allDebridApiKey(), this);
    m_seriesDetailVm->setDownloadController(m_downloadCtrl);

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
    // to the history controller; Details pushes the matching detail
    // page; Streams reuses the detail VMs directly and asks the
    // shell to push `StreamsPage` without an intermediate detail-page
    // push. Series entries thread the saved season + episode through
    // the detail VM so both routes land on the remembered episode.
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::resumeRequested, m_historyCtrl,
        &controllers::HistoryController::resumeFromHistory);
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::removeRequested, m_historyCtrl,
        &controllers::HistoryController::removeEntry);
    const auto openHistoryDetail = [this](const api::HistoryEntry& entry) {
        if (entry.key.kind == api::MediaKind::Movie) {
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
                                        const api::HistoryEntry& entry) {
        if (entry.key.kind == api::MediaKind::Movie) {
            if (!m_movieDetailVm || entry.key.imdbId.isEmpty()) {
                return;
            }
            m_movieDetailVm->clear();
            m_movieDetailVm->load(entry.key.imdbId);
            Q_EMIT showStreamsRequested(m_movieDetailVm);
            return;
        }

        if (!m_seriesDetailVm || entry.key.imdbId.isEmpty()
            || !entry.key.season || !entry.key.episode) {
            openHistoryDetail(entry);
            return;
        }
        m_seriesDetailVm->clear();
        m_seriesDetailVm->loadAt(entry.key.imdbId,
            *entry.key.season, *entry.key.episode);
        Q_EMIT showStreamsRequested(m_seriesDetailVm);
    };
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::detailRequested, this,
        openHistoryDetail);
    connect(m_continueWatchingVm,
        &ContinueWatchingViewModel::streamsRequested, this,
        openHistoryStreams);
    // Resume-from-history fallback: the saved release is gone, so
    // open the matching detail page so the user can pick another
    // stream.
    connect(m_historyCtrl,
        &controllers::HistoryController::resumeFallbackRequested,
        this, openHistoryDetail);

    connect(m_libraryVm, &LibraryViewModel::resumeRequested,
        m_historyCtrl, &controllers::HistoryController::resumeFromHistory);
    connect(m_libraryVm, &LibraryViewModel::openMovieRequested,
        this, &MainController::openMovieDetail);
    connect(m_libraryVm, &LibraryViewModel::openSeriesRequested,
        this, &MainController::openSeriesDetail);
    connect(m_libraryVm, &LibraryViewModel::openSeriesEpisodeRequested,
        this, &MainController::openSeriesDetailAt);

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
        m_tokens.get(), m_indexers, m_settings, m_subtitleCache.get(),
        m_mediaCache.get(), this);
    connect(m_settingsVm,
        &SettingsRootViewModel::tmdbTokenChanged, m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshTmdb(); });
    connect(m_settingsVm,
        &SettingsRootViewModel::realDebridTokenChanged,
        m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshRealDebrid(); });
    connect(m_settingsVm,
        &SettingsRootViewModel::allDebridApiKeyChanged,
        m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshAllDebrid(); });
    connect(m_settingsVm,
        &SettingsRootViewModel::activeDebridProviderChanged,
        m_tokenCtrl, [this] {
            // Pure UX: refresh both tokens so the in-memory copies
            // re-read the keyring (cheap; tokens didn't change but
            // it keeps the contract symmetric with the radio).
            m_tokenCtrl->refreshRealDebrid();
            m_tokenCtrl->refreshAllDebrid();
        });
    connect(m_settingsVm,
        &SettingsRootViewModel::subtitleCredentialsChanged,
        m_tokenCtrl, [this] {
            m_tokenCtrl->refreshOpenSubtitlesCredentials();
        });

#ifdef KINEMA_HAVE_LIBMPV
    m_playbackCtrl = new controllers::PlaybackController(
        *m_historyCtrl, m_settings, m_http.get(), this);
    m_seriesSessionCtrl = new controllers::SeriesPlaybackSessionController(
        *m_playbackCtrl, *m_torrentStreaming, *m_streamActions, this);
    m_mprisCtrl = new controllers::MprisController(
        *m_playbackCtrl, m_seriesSessionCtrl, this);
    connect(m_mprisCtrl, &controllers::MprisController::raiseRequested,
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
    connect(m_mprisCtrl, &controllers::MprisController::quitRequested,
        this, &MainController::requestQuit);
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
        qCInfo(KINEMA_APP) << "preferred media player not found on $PATH";
    }
}

void MainController::wireStatusForwarding()
{
    // Every controller / service that emits user-facing status
    // messages funnels through `passiveMessage` to a single
    // `Kirigami.PassiveNotification` in QML.
    connect(m_player.get(), &core::PlayerLauncher::launched, this,
        [this](core::player::Kind, const QString& title) {
            Q_EMIT passiveMessage(
                i18nc("@info:status", "Playing: %1", title), 4000);
        });
    connect(m_player.get(), &core::PlayerLauncher::launchFailed,
        this, [this](core::player::Kind, const QString& reason) {
            Q_EMIT passiveMessage(reason, 6000);
        });
    connect(m_streamActions, &services::StreamActions::statusMessage,
        this, &MainController::passiveMessage);
    connect(m_torrentStreaming, &torrent::TorrentStreamingService::statusMessage,
        this, &MainController::passiveMessage);
    connect(m_subtitleCtrl,
        &controllers::SubtitleController::statusMessage, this,
        &MainController::passiveMessage);
    connect(m_historyCtrl,
        &controllers::HistoryController::statusMessage, this,
        &MainController::passiveMessage);
    connect(m_libraryCtrl,
        &controllers::LibraryController::statusMessage, this,
        &MainController::passiveMessage);
#ifdef KINEMA_HAVE_LIBMPV
    // Embedded series-pack navigation + auto-next. The controller
    // derives prev/next strictly from the active torrent's file list
    // and requests window close when playback reaches a terminal EOF.
    if (m_playbackCtrl && m_seriesSessionCtrl) {
        connect(m_playbackCtrl,
            &controllers::PlaybackController::activeSessionChanged,
            m_seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::refreshFromPlayback);
        connect(m_playbackCtrl,
            &controllers::PlaybackController::endOfFile,
            m_seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::onPlayerEndOfFile);
        connect(m_playbackCtrl,
            &controllers::PlaybackController::endOfFile,
            this,
            [this](const QString&) {
                if (m_torrentStreaming && m_playbackCtrl) {
                    m_torrentStreaming->stopForContext(
                        m_playbackCtrl->currentContext());
                }
            });
        connect(m_playbackCtrl,
            &controllers::PlaybackController::userClosedWindow,
            m_seriesSessionCtrl,
            [this](const api::PlaybackContext& ctx) {
                if (m_torrentStreaming) {
                    m_torrentStreaming->stopForContext(ctx);
                }
                m_seriesSessionCtrl->onPlayerUserClosed(ctx);
            });
        connect(m_seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::windowCloseRequested,
            this, [this] {
                if (m_playerWindow) {
                    m_playerWindow->stopAndHide();
                }
            });
    }
#endif
#ifdef KINEMA_HAVE_LIBMPV
    connect(m_playbackCtrl,
        &controllers::PlaybackController::statusMessage, this,
        &MainController::passiveMessage);
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
ui::player::PlayerWindow* MainController::ensurePlayerWindow()
{
    if (m_playerWindow) {
        return m_playerWindow;
    }

    m_playerWindow = new ui::player::PlayerWindow(
        m_settings.appearance(), m_settings.player(), m_window);

    // The window persists across successive plays now — closing it
    // via the X button hides it and clears playback state, but
    // keeps the libmpv context alive for the next launch. We only react
    // to `destroyed()` for the application-shutdown case (the
    // window is parented to `m_window`, so it dies when the main
    // window dies).
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
        // `Qt::UniqueConnection` requires PMF on both ends, hence
        // the dedicated `onPlayerVisibilityChanged` slot rather
        // than an inline lambda.
        connect(m_playbackCtrl,
            &controllers::PlaybackController::visibilityChanged,
            this, &MainController::onPlayerVisibilityChanged,
            Qt::UniqueConnection);
    }

    auto* playerVm = m_playerWindow->viewModel();
    if (playerVm && m_seriesSessionCtrl) {
        const auto refreshEpisodeNavigation = [this, playerVm] {
            playerVm->setEpisodeNavigationState(
                m_seriesSessionCtrl->navigationVisible(),
                m_seriesSessionCtrl->canGoPrevious(),
                m_seriesSessionCtrl->canGoNext());
        };

        refreshEpisodeNavigation();
        connect(m_seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::navigationChanged,
            playerVm, refreshEpisodeNavigation);
        connect(m_playerWindow, &ui::player::PlayerWindow::previousRequested,
            m_seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::playPreviousEpisode);
        connect(m_playerWindow, &ui::player::PlayerWindow::nextRequested,
            m_seriesSessionCtrl,
            &controllers::SeriesPlaybackSessionController::playNextEpisode);
    }

    // Player chrome's `SubtitlePicker → Download…` lands on the
    // main window. We restore + raise the main window first so the
    // user can see the pushed page; the player keeps its separate
    // window visible behind it. The pushed Subtitles page tracks
    // the live `PlaybackController` context so its header reflects
    // whatever is currently playing.
    connect(playerVm,
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
        playerVm->setSubtitleDownloadEnabled(
            m_subtitleCtrl->downloadEnabled());
        connect(m_subtitleCtrl,
            &controllers::SubtitleController::downloadEnabledChanged,
            playerVm,
            &ui::player::PlayerViewModel::setSubtitleDownloadEnabled);
    }

    return m_playerWindow;
}

void MainController::openEmbeddedPlayer(const QUrl& url,
    const api::PlaybackContext& ctx)
{
    ensurePlayerWindow();
    if (m_playbackCtrl) {
        m_playbackCtrl->play(url, ctx);
    }
}

void MainController::onPlayerVisibilityChanged(bool /*visible*/)
{
    if (m_tray) {
        m_tray->refreshMenu();
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
#define KINEMA_REGISTER_QML_TYPE(Type) \
    qmlRegisterUncreatableType<Type>("dev.tlmtech.kinema.app", 1, 0, \
        #Type, QStringLiteral(#Type " is owned by C++."))

    KINEMA_REGISTER_QML_TYPE(DiscoverSectionModel);
    KINEMA_REGISTER_QML_TYPE(ResultsListModel);
    KINEMA_REGISTER_QML_TYPE(StreamsListModel);
    KINEMA_REGISTER_QML_TYPE(LibraryListModel);
    KINEMA_REGISTER_QML_TYPE(LibraryRailModel);
    KINEMA_REGISTER_QML_TYPE(LibraryViewModel);
    KINEMA_REGISTER_QML_TYPE(MovieDetailViewModel);
    KINEMA_REGISTER_QML_TYPE(SeriesDetailViewModel);
    KINEMA_REGISTER_QML_TYPE(EpisodesListModel);
    KINEMA_REGISTER_QML_TYPE(SubtitlesViewModel);
    KINEMA_REGISTER_QML_TYPE(SubtitleResultsModel);
    KINEMA_REGISTER_QML_TYPE(DownloadsViewModel);
    KINEMA_REGISTER_QML_TYPE(DownloadsListModel);
    KINEMA_REGISTER_QML_TYPE(SettingsRootViewModel);
    KINEMA_REGISTER_QML_TYPE(GeneralSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(TmdbSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(RealDebridSectionViewModel);
    KINEMA_REGISTER_QML_TYPE(AllDebridSectionViewModel);
    KINEMA_REGISTER_QML_TYPE(DebridSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(StreamsSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(PlayerSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(SubtitlesSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(TorrentStreamingSettingsViewModel);

#undef KINEMA_REGISTER_QML_TYPE

    if (!m_appIconResolver) {
        m_appIconResolver = new AppIconResolver(this);
        qmlRegisterSingletonInstance("dev.tlmtech.kinema.app", 1, 0,
            "AppIconResolver", m_appIconResolver);
    }

    auto* rootCtx = engine.rootContext();
    const std::pair<const char*, QObject*> contextProps[] = {
        { "mainController", this },
        { "discoverVm", m_discoverVm },
        { "continueWatchingVm", m_continueWatchingVm },
        { "libraryVm", m_libraryVm },
        { "searchVm", m_searchVm },
        { "browseVm", m_browseVm },
        { "movieDetailVm", m_movieDetailVm },
        { "seriesDetailVm", m_seriesDetailVm },
        { "subtitlesVm", m_subtitlesVm },
        { "settingsVm", m_settingsVm },
        { "downloadsVm", m_downloadsVm },
    };
    for (const auto& [name, obj] : contextProps) {
        rootCtx->setContextProperty(QString::fromLatin1(name), obj);
    }

    // Kick the initial Discover + Browse fetches once everything's
    // wired so each page lands populated rather than empty-spinner.
    // Token resolution may finish later via `loadAll()`; both VMs
    // listen on `tmdbTokenChanged` for a delayed-arrival refresh.
    m_discoverVm->refresh();
    m_browseVm->refresh();
}

} // namespace kinema::ui::qml
