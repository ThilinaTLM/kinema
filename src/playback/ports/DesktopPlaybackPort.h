// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"
#include "playback/ports/PlayerPort.h"

namespace kinema::playback::ports {

/**
 * Port over the desktop integrations (MPRIS, tray, idle inhibitor).
 *
 * `MprisPlaybackProjection` is the production implementation;
 * tests/headless builds can install a no-op fake.
 */
class DesktopPlaybackPort
{
public:
    virtual ~DesktopPlaybackPort() = default;

    virtual void onContextChanged(const domain::PlaybackContext& ctx) = 0;
    virtual void onSnapshotChanged(const PlayerSnapshot& snapshot) = 0;
    virtual void onPlaybackEnded() = 0;
};

} // namespace kinema::playback::ports
