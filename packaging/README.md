# Kinema packaging

This directory drives `.github/workflows/release.yml`. The release pipeline
fires on tag push (`vX.Y.Z`, `vX.Y.Z-rc1`, …) and produces:

| Artifact                                          | Built where               | Notes |
|---------------------------------------------------|---------------------------|-------|
| `kinema-X.Y.Z.tar.gz`                             | `ubuntu-latest`           | `git archive` of the tag |
| `Kinema-X.Y.Z-x86_64.AppImage`                    | Ubuntu 22.04 source SDK  | bundles Qt6/KF6/qcoro/qtkeychain/libmpv/mpvqt/libtorrent/ssl |
| `kinema-X.Y.Z-x86_64.tar.gz`                      | same as AppImage          | portable: extracted AppDir + launcher script |
| `kinema_X.Y.Z_amd64-ubuntu25.04.deb`              | `ubuntu:25.04` container  | CPack/`dpkg-shlibdeps` |
| `kinema_X.Y.Z_amd64-debian13.deb`                 | `debian:trixie` container | CPack/`dpkg-shlibdeps` |
| `kinema-X.Y.Z-1.fc41.x86_64.rpm`                  | `fedora:41` container     | CPack/`rpmbuild --autoreq` |
| `kinema-X.Y.Z-1.fc42.x86_64.rpm`                  | `fedora:42` container     | CPack/`rpmbuild --autoreq` |
| `SHA256SUMS` (+ `SHA256SUMS.asc` if signed)       | `ubuntu-latest`           | aggregated last |

Pre-release tags (containing a hyphen, e.g. `vX.Y.Z-rc1`) produce a **draft**
release. Final tags publish directly.

## Why no native Ubuntu 24.04 LTS package

Kinema requires:

- Qt ≥ 6.6 (Ubuntu 24.04 ships 6.4)
- KDE Frameworks 6 (Ubuntu 24.04 ships KF5 only)
- QCoro ≥ 0.10
- libmpv ≥ 0.36 plus the Qt 6 MpvQt wrapper

These constraints are checked at `cmake` time (`find_package(... REQUIRED)`),
so a build using only 24.04's stock packages fails at configure. The new
**AppImage** and **portable tarball** pipeline builds modern dependencies
from source on Ubuntu 22.04, targeting glibc ≥ 2.35 instead. Older trixie-built
releases remain incompatible with older hosts; extracting them cannot fix ABI
requirements. The minimums above are application requirements, not a complete
inventory of each Ubuntu release's packages.

## TMDB token

Official releases ship with `KINEMA_TMDB_DEFAULT_TOKEN=""` — the binary has no
embedded TMDB v4 read token. TMDB's terms of service prohibit redistribution
of a personal read token in a publicly downloadable artifact. Users paste
their own token in **Settings → TMDB (Discover)**.

Distro maintainers who want a different default (e.g. a TMDB-issued bot
token allocated for their distro) can re-configure with
`-DKINEMA_TMDB_DEFAULT_TOKEN=…` and rebuild.

## Embedded mpv player

All four package types ship with `KINEMA_ENABLE_MPV_EMBED=ON`. The AppImage
bundles libmpv + mpvqt + the ffmpeg/libplacebo/libass tail; the native deb/rpm
link to the system `libmpv2` / `mpv-libs` (which transitively pulls in the
codec stack). Even with embedded playback, users can still launch an external
mpv or VLC from Kinema's Settings — those binaries are listed as
`Recommends` (deb) / soft dep (rpm).

## Signing

Checksum signing is opt-in. Add two repo-level secrets to enable it:

- `GPG_PRIVATE_KEY` — armored ASCII export of the signing key
  (`gpg --armor --export-secret-key <key-id>`)
- `GPG_PASSPHRASE` — passphrase for that key

When both are set, the release job produces `SHA256SUMS.asc` alongside
`SHA256SUMS`. When absent, only the plain `SHA256SUMS` is uploaded.

No apt/dnf repository signing is currently in scope; the release page itself
is the trust anchor.

## Local reproduction

The matrix jobs are designed to run in any environment with the container
image and Docker / Podman. To reproduce a build locally:

```sh
# .deb on Debian trixie
docker run --rm -it -v "$PWD":/src -w /src debian:trixie bash -lc '
  apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates git pkg-config file cmake ninja-build g++ \
      extra-cmake-modules gettext \
      qt6-base-dev qt6-base-private-dev qt6-declarative-dev \
      qt6-tools-dev qt6-svg-dev \
      libkf6kio-dev libkf6i18n-dev libkf6config-dev \
      libkf6notifications-dev libkf6statusnotifieritem-dev \
      libkf6coreaddons-dev libkirigami-dev \
      qcoro-qt6-dev qtkeychain-qt6-dev \
      libmpv-dev libmpvqt-dev \
      libtorrent-rasterbar-dev libboost-dev libssl-dev
  cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX=/usr -DBUILD_TESTING=OFF \
      -DCPACK_GENERATOR=DEB
  cmake --build build -j"$(nproc)"
  (cd build && cpack -G DEB --verbose)
'
```

```sh
# No FUSE, privileged container, or newer-distro binary dependencies needed.
docker build -t kinema-appimage-sdk packaging/appimage
mkdir -p dist
docker run --rm -v "$PWD":/src -w /src -e VERSION=X.Y.Z \
    kinema-appimage-sdk bash packaging/appimage/build-appimage.sh
```

