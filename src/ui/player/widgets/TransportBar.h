// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QList>
#include <QPointer>
#include <QWidget>

class QLabel;
class QSlider;
class QToolButton;

namespace kinema::ui::player {
class MpvWidget;
}

namespace kinema::ui::player::widgets {

class PlayerSeekBar;

/**
 * Kinema's bottom transport strip: play/pause, seek, current / total
 * time, volume + mute, fullscreen, close. Hosts a PlayerSeekBar which
 * handles the seek-bar rendering / hit-testing and emits
 * `seekRequested` on release.
 *
 * State model: **purely reflects mpv**. Slider values, button
 * icons, and time labels update only in response to MpvWidget's
 * property-change signals. Clicks and drags send commands to mpv
 * and wait for the corresponding property change before repainting,
 * keeping a single source of truth and avoiding drift.
 */
class TransportBar : public QWidget
{
    Q_OBJECT
public:
    TransportBar(MpvWidget* mpv, QWidget* parent = nullptr);

Q_SIGNALS:
    void toggleFullscreenRequested();
    void closeRequested();

protected:
    void paintEvent(QPaintEvent* e) override;

public Q_SLOTS:
    /// Forward chapter ticks (seconds) to the seek bar. Transport
    /// bar itself doesn't use them but we centralise the plumbing
    /// so PlayerOverlay has a single sink.
    void setChapters(const QList<double>& chapterStartsSec);

private Q_SLOTS:
    void onPositionChanged(double seconds);
    void onDurationChanged(double seconds);
    void onPausedChanged(bool paused);
    void onVolumeChanged(double v);
    void onMuteChanged(bool muted);
    void onCacheAheadChanged(double seconds);
    void onSeekRequested(double seconds);
    void onVolumeSliderMoved(int value);

private:
    void refreshTimeLabel();
    void refreshPlayIcon();
    void refreshVolumeIcon();

    QPointer<MpvWidget> m_mpv;

    QToolButton* m_playButton {nullptr};
    PlayerSeekBar* m_seekBar {nullptr};
    QLabel* m_timeLabel {nullptr};
    QToolButton* m_muteButton {nullptr};
    QSlider* m_volumeSlider {nullptr};
    QToolButton* m_fullscreenButton {nullptr};
    QToolButton* m_closeButton {nullptr};

    double m_position {0.0};
    double m_duration {0.0};
    double m_volume {100.0};
    bool m_paused {false};
    bool m_muted {false};

};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
