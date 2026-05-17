# Kinema packaging

This directory drives `.github/workflows/release.yml`. The release pipeline
fires on tag push (`v0.1.0`, `v0.1.0-rc1`, …) and produces:

| Artifact                                          | Built where               | Notes |
|---------------------------------------------------|---------------------------|-------|
| `kinema-X.Y.Z.tar.gz`                             | `ubuntu-latest`           | `git archive` of the tag |
| `Kinema-X.Y.Z-x86_64.AppImage`                    | `debian:trixie` container | bundles Qt6/KF6/qcoro/qtkeychain/libmpv/mpvqt/libtorrent/ssl |
| `kinema-X.Y.Z-x86_64.tar.gz`                      | same as AppImage          | portable: extracted AppDir + launcher script |
| `kinema_X.Y.Z_amd64-ubuntu25.04.deb`              | `ubuntu:25.04` container  | CPack/`dpkg-shlibdeps` |
| `kinema_X.Y.Z_amd64-debian13.deb`                 | `debian:trixie` container | CPack/`dpkg-shlibdeps` |
| `kinema-X.Y.Z-1.fc41.x86_64.rpm`                  | `fedora:41` container     | CPack/`rpmbuild --autoreq` |
| `kinema-X.Y.Z-1.fc42.x86_64.rpm`                  | `fedora:42` container     | CPack/`rpmbuild --autoreq` |
| `SHA256SUMS` (+ `SHA256SUMS.asc` if signed)       | `ubuntu-latest`           | aggregated last |

Pre-release tags (containing a hyphen, e.g. `v0.1.0-rc1`) produce a **draft**
release. Final tags publish directly.

## Why no Ubuntu 24.04 LTS

Kinema requires:

- Qt ≥ 6.6 (Ubuntu 24.04 ships 6.4)
- KDE Frameworks 6 (Ubuntu 24.04 ships KF5 only)
- QCoro ≥ 0.10 (Ubuntu 24.04 ships 0.8)
- libmpv ≥ 0.36 (Ubuntu 24.04 ships 0.35)

These constraints are checked at `cmake` time (`find_package(... REQUIRED)`),
so a forced build on 24.04 would fail at configure. Users on 24.04 should run
the **AppImage** or the **portable tarball**, both of which bundle the full
Qt6/KF6 stack and only rely on host glibc ≥ 2.41, libGL/libGLX, and Wayland/X11
client libraries.

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
      libkf6coreaddons-dev libkf6kirigami-dev \
      libqcoro6-dev qt6keychain-dev \
      libmpv-dev libmpvqt6-dev \
      libtorrent-rasterbar-dev libboost-dev libssl-dev
  cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX=/usr -DBUILD_TESTING=OFF \
      -DCPACK_GENERATOR=DEB
  cmake --build build -j"$(nproc)"
  (cd build && cpack -G DEB --verbose)
'
```

```sh
# AppImage on Debian trixie (needs --privileged for FUSE)
docker run --rm -it --privileged -v "$PWD":/src -w /src debian:trixie bash -lc '
  apt-get update && apt-get install -y --no-install-recommends wget fuse libfuse2t64 \
      ...  # full list mirrored from .github/workflows/release.yml
  VERSION=0.1.0 bash packaging/appimage/build-appimage.sh
'
```

The full apt/dnf install lists live in `.github/workflows/release.yml` and
are the source of truth — keep them in sync with this README when adding
new dependencies.

## Adding a new distro target

1. Append a matrix entry under the relevant job (`deb` or `rpm`) in
   `.github/workflows/release.yml`. Pick a stable `image:` tag (the
   Docker Hub `:NN` tag, not `:latest`) and a short `tag:` / `fcver:`
   that gets baked into the artifact filename.
2. Confirm the distro ships the required dep floor (Qt 6.6+, KF6,
   qcoro ≥ 0.10, libmpv ≥ 0.36, libtorrent-rasterbar 2.0). If any dep
   is missing, the matrix entry doesn't belong — recommend AppImage instead.
3. Verify the dep package names (`libkf6kio-dev` on Debian-family,
   `kf6-kio-devel` on Fedora) and update the install step.
4. Add a row to the artifact table at the top of this file.

## File layout

```
packaging/
├── README.md                   ← this file
├── cpack.cmake                 ← included by ../CMakeLists.txt
└── appimage/
    ├── AppRun.sh               ← AppImage entrypoint / portable-tarball launcher
    └── build-appimage.sh       ← AppImage build orchestrator
```
