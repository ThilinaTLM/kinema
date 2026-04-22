// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/widgets/PlayerSeekBar.h"

#include <QColor>
#include <QEvent>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPalette>
#include <QPointer>

#include <algorithm>
#include <cmath>

namespace kinema::ui::player::widgets {

namespace {

constexpr int kBarHeight = 4;
constexpr int kHoverBarHeight = 6;
constexpr int kVerticalPadding = 10;
constexpr int kKnobRadius = 6;
constexpr int kSideMargin = 8;
constexpr int kChapterTickWidth = 1;
constexpr int kChapterTickExtraHeight = 4;
constexpr int kHoverLabelYOffset = 22;

QString formatTime(double seconds)
{
    if (!std::isfinite(seconds) || seconds < 0.0) {
        return QStringLiteral("--:--");
    }
    const auto total = static_cast<qint64>(seconds);
    const auto h = total / 3600;
    const auto m = (total % 3600) / 60;
    const auto s = total % 60;
    if (h > 0) {
        return QStringLiteral("%1:%2:%3")
            .arg(h)
            .arg(m, 2, 10, QLatin1Char('0'))
            .arg(s, 2, 10, QLatin1Char('0'));
    }
    return QStringLiteral("%1:%2")
        .arg(m, 2, 10, QLatin1Char('0'))
        .arg(s, 2, 10, QLatin1Char('0'));
}

} // namespace

PlayerSeekBar::PlayerSeekBar(QWidget* parent)
    : QWidget(parent)
{
    setMouseTracking(true);
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_OpaquePaintEvent, false);
    setMinimumHeight(kVerticalPadding * 2 + kHoverBarHeight);
    setCursor(Qt::PointingHandCursor);

    // Hover tooltip. Kept hidden until the mouse enters the bar.
    m_hoverLabel = new QLabel(this);
    m_hoverLabel->setAttribute(Qt::WA_TransparentForMouseEvents);
    m_hoverLabel->setStyleSheet(QStringLiteral(
        "QLabel {"
        "  color: white;"
        "  background-color: rgba(0, 0, 0, 200);"
        "  padding: 2px 6px;"
        "  border-radius: 3px;"
        "}"));
    m_hoverLabel->hide();
}

QSize PlayerSeekBar::sizeHint() const
{
    return { 200, kVerticalPadding * 2 + kHoverBarHeight };
}

QSize PlayerSeekBar::minimumSizeHint() const
{
    return { 80, kVerticalPadding * 2 + kHoverBarHeight };
}

int PlayerSeekBar::trackLeftPx() const
{
    return kSideMargin;
}

int PlayerSeekBar::trackRightPx() const
{
    return std::max(trackLeftPx(), width() - kSideMargin);
}

void PlayerSeekBar::setDuration(double seconds)
{
    if (!std::isfinite(seconds) || seconds < 0.0) {
        seconds = 0.0;
    }
    if (qFuzzyCompare(1.0 + m_duration, 1.0 + seconds)) {
        return;
    }
    m_duration = seconds;
    update();
}

void PlayerSeekBar::setPosition(double seconds)
{
    if (!std::isfinite(seconds) || seconds < 0.0) {
        seconds = 0.0;
    }
    if (qFuzzyCompare(1.0 + m_position, 1.0 + seconds)) {
        return;
    }
    m_position = seconds;
    update();
}

void PlayerSeekBar::setCacheAhead(double seconds)
{
    if (!std::isfinite(seconds) || seconds < 0.0) {
        seconds = 0.0;
    }
    if (qFuzzyCompare(1.0 + m_cacheAhead, 1.0 + seconds)) {
        return;
    }
    m_cacheAhead = seconds;
    update();
}

void PlayerSeekBar::setChapters(const QList<double>& chapterStartsSec)
{
    m_chapters = chapterStartsSec;
    update();
}

double PlayerSeekBar::timestampForX(int x) const
{
    const int left = trackLeftPx();
    const int right = trackRightPx();
    if (right <= left || m_duration <= 0.0) {
        return 0.0;
    }
    const int clamped = std::clamp(x, left, right);
    const double frac = static_cast<double>(clamped - left)
        / static_cast<double>(right - left);
    return std::clamp(frac * m_duration, 0.0, m_duration);
}

int PlayerSeekBar::xForTimestamp(double seconds) const
{
    const int left = trackLeftPx();
    const int right = trackRightPx();
    if (m_duration <= 0.0 || right <= left) {
        return left;
    }
    const double frac = std::clamp(seconds / m_duration, 0.0, 1.0);
    return left
        + static_cast<int>(frac * (right - left) + 0.5);
}

