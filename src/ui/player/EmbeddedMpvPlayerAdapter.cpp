// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/EmbeddedMpvPlayerAdapter.h"

#ifdef KINEMA_HAVE_LIBMPV

#include "config/AppSettings.h"
#include "config/PlayerSettings.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/policy/ChapterSkipPolicy.h"
#include "playback/policy/ResumePolicy.h"
#include "ui/player/PlayerViewModel.h"
#include "ui/player/PlayerWindow.h"

#include <KLocalizedString>

#include <utility>

namespace kinema::ui::player {

EmbeddedMpvPlayerAdapter::EmbeddedMpvPlayerAdapter(
    playback::events::PlaybackEventStream& eventStream,
    const config::PlayerSettings& settings,
    QObject* parent)
    : QObject(parent), m_eventStream(eventStream), m_settings(settings)
{
    connect(&m_loadWatchdog,
            &playback::session::PlayerLoadWatchdog::timedOut,
            this,
            &EmbeddedMpvPlayerAdapter::onLoadWatchdogTimedOut);
    connect(this, &EmbeddedMpvPlayerAdapter::visibilityChanged, this, [this](bool visible) {
        if (m_visibilityHandler)
            m_visibilityHandler(visible);
    });
    connect(this,
            &EmbeddedMpvPlayerAdapter::statusMessage,
            this,
            [this](const QString& text, int timeoutMs) {
                if (m_statusHandler)
                    m_statusHandler(text, timeoutMs);
            });
}

EmbeddedMpvPlayerAdapter::~EmbeddedMpvPlayerAdapter() = default;

void EmbeddedMpvPlayerAdapter::setVisibilityHandler(VisibilityHandler handler)
{
    m_visibilityHandler = std::move(handler);
}

void EmbeddedMpvPlayerAdapter::setStatusHandler(StatusHandler handler)
{
    m_statusHandler = std::move(handler);
}

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
        m_pendingResumeSeconds = 0;
        m_skipChapterEnd = -1.0;
        m_chapters.clear();
    }
}

void EmbeddedMpvPlayerAdapter::connectWindow()
{
    if (!m_window) {
        return;
    }
    connect(m_window.data(),
            &ui::player::PlayerWindow::fileLoaded,
            this,
            &EmbeddedMpvPlayerAdapter::onFileLoaded);
    connect(m_window.data(),
            &ui::player::PlayerWindow::endOfFile,
            this,
            &EmbeddedMpvPlayerAdapter::onEndOfFile);
    connect(m_window.data(),
            &ui::player::PlayerWindow::mpvError,
            this,
            &EmbeddedMpvPlayerAdapter::onMpvError);
    connect(m_window.data(),
            &ui::player::PlayerWindow::positionChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onPositionChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::durationChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onDurationChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::pausedChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onPausedChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::volumeChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onVolumeChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::speedChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onSpeedChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::trackListChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onTrackListChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::chaptersChanged,
            this,
            &EmbeddedMpvPlayerAdapter::onChaptersChanged);
    connect(m_window.data(),
            &ui::player::PlayerWindow::userClosedWindow,
            this,
            &EmbeddedMpvPlayerAdapter::onUserClosedWindow);
    connect(m_window.data(),
            &ui::player::PlayerWindow::visibilityChanged,
            this,
            &EmbeddedMpvPlayerAdapter::visibilityChanged);
    // User-action signals from the QML chrome: resume / skip
    // chapter / picker selections. The pickers route back into
    // the window's transport API so tests can stub PlayerWindow
    // without needing a libmpv instance.
    connect(m_window.data(),
            &ui::player::PlayerWindow::resumeAccepted,
            this,
            &EmbeddedMpvPlayerAdapter::onResumeAccepted);
    connect(m_window.data(),
            &ui::player::PlayerWindow::resumeDeclined,
            this,
            &EmbeddedMpvPlayerAdapter::onResumeDeclined);
    connect(m_window.data(),
            &ui::player::PlayerWindow::skipRequested,
            this,
            &EmbeddedMpvPlayerAdapter::onSkipRequested);
    connect(m_window.data(), &ui::player::PlayerWindow::audioPicked, this, [this](int aid) {
        if (m_window) {
            m_window->setAudioTrack(aid);
        }
    });
    connect(m_window.data(), &ui::player::PlayerWindow::subtitlePicked, this, [this](int sid) {
        if (m_window) {
            m_window->setSubtitleTrack(sid);
        }
    });
    connect(m_window.data(), &ui::player::PlayerWindow::speedPicked, this, [this](double s) {
        if (m_window) {
            m_window->setSpeed(s);
        }
    });
}

