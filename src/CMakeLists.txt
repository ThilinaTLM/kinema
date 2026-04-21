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

    core/DateFormat.cpp
    core/DateWindow.cpp
    core/HttpClient.cpp
    core/HttpErrorPresenter.cpp
    core/Magnet.cpp
    core/Player.cpp
    core/PlayerLauncher.cpp
    core/StreamFilter.cpp
    core/TokenStore.cpp
    core/TorrentioConfig.cpp

    api/CinemetaClient.cpp
    api/CinemetaParse.cpp
    api/RealDebridClient.cpp
    api/RealDebridParse.cpp
    api/TmdbClient.cpp
    api/TmdbDiscoverUrl.cpp
    api/TmdbParse.cpp
    api/TorrentioClient.cpp
    api/TorrentioParse.cpp

    ui/BrowseFilterBar.cpp
    ui/BrowsePage.cpp
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

    controllers/MovieDetailController.cpp
    controllers/NavigationController.cpp
    controllers/SearchController.cpp
    controllers/SeriesDetailController.cpp
    controllers/TokenController.cpp
    controllers/TrayController.cpp

    services/StreamActions.cpp
)

if(KINEMA_ENABLE_MPV_EMBED)
    list(APPEND kinema_core_SRCS
        ui/player/MpvWidget.cpp
        ui/player/PlayerWindow.cpp
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
            Qt6::OpenGLWidgets
            PkgConfig::MPV
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

install(TARGETS kinema ${KDE_INSTALL_TARGETS_DEFAULT_ARGS})
