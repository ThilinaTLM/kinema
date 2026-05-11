// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "app/ServiceContainer.h"

#include "api/AllDebridClient.h"
#include "api/CinemetaClient.h"
#include "api/IndexerSelector.h"
#include "api/OpenSubtitlesClient.h"
#include "api/PeerflixIndexer.h"
#include "api/RealDebridClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioIndexer.h"
#include "config/AppSettings.h"
#include "config/DebridSettings.h"
#include "config/DownloadSettings.h"
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
#include "controllers/WatchedController.h"
#include "core/io/HttpClient.h"
#include "core/mpv/PlayerLauncher.h"
#include "core/persistence/Database.h"
#include "core/persistence/DownloadStore.h"
#include "core/persistence/HistoryStore.h"
#include "core/persistence/LibraryStore.h"
#include "core/persistence/MediaCache.h"
#include "core/persistence/SubtitleCacheStore.h"
#include "core/persistence/TokenStore.h"
#include "core/persistence/TorrentCache.h"
#include "core/persistence/WatchedStore.h"
#include "download/DownloadManager.h"
#include "domain/Debrid.h"
#include "kinema_log_app.h"
#include "services/StreamActions.h"
#include "torrent/TorrentStreamingService.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/AppIconResolver.h"
#include "ui/qml-bridge/BrowseViewModel.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/DiscoverViewModel.h"
#include "ui/qml-bridge/DownloadsViewModel.h"
#include "ui/qml-bridge/LibraryViewModel.h"
#include "ui/qml-bridge/MovieDetailViewModel.h"
#include "ui/qml-bridge/SearchViewModel.h"
#include "ui/qml-bridge/SeriesDetailViewModel.h"
#include "ui/qml-bridge/SubtitlesViewModel.h"
#include "ui/qml-bridge/settings/SettingsRootViewModel.h"

#include <QMetaObject>
#include <QObject>
#include <QString>
#include <QTimer>

