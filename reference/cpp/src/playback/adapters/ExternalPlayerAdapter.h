// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"
#include "playback/ports/PlayerPort.h"

#include "domain/PlaybackContext.h"

#include <QObject>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::core {
class PlayerLauncher;
namespace player {
enum class Kind;
}
} // namespace kinema::core

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::adapters {

/**
 * `PlayerPort` adapter over `core::PlayerLauncher` (mpv / VLC /
 * custom-command external players).
 *
 * External players are opaque to Kinema: we spawn them detached
 * and have no IPC channel back. The adapter therefore publishes
 * only the boundary events it can observe:
 *
 *   - `PlayerLoaded` on `PlayerLauncher::launched(kind, title)`
 *     for the active session;
 *   - `PlaybackFailed` on `PlayerLauncher::launchFailed(...)`;
 *   - `PlaybackEnded(UserStop)` on `stop()`.
 *
 * No position, duration, track, or buffering events fire. Most
 * transport methods are no-ops; the embedded adapter is the only
 * one that can drive mpv.
 *
 * The session-id stamping is done via `setActiveSession(...)`
 * called by `PlaybackSession` just before `play()`, because
 * `PlayerPort::play()` itself doesn't carry a session id.
 */
class ExternalPlayerAdapter : public QObject, public ports::PlayerPort
{
    Q_OBJECT
public:
    ExternalPlayerAdapter(core::PlayerLauncher& launcher,
        events::PlaybackEventStream& eventStream,
        QObject* parent = nullptr);
    ~ExternalPlayerAdapter() override;

    /// Stamp future events for this session. Called by
    /// `PlaybackSession` immediately before `play()`.
    void setActiveSession(PlaybackSessionId sessionId,
        const domain::PlaybackContext& ctx);

    // ---------------------- PlayerPort ----------------------
    bool isAvailable() const override;
    void play(const QUrl& url,
        const domain::PlaybackContext& ctx,
        std::optional<qint64> resumeSeconds = std::nullopt) override;
    void pause() override;
    void resume() override;
    void togglePause() override;
    void stop() override;
    void seekRelative(double seconds) override;
    void seekAbsolute(double seconds) override;
    void setVolumePercent(double percent) override;
    void setPlaybackRate(double factor) override;
    void selectAudioTrack(int id) override;
    void selectSubtitleTrack(int id) override;
    bool attachSubtitleFile(const QString& localPath,
        const QString& language) override;
    ports::PlayerSnapshot snapshot() const override;

private Q_SLOTS:
    void onLaunched(kinema::core::player::Kind kind, const QString& title);
    void onLaunchFailed(kinema::core::player::Kind kind,
        const QString& reason);

private:
    core::PlayerLauncher& m_launcher;
    events::PlaybackEventStream& m_eventStream;
    PlaybackSessionId m_sessionId;
    domain::PlaybackContext m_ctx;
    bool m_sessionActive = false;
};

} // namespace kinema::playback::adapters
