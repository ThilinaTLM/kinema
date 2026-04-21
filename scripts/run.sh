#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# Configure, build, (optionally) test, and run Kinema in one go.

set -euo pipefail

# Resolve repo root (parent of the directory containing this script)
# so the script works no matter where it's invoked from.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

BUILD_DIR="${BUILD_DIR:-build}"
BUILD_TYPE="${BUILD_TYPE:-RelWithDebInfo}"
JOBS="${JOBS:-$(nproc)}"

RUN_TESTS=0
CLEAN=0
NO_RUN=0
RECONFIGURE=0
APP_ARGS=()

usage() {
    cat <<EOF
Usage: $(basename "$0") [options] [-- app args...]

Options:
  -t, --test           Run ctest after building
  -c, --clean          Remove the build directory before configuring
  -r, --reconfigure    Force a fresh CMake configure step
  -n, --no-run         Build only; don't launch kinema
  -d, --debug          Use CMAKE_BUILD_TYPE=Debug
      --release        Use CMAKE_BUILD_TYPE=Release
  -B, --build-dir DIR  Build directory (default: build)
  -j, --jobs N         Parallel build jobs (default: $(nproc))
  -h, --help           Show this help

Environment overrides: BUILD_DIR, BUILD_TYPE, JOBS

Anything after '--' is forwarded as arguments to the kinema binary.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|--test)        RUN_TESTS=1 ;;
        -c|--clean)       CLEAN=1 ;;
        -r|--reconfigure) RECONFIGURE=1 ;;
        -n|--no-run)      NO_RUN=1 ;;
        -d|--debug)       BUILD_TYPE="Debug" ;;
        --release)        BUILD_TYPE="Release" ;;
        -B|--build-dir)   BUILD_DIR="$2"; shift ;;
        -j|--jobs)        JOBS="$2"; shift ;;
        -h|--help)        usage; exit 0 ;;
        --)               shift; APP_ARGS=("$@"); break ;;
        *)                echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
    shift
done

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

if [[ "${CLEAN}" -eq 1 && -d "${BUILD_DIR}" ]]; then
    log "Cleaning ${BUILD_DIR}/"
    rm -rf "${BUILD_DIR}"
fi

if [[ "${RECONFIGURE}" -eq 1 || ! -f "${BUILD_DIR}/CMakeCache.txt" ]]; then
    log "Configuring (${BUILD_TYPE}) in ${BUILD_DIR}/"
    cmake -B "${BUILD_DIR}" -S . -DCMAKE_BUILD_TYPE="${BUILD_TYPE}"
fi

log "Building with ${JOBS} job(s)"
cmake --build "${BUILD_DIR}" -j"${JOBS}"

if [[ "${RUN_TESTS}" -eq 1 ]]; then
    log "Running tests"
    ctest --test-dir "${BUILD_DIR}" --output-on-failure
fi

if [[ "${NO_RUN}" -eq 1 ]]; then
    log "Build complete (skipping run)"
    exit 0
fi

BIN="${BUILD_DIR}/bin/kinema"
if [[ ! -x "${BIN}" ]]; then
    echo "error: ${BIN} not found or not executable" >&2
    exit 1
fi

log "Launching ${BIN} ${APP_ARGS[*]:-}"
exec "./${BIN}" "${APP_ARGS[@]}"
