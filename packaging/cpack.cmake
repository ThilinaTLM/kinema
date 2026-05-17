# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# CPack glue for native Linux packages (.deb and .rpm).
#
# Drives kinema's release pipeline (see .github/workflows/release.yml).
# Local developer builds never enable CPack: CPACK_GENERATOR is left
# empty by default so `cmake -B build` stays cheap. CI sets
#   -DCPACK_GENERATOR=DEB   (Ubuntu / Debian containers)
#   -DCPACK_GENERATOR=RPM   (Fedora containers)
# and then runs `cpack -G DEB` / `cpack -G RPM` inside the build dir.
#
# AppImage and the source/portable tarballs are produced outside CPack
# by packaging/appimage/build-appimage.sh and a `git archive` step.

# ---------------------------------------------------------------------------
# Shared metadata. Pulled from project(...) declarations so we have a
# single source of truth for version, summary, and homepage URL.
# ---------------------------------------------------------------------------
set(CPACK_PACKAGE_NAME                "kinema")
set(CPACK_PACKAGE_VERSION             "${PROJECT_VERSION}")
set(CPACK_PACKAGE_VENDOR              "Thilina Lakshan")
set(CPACK_PACKAGE_CONTACT             "Thilina Lakshan <thilinalakshanmail@gmail.com>")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "${PROJECT_DESCRIPTION}")
set(CPACK_PACKAGE_HOMEPAGE_URL        "${PROJECT_HOMEPAGE_URL}")
set(CPACK_PACKAGE_INSTALL_DIRECTORY   "kinema")
set(CPACK_RESOURCE_FILE_LICENSE       "${CMAKE_SOURCE_DIR}/LICENSE")
set(CPACK_RESOURCE_FILE_README        "${CMAKE_SOURCE_DIR}/README.md")

# Strip binaries in release packages. Native distros expect this; the
# debug-info split is left to debhelper / rpm-build defaults if a
# downstream maintainer ever picks the package up.
set(CPACK_STRIP_FILES TRUE)

# Default: no generator selected → cpack does nothing, configure is fast.
set(CPACK_GENERATOR "" CACHE STRING
    "CPack generators (DEB, RPM, ...). Leave empty for non-release builds.")

# ---------------------------------------------------------------------------
# DEB
# ---------------------------------------------------------------------------
# Targets: Ubuntu 25.04 (plucky) and Debian 13 (trixie). Both ship
# Qt 6.6+, KF6, qcoro ≥ 0.10, mpv ≥ 0.39, libtorrent-rasterbar 2.0.
# Ubuntu 24.04 LTS is explicitly NOT supported: it has Qt 6.4 and no KF6
# in main repos.
#
# Per-distro file naming is overridden by CI via
# -DCPACK_DEBIAN_FILE_NAME=kinema_<ver>_amd64-<distro>.deb so artifacts
# from both matrix jobs land in the same release without colliding.
set(CPACK_DEBIAN_PACKAGE_NAME         "kinema")
set(CPACK_DEBIAN_PACKAGE_SECTION      "video")
set(CPACK_DEBIAN_PACKAGE_PRIORITY     "optional")
set(CPACK_DEBIAN_PACKAGE_ARCHITECTURE "amd64")
set(CPACK_DEBIAN_PACKAGE_HOMEPAGE     "${PROJECT_HOMEPAGE_URL}")
set(CPACK_DEBIAN_PACKAGE_MAINTAINER   "${CPACK_PACKAGE_CONTACT}")

# Default file name template; overridden by CI to include the distro tag.
set(CPACK_DEBIAN_FILE_NAME            "DEB-DEFAULT")

# Autodetect ELF library dependencies from the binary's NEEDED entries
# via dpkg-shlibdeps. This catches everything in the dynamic-link audit
# (Qt6, KF6, qcoro, qtkeychain, libmpv, libmpvqt, libtorrent, openssl,
# libstdc++, libgcc, libc, GL, etc.). Pure-QML and runtime-fork deps
# below are not visible to shlibdeps and must be listed manually.
set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS    ON)

