// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/NextEpisodeBanner.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QVBoxLayout>

namespace kinema::ui::player::widgets {

NextEpisodeBanner::NextEpisodeBanner(QWidget* parent)
    : QWidget(parent)
{
    setAttribute(Qt::WA_TransparentForMouseEvents, false);
    setAutoFillBackground(false);
    hide();

    m_headingLabel = new QLabel(i18nc("@title panel heading", "Next up"), this);
    m_headingLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: white; font-size: 15px; font-weight: 700; }"));

    m_episodeLabel = new QLabel(this);
    m_episodeLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: rgba(255, 255, 255, 220); }"));
    m_episodeLabel->setWordWrap(true);

    m_countdownLabel = new QLabel(this);
    m_countdownLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: rgba(255, 255, 255, 200); }"));

    m_playNowButton = new QPushButton(
        i18nc("@action:button", "Play now"), this);
    m_cancelButton = new QPushButton(
        i18nc("@action:button", "Cancel"), this);

    connect(m_playNowButton, &QPushButton::clicked,
        this, &NextEpisodeBanner::playNowRequested);
    connect(m_cancelButton, &QPushButton::clicked, this, [this] {
        hideBanner();
        Q_EMIT cancelRequested();
    });

    auto* buttonRow = new QHBoxLayout;
    buttonRow->setContentsMargins(0, 0, 0, 0);
    buttonRow->addWidget(m_playNowButton);
    buttonRow->addWidget(m_cancelButton);

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(18, 16, 18, 16);
    layout->setSpacing(8);
    layout->addWidget(m_headingLabel);
    layout->addWidget(m_episodeLabel);
    layout->addWidget(m_countdownLabel);
    layout->addLayout(buttonRow);
}

void NextEpisodeBanner::showBanner(const api::PlaybackContext& ctx,
    int countdownSec)
{
    m_ctx = ctx;
    m_countdownSec = qMax(0, countdownSec);
    updateLabels();
    show();
    raise();
}

void NextEpisodeBanner::setCountdown(int seconds)
{
    m_countdownSec = qMax(0, seconds);
    updateLabels();
}

void NextEpisodeBanner::hideBanner()
{
    hide();
}

bool NextEpisodeBanner::acceptVisibleBanner()
{
    if (!isVisible()) {
        return false;
    }
    hideBanner();
    Q_EMIT playNowRequested();
    return true;
}

bool NextEpisodeBanner::cancelVisibleBanner()
{
    if (!isVisible()) {
        return false;
    }
    hideBanner();
    Q_EMIT cancelRequested();
    return true;
}

void NextEpisodeBanner::paintEvent(QPaintEvent* /*e*/)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(15, 15, 15, 228));
    p.drawRoundedRect(rect(), 12, 12);
}

void NextEpisodeBanner::updateLabels()
{
    const QString subtitle = m_ctx.episodeTitle.isEmpty()
        ? m_ctx.title
        : m_ctx.episodeTitle;
    const QString heading = m_ctx.seriesTitle.isEmpty()
        ? m_ctx.title
        : i18nc("@info next episode label",
            "%1 — %2", m_ctx.seriesTitle, subtitle);
    m_episodeLabel->setText(heading);
    m_countdownLabel->setText(i18nc("@info countdown",
        "Starts in %1…", m_countdownSec));
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
