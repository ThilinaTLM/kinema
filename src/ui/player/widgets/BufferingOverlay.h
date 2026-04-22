// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QWidget>

class QLabel;
class QTimer;

namespace kinema::ui::player::widgets {

/**
 * Centred "Buffering\u2026" panel shown while mpv's `paused-for-cache`
 * is true. Small spinner (rotating characters \u2014 we avoid Qt's
 * QMovie to keep the asset footprint flat) + a label with the
 * current buffering percentage when mpv reports one.
 *
 * Displayed on top of the MpvWidget from inside PlayerOverlay; the
 * overlay is responsible for positioning and show/hide timing based
 * on MpvWidget::bufferingChanged.
 */
class BufferingOverlay : public QWidget
{
    Q_OBJECT
public:
    explicit BufferingOverlay(QWidget* parent = nullptr);

public Q_SLOTS:
    /// Show/hide the overlay. When `waiting=true` and `percent>0`,
    /// the percentage is appended to the label.
    void setBuffering(bool waiting, int percent);

protected:
    void paintEvent(QPaintEvent* e) override;

private Q_SLOTS:
    void onSpinnerTick();

private:
    QLabel* m_label {nullptr};
    QTimer* m_spinner {nullptr};
    int m_spinnerFrame {0};
    int m_percent {0};
    bool m_waiting {false};
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
