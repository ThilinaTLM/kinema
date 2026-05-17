#!/bin/sh
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# AppImage entrypoint for Kinema. Also reused as the launcher inside
# the portable-binary .tar.gz (copied to kinema-X.Y.Z-x86_64/kinema.sh).
#
# Resolves $APPDIR even when the script is invoked via a symlink, then
# prepends the bundled bin/, lib/, Qt plugin, and QML import paths so
# the bundle's libraries win over any host-installed copies.

set -e

APPDIR="$(dirname "$(readlink -f "$0")")"
export APPDIR

# Binaries (kinema itself, plus any helper tools linuxdeploy bundles).
export PATH="${APPDIR}/usr/bin:${PATH}"

# Bundled shared libraries (Qt6, KF6, qcoro, qtkeychain, libmpv, mpvqt,
# libtorrent, openssl, plus the entire ffmpeg/libplacebo/libass tail).
# Host-coupled libs (libGL, libva, X11/Wayland, glibc) intentionally
# stay on the host via linuxdeploy's excludelist.
export LD_LIBRARY_PATH="${APPDIR}/usr/lib:${APPDIR}/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"

# Qt plugin search path. linuxdeploy-plugin-qt stages plugins into
# usr/plugins/ on some distros and usr/lib/qt6/plugins/ on others; cover
# both so the platform / iconengine / imageformat plugins resolve.
export QT_PLUGIN_PATH="${APPDIR}/usr/plugins:${APPDIR}/usr/lib/qt6/plugins:${APPDIR}/usr/lib/x86_64-linux-gnu/qt6/plugins"

# QML imports. linuxdeploy-plugin-qt copies the modules listed in
# QML_SOURCES_PATHS (set by build-appimage.sh) under one of these paths;
# we point Qt at both.
export QML2_IMPORT_PATH="${APPDIR}/usr/qml:${APPDIR}/usr/lib/qt6/qml:${APPDIR}/usr/lib/x86_64-linux-gnu/qt6/qml"

# Ensure our .desktop / appstream / icons / mpv configs are visible to
# QStandardPaths::GenericDataLocation lookups (kinema reads its mpv
# config from share/kinema/mpv/).
export XDG_DATA_DIRS="${APPDIR}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

# Kirigami needs a hint for its style on non-Plasma sessions; default
# to "Default" so GNOME / XFCE users get sensible chrome. Respect a
# pre-set QT_QUICK_CONTROLS_STYLE so KDE users still get Breeze.
export QT_QUICK_CONTROLS_STYLE="${QT_QUICK_CONTROLS_STYLE:-org.kde.desktop}"

exec "${APPDIR}/usr/bin/kinema" "$@"