namespace kinema::app {

ServiceContainer::ServiceContainer(config::AppSettings& settings)
    : m_settings(settings)
    , m_anchor(std::make_unique<QObject>())
{
    QObject* a = m_anchor.get();

    // Identical service graph to the legacy
    // `MainController::buildCoreServices`. Cross-cutting routing
    // that needs ShellViewModel slots is wired separately in
    // `ShellViewModel`'s constructor.
    m_http = std::make_unique<core::HttpClient>(a);
    m_tokens = std::make_unique<core::TokenStore>(a);
    m_player = std::make_unique<core::PlayerLauncher>(
        m_settings.player(), a);
    m_cinemeta = new api::CinemetaClient(m_http.get(), a);
    // Indexer abstraction: a selector owns one or more concrete
    // indexers (Torrentio + Peerflix today) and view-models call
    // `selector->active()->streams()`. The active indexer tracks
    // `IndexerSettings`.
    m_indexers = new api::IndexerSelector(m_settings.indexers(), a);
    m_indexers->registerIndexer(std::make_unique<api::TorrentioIndexer>(
        m_http.get(), m_settings.torrentio(), m_settings.filter()));
    m_indexers->registerIndexer(std::make_unique<api::PeerflixIndexer>(
        m_http.get(), m_settings.peerflix()));
    m_tmdb = new api::TmdbClient(m_http.get(), a);
    m_imageLoader = new ui::ImageLoader(m_http.get(), a);
    m_torrentCache = std::make_unique<core::TorrentCache>(
        m_settings.torrentStreaming(), a);
    m_torrentStreaming = new torrent::TorrentStreamingService(
        *m_torrentCache, m_settings.torrentStreaming(), a);
    m_streamActions = new services::StreamActions(
        m_player.get(), m_torrentStreaming, a);

    // Real-Debrid client + unified downloader settings/cache. The
    // RD client picks up its token from the keyring once the
    // TokenController has fired its initial reads. Routing between
    // RD and the libtorrent backend is decided inside
    // `download::BackendSelector` at enqueue time — if RD is
    // configured every stream goes through it; otherwise libtorrent
    // takes over.
    m_rd = std::make_unique<api::RealDebridClient>(m_http.get(), a);
    m_ad = std::make_unique<api::AllDebridClient>(m_http.get(), a);
    m_mediaCache = std::make_unique<core::MediaCache>(
        m_settings.download(), a);

    // History DB. Open is best-effort: a broken DB means history
    // is disabled for this session, not that the app refuses to
    // start. HistoryStore handles `!isOpen()` gracefully.
    m_db = std::make_unique<core::Database>(a);
    if (!m_db->open()) {
        qCWarning(KINEMA_APP)
            << "ServiceContainer: history database unavailable; "
               "history / resume features are disabled this session";
    }
    m_history = std::make_unique<core::HistoryStore>(*m_db, a);
    m_history->runRetentionPass();
    m_library = std::make_unique<core::LibraryStore>(*m_db, a);
    m_watched = std::make_unique<core::WatchedStore>(*m_db, a);
    m_subtitleCache
        = std::make_unique<core::SubtitleCacheStore>(*m_db, a);
    m_downloadStore = std::make_unique<core::DownloadStore>(*m_db, a);

    // The unified downloader. Wires the localhost media server,
    // RD-resolution pipeline, torrent engine, and persistent store.
    m_downloadManager = new download::DownloadManager(*m_http, *m_rd,
        *m_ad, *m_torrentStreaming, *m_downloadStore, *m_mediaCache,
        m_settings.download(), a);
    m_streamActions->setDownloadManager(m_downloadManager);
    m_downloadCtrl = new controllers::DownloadController(
        *m_downloadManager, *m_downloadStore, a);

    // Re-attach Full background downloads from the previous run.
    // OnDemand sessions are intentionally skipped: by definition
    // they only do work while a player is consuming them, and
    // there is no consumer at startup.
    m_downloadManager->resumePersisted();

    // Tokens — built before the OpenSubtitles client + history
    // controller because both reference its live `const QString&`
    // aliases.
    m_tokenCtrl = new controllers::TokenController(
        m_tokens.get(), m_tmdb, m_settings.debrid(), a);

    // Keep the RD / AD clients' in-memory tokens in sync with the
    // keyring-backed `TokenController`. Both backends check
    // `client.token()` / `client.apiKey()` for `canHandle`; the
    // active-provider gate lives in `BackendSelector`.
    m_rd->setToken(m_tokenCtrl->realDebridToken());
    m_ad->setApiKey(m_tokenCtrl->allDebridApiKey());
    QObject::connect(m_tokenCtrl,
        &controllers::TokenController::realDebridTokenChanged,
        m_rd.get(), [this](const QString& tok) {
            m_rd->setToken(tok);
        });
    QObject::connect(m_tokenCtrl,
        &controllers::TokenController::allDebridApiKeyChanged,
        m_ad.get(), [this](const QString& key) {
            m_ad->setApiKey(key);
        });

    // Forward the active-debrid-provider radio to the download
    // manager's selector. Initial value + future changes both go
    // through the same path.
    m_downloadManager->setActiveDebridProvider(
        m_settings.debrid().activeProvider());
    QObject::connect(&m_settings.debrid(),
        &config::DebridSettings::activeProviderChanged,
        m_downloadManager,
        [this](domain::DebridProvider p) {
            m_downloadManager->setActiveDebridProvider(p);
        });

    // OpenSubtitles + subtitle controller.
    m_openSubtitles = new api::OpenSubtitlesClient(m_http.get(),
        m_tokenCtrl->openSubtitlesApiKey(),
        m_tokenCtrl->openSubtitlesUsername(),
        m_tokenCtrl->openSubtitlesPassword(), a);
    m_subtitleCtrl = new controllers::SubtitleController(
        m_openSubtitles, m_subtitleCache.get(),
        m_settings.subtitle(), m_settings.cache(), a);
    QTimer::singleShot(0, m_subtitleCtrl,
        [this] { m_subtitleCtrl->reconcileCacheOnStartup(); });
    // Credentials change → drop JWT so the next request re-logs
    // in, and tell the controller to re-evaluate downloadEnabled.
    const auto onOsCredentialChanged = [this](const QString&) {
        m_openSubtitles->clearJwt();
        m_subtitleCtrl->notifyAuthChanged();
    };
    QObject::connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesApiKeyChanged,
        m_openSubtitles, onOsCredentialChanged);
    QObject::connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesUsernameChanged,
        m_openSubtitles, onOsCredentialChanged);
    QObject::connect(m_tokenCtrl,
        &controllers::TokenController::openSubtitlesPasswordChanged,
        m_openSubtitles, onOsCredentialChanged);

    // History controller. Two-phase: StreamActions is wired now;
    // `setPlayerWindow` lands in `ShellViewModel::ensurePlayerWindow`.
    m_historyCtrl = new controllers::HistoryController(*m_history,
        m_indexers, m_tokenCtrl->realDebridToken(), a);
    m_streamActions->setHistoryController(m_historyCtrl);
    m_historyCtrl->setStreamActions(m_streamActions);

    m_libraryCtrl = new controllers::LibraryController(
        *m_library, m_cinemeta, a);
    // Lazy backfill of v7 schema columns (genres / rating / runtime
    // / cast) for titles saved before that migration. Queued so the
    // first event-loop tick boots the UI cleanly; backfill itself is
    // capped + silent on failure.
    QMetaObject::invokeMethod(m_libraryCtrl,
        &controllers::LibraryController::backfillMetadata,
        Qt::QueuedConnection);
    m_watchedCtrl = new controllers::WatchedController(
        *m_watched, m_historyCtrl, a);

    // Downloads page VM. Lives over the entire app lifetime so the
    // drawer's downloads entry can show counts even before the
    // first navigation to the page.
    m_downloadsVm = new ui::qml::DownloadsViewModel(*m_downloadCtrl,
        *m_downloadManager, m_streamActions, a);

    // Discover / Search / Browse surface VMs. They sit on top of
    // the existing service graph; action signals route back into
    // `ShellViewModel` either for direct controller forwarding
    // (resume / remove) or for navigation events the QML shell
    // listens for.
    m_discoverVm = new ui::qml::DiscoverViewModel(m_tmdb, m_tokenCtrl, a);
    m_continueWatchingVm
        = new ui::qml::ContinueWatchingViewModel(m_historyCtrl, a);
    m_libraryVm = new ui::qml::LibraryViewModel(m_libraryCtrl, m_watchedCtrl,
        m_settings.library(), a);
    m_searchVm = new ui::qml::SearchViewModel(m_cinemeta,
        m_settings.search(), a);
    m_browseVm = new ui::qml::BrowseViewModel(m_tmdb, m_settings.browse(), a);
    m_movieDetailVm = new ui::qml::MovieDetailViewModel(m_cinemeta,
        m_indexers, m_tmdb, m_streamActions, m_libraryCtrl,
        m_watchedCtrl, m_tokenCtrl, m_settings,
        m_tokenCtrl->realDebridToken(),
        m_tokenCtrl->allDebridApiKey(), a);
    m_movieDetailVm->setDownloadController(m_downloadCtrl);
    m_seriesDetailVm = new ui::qml::SeriesDetailViewModel(m_cinemeta,
        m_indexers, m_tmdb, m_streamActions, m_libraryCtrl,
        m_watchedCtrl, m_tokenCtrl, m_settings,
        m_tokenCtrl->realDebridToken(),
        m_tokenCtrl->allDebridApiKey(), a);
    m_seriesDetailVm->setDownloadController(m_downloadCtrl);

    // TMDB token gain/loss propagates from `TokenController` to
    // the Browse VM. We refresh on token gain and clear on loss;
    // the Browse VM's own `tmdbConfigured` property toggles the
    // placeholder visibility from QML.
    QObject::connect(m_tokenCtrl,
        &controllers::TokenController::tmdbTokenChanged, m_browseVm,
        [this](const QString& token) {
            if (token.isEmpty()) {
                m_browseVm->results()->setItems({});
            } else {
                m_browseVm->refresh();
            }
        });

    // Subtitles VM. Wraps `SubtitleController`. Cross-shell
    // routing (settings request, close, attach-to-player) is wired
    // in `ShellViewModel`.
    m_subtitlesVm = new ui::qml::SubtitlesViewModel(m_subtitleCtrl,
        m_settings.subtitle(), a);

    // Settings root + token routing back to the live
    // TokenController. RD / TMDB / OS credential changes refresh
    // their respective in-memory aliases without a keyring
    // round-trip.
    m_settingsVm = new ui::qml::settings::SettingsRootViewModel(m_http.get(),
        m_tokens.get(), m_indexers, m_settings, m_subtitleCache.get(),
        m_mediaCache.get(), a);
    QObject::connect(m_settingsVm,
        &ui::qml::settings::SettingsRootViewModel::tmdbTokenChanged, m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshTmdb(); });
    QObject::connect(m_settingsVm,
        &ui::qml::settings::SettingsRootViewModel::realDebridTokenChanged,
        m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshRealDebrid(); });
    QObject::connect(m_settingsVm,
        &ui::qml::settings::SettingsRootViewModel::allDebridApiKeyChanged,
        m_tokenCtrl,
        [this](const QString&) { m_tokenCtrl->refreshAllDebrid(); });
    QObject::connect(m_settingsVm,
        &ui::qml::settings::SettingsRootViewModel::activeDebridProviderChanged,
        m_tokenCtrl, [this] {
            // Pure UX: refresh both tokens so the in-memory copies
            // re-read the keyring (cheap; tokens didn't change but
            // it keeps the contract symmetric with the radio).
            m_tokenCtrl->refreshRealDebrid();
            m_tokenCtrl->refreshAllDebrid();
        });
    QObject::connect(m_settingsVm,
        &ui::qml::settings::SettingsRootViewModel::subtitleCredentialsChanged,
        m_tokenCtrl, [this] {
            m_tokenCtrl->refreshOpenSubtitlesCredentials();
        });

