// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/TransportBar.h"

#include "ui/player/MpvWidget.h"
#include "ui/player/widgets/PlayerSeekBar.h"
#include "ui/player/widgets/SkipChapterButton.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLinearGradient>
#include <QPainter>
#include <QSlider>
#include <QToolButton>

namespace kinema::ui::player::widgets {

namespace {

constexpr int kVolumeMax = 100;

QString formatTime(double seconds)
{
    if (seconds < 0.0 || !std::isfinite(seconds)) {
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

QToolButton* makeIconButton(const QString& iconName,
    const QString& tooltip, QWidget* parent)
{
    auto* b = new QToolButton(parent);
    b->setIcon(QIcon::fromTheme(iconName));
    b->setToolTip(tooltip);
    b->setAutoRaise(true);
    b->setIconSize(QSize(22, 22));
    b->setFocusPolicy(Qt::NoFocus);
    return b;
}

} // namespace

TransportBar::TransportBar(MpvWidget* mpv, QWidget* parent)
    : QWidget(parent)
    , m_mpv(mpv)
{
    // The bar itself catches mouse events; the parent PlayerOverlay
    // forwards motion through its event filter to re-arm the hide
    // timer, so widgets inside the bar interact normally.
    setAttribute(Qt::WA_TransparentForMouseEvents, false);
    setAutoFillBackground(false);

    m_playButton = makeIconButton(
        QStringLiteral("media-playback-start"),
        i18nc("@info:tooltip", "Play / Pause (Space)"),
        this);
    connect(m_playButton, &QToolButton::clicked, this, [this] {
        if (m_mpv) {
            m_mpv->cyclePause();
        }
    });

    m_seekBar = new PlayerSeekBar(this);
    connect(m_seekBar, &PlayerSeekBar::seekRequested,
        this, &TransportBar::onSeekRequested);

    m_timeLabel = new QLabel(QStringLiteral("--:-- / --:--"), this);
    m_timeLabel->setStyleSheet(
        QStringLiteral("QLabel { color: white; }"));

    m_muteButton = makeIconButton(
        QStringLiteral("audio-volume-high"),
        i18nc("@info:tooltip", "Mute (M)"),
        this);
    connect(m_muteButton, &QToolButton::clicked, this, [this] {
        if (m_mpv) {
            m_mpv->setMuted(!m_mpv->isMuted());
        }
    });

    m_volumeSlider = new QSlider(Qt::Horizontal, this);
    m_volumeSlider->setRange(0, kVolumeMax);
    m_volumeSlider->setValue(100);
    m_volumeSlider->setFixedWidth(100);
    m_volumeSlider->setFocusPolicy(Qt::NoFocus);
    connect(m_volumeSlider, &QSlider::sliderMoved,
        this, &TransportBar::onVolumeSliderMoved);
    connect(m_volumeSlider, &QSlider::valueChanged, this, [this](int v) {
        // Keyboard / wheel adjustments from the user also need to
        // flow to mpv.
        if (m_volumeSlider->isSliderDown()) {
            return; // already handled via sliderMoved
        }
        if (m_mpv) {
            m_mpv->setVolumePercent(static_cast<double>(v));
        }
    });

    m_skipButton = new SkipChapterButton(this);
    connect(m_skipButton, &QPushButton::clicked,
        this, &TransportBar::skipRequested);

    m_fullscreenButton = makeIconButton(
        QStringLiteral("view-fullscreen"),
        i18nc("@info:tooltip", "Fullscreen (F)"),
        this);
    connect(m_fullscreenButton, &QToolButton::clicked,
        this, &TransportBar::toggleFullscreenRequested);

    m_closeButton = makeIconButton(
        QStringLiteral("window-close"),
        i18nc("@info:tooltip", "Close player (Esc)"),
        this);
    connect(m_closeButton, &QToolButton::clicked,
        this, &TransportBar::closeRequested);

    auto* lay = new QHBoxLayout(this);
    lay->setContentsMargins(12, 6, 12, 6);
    lay->setSpacing(6);
    lay->addWidget(m_playButton);
    lay->addWidget(m_seekBar, /*stretch=*/1);
    lay->addWidget(m_timeLabel);
    lay->addSpacing(8);
    lay->addWidget(m_muteButton);
    lay->addWidget(m_volumeSlider);
    lay->addSpacing(8);
    lay->addWidget(m_skipButton);
    lay->addWidget(m_fullscreenButton);
    lay->addWidget(m_closeButton);

    if (m_mpv) {
        connect(m_mpv, &MpvWidget::positionChanged,
            this, &TransportBar::onPositionChanged);
        connect(m_mpv, &MpvWidget::durationChanged,
            this, &TransportBar::onDurationChanged);
        connect(m_mpv, &MpvWidget::pausedChanged,
            this, &TransportBar::onPausedChanged);
        connect(m_mpv, &MpvWidget::volumeChanged,
            this, &TransportBar::onVolumeChanged);
        connect(m_mpv, &MpvWidget::muteChanged,
            this, &TransportBar::onMuteChanged);
        connect(m_mpv, &MpvWidget::cacheAheadChanged,
            this, &TransportBar::onCacheAheadChanged);
    }

    refreshPlayIcon();
    refreshVolumeIcon();
    refreshTimeLabel();
}

void TransportBar::paintEvent(QPaintEvent* /*e*/)
{
    // Translucent gradient backing so the controls read against any
    // video content underneath.
    QPainter p(this);
    QLinearGradient g(rect().topLeft(), rect().bottomLeft());
    g.setColorAt(0.0, QColor(0, 0, 0, 0));
    g.setColorAt(0.4, QColor(0, 0, 0, 140));
    g.setColorAt(1.0, QColor(0, 0, 0, 200));
    p.fillRect(rect(), g);
}

void TransportBar::setChapters(const QList<double>& chapterStartsSec)
{
    m_seekBar->setChapters(chapterStartsSec);
}

void TransportBar::showSkipChapter(const QString& label)
{
    m_skipButton->showLabel(label);
}

void TransportBar::hideSkipChapter()
{
    m_skipButton->hide();
}

void TransportBar::onPositionChanged(double seconds)
{
    m_position = seconds;
    m_seekBar->setPosition(seconds);
    refreshTimeLabel();
}

void TransportBar::onDurationChanged(double seconds)
{
    m_duration = seconds;
    m_seekBar->setDuration(seconds);
    refreshTimeLabel();
}

void TransportBar::onPausedChanged(bool paused)
{
    m_paused = paused;
    refreshPlayIcon();
}

void TransportBar::onVolumeChanged(double v)
{
    m_volume = v;
    if (!m_volumeSlider->isSliderDown()) {
        QSignalBlocker guard(m_volumeSlider);
        m_volumeSlider->setValue(
            static_cast<int>(std::clamp(v, 0.0, 100.0) + 0.5));
    }
    refreshVolumeIcon();
}

void TransportBar::onMuteChanged(bool muted)
{
    m_muted = muted;
    refreshVolumeIcon();
}

void TransportBar::onCacheAheadChanged(double seconds)
{
    m_seekBar->setCacheAhead(seconds);
}

void TransportBar::onSeekRequested(double seconds)
{
    if (!m_mpv) {
        return;
    }
    m_mpv->seekAbsolute(seconds);
}

void TransportBar::onVolumeSliderMoved(int value)
{
    if (m_mpv) {
        m_mpv->setVolumePercent(static_cast<double>(value));
    }
}

void TransportBar::refreshTimeLabel()
{
    m_timeLabel->setText(QStringLiteral("%1 / %2")
            .arg(formatTime(m_position), formatTime(m_duration)));
}

void TransportBar::refreshPlayIcon()
{
    m_playButton->setIcon(QIcon::fromTheme(m_paused
            ? QStringLiteral("media-playback-start")
            : QStringLiteral("media-playback-pause")));
}

void TransportBar::refreshVolumeIcon()
{
    QString name = QStringLiteral("audio-volume-high");
    if (m_muted || m_volume <= 0.0) {
        name = QStringLiteral("audio-volume-muted");
    } else if (m_volume < 34.0) {
        name = QStringLiteral("audio-volume-low");
    } else if (m_volume < 67.0) {
        name = QStringLiteral("audio-volume-medium");
    }
    m_muteButton->setIcon(QIcon::fromTheme(name));
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
