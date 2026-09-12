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

void ServiceContainer::buildInfrastructure()
{
    QObject* a = m_anchor.get();

    m_http = std::make_unique<core::HttpClient>(a);
    m_tokens = std::make_unique<core::TokenStore>(a);
    m_player = std::make_unique<core::PlayerLauncher>(m_settings.player(), a);
    m_cinemeta = new api::CinemetaClient(m_http.get(), a);
    m_tmdb = new api::TmdbClient(m_http.get(), a);

    // TokenController is built early so the debrid credentials
    // resolver can hold a const-ref to it before the indexers
    // are registered below. Later phases (RD/AD client wiring,
    // OpenSubtitles, history controller) find it already
    // constructed.
    m_tokenCtrl = new controllers::TokenController(m_tokens.get(), m_tmdb, m_settings.debrid(), a);
    m_debridCreds =
        std::make_unique<controllers::DebridCredentialsResolver>(m_settings.debrid(), *m_tokenCtrl);

    // Indexer abstraction: a selector owns one or more concrete
    // indexers (Torrentio + Peerflix today) and view-models call
    // `selector->active()->streams()`. The active indexer tracks
    // `IndexerSettings`. Each indexer takes the credentials
    // resolver so it can append the active debrid credential
    // (RD/AD) to its outgoing URL when one is configured.
    m_indexers = new api::IndexerSelector(m_settings.indexers(), a);
    m_indexers->registerIndexer(std::make_unique<api::TorrentioIndexer>(
        m_http.get(), m_settings.torrentio(), m_settings.filter(), m_debridCreds.get()));
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

void ServiceContainer::buildRepositories()
{
    QObject* a = m_anchor.get();

    m_torrentCache = std::make_unique<core::TorrentCache>(m_settings.torrentStreaming(), a);
    m_libtorrentClient =
        new playback::torrent::LibtorrentClient(m_settings.torrentStreaming(), *m_torrentCache, a);
    m_mediaCache = std::make_unique<core::MediaCache>(m_settings.torrentStreaming(), a);

    m_db = std::make_unique<core::Database>(a);
    if (!m_db->open()) {
        qCWarning(KINEMA_APP) << "ServiceContainer: history database unavailable; "
                                 "history / resume features are disabled this session";
    }
    m_history = std::make_unique<core::HistoryStore>(*m_db, a);
    m_history->runRetentionPass();
    m_library = std::make_unique<core::LibraryStore>(*m_db, a);
    m_watched = std::make_unique<core::WatchedStore>(*m_db, a);
    m_subtitleCache = std::make_unique<core::SubtitleCacheStore>(*m_db, a);
    m_downloadStore = std::make_unique<core::DownloadStore>(*m_db, a);

    // Thin SQLite-backed adapters that satisfy the playback
    // `ports` interfaces. The supervisor/use-case need
    // `m_downloadRepo`; the projector / history query / resume
    // path needs `m_historyRepo`. Both are constructed up-front so
    // `buildPlaybackSubsystem()` can wire them without further
    // ordering games.
    m_historyRepo =
        std::make_unique<playback::history::SqlitePlaybackHistoryRepository>(*m_history);
    m_downloadRepo =
        std::make_unique<playback::downloads::SqliteDownloadRepository>(*m_downloadStore);
}

} // namespace kinema::app
