#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# Build a Kinema AppImage from a checked-out repo.
#
# Run inside the Ubuntu 22.04 SDK built by packaging/appimage/Dockerfile.
# The entire payload must target glibc <= 2.35, not only the executable.
# See packaging/README.md for reproduction and compatibility checks.
#
# Outputs:
#   dist/Kinema-${VERSION}-x86_64.AppImage   — the AppImage itself
#   dist/AppDir.tar                          — uncompressed AppDir, repackaged
#                                              into the portable .tar.gz by the
#                                              same CI job
#
# Environment:
#   VERSION   — required. Release version without leading 'v' (e.g. X.Y.Z).

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

# Release artifacts must identify the same project version as the tag. CI names
# are deliberately distinct and never published as releases.
PROJECT_VERSION="$(sed -n 's/^    VERSION \([0-9.]*\)$/\1/p' CMakeLists.txt)"
if [[ ! "${VERSION}" =~ ^[A-Za-z0-9.+-]+$ ]] || \
   [[ "${VERSION}" != ci-* && "${VERSION%%-*}" != "${PROJECT_VERSION}" ]]; then
    echo "build-appimage.sh: artifact version ${VERSION} does not match project ${PROJECT_VERSION}" >&2
    exit 2
fi

log() { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }

: "${KINEMA_SDK:?Build inside the locked Ubuntu 22.04 SDK container}"
export PATH="${KINEMA_SDK}/bin:${PATH}"
export CMAKE_PREFIX_PATH="${KINEMA_SDK}"
export PKG_CONFIG_PATH="${KINEMA_SDK}/lib/pkgconfig:${KINEMA_SDK}/share/pkgconfig"
export LD_LIBRARY_PATH="${KINEMA_SDK}/lib"
export CMAKE_BUILD_PARALLEL_LEVEL="${CMAKE_BUILD_PARALLEL_LEVEL:-4}"
export QMAKE="${KINEMA_SDK}/bin/qmake"
export QML_IMPORT_PATH="${KINEMA_SDK}/qml"
export QML2_IMPORT_PATH="${QML_IMPORT_PATH}"

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
    -DBUILD_TESTING=OFF \
    -DCMAKE_INSTALL_LIBDIR=lib \
    -DKDE_INSTALL_USE_QT_SYS_PATHS=OFF \
    -DKINEMA_TMDB_DEFAULT_TOKEN=""

log "Building"
cmake --build "${BUILD_DIR}" --parallel "${CMAKE_BUILD_PARALLEL_LEVEL}"

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
# 2. Fetch checksummed packaging tools. Extract them explicitly: asking a
#    runtime for --appimage-version does not test FUSE availability.
# ---------------------------------------------------------------------------
RECIPE="${REPO_ROOT}/packaging/appimage"
fetch_tool() {
    python3 "${RECIPE}/fetch-source.py" "${RECIPE}/tools.lock.json" "$1" "${TOOLS_DIR}/locked"
}
extract_tool() {
    local tool dir
    tool="$(fetch_tool "$1")"
    chmod +x "${tool}"
    dir="${tool}.extracted"
    rm -rf "${dir}"
    mkdir -p "${dir}"
    (cd "${dir}" && "${tool}" --appimage-extract >/dev/null)
    printf '%s\n' "${dir}/squashfs-root/AppRun"
}
LINUXDEPLOY="$(extract_tool linuxdeploy)"
LINUXDEPLOY_QT="$(extract_tool linuxdeploy-plugin-qt)"
APPIMAGETOOL="$(extract_tool appimagetool)"
APPIMAGE_RUNTIME="$(fetch_tool runtime)"

# linuxdeploy locates plugins by PATH lookup of `linuxdeploy-plugin-<name>`.
PATH="$(dirname "${LINUXDEPLOY_QT}"):${PATH}"
export PATH
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
# linuxdeploy's scanner uses its own explicit -importPath list, not Qt's env.
export QML_MODULES_PATHS="${QML_STUBS_DIR}:${KINEMA_SDK}/qml"

# Override default AppRun with our custom launcher (sets QML2_IMPORT_PATH
# and Quick Controls style for non-Plasma sessions).
cp "${REPO_ROOT}/packaging/appimage/AppRun.sh" "${APPDIR}/AppRun"
chmod +x "${APPDIR}/AppRun"

# OUTPUT is the literal output FILENAME for the AppImage (despite the name
# looking like a format selector — the format is chosen by --output).
# LINUXDEPLOY_OUTPUT_VERSION is what appimagetool stamps into the default
# filename `<Name>-<version>-<arch>.AppImage` when OUTPUT is unset.
FINAL_APPIMAGE="Kinema-${VERSION}-x86_64.AppImage"

