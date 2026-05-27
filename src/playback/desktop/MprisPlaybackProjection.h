// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVariantMap>

#include <memory>

namespace kinema::core {
class IdleInhibitor;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::ports {
class PlayerPort;
}

namespace kinema::playback::session {
class PlaybackSessionManager;
}

namespace kinema::playback::series {
class SeriesSessionService;
}

namespace kinema::playback::desktop {

/**
 * Desktop MPRIS projection over the playback subsystem.
 *
 * Replaces `controllers::MprisController`. Drives `PlaybackStatus`
 * / `Metadata` / `Position` / `Volume` / `Rate` / `CanGo*` off the
 * `PlaybackEventStream`, sends transport commands into
 * `PlaybackSessionManager`, and owns the registration of
 * `org.mpris.MediaPlayer2.kinema` at `/org/mpris/MediaPlayer2`
 * (two adaptors: `org.mpris.MediaPlayer2` and
 * `org.mpris.MediaPlayer2.Player`).
 *
 * Inputs:
 *   - `PlaybackEventStream& events`: subscription source for
 *     `PlaybackRequested`, `PlayerLoaded`, `PlaybackStateChanged`,
 *     `PositionTicked`, `DurationChanged`, `PlaybackEnded`,
 *     `PlaybackFailed`.
 *   - `PlaybackSessionManager& sessions`: transport sink
 *     (pause, resume, playPause, stop, seekRelative,
 *     seekAbsolute, setVolumePercent, setPlaybackRate).
 *   - `PlayerPort* player` (optional): authoritative snapshot for
 *     `Position` / `Volume` / `Rate`. When null the projection
 *     uses its own event-derived cached state. Production wires
 *     the embedded adapter.
 *   - `SeriesSessionService* series` (optional): authority for
 *     `CanGoNext` / `CanGoPrevious` + `Next` / `Previous`.
 *
 * The projection owns a `core::IdleInhibitor` that reacts to
 * `PlaybackStateChanged` (active when playing, off when paused /
 * ended / failed).
 *
 * D-Bus registration is best-effort: failures are logged and the
 * service degrades gracefully (in-process state still tracked,
 * tests still observe it). MPRIS Properties are mirrored on every
 * meaningful event via a `PropertiesChanged` signal on the player
 * interface so external clients (e.g. `playerctl`) see live
 * updates.
 */
class MprisPlaybackProjection : public QObject
{
    Q_OBJECT
public:
    MprisPlaybackProjection(events::PlaybackEventStream& events,
        session::PlaybackSessionManager& sessions,
        ports::PlayerPort* player,
        series::SeriesSessionService* series,
        QObject* parent = nullptr);
    ~MprisPlaybackProjection() override;

    // Root interface
    QString identity() const;
    QString desktopEntry() const;
    QStringList supportedUriSchemes() const;
    QStringList supportedMimeTypes() const;

    // Player interface - properties
    QString playbackStatus() const;
    QVariantMap metadata() const;
    qlonglong positionUs() const;
    double volume() const;
    double rate() const;
    double minimumRate() const noexcept { return 0.25; }
    double maximumRate() const noexcept { return 4.0; }
    bool canGoNext() const;
    bool canGoPrevious() const;
    bool canPlay() const;
    bool canPause() const;
    bool canSeek() const;
    bool canControl() const;

    QString currentTrackObjectPath() const;

    /// Whether an active session is being tracked. Driven by
    /// `PlaybackRequested` (true) and terminal events
    /// (`PlaybackEnded`/`PlaybackFailed`, false).
    bool hasActiveSession() const noexcept { return m_sessionActive; }

    /// True when the projection is observed as `Playing`: an active
    /// session with `playing && !paused`. Used by the idle
    /// inhibitor and tests.
    bool isActivelyPlaying() const noexcept;

    /// Whether the MPRIS service name is currently held. Best-effort
    /// - D-Bus failures (no session bus, name already owned) are
    /// logged and the projection runs unregistered.
    bool isServiceRegistered() const noexcept { return m_serviceRegistered; }

    // Root interface - commands
    void raise();
    void quit();

    // Player interface - commands
    void next();
    void previous();
    void pause();
    void playPause();
    void stop();
    void play();
    void seek(qlonglong offsetUs);
    void setPosition(const QString& trackPath, qlonglong positionUs);
    void setVolume(double value);
    void setRate(double value);

Q_SIGNALS:
    /// MPRIS `Raise()` arrived. The shell wires this to bring the
    /// main / player window to the foreground.
    void raiseRequested();
    /// MPRIS `Quit()` arrived. The shell wires this to its quit
    /// path.
    void quitRequested();

private:
    void onEvent(const events::PlaybackEvent& e);
    void onPlaybackRequested(const events::PlaybackRequested& e);
    void onPlayerLoaded(const events::PlayerLoaded& e);
    void onPlaybackStateChanged(const events::PlaybackStateChanged& e);
    void onPositionTicked(const events::PositionTicked& e);
    void onDurationChanged(const events::DurationChanged& e);
    void onPlaybackEnded(const events::PlaybackEnded& e);
    void onPlaybackFailed(const events::PlaybackFailed& e);
    void onSeriesNavigationChanged();

    void ensureObjectRegistered();
    void setServiceRegistered(bool on);
    void emitPropertiesChanged();
    void emitSeeked(double seconds);
    void refreshIdleInhibitor();

    events::PlaybackEventStream& m_events;
    session::PlaybackSessionManager& m_sessions;
    ports::PlayerPort* m_player = nullptr;
    series::SeriesSessionService* m_series = nullptr;

    std::unique_ptr<core::IdleInhibitor> m_inhibitor;

    // Event-derived cached state. Used when m_player is null and
    // as the source of truth for properties between snapshots.
    PlaybackSessionId m_sessionId;
    domain::PlaybackContext m_ctx;
    bool m_sessionActive = false;
    bool m_paused = false;
    bool m_playing = false;
    double m_position = 0.0;
    double m_duration = 0.0;

    bool m_objectRegistered = false;
    bool m_serviceRegistered = false;
};

} // namespace kinema::playback::desktop

#endif // KINEMA_HAVE_LIBMPV
