// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/StatsOverlay.h"

#include <KLocalizedString>

#include <QColor>
#include <QGridLayout>
#include <QHideEvent>
#include <QLabel>
#include <QPaintEvent>
#include <QPainter>
#include <QShowEvent>
#include <QTimer>

#include <cmath>

namespace kinema::ui::player::widgets {

namespace {

constexpr int kRefreshMs = 1000;

QString placeholder()
{
    return i18nc("@info:status stats overlay placeholder for "
                 "unknown numeric value", "\u2014");
}

QString formatBitrate(double bps)
{
    if (!std::isfinite(bps) || bps <= 0.0) {
        return placeholder();
    }
    if (bps >= 1'000'000.0) {
        return i18nc("@info:status bitrate in megabits/sec",
            "%1 Mb/s",
            QString::number(bps / 1'000'000.0, 'f', 2));
    }
    if (bps >= 1'000.0) {
        return i18nc("@info:status bitrate in kilobits/sec",
            "%1 kb/s",
            QString::number(bps / 1'000.0, 'f', 1));
    }
    return i18nc("@info:status bitrate in bits/sec",
        "%1 b/s", QString::number(bps, 'f', 0));
}

QString formatFps(double fps)
{
    if (!std::isfinite(fps) || fps <= 0.0) {
        return placeholder();
    }
    return i18nc("@info:status frames per second",
        "%1 fps", QString::number(fps, 'f', 2));
}

QString formatResolution(int w, int h)
{
    if (w <= 0 || h <= 0) {
        return placeholder();
    }
    return QStringLiteral("%1\u00d7%2").arg(w).arg(h);
}

QString formatCodecs(const QString& v, const QString& a)
{
    if (v.isEmpty() && a.isEmpty()) {
        return placeholder();
    }
    if (v.isEmpty()) return a;
    if (a.isEmpty()) return v;
    return i18nc("@info:status video and audio codec names joined "
                 "with a bullet",
        "%1 \u00b7 %2", v, a);
}

QString formatCache(double secs)
{
    if (!std::isfinite(secs) || secs <= 0.0) {
        return placeholder();
    }
    return i18nc("@info:status demuxer cache in seconds",
        "%1 s", QString::number(secs, 'f', 1));
}

QLabel* makeLabel(QWidget* parent, bool value)
{
    auto* l = new QLabel(parent);
    l->setStyleSheet(value
        ? QStringLiteral(
            "QLabel { color: white; font-family: monospace; }")
        : QStringLiteral(
            "QLabel { color: rgba(255, 255, 255, 180); }"));
    return l;
}

} // namespace

StatsOverlay::StatsOverlay(QWidget* parent)
    : QWidget(parent)
{
    // Clicks pass through so play/pause-by-click still reaches the
    // video underneath; the overlay is read-only.
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setAutoFillBackground(false);
    setFocusPolicy(Qt::NoFocus);

    auto* grid = new QGridLayout(this);
    grid->setContentsMargins(12, 8, 12, 8);
    grid->setHorizontalSpacing(16);
    grid->setVerticalSpacing(2);

    int row = 0;
    auto addRow = [&](const QString& key, QLabel*& slot) {
        auto* k = makeLabel(this, /*value=*/false);
        k->setText(key);
        slot = makeLabel(this, /*value=*/true);
        grid->addWidget(k,    row, 0);
        grid->addWidget(slot, row, 1);
        ++row;
    };

    addRow(i18nc("@label stats overlay", "Resolution"),
        m_resolutionLabel);
    addRow(i18nc("@label stats overlay", "Codec"),
        m_codecsLabel);
    addRow(i18nc("@label stats overlay", "Frame rate"),
        m_fpsLabel);
    addRow(i18nc("@label stats overlay", "Video bitrate"),
        m_videoBitrateLabel);
    addRow(i18nc("@label stats overlay", "Audio bitrate"),
        m_audioBitrateLabel);
    addRow(i18nc("@label stats overlay", "Cache"),
        m_cacheLabel);

    m_refresh = new QTimer(this);
    m_refresh->setInterval(kRefreshMs);
    connect(m_refresh, &QTimer::timeout,
        this, &StatsOverlay::refreshLabels);

    refreshLabels();
    // Keep the overlay explicitly hidden until the user toggles it,
    // but only after m_refresh exists: hide() delivers hideEvent()
    // synchronously during construction.
    hide();
}

void StatsOverlay::toggleVisibility()
{
    setVisible(!isVisible());
}

void StatsOverlay::applyStats(const MpvWidget::VideoStats& stats)
{
    // Cache only; the 1 Hz timer does the actual relabelling while
    // visible. When hidden, the labels catch up on the next showEvent.
    m_latest = stats;
}

void StatsOverlay::showEvent(QShowEvent* e)
{
    refreshLabels();
    m_refresh->start();
    QWidget::showEvent(e);
}

void StatsOverlay::hideEvent(QHideEvent* e)
{
    if (m_refresh) {
        m_refresh->stop();
    }
    QWidget::hideEvent(e);
}

void StatsOverlay::paintEvent(QPaintEvent* /*e*/)
{
    // Translucent rounded backing so the monospaced values read
    // against any video content underneath.
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(0, 0, 0, 180));
    p.drawRoundedRect(rect(), 6, 6);
}

void StatsOverlay::refreshLabels()
{
    m_resolutionLabel->setText(
        formatResolution(m_latest.width, m_latest.height));
    m_codecsLabel->setText(
        formatCodecs(m_latest.videoCodec, m_latest.audioCodec));
    m_fpsLabel->setText(formatFps(m_latest.fps));
    m_videoBitrateLabel->setText(formatBitrate(m_latest.videoBitrate));
    m_audioBitrateLabel->setText(formatBitrate(m_latest.audioBitrate));
    m_cacheLabel->setText(formatCache(m_latest.cacheSeconds));
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
