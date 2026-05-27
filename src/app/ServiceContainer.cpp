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
#include "controllers/DebridCredentialsResolver.h"
#include "controllers/DownloadController.h"

#include "controllers/LibraryController.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "playback/desktop/MprisPlaybackProjection.h"

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

#include "domain/Debrid.h"
#include "kinema_log_app.h"
#include "playback/adapters/ActiveStreamIndexerAdapter.h"
#include "playback/adapters/ExternalPlayerAdapter.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "playback/adapters/EmbeddedMpvPlayerAdapter.h"
#endif
#include "playback/downloads/SqliteDownloadRepository.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/history/SqlitePlaybackHistoryRepository.h"
#include "playback/history/TrackMemoryService.h"
#include "playback/progress/PlaybackProgressProjector.h"
#include "playback/resume/ResumeUseCase.h"
#include "playback/series/SeriesSessionService.h"
#include "playback/session/PlaybackSessionManager.h"
#include "playback/sources/AllDebridMediaSource.h"
#include "playback/sources/AllDebridResolver.h"
#include "playback/sources/RealDebridMediaSource.h"
#include "playback/sources/RealDebridResolver.h"
#include "playback/sources/TorrentMediaSource.h"
#include "playback/streaming/LocalHttpStreamGateway.h"
#include "playback/subtitles/MoviehashProbe.h"
#include "playback/subtitles/SubtitleSessionService.h"
#include "playback/transfer/BackendRegistry.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSupervisor.h"
#include "playback/transfer/TransferUseCase.h"

#include "services/StreamActions.h"
#include "playback/torrent/LibtorrentClient.h"
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
    // Identical service graph to the legacy
    // `MainController::buildCoreServices`. Cross-cutting routing
    // that needs ShellViewModel slots is wired separately in
    // `ShellViewModel`'s constructor.
    //
    // The construction body is split into five phases. See the
    // header for the dependency chain.
    buildInfrastructure();
    buildRepositories();
    buildPlaybackSubsystem();
    buildControllersAndViewModels();
    wirePresentation();
}

// ---------------------------------------------------------------------------
// Phase 1 — HTTP / token / launcher backbone, API clients, indexers,
// RD/AD clients. Everything here is needed by at least one of the
// later phases; nothing here depends on the database, the playback
// subsystem, or the QML view-models.
// ---------------------------------------------------------------------------
void ServiceContainer::buildInfrastructure()
{
    QObject* a = m_anchor.get();

    m_http = std::make_unique<core::HttpClient>(a);
    m_tokens = std::make_unique<core::TokenStore>(a);
    m_player = std::make_unique<core::PlayerLauncher>(
        m_settings.player(), a);
    m_cinemeta = new api::CinemetaClient(m_http.get(), a);
    m_tmdb = new api::TmdbClient(m_http.get(), a);

    // TokenController is built early so the debrid credentials
    // resolver can hold a const-ref to it before the indexers
    // are registered below. Later phases (RD/AD client wiring,
    // OpenSubtitles, history controller) find it already
    // constructed.
    m_tokenCtrl = new controllers::TokenController(
        m_tokens.get(), m_tmdb, m_settings.debrid(), a);
    m_debridCreds
        = std::make_unique<controllers::DebridCredentialsResolver>(
            m_settings.debrid(), *m_tokenCtrl);

    // Indexer abstraction: a selector owns one or more concrete
    // indexers (Torrentio + Peerflix today) and view-models call
    // `selector->active()->streams()`. The active indexer tracks
    // `IndexerSettings`. Each indexer takes the credentials
    // resolver so it can append the active debrid credential
    // (RD/AD) to its outgoing URL when one is configured.
    m_indexers = new api::IndexerSelector(m_settings.indexers(), a);
    m_indexers->registerIndexer(std::make_unique<api::TorrentioIndexer>(
        m_http.get(), m_settings.torrentio(), m_settings.filter(),
        m_debridCreds.get()));
    m_indexers->registerIndexer(std::make_unique<api::PeerflixIndexer>(
        m_http.get(), m_settings.peerflix(), m_debridCreds.get()));
    m_imageLoader = new ui::ImageLoader(m_http.get(), a);

    // RD / AD clients pick up their tokens from the keyring once
    // the TokenController has fired its initial reads. Routing
    // between RD/AD and the libtorrent backend is decided by
    // `playback::transfer::BackendRegistry` at session-open time —
    // if a debrid provider is configured every stream goes through
    // it; otherwise libtorrent takes over.
    m_rd = std::make_unique<api::RealDebridClient>(m_http.get(), a);
    m_ad = std::make_unique<api::AllDebridClient>(m_http.get(), a);
}

