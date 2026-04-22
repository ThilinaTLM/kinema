// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QList>
#include <QString>
#include <QWidget>

class QLabel;

namespace kinema::ui::player::widgets {

/**
 * Custom seek bar painted entirely by Qt. Replaces the placeholder
 * QSlider in TransportBar. Shows:
 *   - elapsed portion in an accent colour
 *   - buffered-ahead shading (from current position to
 *     `position + cacheAhead`) in a lighter tint
 *   - chapter ticks (1 px verticals) at each chapter start
 *   - floating timestamp label while the mouse hovers the bar
 *
 * State model: purely reflects what the caller pushes via the
 * setters. The widget never polls, never owns timers. Click / drag
 * translates to `seekRequested(seconds)`; callers forward to
 * MpvWidget::seekAbsolute and wait for the position-change echo to
 * update the visible elapsed region.
 */
class PlayerSeekBar : public QWidget
{
    Q_OBJECT
public:
    explicit PlayerSeekBar(QWidget* parent = nullptr);

    /// Convert an x-coordinate inside the bar to a timestamp, using
    /// the current duration. Clamps to [0, duration]. Exposed for
    /// the unit test; production code uses it via the mouse events.
    double timestampForX(int x) const;

    /// Pixel column at the left edge of the painted track.
    int trackLeftPx() const;
    /// Pixel column at the right edge of the painted track.
    int trackRightPx() const;

    double duration() const { return m_duration; }
    double position() const { return m_position; }
    double cacheAhead() const { return m_cacheAhead; }
    const QList<double>& chapters() const { return m_chapters; }

public Q_SLOTS:
    void setDuration(double seconds);
    void setPosition(double seconds);
    void setCacheAhead(double seconds);
    void setChapters(const QList<double>& chapterStartsSec);

Q_SIGNALS:
    /// User-driven seek request. Payload is the target timestamp in
    /// seconds, already clamped to [0, duration].
    void seekRequested(double seconds);

protected:
    void paintEvent(QPaintEvent* e) override;
    void mousePressEvent(QMouseEvent* e) override;
    void mouseMoveEvent(QMouseEvent* e) override;
    void mouseReleaseEvent(QMouseEvent* e) override;
    void leaveEvent(QEvent* e) override;
    QSize sizeHint() const override;
    QSize minimumSizeHint() const override;

private:
    int xForTimestamp(double seconds) const;
    void updateHover(int x);
    void clearHover();

    double m_duration {0.0};
    double m_position {0.0};
    double m_cacheAhead {0.0};
    QList<double> m_chapters;

    /// True while the user is dragging after a press; the widget
    /// updates the hover label on motion and emits seekRequested on
    /// release. Released drags outside the widget are still honoured.
    bool m_dragging {false};
    /// Last x the user hovered / dragged over, relative to this
    /// widget. -1 when the pointer is outside the bar.
    int m_hoverX {-1};

    /// Floating timestamp tooltip. Parented to the widget so it
    /// clips to the viewport; positioned just above the bar during
    /// hover / drag.
    QLabel* m_hoverLabel {nullptr};
};

} // namespace kinema::ui::player::widgets
