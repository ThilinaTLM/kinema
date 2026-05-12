# SPDX-License-Identifier: Apache-2.0

# ---------------------------------------------------------------------------
# kinema_core — object library containing everything except main(). Re-used
# by the test binaries so they can link against the same code the app uses.
# ---------------------------------------------------------------------------

set(kinema_core_SRCS
    config/AppearanceSettings.cpp
    config/AppSettings.cpp
    config/BrowseSettings.cpp
    config/CacheSettings.cpp
    config/DownloadSettings.cpp
    config/FilterSettings.cpp
    config/PlayerSettings.cpp
    api/IndexerSelector.cpp
    config/DebridSettings.cpp
    config/IndexerSettings.cpp
    config/PeerflixSettings.cpp
    config/SearchSettings.cpp
    config/SubtitleSettings.cpp
    config/TorrentioSettings.cpp
    config/TorrentStreamingSettings.cpp

    core/persistence/Database.cpp
    core/persistence/DownloadStore.cpp
    core/persistence/HistoryStore.cpp
    core/persistence/LibraryStore.cpp
    core/persistence/MediaCache.cpp
    core/persistence/SubtitleCacheStore.cpp
    core/persistence/TokenStore.cpp
    core/persistence/TorrentCache.cpp
    core/persistence/WatchedStore.cpp

    core/mpv/MprisMetadata.cpp
    core/mpv/MpvChapterList.cpp
    core/mpv/MpvConfigPaths.cpp
    core/mpv/MpvTrackList.cpp
    core/mpv/Player.cpp
    core/mpv/PlayerLauncher.cpp

    core/io/CachePaths.cpp
    core/io/HttpClient.cpp
    core/io/HttpErrorPresenter.cpp
    core/io/IdleInhibitor.cpp
    core/io/Logging.cpp
    core/io/UrlRedactor.cpp

    core/util/DateFormat.cpp
    core/util/DateWindow.cpp
    core/util/Language.cpp
    core/util/Magnet.cpp
    core/util/MediaChips.cpp
    core/util/Moviehash.cpp
    core/util/StreamFilter.cpp
    core/util/StreamTokens.cpp
    core/util/TorrentioConfig.cpp

    torrent/LocalStreamServer.cpp
    torrent/MediaFileSelector.cpp
    torrent/PiecePlanner.cpp
    torrent/TorrentStreamingService.cpp

    download/AllDebridBackend.cpp
    download/AllDebridResolver.cpp
    download/AssetSession.cpp
    download/BackendSelector.cpp
    download/DebridFilePicker.cpp
    download/DownloadManager.cpp
    download/HttpAssetSession.cpp
    download/LocalMediaServer.cpp
    download/RealDebridBackend.cpp
    download/RealDebridResolver.cpp
    download/TorrentAssetSession.cpp
    download/TorrentBackend.cpp

    api/AllDebridClient.cpp
    api/AllDebridParse.cpp
    api/CinemetaClient.cpp
    api/CinemetaParse.cpp
    api/OpenSubtitlesClient.cpp
    api/OpenSubtitlesParse.cpp
    api/RealDebridClient.cpp
    api/RealDebridParse.cpp
    api/PeerflixIndexer.cpp
    api/StremioStreamParse.cpp
    api/TmdbClient.cpp
    api/TmdbDiscoverUrl.cpp
    api/TmdbParse.cpp
    api/TorrentioIndexer.cpp

    domain/Debrid.cpp
    domain/Download.cpp
    domain/Indexer.cpp
    domain/PlaybackContext.cpp

    ui/ImageLoader.cpp

    app/KinemaApplication.cpp
    app/ServiceContainer.cpp

    controllers/DownloadController.cpp
    controllers/HistoryController.cpp
    controllers/LibraryController.cpp
    controllers/SubtitleController.cpp
    controllers/TokenController.cpp
    controllers/TrayController.cpp
    controllers/WatchedController.cpp

    services/StreamActions.cpp

    ui/qml-bridge/AppIconResolver.cpp
    ui/qml-bridge/BrowseViewModel.cpp
    ui/qml-bridge/ContinueWatchingViewModel.cpp
    ui/qml-bridge/DiscoverSectionModel.cpp
    ui/qml-bridge/DiscoverViewModel.cpp
    ui/qml-bridge/DownloadsListModel.cpp
    ui/qml-bridge/DownloadsViewModel.cpp
    ui/qml-bridge/EpisodesListModel.cpp
    ui/qml-bridge/KinemaImageProvider.cpp
    ui/qml-bridge/LibraryListModel.cpp
    ui/qml-bridge/LibraryRailModel.cpp
    ui/qml-bridge/LibraryViewModel.cpp
    ui/qml-bridge/MovieDetailViewModel.cpp
    ui/qml-bridge/QmlContext.cpp
    ui/qml-bridge/ShellViewModel.cpp
    ui/qml-bridge/ResultsListModel.cpp
    ui/qml-bridge/SearchViewModel.cpp
    ui/qml-bridge/SeriesDetailViewModel.cpp
    ui/qml-bridge/settings/AllDebridSectionViewModel.cpp
    ui/qml-bridge/settings/DebridSettingsViewModel.cpp
    ui/qml-bridge/settings/GeneralSettingsViewModel.cpp
    ui/qml-bridge/settings/IndexerSettingsViewModel.cpp
    ui/qml-bridge/settings/PeerflixSectionViewModel.cpp
    ui/qml-bridge/settings/PlayerSettingsViewModel.cpp
    ui/qml-bridge/settings/RealDebridSectionViewModel.cpp
    ui/qml-bridge/settings/SettingsRootViewModel.cpp
    ui/qml-bridge/settings/StreamsSettingsViewModel.cpp
    ui/qml-bridge/settings/SubtitlesSettingsViewModel.cpp
    ui/qml-bridge/settings/TmdbSettingsViewModel.cpp
    ui/qml-bridge/settings/TorrentStreamingSettingsViewModel.cpp
    ui/qml-bridge/settings/TorrentioSectionViewModel.cpp
    ui/qml-bridge/StreamSorting.cpp
    ui/qml-bridge/StreamsListModel.cpp
    ui/qml-bridge/SubtitleResultsModel.cpp
    ui/qml-bridge/SubtitlesViewModel.cpp

    tray_resources.qrc
)

