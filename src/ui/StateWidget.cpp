// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

namespace kinema::ui {

StateWidget::StateWidget(QWidget* parent)
    : QWidget(parent)
{
    m_icon = new QLabel(this);
    m_icon->setAlignment(Qt::AlignCenter);
    m_icon->setMinimumSize(64, 64);

    m_title = new QLabel(this);
    m_title->setAlignment(Qt::AlignCenter);
    auto f = m_title->font();
    f.setPointSizeF(f.pointSizeF() * 1.2);
    f.setBold(true);
    m_title->setFont(f);
    m_title->setWordWrap(true);

    m_body = new QLabel(this);
    m_body->setAlignment(Qt::AlignCenter);
    m_body->setWordWrap(true);
    m_body->setTextFormat(Qt::PlainText);

    m_retry = new QPushButton(i18nc("@action:button", "Retry"), this);
    m_retry->setVisible(false);
    m_retry->setIcon(QIcon::fromTheme(QStringLiteral("view-refresh")));
    connect(m_retry, &QPushButton::clicked, this, &StateWidget::retryRequested);

    auto* layout = new QVBoxLayout(this);
    layout->addStretch();
    layout->addWidget(m_icon);
    layout->addWidget(m_title);
    layout->addWidget(m_body);
    auto* retryRow = new QHBoxLayout;
    retryRow->addStretch();
    retryRow->addWidget(m_retry);
    retryRow->addStretch();
    layout->addLayout(retryRow);
    layout->addStretch();
}

void StateWidget::showIdle(const QString& title, const QString& body)
{
    m_icon->setPixmap(QIcon::fromTheme(QStringLiteral("search"))
                          .pixmap(QSize(64, 64)));
    m_title->setText(title);
    m_body->setText(body);
    m_retry->setVisible(false);
}

void StateWidget::showLoading(const QString& text)
{
    m_icon->setPixmap(QIcon::fromTheme(QStringLiteral("view-refresh"))
                          .pixmap(QSize(64, 64)));
    m_title->setText(text);
    m_body->clear();
    m_retry->setVisible(false);
}

void StateWidget::showError(const QString& text, bool retryable)
{
    m_icon->setPixmap(QIcon::fromTheme(QStringLiteral("dialog-error"))
                          .pixmap(QSize(64, 64)));
    m_title->setText(i18nc("@label", "Something went wrong"));
    m_body->setText(text);
    m_retry->setVisible(retryable);
}

} // namespace kinema::ui
