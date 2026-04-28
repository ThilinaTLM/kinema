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
    config/FilterSettings.cpp
    config/PlayerSettings.cpp
    config/RealDebridSettings.cpp
    config/SearchSettings.cpp
    config/SubtitleSettings.cpp
    config/TorrentioSettings.cpp

    core/CachePaths.cpp
    core/Database.cpp
    core/DateFormat.cpp
    core/DateWindow.cpp
    core/HistoryStore.cpp
    core/HttpClient.cpp
    core/HttpErrorPresenter.cpp
    core/Language.cpp
    core/Magnet.cpp
    core/MediaChips.cpp
    core/Moviehash.cpp
    core/MpvChapterList.cpp
    core/MpvConfigPaths.cpp
    core/MpvTrackList.cpp
    core/Player.cpp
    core/PlayerLauncher.cpp
    core/StreamFilter.cpp
    core/StreamTokens.cpp
    core/SubtitleCacheStore.cpp
    core/TokenStore.cpp
    core/TorrentioConfig.cpp
    core/UrlRedactor.cpp

    api/CinemetaClient.cpp
    api/CinemetaParse.cpp
    api/OpenSubtitlesClient.cpp
    api/OpenSubtitlesParse.cpp
    api/PlaybackContext.cpp
    api/RealDebridClient.cpp
    api/RealDebridParse.cpp
    api/TmdbClient.cpp
    api/TmdbDiscoverUrl.cpp
    api/TmdbParse.cpp
    api/TorrentioClient.cpp
    api/TorrentioParse.cpp

    ui/ImageLoader.cpp

    app/KinemaApplication.cpp

    controllers/HistoryController.cpp
    controllers/SubtitleController.cpp
    controllers/TokenController.cpp
    controllers/TrayController.cpp

    services/StreamActions.cpp

    ui/qml-bridge/BrowseViewModel.cpp
    ui/qml-bridge/ContinueWatchingViewModel.cpp
    ui/qml-bridge/DiscoverSectionModel.cpp
    ui/qml-bridge/DiscoverViewModel.cpp
    ui/qml-bridge/EpisodesListModel.cpp
    ui/qml-bridge/KinemaImageProvider.cpp
    ui/qml-bridge/MainController.cpp
    ui/qml-bridge/MovieDetailViewModel.cpp
    ui/qml-bridge/ResultsListModel.cpp
    ui/qml-bridge/SearchViewModel.cpp
    ui/qml-bridge/SeriesDetailViewModel.cpp
    ui/qml-bridge/SettingsViewModels.cpp
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
        controllers/PlaybackController.cpp
        ui/player/AudioTracksModel.cpp
        ui/player/ChaptersModel.cpp
        ui/player/MpvVideoItem.cpp
        ui/player/PlayerViewModel.cpp
        ui/player/PlayerWindow.cpp
        ui/player/SubtitleTracksModel.cpp
    )
endif()

ecm_qt_declare_logging_category(kinema_core_SRCS
    HEADER kinema_debug.h
    IDENTIFIER KINEMA
    CATEGORY_NAME dev.tlmtech.kinema
    DEFAULT_SEVERITY Info
    DESCRIPTION "Kinema"
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
)

# ---- App-side QML module (always built) ----------------------------------
# Carried by a dedicated static library so it can coexist with the
# player module on `kinema_core`: `qt_add_qml_module` permits only
# one module per target. C++ types this module needs (the engine-
# side `KinemaImageProvider` and the `mainController` context-
# property `MainController`) live in `kinema_core` and are exposed
# via context properties — so this module is QML-only and depends
# on `kinema_core` for the engine-side wiring done from `main.cpp`.
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
set_source_files_properties(ui/qml/Theme.qml
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_qml_module(kinema_qml_app
    URI dev.tlmtech.kinema.app
    VERSION 1.0
    QML_FILES
        ui/qml/ApplicationShell.qml
        ui/qml/Theme.qml
        ui/qml/components/BackdropHero.qml
        ui/qml/components/ContentRail.qml
        ui/qml/components/ContinueWatchingRail.qml
        ui/qml/components/DetailOverview.qml
        ui/qml/components/ActiveFiltersBar.qml
        ui/qml/components/BrowseFilterBar.qml
        ui/qml/components/EpisodeRow.qml
        ui/qml/components/GenresMenuButton.qml
        ui/qml/components/KAboutPage.qml
        ui/qml/components/KEmptyState.qml
        ui/qml/components/KErrorState.qml
        ui/qml/components/KSpinner.qml
        ui/qml/components/MediaKindSwitch.qml
        ui/qml/components/MetaChip.qml
        ui/qml/components/MetaSection.qml
        ui/qml/components/PosterCard.qml
        ui/qml/components/PosterGrid.qml
        ui/qml/components/ProgressPosterCard.qml
        ui/qml/components/RatingChip.qml
        ui/qml/components/SeasonTabs.qml
        ui/qml/components/SimilarCarousel.qml
        ui/qml/components/StreamCard.qml
        ui/qml/components/StreamFilterBar.qml
        ui/qml/components/StreamRowActions.qml
        ui/qml/components/StreamSortMenu.qml
        ui/qml/components/StreamsList.qml
        ui/qml/components/UpcomingBadge.qml
        ui/qml/components/LanguagePicker.qml
        ui/qml/components/SubtitleResultRow.qml
        ui/qml/components/SubtitlesPanel.qml
        ui/qml/components/ThreeStateChips.qml
        ui/qml/pages/BrowsePage.qml
        ui/qml/pages/DiscoverPage.qml
        ui/qml/pages/MovieDetailPage.qml
        ui/qml/pages/SearchPage.qml
        ui/qml/pages/SeriesDetailPage.qml
        ui/qml/pages/SettingsPage.qml
        ui/qml/pages/StreamsPage.qml
        ui/qml/pages/SubtitlesPage.qml
        ui/qml/pages/settings/AppearanceSettingsPage.qml
        ui/qml/pages/settings/FiltersSettingsPage.qml
        ui/qml/pages/settings/GeneralSettingsPage.qml
        ui/qml/pages/settings/PlayerSettingsPage.qml
        ui/qml/pages/settings/RealDebridSettingsPage.qml
        ui/qml/pages/settings/SubtitlesSettingsPage.qml
        ui/qml/pages/settings/TmdbSettingsPage.qml
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

# kinema_core is a STATIC library; the linker discards the QML
# module's registration object (`QQmlModuleRegistration`) by default
# because no symbol references it. `qt_import_qml_plugins` emits the
# right link directives to force-link every QML module the static
# library carries — both the always-built `dev.tlmtech.kinema.app`
# shell module and (when libmpv is enabled) the player chrome.
# Without it the engine reports `<C++ type> is not a type` for
# everything declared with `QML_ELEMENT`.
qt_import_qml_plugins(kinema)

install(TARGETS kinema ${KDE_INSTALL_TARGETS_DEFAULT_ARGS})
