# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
# SPDX-License-Identifier: GPL-2.0-or-later

cmake_minimum_required(VERSION 3.22)

project(kinema
    VERSION 0.1.0
    DESCRIPTION "Search movies and TV series, grab magnet links, stream via Real-Debrid"
    HOMEPAGE_URL "https://tlmtech.dev/kinema"
    LANGUAGES CXX
)

set(QT_MIN_VERSION 6.5.0)
set(KF_MIN_VERSION 6.0.0)
set(QCORO_MIN_VERSION 0.10.0)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(ECM ${KF_MIN_VERSION} REQUIRED NO_MODULE)
list(APPEND CMAKE_MODULE_PATH
    ${ECM_MODULE_PATH}
    ${CMAKE_SOURCE_DIR}/cmake
)

include(KDEInstallDirs)
include(KDECMakeSettings)
include(KDECompilerSettings NO_POLICY_SCOPE)
include(ECMSetupVersion)
include(ECMInstallIcons)
include(ECMQtDeclareLoggingCategory)
include(ECMAddTests)
include(FeatureSummary)

find_package(Qt6 ${QT_MIN_VERSION} REQUIRED COMPONENTS
    Core
    Gui
    Widgets
    Network
    OpenGL
    OpenGLWidgets
    Test
)

find_package(KF6 ${KF_MIN_VERSION} REQUIRED COMPONENTS
    CoreAddons
    I18n
    KIO
    ConfigWidgets
    Notifications
    WidgetsAddons
    XmlGui
)

find_package(QCoro6 ${QCORO_MIN_VERSION} REQUIRED COMPONENTS
    Core
    Network
)

# Keyring access (Secret Service on Linux; abstracts over KWallet,
# gnome-keyring, KeePassXC, etc.). Used for the Real-Debrid token.
find_package(Qt6Keychain REQUIRED)

# ---------------------------------------------------------------------------
# Optional libmpv for the in-app "Embedded" player. When present, Kinema
# can render mpv into a QOpenGLWidget with the built-in OSC providing
# playback controls. When absent, the Embedded option is greyed out in
# Settings and PlayerLauncher falls back to the external mpv/VLC path.
# ---------------------------------------------------------------------------
find_package(PkgConfig)
if(PkgConfig_FOUND)
    pkg_check_modules(MPV IMPORTED_TARGET mpv>=0.36)
endif()
option(KINEMA_ENABLE_MPV_EMBED
    "Build the in-app mpv player (requires libmpv ≥ 0.36)"
    ${MPV_FOUND})
if(KINEMA_ENABLE_MPV_EMBED AND NOT MPV_FOUND)
    message(FATAL_ERROR
        "KINEMA_ENABLE_MPV_EMBED=ON was requested but libmpv (≥ 0.36) "
        "was not found via pkg-config. Install the libmpv development "
        "package (e.g. `libmpv-dev` on Debian/Ubuntu, `mpv-libs` / "
        "`mpv` on Fedora/Arch) or pass -DKINEMA_ENABLE_MPV_EMBED=OFF.")
endif()
add_feature_info("Embedded mpv player" KINEMA_ENABLE_MPV_EMBED
    "in-app playback using libmpv's render API")

qcoro_enable_coroutines()

# Optional TMDB v4 Read Access Token compiled into the binary. Empty by
# default. When set, Discover works out-of-the-box without the user
# pasting a token. Users can always override it in Settings.
# Packagers: set your own token here rather than shipping upstream's.
set(KINEMA_TMDB_DEFAULT_TOKEN "" CACHE STRING
    "Optional TMDB v4 Read Access Token baked into the binary for Discover.")

configure_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/src/core/TmdbConfig.h.in
    ${CMAKE_CURRENT_BINARY_DIR}/src/core/TmdbConfig.h
    @ONLY)

ecm_setup_version(PROJECT
    VARIABLE_PREFIX KINEMA
    VERSION_HEADER "${CMAKE_CURRENT_BINARY_DIR}/kinema_version.h"
)

enable_testing()

add_subdirectory(src)
add_subdirectory(data)

if(BUILD_TESTING)
    add_subdirectory(tests)
endif()

feature_summary(WHAT ALL INCLUDE_QUIET_PACKAGES FATAL_ON_MISSING_REQUIRED_PACKAGES)
