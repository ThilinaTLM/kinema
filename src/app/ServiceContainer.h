// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>

#include <memory>

namespace kinema::api {
class AllDebridClient;
class CinemetaClient;
class IndexerSelector;
class OpenSubtitlesClient;
class RealDebridClient;
class TmdbClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::core {
class Database;
class DownloadStore;
class HistoryStore;
class HttpClient;
class LibraryStore;
class MediaCache;
class PlayerLauncher;
class SubtitleCacheStore;
class TokenStore;
class TorrentCache;
class WatchedStore;
}

namespace kinema::playback::desktop {
class MprisPlaybackProjection;
}

namespace kinema::controllers {
class DebridCredentialsResolver;
class DownloadController;
class LibraryController;
class SubtitleController;
class TokenController;
class TrayController;
class WatchedController;
}

namespace kinema::playback::sources {
class AllDebridResolver;
class RealDebridResolver;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::playback::adapters {
class ActiveStreamIndexerAdapter;
}

namespace kinema::playback::downloads {
class SqliteDownloadRepository;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::streaming {
class LocalHttpStreamGateway;
}

namespace kinema::playback::history {
class HistoryQueryService;
class SqlitePlaybackHistoryRepository;
}

namespace kinema::playback::adapters {
class EmbeddedMpvPlayerAdapter;
class ExternalPlayerAdapter;
}

namespace kinema::playback::progress {
class PlaybackProgressProjector;
}

namespace kinema::playback::resume {
class ResumeUseCase;
}

namespace kinema::playback::series {
class SeriesSessionService;
}

namespace kinema::playback::session {
class PlaybackSessionManager;
}

namespace kinema::playback::history {
class TrackMemoryService;
}

namespace kinema::playback::subtitles {
class SubtitleSessionService;
class MoviehashProbe;
}

namespace kinema::playback::torrent {
class LibtorrentClient;
}

namespace kinema::playback::transfer {
class BackendRegistry;
class SessionRegistry;
class TransferSupervisor;
class TransferUseCase;
}


namespace kinema::ui {
class ImageLoader;
}

namespace kinema::ui::qml {
class AppIconResolver;
class BrowseViewModel;
class ContinueWatchingViewModel;
class DiscoverViewModel;
class DownloadsViewModel;
class LibraryViewModel;
class MovieDetailViewModel;
class SearchViewModel;
class SeriesDetailViewModel;
class SubtitlesViewModel;
}

namespace kinema::ui::qml::settings {
class SettingsRootViewModel;
}

namespace kinema::app {

/**
 * Composition root for the application. Plain C++ — not a QObject —
 * but parents every owned `QObject*` to an internal anchor so they
 * are destroyed in the right order alongside the `unique_ptr`s.
 *
 * Construction wires only the relationships among *services* (token
 * controller → debrid clients, history controller → stream actions,
 * etc.). Cross-cutting routing that maps view-model signals to
 * shell-level navigation slots is set up later by
 * `ui::qml::ShellViewModel`, which receives the container by
 * reference.
 *
 * Lifetime contract: construct one `ServiceContainer` per
 * application instance; destroy it after the QML engine is torn
 * down. The anchor `QObject` is destroyed last among the QObject
 * members, which deletes all parented controllers and view-models
 * in reverse-construction order.
 */
class ServiceContainer
{
public:
    explicit ServiceContainer(config::AppSettings& settings);
    ~ServiceContainer();

    ServiceContainer(const ServiceContainer&) = delete;
    ServiceContainer& operator=(const ServiceContainer&) = delete;

    config::AppSettings& settings() { return m_settings; }

