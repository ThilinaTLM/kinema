// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/DiscoverSection.h"

#include "ui/DiscoverCardDelegate.h"
#include "ui/DiscoverGridView.h"
#include "ui/DiscoverRowModel.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QPalette>
#include <QResizeEvent>
#include <QStackedWidget>
#include <QToolButton>
#include <QVBoxLayout>

#include <algorithm>

namespace kinema::ui {

DiscoverSection::DiscoverSection(
    const QString& title, ImageLoader* images, QWidget* parent)
    : QWidget(parent)
{
    m_title = new QLabel(title, this);
    auto f = m_title->font();
    f.setBold(true);
    f.setPointSizeF(f.pointSizeF() * 1.1);
    m_title->setFont(f);

    // Toggle lives in a footer row below the grid as a compact
    // inline action — small enough to read as a secondary control,
    // not another card.
    m_toggle = new QToolButton(this);
    m_toggle->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_toggle->setIconSize(QSize(16, 16));
    m_toggle->setCursor(Qt::PointingHandCursor);
    m_toggle->setAutoRaise(true);
    m_toggle->hide(); // shown by updateToggleVisibility() once items land
    connect(m_toggle, &QToolButton::clicked, this, [this] {
        setCollapsed(!m_collapsed);
    });

    auto* header = new QHBoxLayout;
    header->setContentsMargins(0, 0, 0, 0);
    header->addWidget(m_title);
    header->addStretch(1);

    m_model = new DiscoverRowModel(this);

    m_view = new DiscoverGridView(this);
    // Blend the grid viewport with the surrounding window chrome
    // instead of the darker QPalette::Base default.
    m_view->viewport()->setBackgroundRole(QPalette::Window);
    m_view->setModel(m_model);
    m_view->setMaxVisibleRows(m_collapsedRows);
    m_delegate = new DiscoverCardDelegate(images, m_view);
    m_view->setItemDelegate(m_delegate);

    auto emitActivated = [this](const QModelIndex& idx) {
        if (idx.isValid()) {
            Q_EMIT itemActivated(idx);
        }
    };
    connect(m_view, &QAbstractItemView::activated, this, emitActivated);
    connect(m_view, &QAbstractItemView::clicked, this, emitActivated);

    m_state = new StateWidget(this);
    connect(m_state, &StateWidget::retryRequested,
        this, &DiscoverSection::retryRequested);

    m_stack = new QStackedWidget(this);
    m_stack->addWidget(m_state); // idx 0
    m_stack->addWidget(m_view);  // idx 1

    // Rebalance the toggle when the model or column count changes.
    connect(m_model, &QAbstractItemModel::modelReset,
        this, &DiscoverSection::updateToggleVisibility);
    connect(m_model, &QAbstractItemModel::rowsInserted,
        this, [this](const QModelIndex&, int, int) {
            updateToggleVisibility();
        });
    connect(m_model, &QAbstractItemModel::rowsRemoved,
        this, [this](const QModelIndex&, int, int) {
            updateToggleVisibility();
        });

    auto* footer = new QHBoxLayout;
    footer->setContentsMargins(0, 8, 0, 0);
    footer->addStretch(1);
    footer->addWidget(m_toggle);
    footer->addStretch(1);

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(4);
    root->addLayout(header);
    root->addWidget(m_stack);
    root->addLayout(footer);

    applyCollapsedState();
}

void DiscoverSection::showLoading()
{
    m_showingItems = false;
    m_state->showLoading(QString {});
    m_stack->setCurrentWidget(m_state);
    setCollapsed(true);
    m_toggle->setVisible(false);
}

void DiscoverSection::showEmpty(const QString& body)
{
    m_showingItems = false;
    m_state->showIdle(body, QString {});
    m_stack->setCurrentWidget(m_state);
    m_toggle->setVisible(false);
}

void DiscoverSection::showError(const QString& message, bool retryable)
{
    m_showingItems = false;
    m_state->showError(message, retryable);
    m_stack->setCurrentWidget(m_state);
    m_toggle->setVisible(false);
}

void DiscoverSection::showItems()
{
    m_showingItems = true;
    m_stack->setCurrentWidget(m_view);
    applyCollapsedState();
    updateToggleVisibility();
}

void DiscoverSection::setCollapsed(bool collapsed)
{
    if (m_collapsed == collapsed) {
        return;
    }
    m_collapsed = collapsed;
    applyCollapsedState();
    updateToggleVisibility();
}

void DiscoverSection::setCollapsedRows(int rows)
{
    if (rows < 1) {
        rows = 1;
    }
    if (rows == m_collapsedRows) {
        return;
    }
    m_collapsedRows = rows;
    applyCollapsedState();
    updateToggleVisibility();
}

void DiscoverSection::applyCollapsedState()
{
    m_view->setMaxVisibleRows(m_collapsed ? m_collapsedRows : -1);
    if (m_collapsed) {
        m_toggle->setIcon(QIcon::fromTheme(QStringLiteral("go-down-symbolic"),
            QIcon::fromTheme(QStringLiteral("go-down"))));
        // Label with hidden count (e.g. "Show 12 more") — refreshed
        // again in updateToggleVisibility() once the column count is
        // known.
        m_toggle->setText(i18nc("@action:button", "Show more"));
    } else {
        m_toggle->setIcon(QIcon::fromTheme(QStringLiteral("go-up-symbolic"),
            QIcon::fromTheme(QStringLiteral("go-up"))));
        m_toggle->setText(i18nc("@action:button", "Show less"));
    }
}

void DiscoverSection::updateToggleVisibility()
{
    if (!m_showingItems) {
        m_toggle->setVisible(false);
        return;
    }
    // Use cell count (rows * columns), not just row count, so the
    // toggle hides when the last collapsed row is only partially
    // filled but already shows every item.
    const int total = m_model ? m_model->rowCount() : 0;
    const int cols = m_view->currentColumnCount();
    const int collapsedCells = m_collapsedRows * cols;
    const int hidden = std::max(0, total - collapsedCells);
    const bool expandable = hidden > 0;
    m_toggle->setVisible(expandable);
    if (!expandable) {
        return;
    }
    if (m_collapsed) {
        m_toggle->setText(
            i18ncp("@action:button, %1 is the number of hidden items",
                "Show %1 more", "Show %1 more", hidden));
    } else {
        m_toggle->setText(i18nc("@action:button", "Show less"));
    }
}

void DiscoverSection::resizeEvent(QResizeEvent* e)
{
    QWidget::resizeEvent(e);
    // Column count may have shifted with width → re-evaluate toggle.
    updateToggleVisibility();
}

} // namespace kinema::ui
