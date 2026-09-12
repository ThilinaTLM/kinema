// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "app/ServiceContainer.h"

#include "api/alldebrid/AllDebridClient.h"
#include "api/cinemeta/CinemetaClient.h"
#include "api/indexers/IndexerSelector.h"
#include "api/indexers/PeerflixIndexer.h"
#include "api/indexers/TorrentioIndexer.h"
#include "api/opensubtitles/OpenSubtitlesClient.h"
#include "api/realdebrid/RealDebridClient.h"
#include "api/tmdb/TmdbClient.h"
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

ServiceContainer::ServiceContainer(config::AppSettings& settings)
    : m_settings(settings), m_anchor(std::make_unique<QObject>())
{
    // Identical service graph to the legacy
    // `ShellViewModel::buildCoreServices`. Cross-cutting routing
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

ServiceContainer::~ServiceContainer() = default;

ui::qml::ShellDependencies ServiceContainer::shellDependencies()
{
    return {
        m_settings,
        m_player.get(),
        m_streamActions,
        m_downloadCtrl,
        m_libraryCtrl,
        m_subtitleCtrl,
        m_tokenCtrl,
        m_tray,
        m_playbackEventStream,
        m_resumeUseCase,
        m_playbackSessionManager,
        m_libtorrentClient,
        m_browseVm,
        m_continueWatchingVm,
        m_discoverVm,
        m_downloadsVm,
        m_libraryVm,
        m_movieDetailVm,
        m_searchVm,
        m_seriesDetailVm,
        m_subtitlesVm,
#ifdef KINEMA_HAVE_LIBMPV
        m_mprisProjection,
        m_seriesSessionService,
        m_embeddedPlayerAdapter,
#endif
    };
}

ui::qml::AppIconResolver* ServiceContainer::appIconResolver()
{
    if (!m_appIconResolver) {
        m_appIconResolver = new ui::qml::AppIconResolver(m_anchor.get());
    }
    return m_appIconResolver;
}

} // namespace kinema::app