#ifdef KINEMA_HAVE_LIBMPV
    m_playbackCtrl = new controllers::PlaybackController(
        *m_historyCtrl, m_settings, m_http.get(), a);
    m_seriesSessionCtrl = new controllers::SeriesPlaybackSessionController(
        *m_playbackCtrl, *m_torrentStreaming, *m_streamActions, a);
    m_mprisCtrl = new controllers::MprisController(
        *m_playbackCtrl, m_seriesSessionCtrl, a);
    // Subtitle ↔ playback coupling (moviehash → search) lives at
    // the service layer.
    if (m_subtitleCtrl) {
        QObject::connect(m_playbackCtrl,
            &controllers::PlaybackController::moviehashComputed,
            m_subtitleCtrl,
            &controllers::SubtitleController::setMoviehash);
        QObject::connect(m_playbackCtrl,
            &controllers::PlaybackController::streamCleared,
            m_subtitleCtrl,
            &controllers::SubtitleController::clearMoviehash);
    }
#endif

    if (!m_player->preferredPlayerAvailable()) {
        qCInfo(KINEMA_APP) << "preferred media player not found on $PATH";
    }
}

ServiceContainer::~ServiceContainer() = default;

ui::qml::AppIconResolver* ServiceContainer::appIconResolver()
{
    if (!m_appIconResolver) {
        m_appIconResolver = new ui::qml::AppIconResolver(m_anchor.get());
    }
    return m_appIconResolver;
}

} // namespace kinema::app
