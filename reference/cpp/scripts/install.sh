#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# Build and install Kinema into a user-local prefix (default: ~/.local)
# and refresh desktop integration caches.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

PREFIX="${PREFIX:-${HOME}/.local}"
BUILD_DIR="${BUILD_DIR:-build}"
BUILD_TYPE="${BUILD_TYPE:-RelWithDebInfo}"
JOBS="${JOBS:-$(nproc)}"

CLEAN=0
RECONFIGURE=0
RUN_TESTS=0
MPV_EMBED=1
# Tracks whether any flag was passed that changes the CMake configure
# step, so we re-run `cmake -B` instead of reusing a stale cache.
CONFIG_DIRTY=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Build Kinema and install it into a user-local prefix so it shows up in
the Plasma launcher. Defaults install to \$HOME/.local without sudo.

Options:
  -p, --prefix DIR     Install prefix (default: \$HOME/.local)
  -B, --build-dir DIR  Build directory (default: build)
  -j, --jobs N         Parallel build jobs (default: $(nproc))
  -d, --debug          Configure with CMAKE_BUILD_TYPE=Debug
      --release        Configure with CMAKE_BUILD_TYPE=Release
  -c, --clean          Remove the build directory before configuring
  -r, --reconfigure    Force a fresh CMake configure step
  -t, --test           Run ctest before installing
      --no-mpv-embed   Build without the embedded mpv player
  -h, --help           Show this help

Environment overrides: PREFIX, BUILD_DIR, BUILD_TYPE, JOBS

To remove an installation, run scripts/uninstall.sh.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -p|--prefix)      PREFIX="$2"; CONFIG_DIRTY=1; shift ;;
        -B|--build-dir)   BUILD_DIR="$2"; shift ;;
        -j|--jobs)        JOBS="$2"; shift ;;
        -d|--debug)       BUILD_TYPE="Debug"; CONFIG_DIRTY=1 ;;
        --release)        BUILD_TYPE="Release"; CONFIG_DIRTY=1 ;;
        -c|--clean)       CLEAN=1 ;;
        -r|--reconfigure) RECONFIGURE=1 ;;
        -t|--test)        RUN_TESTS=1 ;;
        --no-mpv-embed)   MPV_EMBED=0; CONFIG_DIRTY=1 ;;
        -h|--help)        usage; exit 0 ;;
        *)                echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
    shift
done