    // ---- Owned services (`unique_ptr`) ---------------------------------
    core::HttpClient* http() const { return m_http.get(); }
    core::TokenStore* tokens() const { return m_tokens.get(); }
    core::PlayerLauncher* player() const { return m_player.get(); }
    core::Database* database() const { return m_db.get(); }
    core::HistoryStore* historyStore() const { return m_history.get(); }
    core::LibraryStore* libraryStore() const { return m_library.get(); }
    core::WatchedStore* watchedStore() const { return m_watched.get(); }
    core::SubtitleCacheStore* subtitleCache() const { return m_subtitleCache.get(); }
    core::TorrentCache* torrentCache() const { return m_torrentCache.get(); }
    core::DownloadStore* downloadStore() const { return m_downloadStore.get(); }
    core::MediaCache* mediaCache() const { return m_mediaCache.get(); }
    api::RealDebridClient* realDebrid() const { return m_rd.get(); }
    api::AllDebridClient* allDebrid() const { return m_ad.get(); }

    // ---- Owned QObjects parented to the anchor -------------------------
    api::CinemetaClient* cinemeta() const { return m_cinemeta; }
    api::IndexerSelector* indexers() const { return m_indexers; }
    api::TmdbClient* tmdb() const { return m_tmdb; }
    api::OpenSubtitlesClient* openSubtitles() const { return m_openSubtitles; }
    ui::ImageLoader* imageLoader() const { return m_imageLoader; }
    ui::qml::AppIconResolver* appIconResolver();

    services::StreamActions* streamActions() const { return m_streamActions; }
    playback::session::PlaybackSessionManager* playbackSessionManager() const
    { return m_playbackSessionManager; }
    playback::transfer::TransferUseCase* transferUseCase() const
    { return m_transferUseCase; }
    playback::progress::PlaybackProgressProjector* playbackProgressProjector() const
    { return m_playbackProgressProjector; }
    playback::resume::ResumeUseCase* resumeUseCase() const
    { return m_resumeUseCase; }
    playback::history::HistoryQueryService* historyQueryService() const
    { return m_historyQueryService; }
    playback::events::PlaybackEventStream* playbackEventStream() const
    { return m_playbackEventStream; }
    playback::adapters::ExternalPlayerAdapter* externalPlayerAdapter() const
    { return m_externalPlayerAdapter; }
#ifdef KINEMA_HAVE_LIBMPV
    playback::adapters::EmbeddedMpvPlayerAdapter* embeddedPlayerAdapter() const
    { return m_embeddedPlayerAdapter; }
    playback::series::SeriesSessionService* seriesSessionService() const
    { return m_seriesSessionService; }
#endif
    playback::subtitles::SubtitleSessionService* subtitleSessionService() const
    { return m_subtitleSessionService; }
    playback::torrent::LibtorrentClient* libtorrentClient() const { return m_libtorrentClient; }
    playback::streaming::LocalHttpStreamGateway* localStreamGateway() const
    { return m_localStreamGateway; }
    playback::transfer::SessionRegistry* sessionRegistry() const
    { return m_sessionRegistry.get(); }
    playback::transfer::BackendRegistry* backendRegistry() const
    { return m_backendRegistry.get(); }
    playback::transfer::TransferSupervisor* transferSupervisor() const
    { return m_transferSupervisor; }

    controllers::DownloadController* downloadController() const { return m_downloadCtrl; }
    controllers::TokenController* tokenController() const { return m_tokenCtrl; }

    controllers::LibraryController* libraryController() const { return m_libraryCtrl; }
    controllers::WatchedController* watchedController() const { return m_watchedCtrl; }
    controllers::SubtitleController* subtitleController() const { return m_subtitleCtrl; }

#ifdef KINEMA_HAVE_LIBMPV
    /// Desktop MPRIS projection — replaces the legacy
    /// `controllers::MprisController`. Owns the
    /// `org.mpris.MediaPlayer2.kinema` D-Bus registration and the
    /// idle inhibitor; drives state off `PlaybackEventStream`.
    playback::desktop::MprisPlaybackProjection* mprisProjection() const
    { return m_mprisProjection; }
#endif