// ---------------------------------------------------------------------------
// Phase 2 — Disk-backed state: KConfig stores, the cache
// hierarchy, the libtorrent client (dormant), and the thin SQLite
// adapters that satisfy the playback `ports` interfaces.
// Database open is best-effort: a broken DB means history is
// disabled this session, not that the app refuses to start.
// ---------------------------------------------------------------------------
void ServiceContainer::buildRepositories()
{
    QObject* a = m_anchor.get();

    m_torrentCache = std::make_unique<core::TorrentCache>(
        m_settings.torrentStreaming(), a);
    m_libtorrentClient = new playback::torrent::LibtorrentClient(
        m_settings.torrentStreaming(), *m_torrentCache, a);
    m_mediaCache = std::make_unique<core::MediaCache>(
        m_settings.download(), a);

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

    // Thin SQLite-backed adapters that satisfy the playback
    // `ports` interfaces. The supervisor/use-case need
    // `m_downloadRepo`; the projector / history query / resume
    // path needs `m_historyRepo`. Both are constructed up-front so
    // `buildPlaybackSubsystem()` can wire them without further
    // ordering games.
    m_historyRepo
        = std::make_unique<playback::history::SqlitePlaybackHistoryRepository>(
            *m_history);
    m_downloadRepo
        = std::make_unique<playback::downloads::SqliteDownloadRepository>(
            *m_downloadStore);
}

