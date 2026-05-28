// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlayerLoadWatchdog.h"

namespace kinema::playback::session {

PlayerLoadWatchdog::PlayerLoadWatchdog(QObject* parent)
    : QObject(parent)
{
    m_timer.setSingleShot(true);
    connect(&m_timer, &QTimer::timeout, this, [this] {
        Q_EMIT timedOut();
    });
}

void PlayerLoadWatchdog::setTimeout(std::chrono::milliseconds timeout)
{
    m_timeout = timeout;
}

void PlayerLoadWatchdog::start()
{
    m_timer.start(m_timeout);
}

void PlayerLoadWatchdog::stop()
{
    m_timer.stop();
}

} // namespace kinema::playback::session