void EmbeddedMpvPlayerAdapter::disconnectWindow()
{
    if (m_window) {
        disconnect(m_window.data(), nullptr, this, nullptr);
    }
}

void EmbeddedMpvPlayerAdapter::setActiveSession(playback::PlaybackSessionId sessionId,
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
        m_eventStream.publish(playback::events::PlaybackFailed{
            m_sessionId,
            QStringLiteral("Embedded player window is not available"),
            ctx,
        });
        Q_EMIT statusMessage(i18nc("@info:status", "Embedded player is not available."), 6000);
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

    // Reset per-attempt UI state (resume prompt + chapter skip).
    m_pendingResumeSeconds = 0;
    m_skipChapterEnd = -1.0;
    m_chapters.clear();
    m_paused = false;
    m_position = 0.0;
    m_duration = 0.0;
    m_window->setLoadingVisible(true);
    m_window->hideSkipChapter();
    m_window->hideResumePrompt();

    // Resume policy: if the stored offset exceeds the user's prompt
    // threshold we defer to a prompt at file-loaded instead of
    // seeking blindly. The load itself goes in without a resume
    // hint so mpv starts from 0 and we land at the deferred seek
    // only after the user picks.
    if (effectiveCtx.resumeSeconds.has_value()
        && playback::policy::shouldShowResumePrompt(*effectiveCtx.resumeSeconds,
                                                    m_settings.resumePromptThresholdSec())) {
        m_pendingResumeSeconds = *effectiveCtx.resumeSeconds;
        effectiveCtx.resumeSeconds.reset();
    }

    if (!effectiveCtx.title.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Loading \u201c%1\u201d\u2026", effectiveCtx.title), 3000);
    }

    // Surface the playable URL on the event stream so probes
    // (moviehash) and projections (subtitles) can subscribe
    // without a direct dependency on the adapter. The assetId is
    // not known here at the adapter boundary; downstream
    // consumers that only care about the URL (e.g. MoviehashProbe)
    // tolerate an empty value.
    m_eventStream.publish(playback::events::PlayableUrlReady{
        m_sessionId,
        /*assetId=*/QString{},
        url,
    });
    m_eventStream.publish(playback::events::PlayerLoading{m_sessionId});
    m_loadWatchdog.start();
    m_window->play(url, effectiveCtx);
}

void EmbeddedMpvPlayerAdapter::pause()
{
    if (m_window)
        m_window->setPaused(true);
}

void EmbeddedMpvPlayerAdapter::resume()
{
    if (m_window)
        m_window->setPaused(false);
}

void EmbeddedMpvPlayerAdapter::togglePause()
{
    if (m_window)
        m_window->togglePause();
}

void EmbeddedMpvPlayerAdapter::stop()
{
    finalizeUserStop(/*stopAndHideWindow=*/true);
}

void EmbeddedMpvPlayerAdapter::finalizeUserStop(bool stopAndHideWindow)
{
    if (!m_sessionActive) {
        return;
    }

    m_loadWatchdog.stop();

    // Publish before stopping mpv. `stopAndHide()` can synchronously or
    // asynchronously trigger mpv `end-file reason="stop"` and may reset
    // position state; history needs the pre-teardown playback position.
    m_eventStream.publish(playback::events::PlaybackEnded{
        m_sessionId,
        playback::PlaybackEndReason::UserStop,
        m_ctx,
    });
    m_sessionActive = false;
    m_loadfileInFlight = false;

    if (stopAndHideWindow && m_window) {
        m_window->stopAndHide();
    }
}

