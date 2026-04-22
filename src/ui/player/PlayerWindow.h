// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/PlaybackContext.h"

#include <QWidget>
#include <QString>
#include <QUrl>

namespace kinema::config {
class AppearanceSettings;
}

namespace kinema::ui::player {

class MpvWidget;

/**
 * Top-level window that hosts the in-process libmpv player. Owns a
 * single MpvWidget as its only child and manages window-level concerns
 * (title, icon, geometry persistence, fullscreen, close semantics)
 * that are orthogonal to the mpv plumbing.
 *
 * Lifecycle: constructed lazily by MainWindow on the first embedded
 * Play. Re-used for subsequent plays. Closing the window (X button,
 * Esc in windowed mode, or end-of-file) stops playback and hides the
 * window, keeping the libmpv context warm for fast re-open. The
 * context is torn down when this widget is destroyed (i.e. when the
 * owning MainWindow goes away).
 *
 * Geometry: first-time default is 1280x720 centred on the parent
 * window's screen; subsequent sessions restore the last
 * saveGeometry() blob through config::AppearanceSettings, preserving
 * the existing [PlayerWindow]/Geometry on-disk storage.
 */
class PlayerWindow : public QWidget
{
    Q_OBJECT
public:
    explicit PlayerWindow(config::AppearanceSettings& appearance,
        QWidget* parent = nullptr);
    ~PlayerWindow() override;

    PlayerWindow(const PlayerWindow&) = delete;
    PlayerWindow& operator=(const PlayerWindow&) = delete;

    /// Load `url` into mpv and show the window (raising + activating
    /// it if already visible). `ctx.title` is used verbatim in the
    /// window title bar, suffixed with " — Kinema". Empty title
    /// collapses to just "Kinema". When `ctx.resumeSeconds` is set,
    /// mpv is asked to start at that offset (clamped below the end
    /// of the file when duration is known).
    void play(const QUrl& url, const kinema::api::PlaybackContext& ctx);

    /// Stop playback, leave fullscreen if needed, and hide the
    /// window. Safe to call repeatedly or before any play().
    void stopAndHide();

    /// True after the first play() call. Used by MainWindow's tray
    /// menu to decide whether to expose a "Show Player" action.
    bool hasEverLoaded() const { return m_hasEverLoaded; }

Q_SIGNALS:
    /// mpv has loaded the file and playback is running. Forwarded
    /// verbatim from MpvWidget so MainWindow can still raise
    /// KNotifications.
    void fileLoaded();
    /// Playback ended. `reason` is mpv's stop reason ("eof",
    /// "stop", "quit", "error" …). Forwarded from MpvWidget; the
    /// window has already hidden itself by the time this fires.
    void endOfFile(const QString& reason);
    /// libmpv call failed; payload is a short human-readable
    /// description. MainWindow surfaces this in the status bar.
    void mpvError(const QString& message);
    /// Emitted from showEvent / hideEvent so MainWindow can refresh
    /// the tray context menu (the "Show Player" entry depends on
    /// visibility).
    void visibilityChanged(bool visible);
    /// Forwarded from MpvWidget. HistoryController consumes these to
    /// keep the persisted progress current during playback.
    void positionChanged(double seconds);
    void durationChanged(double seconds);

protected:
    void closeEvent(QCloseEvent* e) override;
    void keyPressEvent(QKeyEvent* e) override;
    void showEvent(QShowEvent* e) override;
    void hideEvent(QHideEvent* e) override;

private:
    void toggleFullscreen();
    void leaveFullscreenOrClose();
    void mirrorMpvFullscreen(bool on);

    void loadGeometry();
    void saveGeometryToConfig();

    config::AppearanceSettings& m_appearanceSettings;
    MpvWidget* m_mpv {nullptr};

    // Re-entrancy guard for fullscreen toggles driven both by Qt
    // (F key / double-click via MpvWidget) and mpv
    // (fullscreen property change from scripts / OSC).
    bool m_applyingFullscreen {false};

    // Set on first play() so MainWindow's tray menu knows whether
    // a "Show Player" entry is meaningful.
    bool m_hasEverLoaded {false};

    // One-shot flag: true until the first showEvent(), used to
    // apply the centred-default or restored geometry exactly once.
    bool m_geometryApplied {false};
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