// ---------------------------------------------------------------------------
// Phase 3 — The full playback subsystem.
//
// Composition of: `SessionRegistry` (live sessions),
// `BackendRegistry` (`MediaSourcePort` strategies), `TransferSupervisor`
// (event-stream projection), `LocalHttpStreamGateway` (localhost
// HTTP server), `TransferUseCase` (application entry point), the
// player adapters (external + libmpv-gated embedded), the
// progress/history/resume chain, and the event-driven projections
// (moviehash probe, track memory, series adjacency, MPRIS).
//
// Ordering is delicate: the gateway resolver needs the use-case,
// so we build the use-case first and assign the resolver
// afterwards. The session manager is constructed last because it
// needs the adapters and the event stream.
// ---------------------------------------------------------------------------
void ServiceContainer::buildPlaybackSubsystem()
{
    QObject* a = m_anchor.get();

    m_streamActions = new services::StreamActions(m_player.get(), a);

    // ---- Unified downloader -------------------------------------

    m_rdResolver
        = std::make_unique<playback::sources::RealDebridResolver>(
            *m_rd, a);
    m_adResolver
        = std::make_unique<playback::sources::AllDebridResolver>(
            *m_ad, a);

    m_sessionRegistry
        = std::make_unique<playback::transfer::SessionRegistry>(a);
    m_localStreamGateway
        = new playback::streaming::LocalHttpStreamGateway(a);
    if (!m_localStreamGateway->listen()) {
        qCWarning(KINEMA_APP)
            << "ServiceContainer: could not bind localhost stream gateway";
    } else {
        qCInfo(KINEMA_APP)
            << "ServiceContainer: localhost stream gateway listening";
    }

    m_backendRegistry
        = std::make_unique<playback::transfer::BackendRegistry>();
    // Order matters: the selection policy walks backends in the
    // order registered when no override is given. Debrid backends
    // come first; the registry itself gates which one is "active"
    // via `setActiveDebridProvider`, so swapping providers is a
    // single setter call rather than re-registration.
    m_backendRegistry->registerSource(
        std::make_unique<playback::sources::RealDebridMediaSource>(
            *m_http, *m_rd, *m_rdResolver, *m_mediaCache,
            m_settings.download()));
    m_backendRegistry->registerSource(
        std::make_unique<playback::sources::AllDebridMediaSource>(
            *m_http, *m_ad, *m_adResolver, *m_mediaCache,
            m_settings.download()));
    m_backendRegistry->registerSource(
        std::make_unique<playback::sources::TorrentMediaSource>(
            *m_libtorrentClient, *m_mediaCache));

    m_playbackEventStream
        = new playback::events::PlaybackEventStream(a);
    m_transferSupervisor
        = new playback::transfer::TransferSupervisor(
            *m_sessionRegistry, *m_downloadRepo,
            *m_playbackEventStream, a);
    m_transferUseCase = new playback::transfer::TransferUseCase(
        *m_backendRegistry, *m_sessionRegistry,
        *m_transferSupervisor, *m_localStreamGateway,
        *m_downloadRepo, *m_mediaCache, *m_libtorrentClient, a);

    // Gateway's cold-recovery hook: when the player asks for an
    // asset that has no live session (e.g. after restart with a
    // pinned row left over), this re-opens it via the use-case.
    m_localStreamGateway->setSessionResolver(
        [this](const QString& assetId)
            -> QCoro::Task<playback::ports::ByteRangeSource*> {
            co_return co_await m_transferUseCase
                ->ensureSessionForAssetId(assetId);
        });

    m_streamActions->setTransferUseCase(m_transferUseCase);
    m_downloadCtrl = new controllers::DownloadController(
        *m_transferUseCase, *m_downloadStore, a);

    // Re-attach Full background downloads from the previous run.
    // OnDemand sessions are intentionally skipped: by definition
    // they only do work while a player is consuming them, and
    // there is no consumer at startup.
    m_transferUseCase->resumePersisted();

    // ---- Player adapters + history-side wiring -------------------

    m_externalPlayerAdapter
        = new playback::adapters::ExternalPlayerAdapter(
            *m_player, *m_playbackEventStream, a);
#ifdef KINEMA_HAVE_LIBMPV
    m_embeddedPlayerAdapter
        = new playback::adapters::EmbeddedMpvPlayerAdapter(
            *m_playbackEventStream, m_settings.player(), a);
#endif
    m_streamIndexerAdapter
        = std::make_unique<playback::adapters::ActiveStreamIndexerAdapter>(
            m_indexers);
    m_playbackProgressProjector
        = new playback::progress::PlaybackProgressProjector(
            *m_historyRepo, *m_playbackEventStream, a);
    m_historyQueryService = new playback::history::HistoryQueryService(
        *m_historyRepo, *m_history, a);
    m_resumeUseCase
        = new playback::resume::ResumeUseCase(
            *m_historyQueryService,
            *m_playbackProgressProjector,
            *m_streamIndexerAdapter,
            *m_historyRepo,
            *m_streamActions,
            a);
    // StreamActions seeds ctx.resumeSeconds via ResumeUseCase, which
    // checks the projector's live position first and then falls
    // back to the on-disk history row via the ResumePolicy.
    m_streamActions->setResumeUseCase(m_resumeUseCase);

    // ---- Event-driven projections + session manager --------------

#ifdef KINEMA_HAVE_LIBMPV
    // Event-driven moviehash probe. Subscribes to
    // PlayableUrlReady (published by EmbeddedMpvPlayerAdapter on
    // play()), runs the HEAD+Range probe, and republishes
    // MoviehashComputed. Replaces the inline coroutine that
    // previously lived on PlaybackController.
    m_moviehashProbe = new playback::subtitles::MoviehashProbe(
        *m_playbackEventStream, m_http.get(), a);
    // Track memory: applies remembered audio / subtitle language
    // preferences to fresh sessions. Subscribes to
    // PlaybackRequested + TrackListChanged; routes selection
    // commands through PlayerPort (the embedded adapter).
    m_trackMemoryService = new playback::history::TrackMemoryService(
        *m_playbackEventStream, *m_historyQueryService,
        m_embeddedPlayerAdapter, a);
    m_playbackSessionManager = new playback::session::PlaybackSessionManager(
        *m_streamActions, *m_playbackEventStream,
        m_embeddedPlayerAdapter, m_externalPlayerAdapter, a);
    // Event-driven season-pack adjacency. Subscribes to the
    // playback event stream, reads files via the session catalog,
    // and dispatches next/previous through PlaybackSessionManager.
    // Backend-agnostic: torrent and debrid catalogs behave the
    // same. Only the embedded-player build surfaces auto-next UI,
    // so the service is constructed inside the libmpv gate.
    m_seriesSessionService = new playback::series::SeriesSessionService(
        *m_playbackEventStream, *m_sessionRegistry,
        *m_playbackSessionManager, a);
    // Desktop MPRIS surface. Subscribes to PlaybackEventStream,
    // sends transport commands through PlaybackSessionManager,
    // queries EmbeddedMpvPlayerAdapter for live snapshots
    // (Position / Volume / Rate) and SeriesSessionService for
    // CanGoNext / CanGoPrevious.
    m_mprisProjection = new playback::desktop::MprisPlaybackProjection(
        *m_playbackEventStream, *m_playbackSessionManager,
        m_embeddedPlayerAdapter, m_seriesSessionService, a);
#else
    m_playbackSessionManager = new playback::session::PlaybackSessionManager(
        *m_streamActions, *m_playbackEventStream,
        nullptr, m_externalPlayerAdapter, a);
#endif
}

