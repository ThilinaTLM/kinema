// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"
#include "playback/ports/PlayerPort.h"

#include <functional>

namespace kinema::playback::ports {

/** Embedded-player capabilities needed by playback orchestration. */
class EmbeddedPlayerPort : public PlayerPort
{
public:
    using VisibilityHandler = std::function<void(bool)>;
    using StatusHandler = std::function<void(const QString&, int)>;

    ~EmbeddedPlayerPort() override = default;

    virtual void setActiveSession(PlaybackSessionId sessionId,
                                  const domain::PlaybackContext& context) = 0;
    virtual void setVisibilityHandler(VisibilityHandler handler) = 0;
    virtual void setStatusHandler(StatusHandler handler) = 0;
};

} // namespace kinema::playback::ports