void PlayerSeekBar::updateHover(int x)
{
    m_hoverX = std::clamp(x, trackLeftPx(), trackRightPx());
    if (m_duration <= 0.0) {
        m_hoverLabel->hide();
        update();
        return;
    }
    const double t = timestampForX(m_hoverX);
    m_hoverLabel->setText(formatTime(t));
    m_hoverLabel->adjustSize();

    // Centre the label above the hover point; clamp into the widget
    // so it doesn't fall off the edge for timestamps near 0:00 or
    // the end of the file.
    int lx = m_hoverX - m_hoverLabel->width() / 2;
    lx = std::clamp(lx, 0, width() - m_hoverLabel->width());
    const int ly = std::max(0, height() / 2 - kHoverLabelYOffset);
    m_hoverLabel->move(lx, ly);
    m_hoverLabel->show();
    m_hoverLabel->raise();
    update();
}

void PlayerSeekBar::clearHover()
{
    m_hoverX = -1;
    m_hoverLabel->hide();
    update();
}

void PlayerSeekBar::paintEvent(QPaintEvent* /*e*/)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);

    const int left = trackLeftPx();
    const int right = trackRightPx();
    const int trackY = height() / 2 - kBarHeight / 2;
    const bool hot = m_dragging || m_hoverX >= 0;
    const int barHeight = hot ? kHoverBarHeight : kBarHeight;
    const int barY = height() / 2 - barHeight / 2;

    // Track background (neutral, so it reads on top of any video).
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(255, 255, 255, 60));
    p.drawRoundedRect(QRect(left, barY, right - left, barHeight),
        barHeight / 2.0, barHeight / 2.0);

    // Buffered-ahead shading: from current position to
    // position + cacheAhead.
    if (m_duration > 0.0 && m_cacheAhead > 0.0) {
        const int bx0 = xForTimestamp(m_position);
        const int bx1 = xForTimestamp(
            std::min(m_position + m_cacheAhead, m_duration));
        if (bx1 > bx0) {
            p.setBrush(QColor(255, 255, 255, 110));
            p.drawRoundedRect(QRect(bx0, barY, bx1 - bx0, barHeight),
                barHeight / 2.0, barHeight / 2.0);
        }
    }

    // Elapsed fill.
    if (m_duration > 0.0 && m_position > 0.0) {
        const int ex = xForTimestamp(m_position);
        const auto accent = palette().color(QPalette::Highlight);
        p.setBrush(accent);
        p.drawRoundedRect(QRect(left, barY, ex - left, barHeight),
            barHeight / 2.0, barHeight / 2.0);
    }

    // Chapter ticks. Painted after the fills so they remain visible
    // even inside the elapsed / buffered regions.
    if (!m_chapters.isEmpty() && m_duration > 0.0) {
        p.setBrush(Qt::NoBrush);
        QPen pen(QColor(0, 0, 0, 180));
        pen.setWidth(kChapterTickWidth);
        p.setPen(pen);
        const int tickHeight = barHeight + kChapterTickExtraHeight;
        const int tickY = trackY - kChapterTickExtraHeight / 2;
        for (double t : m_chapters) {
            if (t <= 0.0 || t >= m_duration) {
                continue;
            }
            const int cx = xForTimestamp(t);
            p.drawLine(cx, tickY, cx, tickY + tickHeight);
        }
    }

    // Playhead knob.
    if (m_duration > 0.0) {
        const int ex = xForTimestamp(m_position);
        const auto accent = palette().color(QPalette::Highlight);
        p.setBrush(accent);
        p.setPen(QPen(Qt::white, 1.0));
        p.drawEllipse(QPoint(ex, height() / 2),
            kKnobRadius, kKnobRadius);
    }
}

void PlayerSeekBar::mousePressEvent(QMouseEvent* e)
{
    if (e->button() != Qt::LeftButton || m_duration <= 0.0) {
        QWidget::mousePressEvent(e);
        return;
    }
    m_dragging = true;
    updateHover(e->pos().x());
    e->accept();
}

void PlayerSeekBar::mouseMoveEvent(QMouseEvent* e)
{
    if (m_duration <= 0.0) {
        return;
    }
    updateHover(e->pos().x());
    e->accept();
}

void PlayerSeekBar::mouseReleaseEvent(QMouseEvent* e)
{
    if (e->button() != Qt::LeftButton) {
        QWidget::mouseReleaseEvent(e);
        return;
    }
    if (m_dragging) {
        m_dragging = false;
        if (m_duration > 0.0) {
            const double t = timestampForX(e->pos().x());
            Q_EMIT seekRequested(t);
        }
    }
    if (!rect().contains(e->pos())) {
        clearHover();
    }
    e->accept();
}

void PlayerSeekBar::leaveEvent(QEvent* e)
{
    if (!m_dragging) {
        clearHover();
    }
    QWidget::leaveEvent(e);
}

} // namespace kinema::ui::player::widgets