void EmbeddedMpvPlayerAdapter::seekRelative(double seconds)
{
    if (m_window)
        m_window->seekRelative(seconds);
}

void EmbeddedMpvPlayerAdapter::seekAbsolute(double seconds)
{
    if (m_window)
        m_window->seekAbsolute(seconds);
}

void EmbeddedMpvPlayerAdapter::setVolumePercent(double percent)
{
    if (m_window)
        m_window->setVolumePercent(percent);
}

void EmbeddedMpvPlayerAdapter::setPlaybackRate(double factor)
{
    if (m_window)
        m_window->setSpeed(factor);
}

void EmbeddedMpvPlayerAdapter::selectAudioTrack(int id)
{
    if (m_window)
        m_window->setAudioTrack(id);
}

void EmbeddedMpvPlayerAdapter::selectSubtitleTrack(int id)
{
    if (m_window)
        m_window->setSubtitleTrack(id);
}

bool EmbeddedMpvPlayerAdapter::attachSubtitleFile(const QString& localPath, const QString& language)
{
    if (!m_window || !m_sessionActive || localPath.isEmpty()) {
        return false;
    }
    auto* vm = m_window->viewModel();
    if (!vm) {
        return false;
    }
    vm->attachExternalSubtitle(localPath,
                               QString{},
                               language,
                               /*select=*/true);
    m_eventStream.publish(playback::events::SubtitleAttached{m_sessionId, localPath, language});
    return true;
}

playback::ports::PlayerSnapshot EmbeddedMpvPlayerAdapter::snapshot() const
{
    playback::ports::PlayerSnapshot s;
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
    if (m_window) {
        m_window->setLoadingVisible(false);
    }
    if (!m_sessionActive) {
        return;
    }
    if (m_pendingResumeSeconds > 0 && m_window) {
        // Pause the file under the prompt so the user has time to
        // pick before mpv plays from 0. `onResumeAccepted /
        // Declined` re-issue the play state.
        m_window->setPaused(true);
        m_paused = true;
        m_window->showResumePrompt(m_pendingResumeSeconds);
    }
    m_eventStream.publish(playback::events::PlayerLoaded{m_sessionId});
    publishPlaybackState();
}

void EmbeddedMpvPlayerAdapter::publishPlaybackState()
{
    if (!m_sessionActive)
        return;
    m_eventStream.publish(playback::events::PlaybackStateChanged{
        m_sessionId,
        /*playing=*/!m_paused,
        m_paused,
        /*buffering=*/false,
    });
}

std::optional<playback::PlaybackEndReason>
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
        return playback::PlaybackEndReason::NaturalEof;
    }
    if (r == QStringLiteral("stop") || r == QStringLiteral("quit")) {
        return playback::PlaybackEndReason::UserStop;
    }
    if (r == QStringLiteral("error")) {
        return playback::PlaybackEndReason::PlayerError;
    }
    return playback::PlaybackEndReason::NaturalEof;
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
    m_eventStream.publish(playback::events::PlaybackEnded{
        m_sessionId,
        *classified,
        m_ctx,
    });
    m_sessionActive = false;
}

void EmbeddedMpvPlayerAdapter::onMpvError(const QString& message)
{
    m_loadWatchdog.stop();
    if (m_window) {
        m_window->setLoadingVisible(false);
    }
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(playback::events::PlaybackFailed{
        m_sessionId,
        message,
        m_ctx,
    });
}

