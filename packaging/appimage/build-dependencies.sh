#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail
RECIPE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${KINEMA_SDK:-/opt/kinema-sdk}"
JOBS="${CMAKE_BUILD_PARALLEL_LEVEL:-4}"
WORK="${SDK_BUILD_DIR:-/tmp/kinema-sdk-build}"
CACHE="${SDK_SOURCE_CACHE:-/tmp/kinema-sdk-sources}"
mkdir -p "$WORK" "$PREFIX/share/kinema-sdk/licenses"
export PATH="$PREFIX/bin:$PATH"
export CMAKE_PREFIX_PATH="$PREFIX"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:$PREFIX/share/pkgconfig"
export LD_LIBRARY_PATH="$PREFIX/lib"
export CFLAGS="-O2 -march=x86-64 -mtune=generic"
export CXXFLAGS="$CFLAGS"
export CMAKE_BUILD_PARALLEL_LEVEL="$JOBS"
export PYTHONPATH="$PREFIX/lib/python3.10/site-packages:${PYTHONPATH:-}"

while IFS= read -r entry; do
    name="$(jq -r .name <<<"$entry")"
    # Optional package names let Docker cache expensive dependency groups.
    if [[ "$#" -gt 0 && " $* " != *" $name "* ]]; then
        continue
    fi
    kind="$(jq -r .kind <<<"$entry")"
    archive="$(python3 "$RECIPE/fetch-source.py" "$RECIPE/dependencies.lock.json" "$name" "$CACHE")"
    echo "==> Building $name"
    src="$WORK/$name"
    rm -rf "$src"
    mkdir -p "$src"
    tar -xf "$archive" --strip-components=1 -C "$src"
    while IFS= read -r source; do
        nested_name="$(jq -r .name <<<"$source")"
        subdir="$(jq -r .subdir <<<"$source")"
        nested_archive="$(python3 "$RECIPE/fetch-source.py" "$RECIPE/dependencies.lock.json" "$nested_name" "$CACHE")"
        mkdir -p "$src/$subdir"
        tar -xf "$nested_archive" --strip-components=1 -C "$src/$subdir"
    done < <(jq -c '.sources[]?' <<<"$entry")
    mapfile -t args < <(jq -r '.args[]' <<<"$entry")
    case "$kind" in
        bootstrap)
            (cd "$src" && ./bootstrap --prefix="$PREFIX" --parallel="$JOBS" -- -DCMAKE_USE_OPENSSL=ON && make -j"$JOBS" && make install)
            ;;
        meson-tool)
            (cd "$src" && python3 setup.py install --prefix="$PREFIX" \
                --single-version-externally-managed --record="$src/install.txt")
            # Jammy's python does not include a custom prefix's site-packages.
            site="$(find "$PREFIX/lib" -type d -name site-packages -print -quit)"
            export PYTHONPATH="${site}:${PYTHONPATH:-}"
            ;;
        cmake)
            cmake -S "$src" -B "$src/build" -G Ninja \
                -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$PREFIX" \
                -DCMAKE_INSTALL_LIBDIR=lib -DKDE_INSTALL_LIBDIR=lib \
                -DKDE_INSTALL_USE_QT_SYS_PATHS=ON \
                -DBUILD_TESTING=OFF -DBUILD_SHARED_LIBS=ON "${args[@]}"
            cmake --build "$src/build" --parallel "$JOBS"
            cmake --install "$src/build"
            ;;
        meson)
            meson setup "$src/build" "$src" --prefix="$PREFIX" --libdir=lib \
                --buildtype=release --wrap-mode=nodownload "${args[@]}"
            meson compile -C "$src/build" -j "$JOBS"
            meson install -C "$src/build"
            ;;
        boost)
            (cd "$src" && ./bootstrap.sh --prefix="$PREFIX" --with-libraries=system && \
                ./b2 -j"$JOBS" variant=release link=shared install)
            ;;
        ffmpeg)
            (cd "$src" && ./configure --prefix="$PREFIX" --libdir="$PREFIX/lib" \
                --enable-shared --disable-static --disable-doc --disable-debug \
                --enable-gpl --enable-version3 --enable-libdav1d --enable-openssl && \
                make -j"$JOBS" && make install)
            ;;
        *) echo "Unknown build recipe: $kind" >&2; exit 1 ;;
    esac
    # Preserve notices with their source-relative paths, avoiding name collisions.
    notice="$PREFIX/share/kinema-sdk/licenses/$name"
    mkdir -p "$notice"
    (cd "$src" && find . -path ./build -prune -o -type f \
        \( -iname 'copying*' -o -iname 'license*' -o -iname 'copyright*' -o -iname 'notice*' \) \
        -exec cp --parents '{}' "$notice/" \;)
    rm -rf "$src"
done < <(jq -c '.[]' "$RECIPE/dependencies.lock.json")
cp "$RECIPE/dependencies.lock.json" "$PREFIX/share/kinema-sdk/"
dpkg-query -W -f='${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n' \
    > "$PREFIX/share/kinema-sdk/baseline-packages.txt"
mkdir -p "$PREFIX/share/kinema-sdk/licenses/baseline"
(cd /usr/share/doc && find . -name copyright -type f \
    -exec cp --parents '{}' "$PREFIX/share/kinema-sdk/licenses/baseline/" \;)
rm -rf "$WORK"
# A caller-supplied source cache is reusable across dependency groups.
if [[ -z "${SDK_SOURCE_CACHE:-}" ]]; then
    rm -rf "$CACHE"
fi
