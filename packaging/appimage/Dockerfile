# SPDX-License-Identifier: Apache-2.0
# Build modern Qt/KDE on the catalog's oldest supported host, not on trixie.
FROM ubuntu:22.04 AS base
ARG DEBIAN_FRONTEND=noninteractive
ARG JOBS=4
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl git g++ gcc make cmake ninja-build pkg-config \
    python3 python3-setuptools python3-jinja2 python3-packaging jq \
    autoconf automake libtool gettext bison flex gperf perl nasm yasm \
    xz-utils bzip2 unzip patchelf binutils file wget desktop-file-utils appstream \
    libssl-dev libsqlite3-dev libdbus-1-dev libsystemd-dev libudev-dev \
    libgl1-mesa-dev libegl1-mesa-dev libgles2-mesa-dev libglu1-mesa-dev \
    libvulkan-dev glslang-dev spirv-tools libepoxy-dev \
    libx11-dev libx11-xcb-dev libxext-dev libxfixes-dev libxi-dev libxrender-dev \
    libxrandr-dev libxcursor-dev libxinerama-dev libxkbcommon-dev libxkbcommon-x11-dev \
    libxcb1-dev libxcb-cursor-dev libxcb-glx0-dev libxcb-keysyms1-dev \
    libxcb-image0-dev libxcb-randr0-dev libxcb-render-util0-dev libxcb-shape0-dev \
    libxcb-shm0-dev libxcb-sync-dev libxcb-xfixes0-dev libxcb-xinerama0-dev \
    libxcb-xkb-dev libxcb-util-dev libxcb-icccm4-dev libxcb-res0-dev \
    libfontconfig1-dev libfreetype6-dev libharfbuzz-dev libicu-dev \
    libjpeg-dev libpng-dev libtiff-dev libwebp-dev libopenjp2-7-dev \
    libzstd-dev zlib1g-dev liblzma-dev libbz2-dev libpcre2-dev \
    libffi-dev libxml2-dev libxslt1-dev libexpat1-dev \
    libacl1-dev libattr1-dev libmount-dev libblkid-dev libcap-dev \
    libpolkit-gobject-1-dev libsecret-1-dev libgcrypt20-dev libgpgme-dev \
    libgpgmepp-dev libassuan-dev libcanberra-dev libasound2-dev \
    libpulse-dev libpipewire-0.3-dev libpipewire-0.3-modules libspa-0.2-dev libdrm-dev libgbm-dev \
    libass-dev libdav1d-dev libva-dev libvdpau-dev libuchardet-dev \
    libbluray-dev libdvdnav-dev liblcms2-dev libxss-dev libarchive-dev libhunspell-dev \
    && rm -rf /var/lib/apt/lists/*
ENV LANG=C.UTF-8 \
    KINEMA_SDK=/opt/kinema-sdk \
    CMAKE_BUILD_PARALLEL_LEVEL=${JOBS} \
    PATH=/opt/kinema-sdk/bin:${PATH} \
    CMAKE_PREFIX_PATH=/opt/kinema-sdk \
    PKG_CONFIG_PATH=/opt/kinema-sdk/lib/pkgconfig:/opt/kinema-sdk/share/pkgconfig \
    LD_LIBRARY_PATH=/opt/kinema-sdk/lib
FROM base AS sdk
COPY dependencies.lock.json fetch-source.py build-dependencies.sh /opt/recipe/
RUN bash /opt/recipe/build-dependencies.sh cmake meson wayland wayland-protocols
RUN bash /opt/recipe/build-dependencies.sh qtbase qtshadertools qtdeclarative
RUN bash /opt/recipe/build-dependencies.sh qtsvg qtimageformats qttools qtwayland qt5compat ffmpeg qtmultimedia qtspeech
RUN bash /opt/recipe/build-dependencies.sh extra-cmake-modules kcoreaddons kconfig ki18n karchive kcodecs kwidgetsaddons kitemviews plasma-wayland-protocols kwindowsystem kguiaddons
RUN bash /opt/recipe/build-dependencies.sh kcolorscheme kconfigwidgets breeze-icons kiconthemes kdbusaddons kcrash kglobalaccel kcompletion kauth kservice knotifications kjobwidgets solid sonnet
RUN bash /opt/recipe/build-dependencies.sh ktextwidgets kxmlgui kbookmarks qca kwallet kio kitemmodels kirigami qqc2-desktop-style kstatusnotifieritem kirigami-addons
RUN bash /opt/recipe/build-dependencies.sh qcoro qtkeychain boost libtorrent vulkan-headers libplacebo mpv mpvqt
# Client config defaults are conffiles in Jammy's pipewire package. Keep this
# packaging-only addition after the expensive compiler/dependency layers.
RUN apt-get update && apt-get install -y --no-install-recommends pipewire \
    && dpkg-query -W -f='${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n' \
       > /opt/kinema-sdk/share/kinema-sdk/baseline-packages.txt \
    && mkdir -p /opt/kinema-sdk/share/kinema-sdk/licenses/baseline/pipewire \
    && cp /usr/share/doc/pipewire/copyright /opt/kinema-sdk/share/kinema-sdk/licenses/baseline/pipewire/ \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
