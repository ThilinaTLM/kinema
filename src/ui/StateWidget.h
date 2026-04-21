// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QWidget>

class QLabel;
class QPushButton;
class QStackedLayout;

namespace kinema::ui {

/**
 * A placeholder widget shown instead of real content when a pane is empty,
 * loading, or in an error state.
 *
 * Only used via QStackedWidget in the panes that need it; when the pane has
 * actual content, switch the stack away from this widget.
 */
class StateWidget : public QWidget
{
    Q_OBJECT
public:
    explicit StateWidget(QWidget* parent = nullptr);

    void showIdle(const QString& title, const QString& body = {});
    void showLoading(const QString& text);
    void showError(const QString& text, bool retryable = false);

Q_SIGNALS:
    void retryRequested();

private:
    QLabel* m_icon;
    QLabel* m_title;
    QLabel* m_body;
    QPushButton* m_retry;
};

} // namespace kinema::ui
