// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/adapters/EmbeddedMpvPlayerAdapter.h"

#ifdef KINEMA_HAVE_LIBMPV

#include "playback/events/PlaybackEventStream.h"
#include "ui/player/PlayerWindow.h"

namespace kinema::playback::adapters {

EmbeddedMpvPlayerAdapter::EmbeddedMpvPlayerAdapter(
    events::PlaybackEventStream& eventStream,
    QObject* parent)
    : QObject(parent)
    , m_eventStream(eventStream)
{
    connect(&m_loadWatchdog, &session::PlayerLoadWatchdog::timedOut,
        this, &EmbeddedMpvPlayerAdapter::onLoadWatchdogTimedOut);
}

EmbeddedMpvPlayerAdapter::~EmbeddedMpvPlayerAdapter() = default;

ui::player::PlayerWindow* EmbeddedMpvPlayerAdapter::playerWindow() const noexcept
{
    return m_window.data();
}

void EmbeddedMpvPlayerAdapter::setPlayerWindow(ui::player::PlayerWindow* window)
{
    if (m_window.data() == window) {
        return;
    }
    disconnectWindow();
    m_window = window;
    connectWindow();
    if (!window) {
        m_loadWatchdog.stop();
        m_loadfileInFlight = false;
    }
}

void EmbeddedMpvPlayerAdapter::connectWindow()
{
    if (!m_window) {
        return;
    }
    connect(m_window.data(), &ui::player::PlayerWindow::fileLoaded,
        this, &EmbeddedMpvPlayerAdapter::onFileLoaded);
    connect(m_window.data(), &ui::player::PlayerWindow::endOfFile,
        this, &EmbeddedMpvPlayerAdapter::onEndOfFile);
    connect(m_window.data(), &ui::player::PlayerWindow::mpvError,
        this, &EmbeddedMpvPlayerAdapter::onMpvError);
    connect(m_window.data(), &ui::player::PlayerWindow::positionChanged,
        this, &EmbeddedMpvPlayerAdapter::onPositionChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::durationChanged,
        this, &EmbeddedMpvPlayerAdapter::onDurationChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::pausedChanged,
        this, &EmbeddedMpvPlayerAdapter::onPausedChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::volumeChanged,
        this, &EmbeddedMpvPlayerAdapter::onVolumeChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::speedChanged,
        this, &EmbeddedMpvPlayerAdapter::onSpeedChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::trackListChanged,
        this, &EmbeddedMpvPlayerAdapter::onTrackListChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::chaptersChanged,
        this, &EmbeddedMpvPlayerAdapter::onChaptersChanged);
    connect(m_window.data(), &ui::player::PlayerWindow::userClosedWindow,
        this, &EmbeddedMpvPlayerAdapter::onUserClosedWindow);
}

void EmbeddedMpvPlayerAdapter::disconnectWindow()
{
    if (m_window) {
        disconnect(m_window.data(), nullptr, this, nullptr);
    }
}

void EmbeddedMpvPlayerAdapter::setActiveSession(PlaybackSessionId sessionId,
    const domain::PlaybackContext& ctx)
{
    // If a session was active and we're being given a new one, mpv
    // will be replacing the file. Flag the loadfile so the
    // resulting "stop" end-file is filtered.
    if (m_sessionActive && !sessionId.isNull() && sessionId != m_sessionId) {
        m_loadfileInFlight = true;
    }
    m_sessionId = sessionId;
    m_ctx = ctx;
    m_sessionActive = !sessionId.isNull();
}

bool EmbeddedMpvPlayerAdapter::isAvailable() const
{
    return m_window != nullptr;
}

void EmbeddedMpvPlayerAdapter::play(const QUrl& url,
    const domain::PlaybackContext& ctx,
    std::optional<qint64> resumeSeconds)
{
    if (!m_window) {
        m_eventStream.publish(events::PlaybackFailed {
            m_sessionId,
            QStringLiteral("Embedded player window is not available"),
            ctx,
        });
        return;
    }
    domain::PlaybackContext effectiveCtx = ctx;
    if (resumeSeconds.has_value()) {
        effectiveCtx.resumeSeconds = *resumeSeconds;
    }
    // Re-stamp from the call so the adapter is robust if the
    // session forgot to setActiveSession() first.
    if (m_sessionActive) {
        m_ctx = effectiveCtx;
    }
    m_eventStream.publish(events::PlayerLoading { m_sessionId });
    m_loadWatchdog.start();
    m_window->play(url, effectiveCtx);
}

void EmbeddedMpvPlayerAdapter::pause()
{
    if (m_window) m_window->setPaused(true);
}

void EmbeddedMpvPlayerAdapter::resume()
{
    if (m_window) m_window->setPaused(false);
}

void EmbeddedMpvPlayerAdapter::togglePause()
{
    if (m_window) m_window->togglePause();
}

void EmbeddedMpvPlayerAdapter::stop()
{
    m_loadWatchdog.stop();
    if (m_window) {
        m_window->stopAndHide();
    }
    if (m_sessionActive) {
        m_eventStream.publish(events::PlaybackEnded {
            m_sessionId,
            PlaybackEndReason::UserStop,
            m_ctx,
        });
        m_sessionActive = false;
        m_loadfileInFlight = false;
    }
}

void EmbeddedMpvPlayerAdapter::seekRelative(double seconds)
{
    if (m_window) m_window->seekRelative(seconds);
}

void EmbeddedMpvPlayerAdapter::seekAbsolute(double seconds)
{
    if (m_window) m_window->seekAbsolute(seconds);
}

void EmbeddedMpvPlayerAdapter::setVolumePercent(double percent)
{
    if (m_window) m_window->setVolumePercent(percent);
}

void EmbeddedMpvPlayerAdapter::setPlaybackRate(double factor)
{
    if (m_window) m_window->setSpeed(factor);
}

void EmbeddedMpvPlayerAdapter::selectAudioTrack(int id)
{
    if (m_window) m_window->setAudioTrack(id);
}

void EmbeddedMpvPlayerAdapter::selectSubtitleTrack(int id)
{
    if (m_window) m_window->setSubtitleTrack(id);
}

bool EmbeddedMpvPlayerAdapter::attachSubtitleFile(const QString& /*localPath*/,
    const QString& /*language*/)
{
    if (!m_window || !m_sessionActive) {
        return false;
    }
    // The actual sub-add command runs through the chrome
    // view-model; SubtitleSessionService (Phase 9) will route this
    // call. Returning true here would lie about the attachment;
    // returning false until that wiring is in place is the safer
    // default.
    return false;
}

ports::PlayerSnapshot EmbeddedMpvPlayerAdapter::snapshot() const
{
    ports::PlayerSnapshot s;
    s.active = m_sessionActive;
    s.paused = m_paused;
    s.positionSec = m_position;
    s.durationSec = m_duration;
    s.volumePercent = m_volumePercent;
    s.playbackRate = m_playbackRate;
    return s;
}

void EmbeddedMpvPlayerAdapter::onFileLoaded()
{
    m_loadWatchdog.stop();
    m_loadfileInFlight = false;
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(events::PlayerLoaded { m_sessionId });
}

std::optional<PlaybackEndReason>
EmbeddedMpvPlayerAdapter::classifyEndReason(const QString& reason)
{
    const QString r = reason.trimmed().toLower();
    // mpv reports `stop` when a fresh loadfile aborts the
    // currently-loading file. From the session's point of view
    // this is the previous attempt being replaced; suppress it
    // entirely so projections (history, series) don't see a
    // spurious termination of the new attempt.
    if (m_loadfileInFlight && r == QStringLiteral("stop")) {
        return std::nullopt;
    }
    if (r == QStringLiteral("eof")) {
        return PlaybackEndReason::NaturalEof;
    }
    if (r == QStringLiteral("stop") || r == QStringLiteral("quit")) {
        return PlaybackEndReason::UserStop;
    }
    if (r == QStringLiteral("error")) {
        return PlaybackEndReason::PlayerError;
    }
    return PlaybackEndReason::NaturalEof;
}

void EmbeddedMpvPlayerAdapter::onEndOfFile(const QString& reason)
{
    m_loadWatchdog.stop();
    if (!m_sessionActive) {
        return;
    }
    auto classified = classifyEndReason(reason);
    if (!classified.has_value()) {
        // Filtered side-effect of loadfile replacement; the new
        // attempt is still in flight.
        return;
    }
    m_eventStream.publish(events::PlaybackEnded {
        m_sessionId,
        *classified,
        m_ctx,
    });
    m_sessionActive = false;
}

void EmbeddedMpvPlayerAdapter::onMpvError(const QString& message)
{
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(events::PlaybackFailed {
        m_sessionId,
        message,
        m_ctx,
    });
}

void EmbeddedMpvPlayerAdapter::onPositionChanged(double seconds)
{
    m_position = seconds;
    if (!m_sessionActive) return;
    m_eventStream.publish(events::PositionTicked { m_sessionId, seconds });
}

void EmbeddedMpvPlayerAdapter::onDurationChanged(double seconds)
{
    m_duration = seconds;
    if (!m_sessionActive) return;
    m_eventStream.publish(events::DurationChanged { m_sessionId, seconds });
}

void EmbeddedMpvPlayerAdapter::onPausedChanged(bool paused)
{
    m_paused = paused;
    if (!m_sessionActive) return;
    m_eventStream.publish(events::PlaybackStateChanged {
        m_sessionId, /*playing=*/!paused, paused, /*buffering=*/false,
    });
}

void EmbeddedMpvPlayerAdapter::onVolumeChanged(double percent)
{
    m_volumePercent = percent;
}

void EmbeddedMpvPlayerAdapter::onSpeedChanged(double factor)
{
    m_playbackRate = factor;
}

void EmbeddedMpvPlayerAdapter::onTrackListChanged(
    const core::tracks::TrackList& tracks)
{
    if (!m_sessionActive) return;
    m_eventStream.publish(events::TrackListChanged { m_sessionId, tracks });
}

void EmbeddedMpvPlayerAdapter::onChaptersChanged(
    const core::chapters::ChapterList& chapters)
{
    if (!m_sessionActive) return;
    m_eventStream.publish(events::ChapterListChanged {
        m_sessionId, chapters });
}

void EmbeddedMpvPlayerAdapter::onUserClosedWindow()
{
    if (!m_sessionActive) {
        return;
    }
    m_loadWatchdog.stop();
    m_eventStream.publish(events::PlaybackEnded {
        m_sessionId,
        PlaybackEndReason::UserStop,
        m_ctx,
    });
    m_sessionActive = false;
    m_loadfileInFlight = false;
}

void EmbeddedMpvPlayerAdapter::onLoadWatchdogTimedOut()
{
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(events::PlaybackEnded {
        m_sessionId,
        PlaybackEndReason::LoadTimeout,
        m_ctx,
    });
    m_sessionActive = false;
    m_loadfileInFlight = false;
}

} // namespace kinema::playback::adapters

#endif // KINEMA_HAVE_LIBMPV
