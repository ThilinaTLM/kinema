// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "core/MpvChapterList.h"
#include "core/MpvIpc.h"
#include "core/MpvTrackList.h"

#include <QOpenGLWidget>
#include <QString>
#include <QStringList>
#include <QUrl>

#include <deque>
#include <optional>

struct mpv_handle;
struct mpv_render_context;

namespace kinema::config {
class PlayerSettings;
}

namespace kinema::ui::player {

/**
 * QOpenGLWidget that hosts an in-process libmpv instance and draws its
 * output via the render API. mpv's built-in on-screen controller (OSC)
 * provides the playback UI — we do not render any Qt-side controls
 * ourselves (that's a future milestone).
 *
 * Threading: libmpv fires callbacks from render/wakeup threads; we
 * marshal everything back onto the GUI thread via queued invocations
 * before touching Qt state or calling mpv APIs that aren't
 * thread-safe.
 */
class MpvWidget : public QOpenGLWidget
{
    Q_OBJECT
public:
    /// `settings` is injected so the widget can translate the user's
    /// Kinema-level preferences (hardware decoding, preferred audio /
    /// subtitle language) into mpv options at construction time and
    /// apply the remembered volume on every load. The widget does not
    /// write back to settings — PlayerWindow persists on hide.
    explicit MpvWidget(const config::PlayerSettings& settings,
        QWidget* parent = nullptr);
    ~MpvWidget() override;

    MpvWidget(const MpvWidget&) = delete;
    MpvWidget& operator=(const MpvWidget&) = delete;

    /// Replace the currently playing file with `url`. If playback is
    /// idle the file just starts; otherwise the current file is
    /// stopped and the new one loaded.
    ///
    /// `startSeconds` resumes playback at the given offset when set.
    /// We pass it via mpv's `loadfile` options (`start=<seconds>`)
    /// so mpv can seek without decoding leading frames. Out-of-range
    /// values are silently clamped by mpv.
    void loadFile(const QUrl& url,
        std::optional<double> startSeconds = std::nullopt);

    /// Stop playback and return to idle. Safe to call repeatedly.
    void stop();

    /// Toggle / set the `pause` property asynchronously.
    void setPaused(bool paused);
    bool isPaused() const { return m_paused; }

    /// Push fullscreen state into mpv. The outer window owns the Qt
    /// fullscreen toggle; this keeps the OSC's indicator in sync.
    void setMpvFullscreen(bool on);

    // ---- Transport helpers used by the Qt overlay ----------------------
    //
    // Every helper issues an async mpv command / property write and
    // returns immediately. Cached state is NEVER updated here — the
    // property-change observer is the single source of truth so the
    // UI never drifts from mpv.

    void seekAbsolute(double seconds);
    void setVolumePercent(double v);
    void setMuted(bool m);
    void setSpeed(double s);
    void cyclePause();

    /// Select an mpv audio track by id, or pass -1 to disable audio
    /// entirely. Idempotent; the matching property-change observer
    /// drives UI updates.
    void setAudioTrack(int id);
    /// Select / disable subtitle track. -1 = off.
    void setSubtitleTrack(int id);

    /// Push a human-readable title into mpv's `force-media-title`
    /// property. The Lua chrome reads `media-title` so this keeps
    /// the in-mpv display consistent with the Qt window title.
    void setMediaTitle(const QString& title);

    /// Typed IPC bridge to the `kinema-overlays` Lua script. Owned
    /// by this widget; destroyed before the mpv handle in the dtor
    /// so no pending send races `mpv_terminate_destroy`. Null only
    /// in the degraded build where `mpv_create()` or
    /// `mpv_initialize()` failed.
    core::MpvIpc* ipc() const { return m_ipc; }

    // ---- Cached state accessors ---------------------------------------

    double currentVolume() const { return m_lastVolume; }
    bool isMuted() const { return m_muted; }
    double currentSpeed() const { return m_lastSpeed; }
    const core::tracks::TrackList& trackList() const { return m_tracks; }
    const core::chapters::ChapterList& chapters() const { return m_chapters; }

    /// Snapshot of the video / audio stats (width/height, codec,
    /// fps, bitrate, cache). Fields are zero when mpv hasn't
    /// reported them yet; consumers should treat zero as "unknown".
    /// The visible stats HUD is rendered by mpv's own `stats.lua`
    /// (toggled with `I`); this snapshot is kept so controllers
    /// can still reason about codec / resolution.
    struct VideoStats {
        int width = 0;
        int height = 0;
        QString videoCodec;
        QString audioCodec;
        double fps = 0.0;
        double videoBitrate = 0.0;   ///< bits/sec
        double audioBitrate = 0.0;   ///< bits/sec
        double cacheSeconds = 0.0;   ///< demuxer-cache-time
        int audioChannels = 0;       ///< audio-params/channel-count
        QString hdrPrimaries;        ///< video-params/primaries
        QString hdrGamma;            ///< video-params/gamma
    };
    VideoStats currentStats() const { return m_stats; }

