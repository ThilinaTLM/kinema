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
    Test
)

find_package(KF6 ${KF_MIN_VERSION} REQUIRED COMPONENTS
    CoreAddons
    I18n
    KIO
    ConfigWidgets
    XmlGui
)

find_package(QCoro6 ${QCORO_MIN_VERSION} REQUIRED COMPONENTS
    Core
    Network
)

# Keyring access (Secret Service on Linux; abstracts over KWallet,
# gnome-keyring, KeePassXC, etc.). Used for the Real-Debrid token.
find_package(Qt6Keychain REQUIRED)

qcoro_enable_coroutines()

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