Native apt/dnf lists live in `.github/workflows/release.yml`. AppImage SDK
packages and source versions live in `appimage/Dockerfile` and
`appimage/dependencies.lock.json`; do not substitute newer distro binaries.
The AppImage workflow shows portable-tarball creation from `dist/AppDir.tar`.

## AppImage compatibility

Starting with the upcoming **0.5.0** release, the baseline is
**Ubuntu 22.04 x86_64 / glibc 2.35**, using GCC 11. The release
job calls `.github/workflows/appimage.yml` and waits for its complete matrix,
including normal FUSE launch. A downloaded artifact from a failed build/test
run is **not** a validated release. The catalog's own acceptance still requires
a published-release retest: [AppImage catalog PR 7587](https://github.com/AppImage/appimage.github.io/pull/7587).

The SDK locks source archives (including needed submodules) and packaging tools
by SHA-256; cached downloads are verified too. Qt/KDE/media dependencies are
built against the old baseline. Neither glibc nor a newer C++ runtime is bundled.
Docker/BuildKit caches the SDK independently of application source changes.
Cold builds are expensive; `JOBS` controls SDK build parallelism (default 4).
Do not use `-march=native` or change the host baseline to speed up a build.

`validate-appimage.py` checks the final image's structure, metadata, every
nested ELF object's required symbol versions, relocatable search paths, and
mandatory plugins/QML modules. `baseline-cxx.json` records the version definitions
exported by Ubuntu 22.04's updated `libstdc++.so.6`, obtained with
`readelf -W --version-info`; only **definitions**, not required versions, belong
in that file. Do not raise it to make an incompatible bundle pass.

Clean Ubuntu 22.04/24.04 and Debian 13 containers install only the desktop
baseline and test harness from `install-test-deps.sh`. In them, validation with
`--dependencies` also checks `ldd` resolution against `host-libraries.json`.
This list documents permitted host libraries; adding a library requires proving
it exists on every supported baseline, not merely on the SDK host. Never install
Qt/KDE, mpv or PipeWire in a test image to hide bundle omissions.

For an already-built AppImage and portable tarball, reproduce the clean-host test:

```sh
docker run --rm -v "$PWD":/src -w /src ubuntu:22.04 bash -c '
  bash packaging/appimage/install-test-deps.sh
  chmod +x dist/*.AppImage
  bash packaging/appimage/smoke-test.sh dist/*.AppImage reports --extract dist/*.tar.gz
'
```

The smoke test uses disposable HOME/XDG directories, private D-Bus/Xvfb, software
OpenGL, a relocated path containing spaces and an unrelated working directory.
It requires a visible window and process survival, checks loader/QML errors,
exercises the real AppImage runtime and portable launcher, and decodes local audio
through bundled libmpv. Reports include ABI requirements, logs and an XWD screenshot.
It does not prove real GPU rendering, Wayland integration, or physical audio output.

Before releasing:

1. Review and update locked dependency versions for security fixes; fetch the
   exact archives, verify upstream provenance/checksums, update SHA-256 pins,
   and rebuild the SDK from scratch. Never point locks at moving branches.
2. Run `python3 tests/test_appimage_packaging.py`, the standard CMake/CTest suite,
   and the complete compatibility workflow. Retain the test reports.
3. Review bundled third-party licenses/source availability. The bundle contains
   `usr/share/kinema-sdk/` with locked upstream source URLs/checksums, notices and
   baseline package provenance; retain/provide corresponding sources as required
   by each license. Kinema's source tarball alone is not the source for its SDK.
4. Manually test Wayland and embedded video playback with real graphics/audio
   hardware. This remains necessary even when headless checks pass.
5. Publish the newly validated artifact, then comment `/retest` on PR 7587.
   Inspect its result before claiming catalog compatibility. Local checks do not
   substitute for the external catalog test.

## Adding a new distro target

1. Append a matrix entry under the relevant job (`deb` or `rpm`) in
   `.github/workflows/release.yml`. Pick a stable `image:` tag (the
   Docker Hub `:NN` tag, not `:latest`) and a short `tag:` / `fcver:`
   that gets baked into the artifact filename.
2. Confirm the distro ships the required dep floor (Qt 6.6+, KF6,
   qcoro ≥ 0.10, libmpv ≥ 0.36, libtorrent-rasterbar 2.0). If any dep
   is missing, the matrix entry doesn't belong — recommend AppImage instead.
3. Verify the dep package names. Debian-family uses a mix of `libkf6*-dev`
   (kio/i18n/config/notifications/coreaddons/statusnotifieritem),
   `libkirigami-dev` (no kf6 prefix), `qcoro-qt6-dev`, `qtkeychain-qt6-dev`,
   `libmpvqt-dev`. Fedora uses `kf6-*-devel`, `qcoro-qt6-devel`,
   `qtkeychain-qt6-devel`, `mpvqt-devel`. Cross-check on
   packages.debian.org / packages.fedoraproject.org before pushing.
4. Add a row to the artifact table at the top of this file.

## File layout

```
packaging/
├── README.md                   ← this file
├── cpack.cmake                 ← included by ../CMakeLists.txt
└── appimage/
    ├── AppRun.sh               ← AppImage entrypoint / portable-tarball launcher
    ├── Dockerfile             ← Ubuntu 22.04 dependency SDK
    ├── dependencies.lock.json ← source versions, hashes and build options
    ├── tools.lock.json        ← pinned deployment tools/runtime
    ├── build-dependencies.sh  ← source SDK builder
    ├── build-appimage.sh      ← AppImage build orchestrator
    ├── validate-appimage.py   ← metadata, structure, ABI and dependency audit
    └── smoke-test.sh          ← clean-host runtime tests
```
