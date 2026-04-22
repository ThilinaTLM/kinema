// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/PlaybackContext.h"

#include <QPointer>
#include <QWidget>

class QTimer;

namespace kinema::ui::player {
class MpvWidget;
}

namespace kinema::ui::player::widgets {

class TransportBar;
class BufferingOverlay;
class PlayerTitleBar;
class StatsOverlay;
class CheatSheetOverlay;
class ResumePrompt;
class NextEpisodeBanner;

/**
 * Transparent top-level overlay pinned to the video surface. Hosts
 * the Kinema-native controls and drives their auto-hide.
 *
 * Sizing: owned by PlayerWindow, manually resized to cover
 * MpvWidget's rectangle on every parent resize. No layout — we want
 * absolute placement so the transport bar can hug the bottom edge
 * without fighting mpv for OpenGL paint invalidation.
 *
 * Input model: the overlay is `Qt::WA_TransparentForMouseEvents` on
 * its root widget, so clicks pass through to mpv for OSC-style
 * behaviour. Individual sub-widgets flip the attribute off so they
 * can receive clicks. An event filter on MpvWidget catches mouse
 * motion to drive the show/hide timer.
 *
 * Reserved keys: the overlay intercepts `?` and `I` on MpvWidget's
 * event stream *before* they reach mpv, so `input.conf` must never
 * bind them. See data/kinema/mpv/input.conf's reserved block.
 */
class PlayerOverlay : public QWidget
{
    Q_OBJECT
public:
    PlayerOverlay(MpvWidget* mpv, QWidget* parent = nullptr);
    ~PlayerOverlay() override;

    /// Called by PlayerWindow when the window enters / exits
    /// fullscreen so the overlay can adjust its z-order or size hints
    /// if needed. No-op today; reserved for later polish.
    void setFullscreen(bool on);

    /// Push the current playback context so the title bar can render
    /// the right labels. Called by PlayerWindow::play.
    void setContext(const api::PlaybackContext& ctx);

    /// Access to sub-widgets so PlayerWindow can wire their
    /// high-level actions (close, fullscreen toggle) without routing
    /// through extra signals.
    TransportBar* transportBar() const { return m_transport; }
    BufferingOverlay* bufferingOverlay() const { return m_buffering; }
    PlayerTitleBar* titleBar() const { return m_titleBar; }

    /// Reset any residual state between plays (position, buffering).
    /// Called from PlayerWindow::stopAndHide.
    void resetForNewSession();

    /// Series overlays controlled by PlaybackController.
    void showResumePrompt(qint64 seconds);
    void hideResumePrompt();
    void showNextEpisodeBanner(const api::PlaybackContext& ctx, int countdownSec);
    void updateNextEpisodeCountdown(int seconds);
    void hideNextEpisodeBanner();
    bool acceptNextEpisodeBanner();
    bool cancelNextEpisodeBanner();
    void showSkipChapter(const QString& label);
    void hideSkipChapter();

    /// Toggle the cheat-sheet / stats overlays. Exposed so
    /// PlayerWindow can route keyboard shortcuts it captures when
    /// focus sits on the window instead of MpvWidget.
    void toggleCheatSheet();
    void toggleStats();

Q_SIGNALS:
    /// Emitted when the user interacts with the transport bar's
    /// fullscreen button. PlayerWindow handles the actual mode
    /// switch.
    void toggleFullscreenRequested();
    /// Emitted when the user clicks the close button.
    void closeRequested();
    void resumeRequested(qint64 seconds);
    void restartRequested();
    void nextEpisodeAccepted();
    void nextEpisodeCancelled();
    void skipRequested();

protected:
    void resizeEvent(QResizeEvent* e) override;
    bool eventFilter(QObject* watched, QEvent* e) override;

private Q_SLOTS:
    void onActivityTimeout();

private:
    void nudgeActivity();
    void setChromeVisible(bool visible);

    QPointer<MpvWidget> m_mpv;
    TransportBar* m_transport {nullptr};
    BufferingOverlay* m_buffering {nullptr};
    PlayerTitleBar* m_titleBar {nullptr};
    StatsOverlay* m_stats {nullptr};
    CheatSheetOverlay* m_cheatSheet {nullptr};
    ResumePrompt* m_resumePrompt {nullptr};
    NextEpisodeBanner* m_nextEpisodeBanner {nullptr};

    QTimer* m_hideTimer {nullptr};
    bool m_chromeVisible {true};
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
