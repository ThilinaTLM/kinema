// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QObject>
#include <QString>

#include <memory>
#include <optional>

namespace kinema::services {
class StreamActions;
}

namespace kinema::controllers {
class PlaybackController;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::session {

class PlaybackSession;

/**
 * User-facing playback orchestrator.
 *
 * This is the long-term replacement for `services::StreamActions`'
 * play paths and `controllers::PlaybackController`'s embedded
 * transport API. During the refactor it forwards to those
 * collaborators; Phase 6 will move the actual playback orchestration
 * here and the inner collaborators will lose their public API.
 *
 * UI code (QML, view-models) is expected to depend on this class
 * — not on `StreamActions` or `PlaybackController` directly — so
 * that swapping the inner implementation later is a one-place
 * change.
 */
class PlaybackSessionManager : public QObject
{
    Q_OBJECT
public:
    PlaybackSessionManager(services::StreamActions& actions,
        events::PlaybackEventStream& eventStream,
        controllers::PlaybackController* embedded,
        QObject* parent = nullptr);
    ~PlaybackSessionManager() override;

    /// Currently active session, or nullptr if no play attempt is
    /// in flight. Exposed for projections (history, series) that
    /// need to associate inbound events with the current attempt
    /// during the transitional phase.
    PlaybackSession* activeSession() const noexcept { return m_session.get(); }

    /// Id of the active session, or a null QUuid when there is no
    /// active attempt.
    PlaybackSessionId activeSessionId() const noexcept;

public Q_SLOTS:
    void play(const domain::Stream& stream,
        const domain::PlaybackContext& ctx);
    void playWithBackend(const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadBackendKind backend);
    void download(const domain::Stream& stream,
        const domain::PlaybackContext& ctx);
    void downloadWithBackend(const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadBackendKind backend);

    void pause();
    void resume();
    void playPause();
    void stop();

    void seekRelativeSeconds(double seconds);
    void seekAbsoluteSeconds(double seconds);
    void setVolumePercent(double percent);
    void setPlaybackRate(double factor);

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    void supersedeActiveSession();

    services::StreamActions& m_actions;
    events::PlaybackEventStream& m_eventStream;
    controllers::PlaybackController* m_embedded;
    std::unique_ptr<PlaybackSession> m_session;
};

} // namespace kinema::playback::session
