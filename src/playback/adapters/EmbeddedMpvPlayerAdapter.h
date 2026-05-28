// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "playback/events/PlaybackEvent.h"
#include "playback/ports/PlayerPort.h"
#include "playback/session/PlayerLoadWatchdog.h"

#include "core/mpv/MpvChapterList.h"
#include "core/mpv/MpvTrackList.h"
#include "domain/PlaybackContext.h"

#include <QObject>
#include <QPointer>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::config {
class PlayerSettings;
}

namespace kinema::ui::player {
class PlayerWindow;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::adapters {

/**
 * `PlayerPort` adapter over `ui::player::PlayerWindow`.
 *
 * Translates `PlayerWindow` signals into typed events on
 * `PlaybackEventStream`, owns the load watchdog, and forwards
 * transport calls.
 *
 * The `PlayerWindow` is created lazily by `ShellViewModel`, so the
 * adapter accepts late binding via `setPlayerWindow(...)`. Before
 * a window is attached, transport calls are dropped and
 * `isAvailable()` returns false.
 *
 * Session bookkeeping (id stamping, replace-by-new-source filter)
 * is intentionally minimal: `PlaybackSession` (Phase 6, not yet
 * landed) is the real owner of the lifecycle. The adapter just
 * needs enough state to stamp events with the current session id
 * and to suppress mpv's `end-file reason="stop"` that occurs as a
 * side-effect of loadfile-replacing the previous file.
 */
class EmbeddedMpvPlayerAdapter : public QObject, public ports::PlayerPort
{
    Q_OBJECT
public:
    EmbeddedMpvPlayerAdapter(events::PlaybackEventStream& eventStream,
        const config::PlayerSettings& settings,
        QObject* parent = nullptr);
    ~EmbeddedMpvPlayerAdapter() override;

    /// Wire the embedded `PlayerWindow`. Pass `nullptr` to detach.
    /// Late binding pattern: `ShellViewModel::ensurePlayerWindow`
    /// calls this once the QML side has built the window.
    void setPlayerWindow(ui::player::PlayerWindow* window);

    ui::player::PlayerWindow* playerWindow() const noexcept;

    /// Stamp future events for this session. Called by
    /// `PlaybackSession` immediately before `play()`. Pass a null
    /// id to clear the active session without publishing
    /// terminal events (used at shutdown).
    void setActiveSession(PlaybackSessionId sessionId,
        const domain::PlaybackContext& ctx);

    PlaybackSessionId activeSessionId() const noexcept { return m_sessionId; }
    const domain::PlaybackContext& activeContext() const noexcept { return m_ctx; }

    /// Direct access to the watchdog for tests; the adapter owns it
    /// and arms/disarms it around `play()` and load events.
    session::PlayerLoadWatchdog& loadWatchdog() noexcept { return m_loadWatchdog; }

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

Q_SIGNALS:
    /// User-facing status text ("Loading X…", "Could not start X",
    /// "Embedded player is not available"). Routed to the shell's
    /// passive notification by `ShellViewModel`.
    void statusMessage(const QString& text, int timeoutMs = 3000);

    /// Re-emitted from `PlayerWindow::visibilityChanged` so the
    /// shell can refresh the tray menu without a direct
    /// `PlayerWindow` dependency.
    void visibilityChanged(bool visible);

private Q_SLOTS:
    void onFileLoaded();
    void onEndOfFile(const QString& reason);
    void onMpvError(const QString& message);
    void onPositionChanged(double seconds);
    void onDurationChanged(double seconds);
    void onPausedChanged(bool paused);
    void onVolumeChanged(double percent);
    void onSpeedChanged(double factor);
    void onTrackListChanged(const core::tracks::TrackList& tracks);
    void onChaptersChanged(const core::chapters::ChapterList& chapters);
    void onUserClosedWindow();
    void onLoadWatchdogTimedOut();
    void onResumeAccepted();
    void onResumeDeclined();
    void onSkipRequested();

private:
    void disconnectWindow();
    void connectWindow();
    void publishPlaybackState();
    /// Finalize an explicit user stop/close before tearing down mpv
    /// so the history projector sees the last reliable position.
    void finalizeUserStop(bool stopAndHideWindow);
    /// Convert mpv's stringly-typed `end-file` reason into a
    /// `PlaybackEndReason`. Returns nullopt when the reason should
    /// be filtered (intermediate "stop" caused by loadfile
    /// replacing the previous file).
    std::optional<PlaybackEndReason> classifyEndReason(const QString& reason);

    events::PlaybackEventStream& m_eventStream;
    const config::PlayerSettings& m_settings;
    QPointer<ui::player::PlayerWindow> m_window;
    session::PlayerLoadWatchdog m_loadWatchdog { this };

    PlaybackSessionId m_sessionId;
    domain::PlaybackContext m_ctx;
    /// Resume position that exceeds the user's prompt threshold;
    /// stashed at `play()` so `file-loaded` can render the prompt
    /// instead of seeking blindly. Consumed by `onResumeAccepted`
    /// / `onResumeDeclined`.
    qint64 m_pendingResumeSeconds = 0;
    /// Cached chapter list from the active session. Re-evaluated
    /// on every `PositionTicked` to surface the "skip intro / outro
    /// / credits" prompt at the right windows.
    core::chapters::ChapterList m_chapters;
    /// End of the active skip-chapter (exclusive). Drives the
    /// `Skip…` seek target when the user accepts the prompt.
    /// -1 when no skip prompt is currently shown.
    double m_skipChapterEnd = -1.0;
    bool m_sessionActive = false;
    /// True between a fresh `play()` that supersedes an active
    /// session and the corresponding `file-loaded` event. The mpv
    /// `end-file reason="stop"` that loadfile generates as a
    /// side-effect of replacing the previous file is filtered
    /// while this flag is set so series auto-next and history
    /// don't see a spurious termination of the new attempt.
    bool m_loadfileInFlight = false;
    double m_volumePercent = 100.0;
    double m_playbackRate = 1.0;
    double m_position = 0.0;
    double m_duration = 0.0;
    bool m_paused = false;
};

} // namespace kinema::playback::adapters

#endif // KINEMA_HAVE_LIBMPV