    // ---- Page view-models ----------------------------------------------
    ui::qml::DiscoverViewModel* discoverVm() const { return m_discoverVm; }
    ui::qml::ContinueWatchingViewModel* continueWatchingVm() const { return m_continueWatchingVm; }
    ui::qml::LibraryViewModel* libraryVm() const { return m_libraryVm; }
    ui::qml::SearchViewModel* searchVm() const { return m_searchVm; }
    ui::qml::BrowseViewModel* browseVm() const { return m_browseVm; }
    ui::qml::MovieDetailViewModel* movieDetailVm() const { return m_movieDetailVm; }
    ui::qml::SeriesDetailViewModel* seriesDetailVm() const { return m_seriesDetailVm; }
    ui::qml::SubtitlesViewModel* subtitlesVm() const { return m_subtitlesVm; }
    ui::qml::settings::SettingsRootViewModel* settingsVm() const { return m_settingsVm; }
    ui::qml::DownloadsViewModel* downloadsVm() const { return m_downloadsVm; }

    /// Created lazily by `ShellViewModel::attachWindow`. The
    /// container owns the controller's lifetime so the menu can be
    /// torn down with the rest of the services.
    controllers::TrayController* tray() const { return m_tray; }
    void setTray(controllers::TrayController* t) { m_tray = t; }

private:
    // ---- Construction helpers ------------------------------------------
    //
    // Splits the ~470-line ctor into per-responsibility phases.
    // Each helper assumes its predecessors have already run; the
    // dependency chain is:
    //
    //   buildInfrastructure()
    //     ↓ (http, tokens, player, indexers, RD/AD clients,
    //        TokenController, TmdbClient, CinemetaClient)
    //   buildRepositories()
    //     ↓ (database open, KConfig stores, caches,
    //        TorrentCache, MediaCache, LibtorrentClient,
    //        SqlitePlaybackHistoryRepository,
    //        SqliteDownloadRepository)
    //   buildPlaybackSubsystem()
    //     ↓ (StreamActions, downloader pipeline, adapters,
    //        ProgressProjector, HistoryQueryService, ResumeUseCase,
    //        DownloadController, projections, PlaybackSessionManager,
    //        SeriesSessionService, MprisPlaybackProjection)
    //   buildControllersAndViewModels()
    //     ↓ (OpenSubtitles, SubtitleController +
    //        SubtitleSessionService, LibraryController,
    //        WatchedController, every page view-model,
    //        SettingsRootViewModel)
    //   wirePresentation()
    //        (TokenController ↔ RD/AD/OpenSubtitles, settings VM ↔
    //         TokenController routing, active-debrid-provider sync,
    //         preferred-player check)
    //
    // `EmbeddedMpvPlayerAdapter` still binds its `PlayerWindow`
    // lazily through `setPlayerWindow` from
    // `ShellViewModel::ensurePlayerWindow`; the window is created
    // on first embedded play, which cannot happen during the
    // ServiceContainer constructor.
    void buildInfrastructure();
    void buildRepositories();
    void buildPlaybackSubsystem();
    void buildControllersAndViewModels();
    void wirePresentation();

    config::AppSettings& m_settings;
    /// Debrid credential resolver — read-only port consumed by the
    /// Torrentio + Peerflix indexers (raw pointer). Declared before
    /// `m_anchor` so it is destroyed *after* the QObject anchor
    /// (reverse declaration order): any indexer destructor running
    /// during anchor teardown sees a live resolver, never a
    /// dangling pointer.
    std::unique_ptr<controllers::DebridCredentialsResolver> m_debridCreds;
    /// Parent for every QObject we own raw. Declared after
    /// `m_debridCreds` so it is destroyed first — the anchor
    /// tearing down its parented children (`m_indexers` and the
    /// indexers themselves) happens while the resolver they reference
    /// is still alive.
    std::unique_ptr<QObject> m_anchor;

