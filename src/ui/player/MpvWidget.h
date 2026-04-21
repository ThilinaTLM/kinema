// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QOpenGLWidget>
#include <QString>
#include <QUrl>

struct mpv_handle;
struct mpv_render_context;

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
    explicit MpvWidget(QWidget* parent = nullptr);
    ~MpvWidget() override;

    MpvWidget(const MpvWidget&) = delete;
    MpvWidget& operator=(const MpvWidget&) = delete;

    /// Replace the currently playing file with `url`. If playback is
    /// idle the file just starts; otherwise the current file is
    /// stopped and the new one loaded.
    void loadFile(const QUrl& url);

    /// Stop playback and return to idle. Safe to call repeatedly.
    void stop();

    /// Toggle / set the `pause` property asynchronously.
    void setPaused(bool paused);
    bool isPaused() const { return m_paused; }

    /// Push fullscreen state into mpv. The outer window owns the Qt
    /// fullscreen toggle; this keeps the OSC's indicator in sync.
    void setMpvFullscreen(bool on);

Q_SIGNALS:
    /// mpv has loaded the file and playback is running.
    void fileLoaded();
    /// Playback has ended. `reason` is mpv's stop reason ("eof",
    /// "stop", "quit", "error" …).
    void endOfFile(const QString& reason);
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

    mpv_handle* m_mpv {nullptr};
    mpv_render_context* m_renderCtx {nullptr};

    // Cached property state, updated from property-change events.
    bool m_paused {false};
    bool m_fullscreen {false};

    // When true we've requested fullscreen via setMpvFullscreen() and
    // should suppress the resulting property-change echo to avoid a
    // feedback loop with the outer window.
    bool m_suppressFullscreenEcho {false};
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
