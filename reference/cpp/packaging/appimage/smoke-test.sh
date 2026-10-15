#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# For clean disposable hosts only; runs trusted build artifacts without credentials.
set -euo pipefail
RECIPE="$(cd "$(dirname "$0")" && pwd)"
if [[ "${1:-}" == --session ]]; then
    launcher="$2"
    reports="$3"
    mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
    chmod 700 "$XDG_RUNTIME_DIR"
    "$launcher" --help > "$reports/help.log" 2>&1
    "$launcher" --version > "$reports/version.log" 2>&1
    project_version="$(sed -n 's/^    VERSION \([0-9.]*\)$/\1/p' "$RECIPE/../../CMakeLists.txt")"
    grep -F "$project_version" "$reports/version.log"
    setsid "$launcher" > "$reports/startup.log" 2>&1 &
    pid=$!
    trap 'kill -- -"$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
    window=""
    for ((i=0; i<60; i++)); do
        kill -0 "$pid" || { cat "$reports/startup.log"; exit 1; }
        window="$(xdotool search --onlyvisible --class kinema 2>/dev/null | head -n1 || true)"
        [[ -z "$window" ]] || break
        sleep 1
    done
    [[ -n "$window" ]] || { echo 'No visible application window'; cat "$reports/startup.log"; exit 1; }
    sleep 15
    kill -0 "$pid"
    xwd -silent -id "$window" -out "$reports/window.xwd"
    if [[ "${KINEMA_SMOKE_REQUIRE_FUSE:-0}" == 1 ]]; then
        window_pid="$(xdotool getwindowpid "$window")"
        executable="$(readlink -f "/proc/$window_pid/exe")"
        filesystem="$(findmnt -n -T "$executable" -o FSTYPE)"
        printf 'Executable filesystem: %s\n' "$filesystem" > "$reports/runtime-mount.log"
        [[ "$filesystem" == fuse* ]] || { echo 'AppImage did not launch from a FUSE mount'; exit 1; }
    fi
    if grep -E 'error while loading shared libraries|version .+ not found|QQmlApplicationEngine failed|module .+ is not installed|Cannot load library|is not a type' "$reports/startup.log"; then
        exit 1
    fi
    exit 0
fi

artifact="$(realpath "${1:?Usage: smoke-test.sh AppImage report-directory [--fuse]}")"
reports="$(realpath -m "${2:?Missing report directory}")"
mkdir -p "$reports"
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
mkdir -p "$temporary/relocated payload"
cp "$artifact" "$temporary/relocated payload/Kinema.AppImage"
artifact="$temporary/relocated payload/Kinema.AppImage"
chmod +x "$artifact"
(cd "$temporary/relocated payload" && "$artifact" --appimage-extract >/dev/null)
appdir="$temporary/relocated payload/squashfs-root"
python3 "$RECIPE/validate-appimage.py" "$appdir" --dependencies --report "$reports/abi.json"
export HOME="$temporary/home" XDG_CONFIG_HOME="$temporary/config" XDG_DATA_HOME="$temporary/data"
export XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" XDG_RUNTIME_DIR="$temporary/run"
export LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=xcb
unset LD_LIBRARY_PATH LD_PRELOAD QT_PLUGIN_PATH QML_IMPORT_PATH QML2_IMPORT_PATH
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# Launch through the runtime, with no FUSE privileges in the container matrix.
if [[ "${3:-}" != --fuse ]]; then
    export APPIMAGE_EXTRACT_AND_RUN=1
else
    unset APPIMAGE_EXTRACT_AND_RUN
    export KINEMA_SMOKE_REQUIRE_FUSE=1
fi
portable="$(realpath "${4:?Pass the portable tarball as argument 4}")"
mkdir -p "$reports/appimage" "$reports/portable" "$temporary/portable payload"
tar -xf "$portable" -C "$temporary/portable payload"
mapfile -t launchers < <(find "$temporary/portable payload" -name kinema.sh -type f)
[[ "${#launchers[@]}" == 1 ]] || { echo 'Expected one portable launcher'; exit 1; }
cd /tmp
# Each invocation gets a private X server; window lookup works even when the
# AppImage runtime forks to manage extraction/mount cleanup.
dbus-run-session -- xvfb-run -a bash "$RECIPE/smoke-test.sh" --session "$artifact" "$reports/appimage"
unset KINEMA_SMOKE_REQUIRE_FUSE
dbus-run-session -- xvfb-run -a bash "$RECIPE/smoke-test.sh" --session "${launchers[0]}" "$reports/portable"
LD_LIBRARY_PATH="$appdir/usr/lib:$appdir/usr/lib/x86_64-linux-gnu" \
    python3 "$RECIPE/smoke-media.py" "$appdir" > "$reports/media.log" 2>&1
