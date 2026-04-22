#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# Remove every file a previous `scripts/install.sh` placed on disk,
# using the CMake-generated install_manifest.txt as the source of
# truth. User config and keyring tokens are left untouched.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

BUILD_DIR="${BUILD_DIR:-build}"
MANIFEST=""
DRY_RUN=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Uninstall Kinema using the CMake install manifest.

Options:
  -B, --build-dir DIR  Build directory (default: build)
  -m, --manifest PATH  Path to install_manifest.txt (overrides --build-dir)
  -n, --dry-run        List what would be removed without deleting
  -h, --help           Show this help

Environment overrides: BUILD_DIR
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -B|--build-dir) BUILD_DIR="$2"; shift ;;
        -m|--manifest)  MANIFEST="$2"; shift ;;
        -n|--dry-run)   DRY_RUN=1 ;;
        -h|--help)      usage; exit 0 ;;
        *)              echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
    shift
done

log()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!! \033[0m %s\n' "$*" >&2; }
err()  { printf '\033[1;31mxx \033[0m %s\n' "$*" >&2; }

refresh_caches() {
    local prefix="$1"
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${prefix}/share/applications" \
            >/dev/null 2>&1 || true
    fi
    if [[ -d "${prefix}/share/icons/hicolor" ]]; then
        touch "${prefix}/share/icons/hicolor" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${prefix}/share/icons/hicolor" \
            >/dev/null 2>&1 || true
    fi
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
        kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
    fi
}

if [[ -z "${MANIFEST}" ]]; then
    MANIFEST="${BUILD_DIR}/install_manifest.txt"
fi

if [[ ! -f "${MANIFEST}" ]]; then
    err "install manifest not found: ${MANIFEST}"
    err "Re-run scripts/install.sh first, or pass --manifest PATH."
    exit 1
fi

# Derive the install prefix from the manifest so cache refresh and
# the empty-dir sweep stay scoped correctly.
first="$(sed -n '1p' "${MANIFEST}")"
case "${first}" in
    */bin/kinema)              PREFIX="${first%/bin/kinema}" ;;
    */share/applications/*)    PREFIX="${first%/share/applications/*}" ;;
    */share/metainfo/*)        PREFIX="${first%/share/metainfo/*}" ;;
    */share/icons/*)           PREFIX="${first%/share/icons/*}" ;;
    */share/knotifications6/*) PREFIX="${first%/share/knotifications6/*}" ;;
    *)                         PREFIX="$(dirname "${first}")" ;;
esac

log "Using manifest: ${MANIFEST}"
log "Detected prefix: ${PREFIX}"

if [[ "${DRY_RUN}" -eq 1 ]]; then
    log "Dry run — the following files would be removed:"
    while IFS= read -r f; do
        [[ -z "${f}" || "${f}" == \#* ]] && continue
        printf '    %s\n' "${f}"
    done < "${MANIFEST}"
    exit 0
fi

removed=0
missing=0
declare -A seen_dirs

while IFS= read -r f; do
    [[ -z "${f}" || "${f}" == \#* ]] && continue
    if [[ -e "${f}" || -L "${f}" ]]; then
        rm -f -- "${f}"
        log "removed ${f}"
        removed=$((removed + 1))
    else
        warn "missing ${f}"
        missing=$((missing + 1))
    fi
    seen_dirs["$(dirname "${f}")"]=1
done < "${MANIFEST}"

# Sweep empty parent directories, bounded by the prefix so we never
# touch anything outside the install tree.
for d in "${!seen_dirs[@]}"; do
    while [[ "${d}" == "${PREFIX}"/* && "${d}" != "${PREFIX}" ]]; do
        if rmdir --ignore-fail-on-non-empty -- "${d}" 2>/dev/null; then
            [[ -d "${d}" ]] && break
            d="$(dirname "${d}")"
        else
            break
        fi
    done
done

log "Refreshing desktop integration caches"
refresh_caches "${PREFIX}"

log "Done."
printf '    Removed: %d file(s)\n' "${removed}"
if [[ "${missing}" -gt 0 ]]; then
    printf '    Missing: %d (already gone)\n' "${missing}"
fi
printf '    Note:    user config (~/.config/kinemarc) and any stored\n'
printf '             Real-Debrid/TMDB tokens in the system keyring\n'
printf '             were left untouched.\n'