log()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!! \033[0m %s\n' "$*" >&2; }

refresh_caches() {
    local prefix="$1"
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${prefix}/share/applications" \
            >/dev/null 2>&1 || true
    fi
    # Bump the hicolor dir mtime so KIconLoader notices the new icon
    # even when no index.theme is present at the user prefix.
    if [[ -d "${prefix}/share/icons/hicolor" ]]; then
        touch "${prefix}/share/icons/hicolor" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${prefix}/share/icons/hicolor" \
            >/dev/null 2>&1 || true
    fi
    # --noincremental forces a full rebuild so stale .desktop / icon
    # entries from prior installs are dropped.
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
        kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
    fi
}

if [[ "${CLEAN}" -eq 1 && -d "${BUILD_DIR}" ]]; then
    log "Cleaning ${BUILD_DIR}/"
    rm -rf "${BUILD_DIR}"
fi

CMAKE_ARGS=(
    -B "${BUILD_DIR}" -S .
    -DCMAKE_BUILD_TYPE="${BUILD_TYPE}"
    -DCMAKE_INSTALL_PREFIX="${PREFIX}"
)
if [[ "${MPV_EMBED}" -eq 0 ]]; then
    CMAKE_ARGS+=(-DKINEMA_ENABLE_MPV_EMBED=OFF)
fi

# Detect cache drift so a previous build with a different prefix,
# build type, or mpv-embed setting doesn't silently win. Otherwise
# `cmake --install` may target the old prefix (e.g. /usr) and fail.
cache_value() {
    local key="$1" cache="${BUILD_DIR}/CMakeCache.txt"
    [[ -f "${cache}" ]] || return 0
    sed -n "s|^${key}:[^=]*=\(.*\)$|\1|p" "${cache}" | head -n1
}

if [[ -f "${BUILD_DIR}/CMakeCache.txt" ]]; then
    cached_prefix="$(cache_value CMAKE_INSTALL_PREFIX)"
    cached_type="$(cache_value CMAKE_BUILD_TYPE)"
    cached_mpv="$(cache_value KINEMA_ENABLE_MPV_EMBED)"
    want_mpv=$([[ "${MPV_EMBED}" -eq 1 ]] && echo ON || echo OFF)

    if [[ -n "${cached_prefix}" && "${cached_prefix}" != "${PREFIX}" ]]; then
        log "Cache prefix (${cached_prefix}) differs from ${PREFIX}; reconfiguring"
        CONFIG_DIRTY=1
    fi
    if [[ -n "${cached_type}" && "${cached_type}" != "${BUILD_TYPE}" ]]; then
        log "Cache build type (${cached_type}) differs from ${BUILD_TYPE}; reconfiguring"
        CONFIG_DIRTY=1
    fi
    if [[ -n "${cached_mpv}" && "${cached_mpv}" != "${want_mpv}" ]]; then
        log "Cache KINEMA_ENABLE_MPV_EMBED (${cached_mpv}) differs from ${want_mpv}; reconfiguring"
        CONFIG_DIRTY=1
    fi
fi

if [[ "${RECONFIGURE}" -eq 1 \
      || "${CONFIG_DIRTY}" -eq 1 \
      || ! -f "${BUILD_DIR}/CMakeCache.txt" ]]; then
    log "Configuring (${BUILD_TYPE}, prefix=${PREFIX}) in ${BUILD_DIR}/"
    cmake "${CMAKE_ARGS[@]}"
fi

log "Building with ${JOBS} job(s)"
cmake --build "${BUILD_DIR}" -j"${JOBS}"

if [[ "${RUN_TESTS}" -eq 1 ]]; then
    log "Running tests"
    ctest --test-dir "${BUILD_DIR}" --output-on-failure
fi

log "Installing to ${PREFIX}"
cmake --install "${BUILD_DIR}"

# For non-system prefixes the Plasma session's PATH often does not
# include ${PREFIX}/bin, so `Exec=kinema %U` resolves to nothing when
# launched from the menu. Rewrite Exec= to the absolute binary path
# so the .desktop entry works regardless of session PATH.
DESKTOP_FILE="${PREFIX}/share/applications/dev.tlmtech.kinema.desktop"
if [[ -f "${DESKTOP_FILE}" && "${PREFIX}" != "/usr" && "${PREFIX}" != "/usr/local" ]]; then
    log "Patching Exec= in ${DESKTOP_FILE} to use absolute binary path"
    # Use | as sed delimiter so slashes in the path don't need escaping.
    sed -i "s|^Exec=kinema |Exec=${PREFIX}/bin/kinema |" "${DESKTOP_FILE}"
fi

log "Refreshing desktop integration caches"
refresh_caches "${PREFIX}"

MANIFEST="${BUILD_DIR}/install_manifest.txt"
BIN_PATH="${PREFIX}/bin/kinema"

case ":${PATH}:" in
    *":${PREFIX}/bin:"*) ;;
    *) warn "${PREFIX}/bin is not on PATH. Add:
  export PATH=\"${PREFIX}/bin:\$PATH\"" ;;
esac

# ~/.local/share is part of XDG_DATA_DIRS by spec, so only warn for
# non-default prefixes the session may not know about.
if [[ "${PREFIX}" != "${HOME}/.local" ]]; then
    case ":${XDG_DATA_DIRS:-}:" in
        *":${PREFIX}/share:"*) ;;
        *) warn "${PREFIX}/share is not in XDG_DATA_DIRS. Add:
  export XDG_DATA_DIRS=\"${PREFIX}/share:\${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\"" ;;
    esac
fi

log "Done."
printf '    Prefix:   %s\n' "${PREFIX}"
printf '    Binary:   %s\n' "${BIN_PATH}"
printf '    Manifest: %s\n' "${REPO_ROOT}/${MANIFEST}"
printf '    Uninstall: ./scripts/uninstall.sh\n'