# Tell linuxdeploy-plugin-qt to also bundle the Wayland platform plugin
# in addition to the default xcb. Without this, Qt logs
#   qt.qpa.plugin: Could not find the Qt platform plugin "wayland" in ""
# on Wayland sessions and silently falls back to xcb. The qt plugin
# walks libqwayland-generic.so's deps and also pulls in the related
# wayland-decoration-client / wayland-graphics-integration-client /
# wayland-shell-integration plugin dirs.
export EXTRA_PLATFORM_PLUGINS="libqwayland-generic.so"

# linuxdeploy's system-library excludelist omits PipeWire. Debian's libmpv
# links to it directly, though, so the executable cannot even start on hosts
# without PipeWire (for example, the AppImage catalog's test image). Force the
# library into the bundle; linuxdeploy will deploy any non-system dependencies.
# HarfBuzz/FriBidi are excluded by linuxdeploy but absent on minimal desktop
# test hosts. Bundle the baseline versions instead of installing them to hide
# omissions in the clean-host gate.
FORCED_SONAMES=(libpipewire-0.3.so.0 libharfbuzz.so.0 libfribidi.so.0 libgpg-error.so.0)
FORCED_LIBRARIES=()
FORCED_DEPLOY_ARGS=()
for soname in "${FORCED_SONAMES[@]}"; do
    library="$(ldconfig -p | awk -v name="${soname}" '$1 == name && !found { path = $NF; found = 1 } END { if (found) print path; else exit 1 }')"
    library="$(readlink -f "${library}")"
    FORCED_LIBRARIES+=("${library}")
    FORCED_DEPLOY_ARGS+=(--library "${library}")
done

log "Running linuxdeploy + qt plugin (deploy only)"
LINUXDEPLOY_OUTPUT_VERSION="${VERSION}" \
"${LINUXDEPLOY}" \
    --appdir "${APPDIR}" \
    --executable "${APPDIR}/usr/bin/kinema" \
    "${FORCED_DEPLOY_ARGS[@]}" \
    --desktop-file "${APPDIR}/dev.tlmtech.kinema.desktop" \
    --icon-file "${APPDIR}/dev.tlmtech.kinema.svg" \
    --plugin qt

# --library copies fully versioned files but does not create their SONAME links.
for i in "${!FORCED_LIBRARIES[@]}"; do
    bundled_name="$(basename "${FORCED_LIBRARIES[$i]}")"
    if [[ ! -f "${APPDIR}/usr/lib/${bundled_name}" ]]; then
        echo "build-appimage.sh: linuxdeploy did not bundle ${FORCED_LIBRARIES[$i]}" >&2
        exit 3
    fi
    ln -sf "${bundled_name}" "${APPDIR}/usr/lib/${FORCED_SONAMES[$i]}"
done

# PipeWire dlopens its backend modules; DT_NEEDED discovery cannot find them.
# Batch manual dependency deployment: each invocation revisits the entire AppDir.
MANUAL_DEPLOY_ARGS=()
# Qt's input plugin deployer does not reliably discover Wayland's dlopened
# shell/graphics/decoration integrations. A platform .so alone cannot start.
for plugin_dir in wayland-shell-integration wayland-graphics-integration-client wayland-decoration-client; do
    mkdir -p "${APPDIR}/usr/plugins/${plugin_dir}"
    cp -a "${KINEMA_SDK}/plugins/${plugin_dir}/"*.so "${APPDIR}/usr/plugins/${plugin_dir}/"
    for so in "${APPDIR}/usr/plugins/${plugin_dir}/"*.so; do
        MANUAL_DEPLOY_ARGS+=(--deploy-deps-only "${so}")
    done
done
for module_dir in pipewire-0.3 spa-0.2; do
    source_dir="/usr/lib/x86_64-linux-gnu/${module_dir}"
    [[ -d "${source_dir}" ]] || { echo "Missing ${source_dir}" >&2; exit 3; }
    cp -a "${source_dir}" "${APPDIR}/usr/lib/"
    while IFS= read -r -d '' so; do
        MANUAL_DEPLOY_ARGS+=(--deploy-deps-only "${so}")
    done < <(find "${APPDIR}/usr/lib/${module_dir}" -type f -name '*.so' -print0)
done
if [[ -d /usr/share/pipewire ]]; then
    cp -a /usr/share/pipewire "${APPDIR}/usr/share/"
elif [[ -d /etc/pipewire ]]; then
    # Jammy ships the defaults as conffiles, newer distros use /usr/share.
    cp -a /etc/pipewire "${APPDIR}/usr/share/"
else
    echo "Missing PipeWire client configuration defaults" >&2
    exit 3
fi