void EmbeddedMpvPlayerAdapter::onPositionChanged(double seconds)
{
    m_position = seconds;
    if (!m_sessionActive)
        return;
    m_eventStream.publish(playback::events::PositionTicked{m_sessionId, seconds});

    // Skip-chapter prompt is only meaningful for series and only
    // when the user opted in.
    if (!m_window)
        return;
    if (m_ctx.key.kind != domain::MediaKind::Series || !m_settings.skipIntroChapters()) {
        if (m_skipChapterEnd > 0.0) {
            m_skipChapterEnd = -1.0;
            m_window->hideSkipChapter();
        }
        return;
    }
    const auto skip = playback::policy::activeSkipChapter(m_chapters, seconds, m_duration);
    if (skip.has_value()) {
        m_skipChapterEnd = skip->endSec;
        m_window->showSkipChapter(playback::policy::skipChapterKind(skip->kind),
                                  playback::policy::skipButtonLabel(skip->kind),
                                  static_cast<qint64>(skip->startSec),
                                  static_cast<qint64>(skip->endSec));
    } else if (m_skipChapterEnd > 0.0) {
        m_skipChapterEnd = -1.0;
        m_window->hideSkipChapter();
    }
}

void EmbeddedMpvPlayerAdapter::onDurationChanged(double seconds)
{
    m_duration = seconds;
    if (!m_sessionActive)
        return;
    m_eventStream.publish(playback::events::DurationChanged{m_sessionId, seconds});
}

void EmbeddedMpvPlayerAdapter::onPausedChanged(bool paused)
{
    m_paused = paused;
    publishPlaybackState();
}

void EmbeddedMpvPlayerAdapter::onVolumeChanged(double percent)
{
    m_volumePercent = percent;
}

void EmbeddedMpvPlayerAdapter::onSpeedChanged(double factor)
{
    m_playbackRate = factor;
}

void EmbeddedMpvPlayerAdapter::onTrackListChanged(const core::tracks::TrackList& tracks)
{
    if (!m_sessionActive)
        return;
    m_eventStream.publish(playback::events::TrackListChanged{m_sessionId, tracks});
}

void EmbeddedMpvPlayerAdapter::onChaptersChanged(const core::chapters::ChapterList& chapters)
{
    m_chapters = chapters;
    if (!m_sessionActive)
        return;
    m_eventStream.publish(playback::events::ChapterListChanged{m_sessionId, chapters});
}

void EmbeddedMpvPlayerAdapter::onUserClosedWindow()
{
    // PlayerWindow::closeEvent() calls stopAndHide() after this signal
    // returns, so only finalize the session here.
    finalizeUserStop(/*stopAndHideWindow=*/false);
}

void EmbeddedMpvPlayerAdapter::onLoadWatchdogTimedOut()
{
    if (!m_sessionActive) {
        return;
    }
    Q_EMIT statusMessage(i18nc("@info:status",
                               "Could not start \u201c%1\u201d \u2014 the source did not respond.",
                               m_ctx.title),
                         6000);
    m_eventStream.publish(playback::events::PlaybackEnded{
        m_sessionId,
        playback::PlaybackEndReason::LoadTimeout,
        m_ctx,
    });
    m_sessionActive = false;
    m_loadfileInFlight = false;
}

void EmbeddedMpvPlayerAdapter::onResumeAccepted()
{
    const qint64 seconds = m_pendingResumeSeconds;
    m_pendingResumeSeconds = 0;
    if (!m_window) {
        return;
    }
    m_window->hideResumePrompt();
    m_window->seekAbsolute(static_cast<double>(seconds));
    m_window->setPaused(false);
}

void EmbeddedMpvPlayerAdapter::onResumeDeclined()
{
    m_pendingResumeSeconds = 0;
    if (!m_window) {
        return;
    }
    m_window->hideResumePrompt();
    m_window->seekAbsolute(0.0);
    m_window->setPaused(false);
}

void EmbeddedMpvPlayerAdapter::onSkipRequested()
{
    if (!m_window || m_skipChapterEnd <= 0.0) {
        return;
    }
    m_window->hideSkipChapter();
    // 0.25 s of cushion so we land just after the chapter boundary
    // and don't immediately re-trigger the prompt on the next
    // PositionTicked tick.
    m_window->seekAbsolute(m_skipChapterEnd + 0.25);
    m_skipChapterEnd = -1.0;
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
