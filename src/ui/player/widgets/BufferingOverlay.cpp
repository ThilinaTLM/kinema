// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/BufferingOverlay.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QTimer>

namespace kinema::ui::player::widgets {

namespace {

// Lightweight ASCII "spinner" \u2014 eight rotational phases.
// Using a text spinner keeps this widget zero-asset and sidesteps
// QMovie's GIF dependency.
const QString kSpinnerFrames[8] = {
    QStringLiteral("\u280B"), QStringLiteral("\u2819"),
    QStringLiteral("\u2839"), QStringLiteral("\u2838"),
    QStringLiteral("\u283C"), QStringLiteral("\u2834"),
    QStringLiteral("\u2826"), QStringLiteral("\u2827"),
};

constexpr int kSpinnerIntervalMs = 90;

} // namespace

BufferingOverlay::BufferingOverlay(QWidget* parent)
    : QWidget(parent)
{
    // Non-modal visual hint: we want the click to still reach mpv in
    // case the user decides to pause / seek through a slow point.
    setAttribute(Qt::WA_TransparentForMouseEvents);
    // The widget paints its own translucent background via paintEvent.
    setAutoFillBackground(false);

    m_label = new QLabel(this);
    m_label->setAlignment(Qt::AlignCenter);
    m_label->setTextFormat(Qt::PlainText);
    // Catppuccin-friendly: let the stylesheet drive actual colours.
    m_label->setStyleSheet(QStringLiteral(
        "QLabel { color: white; font-size: 14pt; padding: 12px 24px; }"));

    auto* lay = new QHBoxLayout(this);
    lay->setContentsMargins(0, 0, 0, 0);
    lay->addStretch(1);
    lay->addWidget(m_label);
    lay->addStretch(1);

    m_spinner = new QTimer(this);
    m_spinner->setInterval(kSpinnerIntervalMs);
    connect(m_spinner, &QTimer::timeout,
        this, &BufferingOverlay::onSpinnerTick);

    hide();
}

void BufferingOverlay::setBuffering(bool waiting, int percent)
{
    m_waiting = waiting;
    m_percent = percent;
    if (!waiting) {
        m_spinner->stop();
        hide();
        return;
    }

    const QString frame = kSpinnerFrames[m_spinnerFrame % 8];
    QString text = frame + QLatin1Char(' ')
        + i18nc("@info:status buffering indicator", "Buffering\u2026");
    if (percent > 0 && percent < 100) {
        text += QStringLiteral(" %1%").arg(percent);
    }
    m_label->setText(text);

    if (!isVisible()) {
        show();
        m_spinner->start();
    }
}

void BufferingOverlay::onSpinnerTick()
{
    m_spinnerFrame = (m_spinnerFrame + 1) % 8;
    // Re-render with the next frame, preserving the percentage text.
    setBuffering(m_waiting, m_percent);
}

void BufferingOverlay::paintEvent(QPaintEvent* /*e*/)
{
    // Translucent pill behind the label. We paint it ourselves (not
    // via stylesheet) to keep the geometry centred regardless of
    // parent layout decisions.
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);
    const QRect r = m_label->geometry();
    if (r.isEmpty()) {
        return;
    }
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(0, 0, 0, 180));
    p.drawRoundedRect(r.adjusted(-4, -4, 4, 4), 8, 8);
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
