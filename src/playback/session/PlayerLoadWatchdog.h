// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QTimer>

#include <chrono>

namespace kinema::playback::session {

/**
 * Single-shot timer that fires `timedOut` when a player has not
 * reported either a successful load or a terminal end-of-file
 * within the configured deadline.
 *
 * Owned by `PlaybackSession` (and, transitionally, by the legacy
 * `controllers::PlaybackController`); armed when a `play()` is
 * issued, disarmed on first observation of the player's response.
 *
 * Extracted as its own type so the timeout policy can be tested
 * without spinning up the rest of the playback graph.
 */
class PlayerLoadWatchdog : public QObject
{
    Q_OBJECT
public:
    explicit PlayerLoadWatchdog(QObject* parent = nullptr);

    /// Default deadline used by `start()` when none has been set.
    /// Picked to comfortably exceed mpv's `network-timeout` so the
    /// watchdog only fires when the underlying player has not
    /// surfaced a failure in time.
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
    /// having been called. Single-shot — the watchdog disarms
    /// itself before the signal is emitted.
    void timedOut();

private:
    QTimer m_timer;
    std::chrono::milliseconds m_timeout = kDefaultTimeout;
};

} // namespace kinema::playback::session