# Runtime deps that shlibdeps cannot see:
#  - QML modules our QML imports at runtime (compiled-in QML resources
#    still trigger `import` statements at scene load time);
#  - AppStream/desktop-file integration (cache refresh on install);
#  - `mpv` / `vlc` for the external-player fallback in PlayerLauncher.
#
# Names are the Ubuntu/Debian packages as of 25.04 / trixie. If a
# rename happens, surface it as a release-blocking lint failure rather
# than papering over with "| qml6-module-foo-old".
set(CPACK_DEBIAN_PACKAGE_DEPENDS
    "qml6-module-qtquick, qml6-module-qtquick-controls, \
qml6-module-qtquick-layouts, qml6-module-qtquick-templates, \
qml6-module-qtquick-window, qml6-module-qtquick-effects, \
qml6-module-qtqml-workerscript, qml6-module-qt-labs-platform, \
qml6-module-org-kde-kirigami, qml6-module-org-kde-kirigami-platform, \
mpv | vlc")

# Recommends pick up niceties that aren't strictly required (e.g. the
# external player even if libmpv is embedded — useful for codecs not
# bundled with the embedded build).
set(CPACK_DEBIAN_PACKAGE_RECOMMENDS   "mpv")

# ---------------------------------------------------------------------------
# RPM
# ---------------------------------------------------------------------------
# Targets: Fedora 41 and Fedora 42. Per-distro release suffix is set by
# CI via -DCPACK_RPM_PACKAGE_RELEASE=1.fc41 / 1.fc42.
set(CPACK_RPM_PACKAGE_NAME            "kinema")
set(CPACK_RPM_PACKAGE_SUMMARY         "${PROJECT_DESCRIPTION}")
set(CPACK_RPM_PACKAGE_LICENSE         "Apache-2.0")
set(CPACK_RPM_PACKAGE_GROUP           "Applications/Multimedia")
set(CPACK_RPM_PACKAGE_URL             "${PROJECT_HOMEPAGE_URL}")
set(CPACK_RPM_PACKAGE_VENDOR          "${CPACK_PACKAGE_VENDOR}")
set(CPACK_RPM_FILE_NAME               "RPM-DEFAULT")

# Autoreq picks up shared-library Requires from the binary's NEEDED
# entries; no need to enumerate Qt6/KF6 sonames by hand.
set(CPACK_RPM_PACKAGE_AUTOREQ         ON)
set(CPACK_RPM_PACKAGE_AUTOPROV        ON)

# Pure-QML / runtime-fork deps that autoreq can't see. Names below
# match Fedora 41/42's package naming.
set(CPACK_RPM_PACKAGE_REQUIRES
    "qt6-qtdeclarative, qt6-qtquickcontrols2, \
kf6-kirigami, kf6-kirigami-addons, \
hicolor-icon-theme, mpv")

# Don't let rpmbuild auto-strip files we deliberately ship (mpv configs
# under /usr/share/kinema/mpv/ are text). Suppress the unpackaged-files
# check for the install prefix dirs we already populate via KDE's
# KDEInstallDirs.
set(CPACK_RPM_EXCLUDE_FROM_AUTO_FILELIST_ADDITION
    "%dir /usr/share/applications"
    "%dir /usr/share/icons"
    "%dir /usr/share/icons/hicolor"
    "%dir /usr/share/icons/hicolor/scalable"
    "%dir /usr/share/icons/hicolor/scalable/apps"
    "%dir /usr/share/metainfo"
    "%dir /usr/share/knotifications6")

# ---------------------------------------------------------------------------
# Activate. CPackConfig.cmake is regenerated whenever this file changes,
# so a `cpack -G DEB` immediately after `cmake -B build` picks up edits.
# ---------------------------------------------------------------------------
include(CPack)
