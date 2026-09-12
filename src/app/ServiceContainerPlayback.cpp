// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/alldebrid/AllDebridClient.h"
#include "api/cinemeta/CinemetaClient.h"
#include "api/indexers/IndexerSelector.h"
#include "api/indexers/PeerflixIndexer.h"
#include "api/indexers/TorrentioIndexer.h"
#include "api/opensubtitles/OpenSubtitlesClient.h"
#include "api/realdebrid/RealDebridClient.h"
#include "api/tmdb/TmdbClient.h"
#include "app/ServiceContainer.h"
#include "config/AppSettings.h"
#include "config/DebridSettings.h"
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
#include "ui/player/EmbeddedMpvPlayerAdapter.h"
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
#include "playback/torrent/LibtorrentClient.h"
#include "playback/transfer/BackendRegistry.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSupervisor.h"
#include "playback/transfer/TransferUseCase.h"
#include "services/StreamActions.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/browse/BrowseViewModel.h"
#include "ui/qml-bridge/details/MovieDetailViewModel.h"
#include "ui/qml-bridge/details/SeriesDetailViewModel.h"
#include "ui/qml-bridge/discover/DiscoverViewModel.h"
#include "ui/qml-bridge/downloads/DownloadsViewModel.h"
#include "ui/qml-bridge/library/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/library/LibraryViewModel.h"
#include "ui/qml-bridge/search/SearchViewModel.h"
#include "ui/qml-bridge/settings/SettingsRootViewModel.h"
#include "ui/qml-bridge/shell/AppIconResolver.h"
#include "ui/qml-bridge/shell/ShellDependencies.h"
#include "ui/qml-bridge/subtitles/SubtitlesViewModel.h"

#include <QMetaObject>
#include <QObject>
#include <QString>
#include <QTimer>