    // unique_ptr-owned services (raw `new` would be valid too;
    // unique_ptr documents reverse-destruction order).
    std::unique_ptr<core::HttpClient> m_http;
    std::unique_ptr<core::TokenStore> m_tokens;
    std::unique_ptr<core::PlayerLauncher> m_player;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_history;
    std::unique_ptr<core::LibraryStore> m_library;
    std::unique_ptr<core::WatchedStore> m_watched;
    std::unique_ptr<core::SubtitleCacheStore> m_subtitleCache;
    std::unique_ptr<core::TorrentCache> m_torrentCache;
    std::unique_ptr<core::DownloadStore> m_downloadStore;
    std::unique_ptr<core::MediaCache> m_mediaCache;
    std::unique_ptr<api::RealDebridClient> m_rd;
    std::unique_ptr<api::AllDebridClient> m_ad;

    // QObject-parented to `m_anchor`.
    api::CinemetaClient* m_cinemeta {};
    api::IndexerSelector* m_indexers {};
    api::TmdbClient* m_tmdb {};
    api::OpenSubtitlesClient* m_openSubtitles {};
    ui::ImageLoader* m_imageLoader {};

    ui::qml::AppIconResolver* m_appIconResolver {};
    services::StreamActions* m_streamActions {};
    playback::events::PlaybackEventStream* m_playbackEventStream {};
    std::unique_ptr<playback::history::SqlitePlaybackHistoryRepository>
        m_historyRepo;
    std::unique_ptr<playback::downloads::SqliteDownloadRepository>
        m_downloadRepo;
    std::unique_ptr<playback::adapters::ActiveStreamIndexerAdapter>
        m_streamIndexerAdapter;
    std::unique_ptr<playback::sources::RealDebridResolver> m_rdResolver;
    std::unique_ptr<playback::sources::AllDebridResolver> m_adResolver;
    std::unique_ptr<playback::transfer::SessionRegistry> m_sessionRegistry;
    std::unique_ptr<playback::transfer::BackendRegistry> m_backendRegistry;
    playback::streaming::LocalHttpStreamGateway* m_localStreamGateway {};
    playback::transfer::TransferSupervisor* m_transferSupervisor {};
    playback::transfer::TransferUseCase* m_transferUseCase {};
    playback::resume::ResumeUseCase* m_resumeUseCase {};
    playback::progress::PlaybackProgressProjector* m_playbackProgressProjector {};
    playback::history::HistoryQueryService* m_historyQueryService {};
    playback::session::PlaybackSessionManager* m_playbackSessionManager {};
    playback::adapters::ExternalPlayerAdapter* m_externalPlayerAdapter {};
    playback::subtitles::SubtitleSessionService* m_subtitleSessionService {};
#ifdef KINEMA_HAVE_LIBMPV
    playback::adapters::EmbeddedMpvPlayerAdapter* m_embeddedPlayerAdapter {};
    playback::series::SeriesSessionService* m_seriesSessionService {};
    playback::subtitles::MoviehashProbe* m_moviehashProbe {};
    playback::history::TrackMemoryService* m_trackMemoryService {};
#endif
    playback::torrent::LibtorrentClient* m_libtorrentClient {};
    controllers::DownloadController* m_downloadCtrl {};
    controllers::TokenController* m_tokenCtrl {};

    controllers::LibraryController* m_libraryCtrl {};
    controllers::WatchedController* m_watchedCtrl {};
    controllers::SubtitleController* m_subtitleCtrl {};
    controllers::TrayController* m_tray {};

    ui::qml::DiscoverViewModel* m_discoverVm {};
    ui::qml::ContinueWatchingViewModel* m_continueWatchingVm {};
    ui::qml::LibraryViewModel* m_libraryVm {};
    ui::qml::SearchViewModel* m_searchVm {};
    ui::qml::BrowseViewModel* m_browseVm {};
    ui::qml::MovieDetailViewModel* m_movieDetailVm {};
    ui::qml::SeriesDetailViewModel* m_seriesDetailVm {};
    ui::qml::SubtitlesViewModel* m_subtitlesVm {};
    ui::qml::settings::SettingsRootViewModel* m_settingsVm {};
    ui::qml::DownloadsViewModel* m_downloadsVm {};

#ifdef KINEMA_HAVE_LIBMPV
    playback::desktop::MprisPlaybackProjection* m_mprisProjection {};
#endif
};

} // namespace kinema::app
