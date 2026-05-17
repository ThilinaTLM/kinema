#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# Build a Kinema AppImage from a checked-out repo.
#
# Intended to run inside the Debian 13 (trixie) container used by
# .github/workflows/release.yml. Trixie sets the glibc / libstdc++
# baseline for the AppImage. Required system packages are installed by
# the workflow before this script is invoked (see release.yml → appimage
# job). For local reproduction, install the same `apt-get install` list.
#
# Outputs:
#   dist/Kinema-${VERSION}-x86_64.AppImage   — the AppImage itself
#   dist/AppDir.tar                          — uncompressed AppDir, repackaged
#                                              into the portable .tar.gz by the
#                                              same CI job
#
# Environment:
#   VERSION   — required. Release version without leading 'v' (e.g. 0.1.0).

set -euo pipefail

if [[ -z "${VERSION:-}" ]]; then
    if [[ -n "${GITHUB_REF_NAME:-}" ]]; then
        VERSION="${GITHUB_REF_NAME#v}"
    else
        echo "build-appimage.sh: VERSION env var is required" >&2
        exit 2
    fi
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

BUILD_DIR="${REPO_ROOT}/build-appimage"
APPDIR="${REPO_ROOT}/AppDir"
DIST_DIR="${REPO_ROOT}/dist"
TOOLS_DIR="${REPO_ROOT}/.appimage-tools"

mkdir -p "${DIST_DIR}" "${TOOLS_DIR}"
rm -rf "${BUILD_DIR}" "${APPDIR}"

# ---------------------------------------------------------------------------
# 1. Configure + build into a clean tree.
# ---------------------------------------------------------------------------
log "Configuring (Release, prefix=/usr) into ${BUILD_DIR}/"
cmake -B "${BUILD_DIR}" -S . -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DKINEMA_ENABLE_MPV_EMBED=ON \
    -DKINEMA_TMDB_DEFAULT_TOKEN=""

log "Building"
cmake --build "${BUILD_DIR}" -j"$(nproc)"

log "Staging to ${APPDIR}/"
DESTDIR="${APPDIR}" cmake --install "${BUILD_DIR}"

# linuxdeploy expects the .desktop file and icon at the AppDir root.
# The install put them under /usr/share/{applications,icons,metainfo}.
# Symlink (not copy) so the in-AppDir /usr/share tree stays canonical.
ln -sf "usr/share/applications/dev.tlmtech.kinema.desktop" \
    "${APPDIR}/dev.tlmtech.kinema.desktop"
ln -sf "usr/share/icons/hicolor/scalable/apps/dev.tlmtech.kinema.svg" \
    "${APPDIR}/dev.tlmtech.kinema.svg"

# ---------------------------------------------------------------------------
# 2. Download linuxdeploy + qt plugin + appimage plugin. Pinned versions
#    keep the AppImage reproducible-ish across CI runs.
# ---------------------------------------------------------------------------
LD_VER="continuous"      # linuxdeploy publishes only 'continuous' atm
LD_BASE="https://github.com/linuxdeploy/linuxdeploy/releases/download/${LD_VER}"
LDQT_BASE="https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/${LD_VER}"

fetch() {
    local url="$1" out="$2"
    if [[ ! -f "${out}" ]]; then
        log "Downloading ${url}"
        wget -q -O "${out}" "${url}"
        chmod +x "${out}"
    fi
}

fetch "${LD_BASE}/linuxdeploy-x86_64.AppImage" \
      "${TOOLS_DIR}/linuxdeploy-x86_64.AppImage"
fetch "${LDQT_BASE}/linuxdeploy-plugin-qt-x86_64.AppImage" \
      "${TOOLS_DIR}/linuxdeploy-plugin-qt-x86_64.AppImage"

# AppImages need FUSE. In CI we have it; in restricted environments
# (some Docker setups) we have to extract them and run the AppRun.
# Detect and fall back automatically.
extract_if_needed() {
    local tool="$1"
    if ! "${tool}" --appimage-version >/dev/null 2>&1; then
        log "FUSE unavailable, extracting $(basename "${tool}")"
        local dir
        dir="${tool}.extracted"
        rm -rf "${dir}"
        (cd "$(dirname "${tool}")" && "${tool}" --appimage-extract >/dev/null)
        mv "$(dirname "${tool}")/squashfs-root" "${dir}"
        echo "${dir}/AppRun"
    else
        echo "${tool}"
    fi
}

LINUXDEPLOY="$(extract_if_needed "${TOOLS_DIR}/linuxdeploy-x86_64.AppImage")"
LINUXDEPLOY_QT="$(extract_if_needed "${TOOLS_DIR}/linuxdeploy-plugin-qt-x86_64.AppImage")"

# linuxdeploy locates plugins by PATH lookup of `linuxdeploy-plugin-<name>`.
export PATH="$(dirname "${LINUXDEPLOY_QT}"):${PATH}"
ln -sf "${LINUXDEPLOY_QT}" \
    "$(dirname "${LINUXDEPLOY_QT}")/linuxdeploy-plugin-qt"

