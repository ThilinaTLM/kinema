// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/ResumePrompt.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QTimer>
#include <QVBoxLayout>

namespace kinema::ui::player::widgets {

namespace {

constexpr int kAutoResumeMs = 15000;

QString formatTime(qint64 seconds)
{
    const auto h = seconds / 3600;
    const auto m = (seconds % 3600) / 60;
    const auto s = seconds % 60;
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

ResumePrompt::ResumePrompt(QWidget* parent)
    : QWidget(parent)
{
    setAttribute(Qt::WA_NoSystemBackground);
    setAutoFillBackground(false);
    setFocusPolicy(Qt::StrongFocus);
    hide();

    m_titleLabel = new QLabel(
        i18nc("@title panel heading", "Resume playback?"), this);
    m_titleLabel->setAlignment(Qt::AlignCenter);
    m_titleLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: white; font-size: 16px; font-weight: 700; }"));

    m_messageLabel = new QLabel(this);
    m_messageLabel->setAlignment(Qt::AlignCenter);
    m_messageLabel->setStyleSheet(QStringLiteral(
        "QLabel { color: rgba(255, 255, 255, 220); }"));

    m_resumeButton = new QPushButton(this);
    m_restartButton = new QPushButton(
        i18nc("@action:button", "Start over"), this);

    connect(m_resumeButton, &QPushButton::clicked, this, [this] {
        hidePrompt();
        Q_EMIT resumeRequested(m_resumeSeconds);
    });
    connect(m_restartButton, &QPushButton::clicked,
        this, [this] {
            hidePrompt();
            Q_EMIT restartRequested();
        });

    auto* buttonRow = new QHBoxLayout;
    buttonRow->addStretch(1);
    buttonRow->addWidget(m_resumeButton);
    buttonRow->addWidget(m_restartButton);
    buttonRow->addStretch(1);

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(28, 24, 28, 24);
    layout->setSpacing(10);
    layout->addWidget(m_titleLabel);
    layout->addWidget(m_messageLabel);
    layout->addLayout(buttonRow);

    m_autoResumeTimer = new QTimer(this);
    m_autoResumeTimer->setSingleShot(true);
    m_autoResumeTimer->setInterval(kAutoResumeMs);
    connect(m_autoResumeTimer, &QTimer::timeout, this, [this] {
        hidePrompt();
        Q_EMIT resumeRequested(m_resumeSeconds);
    });
}

void ResumePrompt::showPrompt(qint64 seconds)
{
    m_resumeSeconds = qMax<qint64>(0, seconds);
    updateLabels();
    show();
    raise();
    setFocus(Qt::OtherFocusReason);
    m_autoResumeTimer->start();
}

void ResumePrompt::hidePrompt()
{
    m_autoResumeTimer->stop();
    hide();
}

void ResumePrompt::paintEvent(QPaintEvent* /*e*/)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(15, 15, 15, 228));
    p.drawRoundedRect(rect(), 12, 12);
}

void ResumePrompt::keyPressEvent(QKeyEvent* e)
{
    if (e->key() == Qt::Key_Return || e->key() == Qt::Key_Enter) {
        hidePrompt();
        Q_EMIT resumeRequested(m_resumeSeconds);
        e->accept();
        return;
    }
    if (e->key() == Qt::Key_Escape) {
        hidePrompt();
        Q_EMIT restartRequested();
        e->accept();
        return;
    }
    QWidget::keyPressEvent(e);
}

void ResumePrompt::updateLabels()
{
    const auto formatted = formatTime(m_resumeSeconds);
    m_messageLabel->setText(i18nc("@info",
        "Resume from %1?", formatted));
    m_resumeButton->setText(i18nc("@action:button",
        "Resume at %1", formatted));
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
