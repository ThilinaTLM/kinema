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

namespace kinema::controllers {
class DownloadController;
class HistoryController;
class LibraryController;
class MprisController;
class PlaybackController;
class SeriesPlaybackSessionController;
class SubtitleController;
class TokenController;
class TrayController;
class WatchedController;
}

namespace kinema::download {
class DownloadManager;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::torrent {
class TorrentStreamingService;
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
    torrent::TorrentStreamingService* torrentStreaming() const { return m_torrentStreaming; }
    download::DownloadManager* downloadManager() const { return m_downloadManager; }

    controllers::DownloadController* downloadController() const { return m_downloadCtrl; }
    controllers::TokenController* tokenController() const { return m_tokenCtrl; }
    controllers::HistoryController* historyController() const { return m_historyCtrl; }
    controllers::LibraryController* libraryController() const { return m_libraryCtrl; }
    controllers::WatchedController* watchedController() const { return m_watchedCtrl; }
    controllers::SubtitleController* subtitleController() const { return m_subtitleCtrl; }

#ifdef KINEMA_HAVE_LIBMPV
    controllers::MprisController* mprisController() const { return m_mprisCtrl; }
    controllers::PlaybackController* playbackController() const { return m_playbackCtrl; }
    controllers::SeriesPlaybackSessionController* seriesSessionController() const
    {
        return m_seriesSessionCtrl;
    }
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
    config::AppSettings& m_settings;
    /// Parent for every QObject we own raw. Lives at the end of
    /// the member list so it is destroyed last, after every
    /// `unique_ptr` above has dropped its referent.
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
    torrent::TorrentStreamingService* m_torrentStreaming {};
    download::DownloadManager* m_downloadManager {};
    controllers::DownloadController* m_downloadCtrl {};
    controllers::TokenController* m_tokenCtrl {};
    controllers::HistoryController* m_historyCtrl {};
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
    controllers::MprisController* m_mprisCtrl {};
    controllers::PlaybackController* m_playbackCtrl {};
    controllers::SeriesPlaybackSessionController* m_seriesSessionCtrl {};
#endif
};

} // namespace kinema::app