# ---------------------------------------------------------------------------
# 3b. Manually deploy KF6 platform plugins that linuxdeploy-plugin-qt has
#     no knowledge of. Kirigami loads its style plugin via
#     KPluginFactory from <plugin_path>/kf6/kirigami/platform/<style>.so
#     (see Kirigami's PlatformPluginFactory). Without these, Kirigami
#     logs:
#         Failed to find a Kirigami platform plugin for style "org.kde.desktop"
#     and the UI falls back to its basic theme.
#
#     We copy the .so files into the AppDir, then run linuxdeploy once
#     more with --deploy-deps-only on each so its transitive deps
#     (libKF6IconThemes, libKF6ColorScheme, …) end up in usr/lib/ and
#     are filtered against linuxdeploy's excludelist.
# ---------------------------------------------------------------------------
KF6_KIRIGAMI_SRC=""
for candidate in \
    "${KINEMA_SDK}/plugins/kf6/kirigami/platform" \
    "${KINEMA_SDK}/lib/plugins/kf6/kirigami/platform" \
    /usr/lib/x86_64-linux-gnu/qt6/plugins/kf6/kirigami/platform \
    /usr/lib/qt6/plugins/kf6/kirigami/platform; do
    if [[ -d "${candidate}" ]] && compgen -G "${candidate}/*.so" >/dev/null; then
        KF6_KIRIGAMI_SRC="${candidate}"
        break
    fi
done
if [[ -n "${KF6_KIRIGAMI_SRC}" ]]; then
    KF6_KIRIGAMI_DST="${APPDIR}/usr/plugins/kf6/kirigami/platform"
    log "Bundling Kirigami platform plugins from ${KF6_KIRIGAMI_SRC}"
    mkdir -p "${KF6_KIRIGAMI_DST}"
    cp -a "${KF6_KIRIGAMI_SRC}/"*.so "${KF6_KIRIGAMI_DST}/"
    for so in "${KF6_KIRIGAMI_DST}"/*.so; do
        MANUAL_DEPLOY_ARGS+=(--deploy-deps-only "${so}")
    done
else
    echo "ERROR: required Kirigami platform plugin missing from SDK" >&2
    exit 3
fi

# OpenUrlJob also loads KIO workers at runtime (including MIME probing of
# remote URLs). Thread workers and the process fallback must both be portable.
mkdir -p "${APPDIR}/usr/plugins/kf6/kio" "${APPDIR}/usr/libexec/kf6"
cp -a "${KINEMA_SDK}/plugins/kf6/kio/"*.so "${APPDIR}/usr/plugins/kf6/kio/"
for helper in kioworker kioexec kiod6; do
    cp "${KINEMA_SDK}/lib/libexec/kf6/${helper}" "${APPDIR}/usr/libexec/kf6/"
    MANUAL_DEPLOY_ARGS+=(--deploy-deps-only "${APPDIR}/usr/libexec/kf6/${helper}")
done
for so in "${APPDIR}/usr/plugins/kf6/kio/"*.so; do
    MANUAL_DEPLOY_ARGS+=(--deploy-deps-only "${so}")
done

"${LINUXDEPLOY}" --appdir "${APPDIR}" "${MANUAL_DEPLOY_ARGS[@]}"

# linuxdeploy replaces AppRun with its own wrapper — restore ours.
cp "${REPO_ROOT}/packaging/appimage/AppRun.sh" "${APPDIR}/AppRun"
chmod +x "${APPDIR}/AppRun"

# Keep source provenance and third-party notices with both portable formats.
cp -a "${KINEMA_SDK}/share/kinema-sdk" "${APPDIR}/usr/share/"
cp "${RECIPE}/tools.lock.json" "${APPDIR}/usr/share/kinema-sdk/"
ln -sf dev.tlmtech.kinema.svg "${APPDIR}/.DirIcon"

log "Producing the AppImage → ${FINAL_APPIMAGE}"
ARCH=x86_64 VERSION="${VERSION}" "${APPIMAGETOOL}" \
    --runtime-file "${APPIMAGE_RUNTIME}" "${APPDIR}" "${FINAL_APPIMAGE}"

if [[ ! -f "${FINAL_APPIMAGE}" ]]; then
    echo "build-appimage.sh: expected ${FINAL_APPIMAGE} to exist after" \
         "linuxdeploy --output appimage, but it does not." >&2
    ls -la "${REPO_ROOT}" >&2
    exit 3
fi
mv -f "${FINAL_APPIMAGE}" "${DIST_DIR}/${FINAL_APPIMAGE}"
log "Produced ${DIST_DIR}/${FINAL_APPIMAGE}"
# The SDK's runtime must not hide newer ABI requirements in nested plugins.
python3 "${RECIPE}/validate-appimage.py" "${DIST_DIR}/${FINAL_APPIMAGE}" \
    --report "${DIST_DIR}/abi-report.json"

# ---------------------------------------------------------------------------
# 4. Capture the AppDir for the portable-tarball job to repackage.
#    Uncompressed tar preserves symlinks and avoids double-gzipping.
# ---------------------------------------------------------------------------
log "Capturing AppDir for the portable tarball job"
tar -cf "${DIST_DIR}/AppDir.tar" -C "${REPO_ROOT}" AppDir

log "Done."
ls -lh "${DIST_DIR}/"