    /// Most recent log lines (up to ~64) for post-hoc classification
    /// by PlaybackController when an end-file error fires.
    QStringList recentLogLines() const;

Q_SIGNALS:
    /// mpv has loaded the file and playback is running.
    void fileLoaded();
    /// Playback has ended. `reason` is mpv's stop reason ("eof",
    /// "stop", "quit", "error" …).
    void endOfFile(const QString& reason);
    /// Current playback position, in seconds. Fires at roughly the
    /// cadence mpv reports time-pos changes, throttled by a small
    /// epsilon to avoid UI flooding.
    void positionChanged(double seconds);
    /// Total duration of the currently loaded file, in seconds. Fires
    /// once per loaded file as soon as the duration becomes known.
    void durationChanged(double seconds);
    /// The `fullscreen` property flipped inside mpv (e.g. the user
    /// pressed `f` with default bindings). The outer window mirrors
    /// this into `showFullScreen()` / `showNormal()`.
    void fullscreenChanged(bool on);
    /// User requested a window-level fullscreen toggle — either by
    /// double-clicking the video or pressing Esc. The outer window
    /// interprets Esc as "leave fullscreen if fullscreen, otherwise
    /// ignore".
    void toggleFullscreenRequested();
    void leaveFullscreenRequested();
    /// A libmpv call failed; payload is a short human-readable
    /// description. The launcher surfaces this in the status bar.
    void mpvError(const QString& message);

    // ---- Property-change mirrors --------------------------------------
    //
    // Everything the Qt overlay needs to repaint. Fired on the GUI
    // thread from onMpvEvents; values are the ones libmpv reported.

    void pausedChanged(bool paused);
    void volumeChanged(double percent);
    void muteChanged(bool muted);
    void speedChanged(double factor);
    /// True while mpv is stalled waiting for the demuxer cache; second
    /// arg is the percentage mpv reports (0 when `paused-for-cache` is
    /// false, so a single signal is enough to drive the UI).
    void bufferingChanged(bool waiting, int percent);
    /// Seconds of demuxer cache ahead of the current position; used by
    /// the seek bar to shade the buffered-ahead region.
    void cacheAheadChanged(double seconds);
    /// mpv's `track-list` changed — either a new file loaded or the
    /// selected track flipped. Payload mirrors MpvWidget::trackList().
    void trackListChanged(const core::tracks::TrackList& tracks);
    /// mpv's `chapter-list` changed.
    void chaptersChanged(const core::chapters::ChapterList& chapters);
    /// Selected audio / subtitle track ids. -1 when audio / subs are
    /// disabled (mpv's "no" value).
    void audioTrackChanged(int id);
    void subtitleTrackChanged(int id);
    /// Video / audio stats refreshed. Consumers hold the struct by
    /// value; zero fields indicate "not reported yet".
    void videoStatsChanged(const VideoStats& stats);

protected:
    // QOpenGLWidget
    void initializeGL() override;
    void paintGL() override;

    // Input forwarding into mpv (so the OSC, default bindings, and the
    // user's own input.conf all work inside the widget).
    void keyPressEvent(QKeyEvent* e) override;
    void keyReleaseEvent(QKeyEvent* e) override;
    void mouseMoveEvent(QMouseEvent* e) override;
    void mousePressEvent(QMouseEvent* e) override;
    void mouseReleaseEvent(QMouseEvent* e) override;
    void mouseDoubleClickEvent(QMouseEvent* e) override;
    void wheelEvent(QWheelEvent* e) override;

private Q_SLOTS:
    /// Drains pending mpv_events on the GUI thread. Invoked via
    /// QueuedConnection from the mpv wakeup callback.
    void onMpvEvents();
    /// Schedules a repaint; invoked via QueuedConnection from the
    /// render update callback.
    void onRenderUpdate();

private:
    void maybeTeardownRenderContext();
    void forwardKey(QKeyEvent* e, bool down);
    void forwardMouseButton(QMouseEvent* e, bool down);
    void updateMousePos(const QPoint& pos);
    void appendLogLine(const QString& line);
    /// Re-fetch the named list property via
    /// `mpv_get_property_string` (which serialises any mpv node to
    /// JSON) and hand it to the matching pure parser. Called on
    /// track-list / chapter-list property-change events.
    QByteArray fetchPropertyJson(const char* name);
    void refreshTrackList();
    void refreshChapterList();

    const config::PlayerSettings& m_settings;

    mpv_handle* m_mpv {nullptr};
    mpv_render_context* m_renderCtx {nullptr};

    // Cached property state, updated from property-change events.
    bool m_paused {false};
    bool m_fullscreen {false};
    bool m_muted {false};
    double m_lastPosition {-1.0};
    double m_lastDuration {-1.0};
    double m_lastVolume {-1.0};
    double m_lastSpeed {1.0};
    double m_lastCacheAhead {-1.0};
    bool m_bufferingWaiting {false};
    int m_bufferingPct {0};

    core::MpvIpc* m_ipc {nullptr};

    core::tracks::TrackList m_tracks;
    core::chapters::ChapterList m_chapters;
    int m_lastAid {-1};
    int m_lastSid {-1};
    VideoStats m_stats;

    // When true we've requested fullscreen via setMpvFullscreen() and
    // should suppress the resulting property-change echo to avoid a
    // feedback loop with the outer window.
    bool m_suppressFullscreenEcho {false};

    // Ring buffer of recent log lines, consumed by PlaybackController
    // to classify end-file errors. Bounded so the widget's memory
    // footprint stays flat across long playbacks.
    std::deque<QString> m_logTail;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