// ---------------------------------------------------------------------------
// Phase 4 — OpenSubtitles client + subtitle controller + library /
// watched controllers, then every QML page view-model. The
// SubtitleSessionService also lives here because it depends on the
// subtitle controller (constructed at the top of this phase) and
// the playback event stream (constructed in phase 3).
// ---------------------------------------------------------------------------
void ServiceContainer::buildControllersAndViewModels()
{
    QObject* a = m_anchor.get();

    // OpenSubtitles + subtitle controller. Token / credential
    // routing back to the live `TokenController` is set up in
    // `wirePresentation()`.
    m_openSubtitles = new api::OpenSubtitlesClient(m_http.get(),
        m_tokenCtrl->openSubtitlesApiKey(),
        m_tokenCtrl->openSubtitlesUsername(),
        m_tokenCtrl->openSubtitlesPassword(), a);
    m_subtitleCtrl = new controllers::SubtitleController(
        m_openSubtitles, m_subtitleCache.get(),
        m_settings.subtitle(), m_settings.cache(), a);
    QTimer::singleShot(0, m_subtitleCtrl,
        [this] { m_subtitleCtrl->reconcileCacheOnStartup(); });

    if (m_subtitleCtrl) {
        // The session service subscribes to PlaybackRequested
        // (clearMoviehash) and MoviehashComputed (setMoviehash),
        // replacing the direct PlaybackController <-> subtitle
        // signal wiring that lived here before.
        m_subtitleSessionService
            = new playback::subtitles::SubtitleSessionService(
                *m_subtitleCtrl, m_playbackEventStream, a);
    }

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
        *m_watched, m_historyQueryService, a);

    // Downloads page VM. Lives over the entire app lifetime so the
    // drawer's downloads entry can show counts even before the
    // first navigation to the page.
    m_downloadsVm = new ui::qml::DownloadsViewModel(*m_downloadCtrl,
        m_streamActions, a);

    // Discover / Search / Browse surface VMs. They sit on top of
    // the existing service graph; action signals route back into
    // `ShellViewModel` either for direct controller forwarding
    // (resume / remove) or for navigation events the QML shell
    // listens for.
    m_discoverVm = new ui::qml::DiscoverViewModel(m_tmdb, m_tokenCtrl, a);
    m_discoverVm->setLibraryController(m_libraryCtrl);
    m_discoverVm->setWatchedController(m_watchedCtrl);
    m_continueWatchingVm
        = new ui::qml::ContinueWatchingViewModel(
            m_historyQueryService, a);
    m_libraryVm = new ui::qml::LibraryViewModel(m_libraryCtrl, m_watchedCtrl, a);
    m_searchVm = new ui::qml::SearchViewModel(m_cinemeta,
        m_settings.search(), a);
    m_searchVm->setLibraryController(m_libraryCtrl);
    m_searchVm->setWatchedController(m_watchedCtrl);
    m_browseVm = new ui::qml::BrowseViewModel(m_tmdb, m_settings.browse(), a);
    m_browseVm->setLibraryController(m_libraryCtrl);
    m_browseVm->setWatchedController(m_watchedCtrl);
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

    // Subtitles VM. Wraps `SubtitleController`. Cross-shell
    // routing (settings request, close, attach-to-player) is wired
    // in `ShellViewModel`.
    m_subtitlesVm = new ui::qml::SubtitlesViewModel(m_subtitleCtrl,
        m_settings.subtitle(), a);

    // Settings root. Token routing back to the live
    // `TokenController` is set up in `wirePresentation()`.
    m_settingsVm = new ui::qml::settings::SettingsRootViewModel(m_http.get(),
        m_tokens.get(), m_indexers, m_settings, m_subtitleCache.get(),
        m_mediaCache.get(), a);
}

// ---------------------------------------------------------------------------
// Phase 5 — Cross-cutting routing that requires both the
// controllers (phase 4) and the playback subsystem (phase 3) to be
// constructed. Pure connect() calls; no new objects.
// ---------------------------------------------------------------------------
void ServiceContainer::wirePresentation()
{
    QObject* a = m_anchor.get();

    // ---- TokenController ↔ RD / AD clients ----------------------
    // Keep the RD / AD clients' in-memory tokens in sync with the
    // keyring-backed `TokenController`. Both backends check
    // `client.token()` / `client.apiKey()` for `canHandle`; the
    // active-provider gate lives in `BackendRegistry`.
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

    // Forward the active-debrid-provider radio to the backend
    // registry. Initial value + future changes both go through the
    // same path.
    m_backendRegistry->setActiveDebridProvider(
        m_settings.debrid().activeProvider());
    QObject::connect(&m_settings.debrid(),
        &config::DebridSettings::activeProviderChanged, a,
        [this](domain::DebridProvider p) {
            m_backendRegistry->setActiveDebridProvider(p);
        });

    // ---- TokenController ↔ OpenSubtitles -----------------------
    // Credentials change → drop JWT so the next request re-logs in,
    // and tell the controller to re-evaluate downloadEnabled.
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

    // ---- TMDB token ↔ Browse VM --------------------------------
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

    // ---- Settings VM ↔ TokenController -------------------------
    // RD / TMDB / OS credential changes refresh their respective
    // in-memory aliases without a keyring round-trip.
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

    // ---- Final boot checks --------------------------------------
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
