#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Desktop baseline + test harness only. Never install Qt/KDE, libmpv or PipeWire:
# their absence is part of the portability test.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
alsa=libasound2
if apt-cache show libasound2t64 >/dev/null 2>&1; then
    alsa=libasound2t64
fi
apt-get install -y --no-install-recommends \
    ca-certificates python3 binutils file desktop-file-utils appstream \
    dbus-x11 xvfb xauth xdotool x11-apps procps util-linux \
    libstdc++6 libgl1-mesa-dri libgl1 libegl1 libopengl0 libgbm1 libvulkan1 \
    libx11-6 libx11-xcb1 libxext6 libxfixes3 libxi6 libxrender1 libxrandr2 \
    libxcursor1 libxinerama1 libxss1 libsm6 libice6 libxkbcommon0 libxkbcommon-x11-0 \
    libxcb1 libxcb-cursor0 libxcb-glx0 libxcb-keysyms1 libxcb-image0 \
    libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-shm0 libxcb-sync1 \
    libxcb-xfixes0 libxcb-xinerama0 libxcb-xkb1 libxcb-util1 libxcb-icccm4 \
    libfontconfig1 libfreetype6 libudev1 "$alsa" libva2 libva-x11-2 libva-drm2 libvdpau1