# ---------------------------------------------------------------------------
# 3. Bundle. QML_SOURCES_PATHS tells linuxdeploy-plugin-qt where to look
#    for `import` statements — necessary because our own QML files are
#    compiled into static-lib Qt resources, so the plugin can't walk
#    them inside the binary.
#
#    Our QML imports include two internal modules (dev.tlmtech.kinema.app
#    and dev.tlmtech.kinema.player) whose registration types live inside
#    the static libs kinema_core / kinema_qml_app and are referenced via
#    qrc:/qt/qml/... at runtime. qmlimportscanner expects them on disk
#    under one of QML_IMPORT_PATH and bails out with "Missing qml module"
#    if it can't find a qmldir. We satisfy it with empty stub qmldirs in
#    a scratch directory prepended to QML_IMPORT_PATH; nothing from these
#    stubs is ever copied into the AppImage (linuxdeploy only deploys
#    files referenced by the qmldir, and the stubs are empty).
# ---------------------------------------------------------------------------
QML_STUBS_DIR="${REPO_ROOT}/.appimage-qml-stubs"
rm -rf "${QML_STUBS_DIR}"
for mod in dev/tlmtech/kinema/app dev/tlmtech/kinema/player; do
    mkdir -p "${QML_STUBS_DIR}/${mod}"
    uri="${mod//\//.}"
    cat >"${QML_STUBS_DIR}/${mod}/qmldir" <<EOF
module ${uri}
EOF
done

# qmlimportscanner only follows literal `import` statements. Modules we
# load by name at runtime (QQuickStyle::setStyle / QT_QUICK_CONTROLS_STYLE
# in main.cpp + AppRun.sh) won't be picked up. Drop a stub QML file that
# imports them so the plugin deploys the module + transitively its
# C++ runtime (libkf6qqc2desktopstyle*).
RUNTIME_IMPORTS_DIR="${REPO_ROOT}/.appimage-runtime-imports"
rm -rf "${RUNTIME_IMPORTS_DIR}"
mkdir -p "${RUNTIME_IMPORTS_DIR}"
cat >"${RUNTIME_IMPORTS_DIR}/RuntimeImports.qml" <<'EOF'
import QtQuick
import org.kde.desktop
Item {}
EOF

export QML_SOURCES_PATHS="${REPO_ROOT}/src/ui/qml:${RUNTIME_IMPORTS_DIR}"
if [[ -d "${REPO_ROOT}/src/ui/player/qml" ]]; then
    QML_SOURCES_PATHS+=":${REPO_ROOT}/src/ui/player/qml"
fi

# QML_IMPORT_PATH is what qmlimportscanner consults; QML2_IMPORT_PATH is
# the legacy env var still respected by some Qt builds. Set both.
export QML_IMPORT_PATH="${QML_STUBS_DIR}:${QML_IMPORT_PATH:-}"
export QML2_IMPORT_PATH="${QML_STUBS_DIR}:${QML2_IMPORT_PATH:-}"

# Override default AppRun with our custom launcher (sets QML2_IMPORT_PATH
# and Quick Controls style for non-Plasma sessions).
cp "${REPO_ROOT}/packaging/appimage/AppRun.sh" "${APPDIR}/AppRun"
chmod +x "${APPDIR}/AppRun"

# OUTPUT is the literal output FILENAME for the AppImage (despite the name
# looking like a format selector — the format is chosen by --output).
# LINUXDEPLOY_OUTPUT_VERSION is what appimagetool stamps into the default
# filename `<Name>-<version>-<arch>.AppImage` when OUTPUT is unset.
FINAL_APPIMAGE="Kinema-${VERSION}-x86_64.AppImage"

log "Running linuxdeploy + qt plugin (deploy only)"
LINUXDEPLOY_OUTPUT_VERSION="${VERSION}" \
"${LINUXDEPLOY}" \
    --appdir "${APPDIR}" \
    --executable "${APPDIR}/usr/bin/kinema" \
    --desktop-file "${APPDIR}/dev.tlmtech.kinema.desktop" \
    --icon-file "${APPDIR}/dev.tlmtech.kinema.svg" \
    --plugin qt

# linuxdeploy replaces AppRun with its own wrapper — restore ours.
cp "${REPO_ROOT}/packaging/appimage/AppRun.sh" "${APPDIR}/AppRun"
chmod +x "${APPDIR}/AppRun"

log "Producing the AppImage → ${FINAL_APPIMAGE}"
OUTPUT="${FINAL_APPIMAGE}" \
LINUXDEPLOY_OUTPUT_VERSION="${VERSION}" \
"${LINUXDEPLOY}" \
    --appdir "${APPDIR}" \
    --output appimage

if [[ ! -f "${FINAL_APPIMAGE}" ]]; then
    echo "build-appimage.sh: expected ${FINAL_APPIMAGE} to exist after" \
         "linuxdeploy --output appimage, but it does not." >&2
    ls -la "${REPO_ROOT}" >&2
    exit 3
fi
mv -f "${FINAL_APPIMAGE}" "${DIST_DIR}/${FINAL_APPIMAGE}"
log "Produced ${DIST_DIR}/${FINAL_APPIMAGE}"

# ---------------------------------------------------------------------------
# 4. Capture the AppDir for the portable-tarball job to repackage.
#    Uncompressed tar preserves symlinks and avoids double-gzipping.
# ---------------------------------------------------------------------------
log "Capturing AppDir for the portable tarball job"
tar -cf "${DIST_DIR}/AppDir.tar" -C "${REPO_ROOT}" AppDir

log "Done."
ls -lh "${DIST_DIR}/"
