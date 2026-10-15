// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <memory>
#include <optional>

namespace kinema::playback::adapters {
class ExternalPlayerAdapter;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::ports {
class EmbeddedPlayerPort;
class PlayerPort;
} // namespace kinema::playback::ports

namespace kinema::playback::resume {
class ResumeUseCase;
}

namespace kinema::playback::series {
class SeriesSessionService;
}

namespace kinema::playback::transfer {
class TransferUseCase;
}

namespace kinema::playback::session {

class PlaybackSession;

/**
 * Session-centric playback orchestrator. Every user-visible play
 * request enters here, receives a stable PlaybackSessionId, and then
 * drives source resolution, transfer opening, player load and command
 * routing through typed playback events.
 */
class PlaybackSessionManager : public QObject
{
    Q_OBJECT
public:
    PlaybackSessionManager(events::PlaybackEventStream& eventStream,
                           transfer::TransferUseCase& transfers,
                           ports::EmbeddedPlayerPort* embeddedAdapter,
                           adapters::ExternalPlayerAdapter* externalAdapter,
                           QObject* parent = nullptr);
    /// Test/command-only constructor. `play()`/`download()` fail with a
    /// status message unless a transfer use-case is supplied by the full
    /// constructor, but transport methods remain usable for projections.
    PlaybackSessionManager(events::PlaybackEventStream& eventStream,
                           ports::EmbeddedPlayerPort* embeddedAdapter,
                           adapters::ExternalPlayerAdapter* externalAdapter,
                           QObject* parent = nullptr);
    ~PlaybackSessionManager() override;

    PlaybackSession* activeSession() const noexcept { return m_session.get(); }
    PlaybackSessionId activeSessionId() const noexcept;

    void setResumeUseCase(resume::ResumeUseCase* resume) noexcept;
    void setSeriesSessionService(series::SeriesSessionService* series) noexcept;

public Q_SLOTS:
    virtual void play(const domain::Stream& stream, const domain::PlaybackContext& ctx);
    void playWithBackend(const domain::Stream& stream,
                         const domain::PlaybackContext& ctx,
                         domain::DownloadBackendKind backend);
    void download(const domain::Stream& stream, const domain::PlaybackContext& ctx);
    void downloadWithBackend(const domain::Stream& stream,
                             const domain::PlaybackContext& ctx,
                             domain::DownloadBackendKind backend);

    virtual void pause();
    virtual void resume();
    virtual void playPause();
    virtual void stop();

    virtual void seekRelativeSeconds(double seconds);
    virtual void seekAbsoluteSeconds(double seconds);
    virtual void setVolumePercent(double percent);
    virtual void setPlaybackRate(double factor);
    virtual void selectAudioTrack(int id);
    virtual void selectSubtitleTrack(int id);
    virtual void attachSubtitle(const QString& localPath, const QString& language = {});
    virtual void playNextEpisode();
    virtual void playPreviousEpisode();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);
    void activeSessionChanged(bool active);
    void playbackStateChanged();
    void positionChanged(double seconds);
    void durationChanged(double seconds);
    void pausedChanged(bool paused);
    void navigationChanged();
    void visibilityChanged(bool visible);

private:
    void supersedeActiveSession();
    domain::PlaybackContext effectiveContext(const domain::Stream& stream,
                                             const domain::PlaybackContext& ctx) const;
    void startPlay(const domain::Stream& stream,
                   const domain::PlaybackContext& ctx,
                   std::optional<domain::DownloadBackendKind> backendOverride);
    QCoro::Task<void> playTask(PlaybackSessionId sessionId,
                               domain::Stream stream,
                               domain::PlaybackContext ctx,
                               std::optional<domain::DownloadBackendKind> backendOverride);
    void stampAdapters(const domain::PlaybackContext& ctx);
    ports::PlayerPort* commandPlayer() const noexcept;
    void onEvent(const events::PlaybackEvent& event);

    events::PlaybackEventStream& m_eventStream;
    transfer::TransferUseCase* m_transfers{};
    ports::EmbeddedPlayerPort* m_embeddedAdapter;
    adapters::ExternalPlayerAdapter* m_externalAdapter;
    resume::ResumeUseCase* m_resume{};
    series::SeriesSessionService* m_series{};
    std::unique_ptr<PlaybackSession> m_session;
};

} // namespace kinema::playback::session
