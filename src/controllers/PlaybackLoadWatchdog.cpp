// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/PlaybackLoadWatchdog.h"

#ifdef KINEMA_HAVE_LIBMPV

namespace kinema::controllers {

PlaybackLoadWatchdog::PlaybackLoadWatchdog(QObject* parent)
    : QObject(parent)
{
    m_timer.setSingleShot(true);
    connect(&m_timer, &QTimer::timeout, this, [this] {
        Q_EMIT timedOut();
    });
}

void PlaybackLoadWatchdog::setTimeout(std::chrono::milliseconds timeout)
{
    m_timeout = timeout;
}

void PlaybackLoadWatchdog::start()
{
    m_timer.start(m_timeout);
}

void PlaybackLoadWatchdog::stop()
{
    m_timer.stop();
}

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