# Embedded-player sources. The video surface is `MpvVideoItem`
# (subclass of `mpvqt`'s `MpvAbstractItem`); the chrome is composed
# in QtQuick / QML inside the same `QQuickView`-based
# `PlayerWindow`. `PlayerViewModel` plus the three list models drive
# chrome state. No QWidget overlays the video surface; no Lua
# scripts; no IPC bridge.
if(KINEMA_ENABLE_MPV_EMBED)
    list(APPEND kinema_core_SRCS
        controllers/MprisController.cpp
        controllers/PlaybackController.cpp
        controllers/PlaybackLoadWatchdog.cpp
        controllers/SeriesPlaybackSessionController.cpp
        ui/player/AudioTracksModel.cpp
        ui/player/ChaptersModel.cpp
        ui/player/MpvLoadCommand.cpp
        ui/player/MpvVideoItem.cpp
        ui/player/PlayerViewModel.cpp
        ui/player/PlayerWindow.cpp
        ui/player/PlayerWindowPresentation.cpp
        ui/player/SubtitleTracksModel.cpp
    )
endif()

# Per-subsystem logging categories. Each block emits a generated
# header (kinema_log_<name>.h) under ${CMAKE_CURRENT_BINARY_DIR}.
# Files include only the headers they need and call qC*() with the
# matching identifier so users can tune verbosity per subsystem with
# QT_LOGGING_RULES, e.g.
#   QT_LOGGING_RULES='kinema.torrent.debug=true'
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_app.h
    IDENTIFIER KINEMA_APP
    CATEGORY_NAME kinema.app
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema (app shell, ShellViewModel, KinemaApplication)"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_db.h
    IDENTIFIER KINEMA_DB
    CATEGORY_NAME kinema.db
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema database and per-table stores"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_controller.h
    IDENTIFIER KINEMA_CONTROLLER
    CATEGORY_NAME kinema.controller
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema controllers (history, subtitles, library, …)"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_ui.h
    IDENTIFIER KINEMA_UI
    CATEGORY_NAME kinema.ui
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema view-models, image loading, QML bridge"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_player.h
    IDENTIFIER KINEMA_PLAYER
    CATEGORY_NAME kinema.player
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema embedded mpv player"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_api.h
    IDENTIFIER KINEMA_API
    CATEGORY_NAME kinema.api
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema upstream API clients (TMDB, OpenSubtitles, …)"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_torrent.h
    IDENTIFIER KINEMA_TORRENT
    CATEGORY_NAME kinema.torrent
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema torrent engine"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_download.h
    IDENTIFIER KINEMA_DOWNLOAD
    CATEGORY_NAME kinema.download
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema unified download manager"
    EXPORT KINEMA
)
ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_log_http.h
    IDENTIFIER KINEMA_HTTP
    CATEGORY_NAME kinema.http
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema HTTP client + image fetches"
    EXPORT KINEMA
)

