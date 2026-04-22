// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/PlaybackContext.h"

#include <QObject>
#include <QUrl>

class QTimer;

#include <QCoro/QCoroTask>

#include "core/MpvChapterList.h"
#include "core/MpvTrackList.h"

namespace kinema::api {
class CinemetaClient;
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::controllers {
class HistoryController;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui::player {
class PlayerWindow;
}

namespace kinema::controllers {

class PlaybackController : public QObject
{
    Q_OBJECT
public:
    PlaybackController(api::CinemetaClient& cinemeta,
        api::TorrentioClient& torrentio,
        services::StreamActions& actions,
        HistoryController& history,
        const config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);

    /// Wires / unwires the detached player window. Safe to call
    /// repeatedly with the same pointer or with null during teardown.
    void setPlayerWindow(ui::player::PlayerWindow* window);

public Q_SLOTS:
    /// Entry point for embedded playback. The controller remembers the
    /// active context so later end-of-file / error signals carry media
    /// identity even after the detached window has hidden itself.
    void play(const QUrl& url, const api::PlaybackContext& ctx);

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);
    void fileLoaded(const api::PlaybackContext& ctx);
    void playbackError(const QString& reason, const api::PlaybackContext& ctx);
    void endOfFile(const QString& reason, const api::PlaybackContext& ctx);
    void visibilityChanged(bool visible);

private Q_SLOTS:
    void onFileLoaded();
    void onPlaybackError(const QString& reason);
    void onEndOfFile(const QString& reason);
    void onPositionChanged(double seconds);
    void onDurationChanged(double seconds);
    void onTrackListChanged(const core::tracks::TrackList& tracks);
    void onChaptersChanged(const core::chapters::ChapterList& chapters);
    void onResumeAccepted();
    void onResumeDeclined();
    void onSkipRequested();
    void onNextEpisodeAccepted();
    void onNextEpisodeCancelled();
    void onNextEpisodeCountdownTick();

private:
    enum class Phase {
        Idle,
        Loading,
        ShowingResumePrompt,
        Playing,
        NearEnd,
    };

    api::CinemetaClient& m_cinemeta;
    api::TorrentioClient& m_torrentio;
    services::StreamActions& m_actions;
    HistoryController& m_history;
    const config::AppSettings& m_settings;
    const QString& m_rdToken;
    ui::player::PlayerWindow* m_window = nullptr;

    QCoro::Task<void> playNextEpisodeTask(api::PlaybackKey from);

    api::PlaybackContext m_ctx;
    api::PlaybackContext m_nextEpisodeCtx;
    api::Stream m_nextEpisodeStream;
    core::chapters::ChapterList m_chapters;
    qint64 m_pendingResumeSeconds = 0;
    Phase m_phase = Phase::Idle;
    double m_duration = 0.0;
    double m_skipChapterEnd = -1.0;
    int m_nextEpisodeCountdownRemaining = 0;
    QTimer* m_nextEpisodeTimer = nullptr;
    bool m_nextEpisodeTriggered = false;
    bool m_trackMemoryApplied = false;
    quint64 m_epoch = 0;
};

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
