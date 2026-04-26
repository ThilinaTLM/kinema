# SPDX-License-Identifier: Apache-2.0

# ---------------------------------------------------------------------------
# kinema_core — object library containing everything except main(). Re-used
# by the test binaries so they can link against the same code the app uses.
# ---------------------------------------------------------------------------

set(kinema_core_SRCS
    config/AppearanceSettings.cpp
    config/AppSettings.cpp
    config/BrowseSettings.cpp
    config/FilterSettings.cpp
    config/PlayerSettings.cpp
    config/RealDebridSettings.cpp
    config/SearchSettings.cpp
    config/TorrentioSettings.cpp

    core/ShortcutSections.cpp
    core/Database.cpp
    core/DateFormat.cpp
    core/DateWindow.cpp
    core/HistoryStore.cpp
    core/HttpClient.cpp
    core/HttpErrorPresenter.cpp
    core/Magnet.cpp
    core/MediaChips.cpp
    core/MpvChapterList.cpp
    core/MpvConfigPaths.cpp
    core/MpvTrackList.cpp
    core/NextEpisode.cpp
    core/Player.cpp
    core/PlayerLauncher.cpp
    core/StreamFilter.cpp
    core/TokenStore.cpp
    core/TorrentioConfig.cpp

    api/CinemetaClient.cpp
    api/CinemetaParse.cpp
    api/PlaybackContext.cpp
    api/RealDebridClient.cpp
    api/RealDebridParse.cpp
    api/TmdbClient.cpp
    api/TmdbDiscoverUrl.cpp
    api/TmdbParse.cpp
    api/TorrentioClient.cpp
    api/TorrentioParse.cpp

    ui/BrowseFilterBar.cpp
    ui/BrowsePage.cpp
    ui/ContinueWatchingCardDelegate.cpp
    ui/ContinueWatchingSection.cpp
    ui/DiscoverCardDelegate.cpp
    ui/DiscoverGridView.cpp
    ui/DiscoverPage.cpp
    ui/DiscoverRowModel.cpp
    ui/DiscoverSection.cpp
    ui/EpisodeDelegate.cpp
    ui/EpisodesModel.cpp
    ui/ImageLoader.cpp
    ui/ResultsModel.cpp
    ui/ResultCardDelegate.cpp
    ui/SeriesDetailPane.cpp
    ui/SeriesPicker.cpp
    ui/SimilarStrip.cpp
    ui/TorrentsModel.cpp
    ui/SearchBar.cpp
    ui/StateWidget.cpp
    ui/UpcomingBadge.cpp
    ui/DetailPane.cpp
    ui/MainWindow.cpp

    ui/widgets/MetaHeaderWidget.cpp
    ui/widgets/StreamsPanel.cpp

    ui/settings/FiltersSettingsPage.cpp
    ui/settings/GeneralSettingsPage.cpp
    ui/settings/PlayerSettingsPage.cpp
    ui/settings/RealDebridSettingsPage.cpp
    ui/settings/SettingsDialog.cpp
    ui/settings/TmdbSettingsPage.cpp

    app/KinemaApplication.cpp

    controllers/HistoryController.cpp
    controllers/MovieDetailController.cpp
    controllers/NavigationController.cpp
    controllers/SearchController.cpp
    controllers/SeriesDetailController.cpp
    controllers/TokenController.cpp
    controllers/TrayController.cpp

    services/StreamActions.cpp

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
        Qt6::Sql
        Qt6::Svg
        KF6::CoreAddons
        KF6::I18n
        KF6::KIOCore
        KF6::KIOGui
        KF6::ConfigWidgets
        KF6::Notifications
        KF6::StatusNotifierItem
        KF6::WidgetsAddons
        KF6::XmlGui
        QCoro6::Core
        QCoro6::Network
        Qt6Keychain::Qt6Keychain
)

if(KINEMA_ENABLE_MPV_EMBED)
    target_compile_definitions(kinema_core PUBLIC KINEMA_HAVE_LIBMPV)
    target_link_libraries(kinema_core
        PUBLIC
            Qt6::OpenGL
            Qt6::Qml
            Qt6::Quick
            Qt6::QuickControls2
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
            ui/player/qml/components/AccentButton.qml
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
            ui/player/qml/components/VolumeHUD.qml
            ui/player/qml/pickers/AudioPicker.qml
            ui/player/qml/pickers/SpeedPicker.qml
            ui/player/qml/pickers/SubtitlePicker.qml
            ui/player/qml/prompts/InfoOverlay.qml
            ui/player/qml/prompts/NextEpisodeBanner.qml
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

target_link_libraries(kinema PRIVATE kinema_core)

# kinema_core is a STATIC library; the linker discards the QML
# module's registration object (`QQmlModuleRegistration`) by default
# because no symbol references it. `qt_import_qml_plugins` emits the
# right link directives to force-link every QML module the static
# library carries, including our in-tree `dev.tlmtech.kinema.player`
# module. Without it the engine reports `<C++ type> is not a type`
# for everything declared with `QML_ELEMENT`.
if(KINEMA_ENABLE_MPV_EMBED)
    qt_import_qml_plugins(kinema)
endif()

install(TARGETS kinema ${KDE_INSTALL_TARGETS_DEFAULT_ARGS})