add_library(kinema_core STATIC ${kinema_core_SRCS})

target_include_directories(kinema_core
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}
        ${CMAKE_CURRENT_BINARY_DIR}
        ${CMAKE_BINARY_DIR}
)

set_target_properties(kinema_core PROPERTIES
    AUTOMOC ON
    AUTOUIC ON
    AUTORCC ON
    POSITION_INDEPENDENT_CODE ON
)

target_link_libraries(kinema_core
    PUBLIC
        Qt6::Core
        Qt6::DBus
        Qt6::Gui
        Qt6::Widgets
        Qt6::Network
        Qt6::Qml
        Qt6::Quick
        Qt6::QuickControls2
        Qt6::Sql
        Qt6::Svg
        KF6::CoreAddons
        KF6::I18n
        KF6::KIOCore
        KF6::KIOGui
        KF6::ConfigCore
        KF6::Kirigami
        KF6::Notifications
        KF6::StatusNotifierItem
        QCoro6::Core
        QCoro6::Network
        Qt6Keychain::Qt6Keychain
        Boost::headers
        PkgConfig::LIBTORRENT
)

# ---- App-side QML module (always built) ----------------------------------
# Carried by a dedicated static library so it can coexist with the
# player module on `kinema_core`: `qt_add_qml_module` permits only
# one module per target. C++ types this module needs (the engine-
# side `KinemaImageProvider` and the `mainController` context-
# property `shell` of type `ShellViewModel`) live in `kinema_core`
# and are exposed via context properties — so this module is QML-
# only and depends on `kinema_core` for the engine-side wiring done
# from `main.cpp` via `installQmlContext()`.
#
# `Theme.qml` is a QML singleton — the property must be set on the
# source before `qt_add_qml_module()` so the generated qmldir picks
# it up.
add_library(kinema_qml_app STATIC)
set_target_properties(kinema_qml_app PROPERTIES
    AUTOMOC ON
    POSITION_INDEPENDENT_CODE ON
)
# Suppress QTP0004 ("need qmldir for each subdirectory with QML
# files") — we keep components/ and pages/ as plain folders inside
# one module URI on purpose; opting NEW makes Qt generate the extra
# qmldir files automatically.
qt_policy(SET QTP0004 NEW)
set_source_files_properties(
    ui/qml/Theme.qml
    ui/qml/AppIcons.qml
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_qml_module(kinema_qml_app
    URI dev.tlmtech.kinema.app
    VERSION 1.0
    QML_FILES
        ui/qml/ApplicationShell.qml
        ui/qml/AppIcons.qml
        ui/qml/Theme.qml
        ui/qml/components/BackdropHero.qml
        ui/qml/components/ContentRail.qml
        ui/qml/components/ContinueWatchingRail.qml
        ui/qml/components/DetailOverview.qml
        ui/qml/components/EpisodeRailCard.qml
        ui/qml/components/EpisodeRow.qml
        ui/qml/components/FilterMenuButton.qml
        ui/qml/components/KinemaArtworkFrame.qml
        ui/qml/components/LibraryCard.qml
        ui/qml/components/LibraryRail.qml
        ui/qml/components/MetaChip.qml
        ui/qml/components/MetaSection.qml
        ui/qml/components/PageHeaderBar.qml
        ui/qml/components/PosterCard.qml
        ui/qml/components/PosterGrid.qml
        ui/qml/components/RatingChip.qml
        ui/qml/components/SeasonTabs.qml
        ui/qml/components/SimilarCarousel.qml
        ui/qml/components/StatusChip.qml
        ui/qml/components/StreamCard.qml
        ui/qml/components/StreamRowActions.qml
        ui/qml/components/StreamSortDialog.qml
        ui/qml/components/StreamsList.qml
        ui/qml/components/DownloadRow.qml
        ui/qml/components/UpcomingBadge.qml
        ui/qml/components/SubtitleResultRow.qml
        ui/qml/components/SubtitlesPanel.qml
        ui/qml/pages/BrowsePage.qml
        ui/qml/pages/DiscoverPage.qml
        ui/qml/pages/DownloadsPage.qml
        ui/qml/pages/LibraryPage.qml
        ui/qml/pages/MovieDetailPage.qml
        ui/qml/pages/SearchPage.qml
        ui/qml/pages/SeriesDetailPage.qml
        ui/qml/pages/StreamsPage.qml
        ui/qml/pages/SubtitlesPage.qml
        ui/qml/pages/UpNextPage.qml
        ui/qml/pages/settings/AboutSettingsPage.qml
        ui/qml/pages/settings/ApplicationSettingsPage.qml
        ui/qml/pages/settings/DebridSettingsPage.qml
        ui/qml/pages/settings/IndexersSettingsPage.qml
        ui/qml/pages/settings/PlayerSettingsPage.qml
        ui/qml/pages/settings/StreamsSettingsPage.qml
        ui/qml/pages/settings/SubtitlesSettingsPage.qml
        ui/qml/pages/settings/TmdbSettingsPage.qml
        ui/qml/pages/settings/TorrentStreamingSettingsPage.qml
    RESOURCES
        ui/qml/icons/lucide/arrow-down.svg
        ui/qml/icons/lucide/arrow-up-down.svg
        ui/qml/icons/lucide/arrow-up.svg
        ui/qml/icons/lucide/captions.svg
        ui/qml/icons/lucide/chevron-left.svg
        ui/qml/icons/lucide/chevron-right.svg
        ui/qml/icons/lucide/chevrons-down.svg
        ui/qml/icons/lucide/circle-alert.svg
        ui/qml/icons/lucide/circle-check.svg
        ui/qml/icons/lucide/circle-dashed.svg
        ui/qml/icons/lucide/circle-play.svg
        ui/qml/icons/lucide/clapperboard.svg
        ui/qml/icons/lucide/clock-arrow-down.svg
        ui/qml/icons/lucide/compass.svg
        ui/qml/icons/lucide/copy.svg
        ui/qml/icons/lucide/download.svg
        ui/qml/icons/lucide/ellipsis-vertical.svg
        ui/qml/icons/lucide/ellipsis.svg
        ui/qml/icons/lucide/eraser.svg
        ui/qml/icons/lucide/external-link.svg
        ui/qml/icons/lucide/film.svg
        ui/qml/icons/lucide/flag.svg
        ui/qml/icons/lucide/folder-open.svg
        ui/qml/icons/lucide/funnel.svg
        ui/qml/icons/lucide/grid-2x2.svg
        ui/qml/icons/lucide/grip-vertical.svg
        ui/qml/icons/lucide/headphones.svg
        ui/qml/icons/lucide/history.svg
        ui/qml/icons/lucide/info.svg
        ui/qml/icons/lucide/languages.svg
        ui/qml/icons/lucide/library.svg
        ui/qml/icons/lucide/list.svg
        ui/qml/icons/lucide/list-plus.svg
        ui/qml/icons/lucide/list-video.svg
        ui/qml/icons/lucide/list-x.svg
        ui/qml/icons/lucide/minus.svg
        ui/qml/icons/lucide/monitor-play.svg
        ui/qml/icons/lucide/palette.svg
        ui/qml/icons/lucide/pause.svg
        ui/qml/icons/lucide/pin.svg
        ui/qml/icons/lucide/play.svg
        ui/qml/icons/lucide/plus.svg
        ui/qml/icons/lucide/refresh-cw.svg
        ui/qml/icons/lucide/save.svg
        ui/qml/icons/lucide/search-check.svg
        ui/qml/icons/lucide/search.svg
        ui/qml/icons/lucide/server.svg
        ui/qml/icons/lucide/settings-2.svg
        ui/qml/icons/lucide/settings.svg
        ui/qml/icons/lucide/skip-forward.svg
        ui/qml/icons/lucide/sparkles.svg
        ui/qml/icons/lucide/star.svg
        ui/qml/icons/lucide/tags.svg
        ui/qml/icons/lucide/trash-2.svg
        ui/qml/icons/lucide/triangle-alert.svg
        ui/qml/icons/lucide/tv.svg
        ui/qml/icons/lucide/users.svg
        ui/qml/icons/lucide/x.svg
    OUTPUT_DIRECTORY
        "${CMAKE_BINARY_DIR}/qml/dev/tlmtech/kinema/app"
    NO_PLUGIN
)
target_link_libraries(kinema_qml_app PUBLIC kinema_core)

if(KINEMA_ENABLE_MPV_EMBED)
    target_compile_definitions(kinema_core PUBLIC KINEMA_HAVE_LIBMPV)
    target_link_libraries(kinema_core
        PUBLIC
            Qt6::OpenGL
            PkgConfig::MPV
            MpvQt::MpvQt
    )
    # Qt's QML type-registration generator includes player headers by
    # basename (e.g. `<MpvVideoItem.h>`), so the directory must be on
    # the include path or the auto-generated reg file won't see the
    # namespaced classes.
    target_include_directories(kinema_core
        PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/ui/player
    )

    # Player QML module. Files are listed explicitly so a missing
    # entry is a build error, not a silent runtime miss. The URI
    # `dev.tlmtech.kinema.player` matches the resource path under
    # `qrc:/qt/qml/dev/tlmtech/kinema/player/`. C++ types are
    # auto-registered via QML_ELEMENT macros in their headers.
    # Theme.qml is a QML singleton; the property must be set on the
    # source before qt_add_qml_module is called so the generated
    # qmldir picks it up.
    set_source_files_properties(ui/player/qml/Theme.qml
        PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

    qt_add_qml_module(kinema_core
        URI dev.tlmtech.kinema.player
        VERSION 1.0
        QML_FILES
            ui/player/qml/PlayerScene.qml
            ui/player/qml/Theme.qml
            ui/player/qml/components/BufferingIndicator.qml
            ui/player/qml/components/CenterFlash.qml
            ui/player/qml/components/IconButton.qml
            ui/player/qml/components/IconGlyph.qml
            ui/player/qml/components/MediaChips.qml
            ui/player/qml/components/PickerItem.qml
            ui/player/qml/components/PlayerInputs.qml
            ui/player/qml/components/PopupPanel.qml
            ui/player/qml/components/SectionHeader.qml
            ui/player/qml/components/SeekBar.qml
            ui/player/qml/components/TopBar.qml
            ui/player/qml/components/TransportBar.qml
            ui/player/qml/components/VolumeControl.qml
            ui/player/qml/pickers/AudioPicker.qml
            ui/player/qml/pickers/SpeedPicker.qml
            ui/player/qml/pickers/SubtitlePicker.qml
            ui/player/qml/prompts/ResumePrompt.qml
            ui/player/qml/prompts/SkipChapterPill.qml
        OUTPUT_DIRECTORY
            "${CMAKE_BINARY_DIR}/qml/dev/tlmtech/kinema/player"
        NO_PLUGIN
    )
endif()

target_compile_definitions(kinema_core
    PUBLIC
        QT_NO_CAST_FROM_ASCII
        QT_NO_CAST_TO_ASCII
        QT_NO_URL_CAST_FROM_STRING
        QT_NO_CAST_FROM_BYTEARRAY
        QT_STRICT_ITERATORS
        QT_USE_QSTRINGBUILDER
)

# Build-tree fallback for core::mpv_config: when the user runs
# ./build/bin/kinema without installing, GenericDataLocation can't see
# our shipped mpv configs. This bakes the build's share/ directory in
# so the resolver can fall back to it. Installed builds find the files
# via GenericDataLocation first and ignore this path.
target_compile_definitions(kinema_core
    PRIVATE
        KINEMA_BUILD_DATA_DIR="${CMAKE_BINARY_DIR}/share"
)

# KDECompilerSettings disables exceptions by default. We use them narrowly
# in HttpClient / parsers to signal network and JSON errors, caught at the
# UI coroutine boundary, so re-enable them for the whole core library.
kde_target_enable_exceptions(kinema_core PUBLIC)

# ---------------------------------------------------------------------------
# kinema — the application executable
# ---------------------------------------------------------------------------

add_executable(kinema main.cpp)

set_target_properties(kinema PROPERTIES
    AUTOMOC ON
)

target_link_libraries(kinema PRIVATE kinema_core kinema_qml_app)

# kinema_core / kinema_qml_app are static libraries, so their QML
# module registration objects would normally be dead-stripped. The
# executable keeps them alive explicitly via the generated
# `qml_register_types_dev_tlmtech_kinema_app()` and
# `qml_register_types_dev_tlmtech_kinema_player()` calls in main.cpp,
# which avoids `qt_import_qml_plugins()` and its noisy import scan over
# shared system QML modules.

install(TARGETS kinema ${KDE_INSTALL_TARGETS_DEFAULT_ARGS})
