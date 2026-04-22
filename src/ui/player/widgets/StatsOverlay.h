// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/MpvWidget.h"

#include <QWidget>

class QLabel;
class QTimer;

namespace kinema::ui::player::widgets {

/**
 * Translucent read-only panel in the top-right of the video surface,
 * analogous to mpv's `stats.lua` but sized to sit inside Kinema's
 * overlay chrome. Toggled by the `I` key (intercepted by
 * PlayerOverlay) or the settings menu.
 *
 * Refresh cadence is 1 Hz. Values arrive through
 * MpvWidget::videoStatsChanged but only the cached last snapshot is
 * repainted when the timer ticks; back-to-back property events can
 * flood the queue, and a second-granularity refresh is plenty for a
 * stats display.
 */
class StatsOverlay : public QWidget
{
    Q_OBJECT
public:
    explicit StatsOverlay(QWidget* parent = nullptr);

    /// Toggle visibility without touching the last-known stats so
    /// re-showing doesn't briefly flash empty rows.
    void toggleVisibility();

public Q_SLOTS:
    void applyStats(const MpvWidget::VideoStats& stats);

protected:
    void paintEvent(QPaintEvent* e) override;
    void showEvent(QShowEvent* e) override;
    void hideEvent(QHideEvent* e) override;

private Q_SLOTS:
    void refreshLabels();

private:
    QLabel* m_resolutionLabel {nullptr};
    QLabel* m_codecsLabel {nullptr};
    QLabel* m_fpsLabel {nullptr};
    QLabel* m_videoBitrateLabel {nullptr};
    QLabel* m_audioBitrateLabel {nullptr};
    QLabel* m_cacheLabel {nullptr};

    QTimer* m_refresh {nullptr};
    MpvWidget::VideoStats m_latest;
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