namespace kinema::app {

void ServiceContainer::buildPlaybackSubsystem()
{
    QObject* a = m_anchor.get();

    m_streamActions = new services::StreamActions(a);

    // ---- Unified downloader -------------------------------------

    m_rdResolver = std::make_unique<playback::sources::RealDebridResolver>(*m_rd, a);
    m_adResolver = std::make_unique<playback::sources::AllDebridResolver>(*m_ad, a);

    m_sessionRegistry = std::make_unique<playback::transfer::SessionRegistry>(a);
    m_localStreamGateway = new playback::streaming::LocalHttpStreamGateway(a);
    if (!m_localStreamGateway->listen()) {
        qCWarning(KINEMA_APP) << "ServiceContainer: could not bind localhost stream gateway";
    } else {
        qCInfo(KINEMA_APP) << "ServiceContainer: localhost stream gateway listening";
    }

    m_backendRegistry = std::make_unique<playback::transfer::BackendRegistry>();
    // Order matters: the selection policy walks backends in the
    // order registered when no override is given. Debrid backends
    // come first; the registry itself gates which one is "active"
    // via `setActiveDebridProvider`, so swapping providers is a
    // single setter call rather than re-registration.
    m_backendRegistry->registerSource(std::make_unique<playback::sources::RealDebridMediaSource>(
        *m_http, *m_rdResolver, *m_mediaCache, m_settings.torrentStreaming()));
    m_backendRegistry->registerSource(std::make_unique<playback::sources::AllDebridMediaSource>(
        *m_http, *m_adResolver, *m_mediaCache, m_settings.torrentStreaming()));
    m_backendRegistry->registerSource(std::make_unique<playback::sources::TorrentMediaSource>(
        *m_libtorrentClient, *m_mediaCache));

    m_playbackEventStream = new playback::events::PlaybackEventStream(a);
    m_transferSupervisor = new playback::transfer::TransferSupervisor(
        *m_sessionRegistry, *m_downloadRepo, *m_playbackEventStream, a);
    m_transferUseCase = new playback::transfer::TransferUseCase(*m_backendRegistry,
                                                                *m_sessionRegistry,
                                                                *m_transferSupervisor,
                                                                *m_localStreamGateway,
                                                                *m_downloadRepo,
                                                                *m_mediaCache,
                                                                *m_libtorrentClient,
                                                                a);

    // Gateway's cold-recovery hook: when the player asks for an
    // asset that has no live session (e.g. after restart with a
    // pinned row left over), this re-opens it via the use-case.
    m_localStreamGateway->setSessionResolver(
        [this](const QString& assetId) -> QCoro::Task<playback::ports::ByteRangeSource*> {
            co_return co_await m_transferUseCase->ensureSessionForAssetId(assetId);
        });

    m_downloadCtrl = new controllers::DownloadController(*m_transferUseCase, *m_downloadStore, a);

    // Re-attach Full background downloads from the previous run.
    // OnDemand sessions are intentionally skipped: by definition
    // they only do work while a player is consuming them, and
    // there is no consumer at startup.
    m_transferUseCase->resumePersisted();

    // ---- Player adapters + history-side wiring -------------------

    m_externalPlayerAdapter =
        new playback::adapters::ExternalPlayerAdapter(*m_player, *m_playbackEventStream, a);
#ifdef KINEMA_HAVE_LIBMPV
    m_embeddedPlayerAdapter =
        new ui::player::EmbeddedMpvPlayerAdapter(*m_playbackEventStream, m_settings.player(), a);
#endif
    m_streamIndexerAdapter =
        std::make_unique<playback::adapters::ActiveStreamIndexerAdapter>(m_indexers);
    m_playbackProgressProjector = new playback::progress::PlaybackProgressProjector(
        *m_historyRepo, *m_playbackEventStream, a);
    m_historyQueryService =
        new playback::history::HistoryQueryService(*m_historyRepo, *m_history, a);

    // ---- Event-driven projections + session manager --------------

#ifdef KINEMA_HAVE_LIBMPV
    m_playbackSessionManager =
        new playback::session::PlaybackSessionManager(*m_playbackEventStream,
                                                      *m_transferUseCase,
                                                      m_embeddedPlayerAdapter,
                                                      m_externalPlayerAdapter,
                                                      a);
#else
    m_playbackSessionManager = new playback::session::PlaybackSessionManager(
        *m_playbackEventStream, *m_transferUseCase, nullptr, m_externalPlayerAdapter, a);
#endif

    m_resumeUseCase = new playback::resume::ResumeUseCase(*m_historyQueryService,
                                                          *m_playbackProgressProjector,
                                                          *m_streamIndexerAdapter,
                                                          *m_historyRepo,
                                                          *m_playbackSessionManager,
                                                          a);
    m_playbackSessionManager->setResumeUseCase(m_resumeUseCase);

#ifdef KINEMA_HAVE_LIBMPV
    // Event-driven moviehash probe. Subscribes to
    // PlayableUrlReady (published by EmbeddedMpvPlayerAdapter on
    // play()), runs the HEAD+Range probe, and republishes
    // MoviehashComputed. Replaces the inline coroutine that
    // previously lived on PlaybackController.
    m_moviehashProbe =
        new playback::subtitles::MoviehashProbe(*m_playbackEventStream, m_http.get(), a);
    // Track memory: applies remembered audio / subtitle language
    // preferences to fresh sessions. Subscribes to
    // PlaybackRequested + TrackListChanged; routes selection
    // commands through PlayerPort (the embedded adapter).
    m_trackMemoryService = new playback::history::TrackMemoryService(
        *m_playbackEventStream, *m_historyQueryService, m_embeddedPlayerAdapter, a);
    // Event-driven season-pack adjacency. Subscribes to the
    // playback event stream, reads files via the session catalog,
    // and dispatches next/previous through PlaybackSessionManager.
    // Backend-agnostic: torrent and debrid catalogs behave the
    // same. Only the embedded-player build surfaces auto-next UI,
    // so the service is constructed inside the libmpv gate.
    m_seriesSessionService = new playback::series::SeriesSessionService(
        *m_playbackEventStream, *m_sessionRegistry, *m_playbackSessionManager, a);
    m_playbackSessionManager->setSeriesSessionService(m_seriesSessionService);
    // Desktop MPRIS surface. Subscribes to PlaybackEventStream,
    // sends transport commands through PlaybackSessionManager,
    // queries EmbeddedMpvPlayerAdapter for live snapshots
    // (Position / Volume / Rate) and SeriesSessionService for
    // CanGoNext / CanGoPrevious.
    m_mprisProjection = new playback::desktop::MprisPlaybackProjection(*m_playbackEventStream,
                                                                       *m_playbackSessionManager,
                                                                       m_embeddedPlayerAdapter,
                                                                       m_seriesSessionService,
                                                                       a);
#else
#endif
}

} // namespace kinema::app
