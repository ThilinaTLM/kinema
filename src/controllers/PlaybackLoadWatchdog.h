// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QObject>
#include <QTimer>

#include <chrono>

namespace kinema::controllers {

/**
 * Single-shot timer that fires `timedOut` when an mpv `loadfile`
 * does not produce either `fileLoaded` or `endOfFile` within the
 * configured deadline.
 *
 * Owned by `PlaybackController`; armed in `play()`, disarmed in
 * `onFileLoaded()` / `onEndOfFile()` / `stop()` and on player
 * detach. Extracted as its own type so the timeout policy can be
 * tested without spinning up the rest of the controller graph.
 */
class PlaybackLoadWatchdog : public QObject
{
    Q_OBJECT
public:
    explicit PlaybackLoadWatchdog(QObject* parent = nullptr);

    /// Default deadline used by `start()` when none has been set.
    /// Picked to comfortably exceed mpv's `network-timeout` so the
    /// watchdog only fires when mpv itself has not surfaced a
    /// failure in time.
    static constexpr std::chrono::milliseconds kDefaultTimeout {
        std::chrono::seconds(75)
    };

    void setTimeout(std::chrono::milliseconds timeout);
    std::chrono::milliseconds timeout() const noexcept { return m_timeout; }

    /// Arm the watchdog. Restarts the timer if already running.
    void start();

    /// Disarm the watchdog. Safe to call when not running.
    void stop();

    bool isActive() const noexcept { return m_timer.isActive(); }

Q_SIGNALS:
    /// Fired when the configured deadline elapses without `stop()`
    /// having been called. Single-shot \u2014 the watchdog disarms
    /// itself before the signal is emitted.
    void timedOut();

private:
    QTimer m_timer;
    std::chrono::milliseconds m_timeout = kDefaultTimeout;
};

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
