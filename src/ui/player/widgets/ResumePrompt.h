// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QWidget>

class QLabel;
class QPushButton;
class QTimer;

namespace kinema::ui::player::widgets {

class ResumePrompt : public QWidget
{
    Q_OBJECT
public:
    explicit ResumePrompt(QWidget* parent = nullptr);

    void showPrompt(qint64 seconds);
    void hidePrompt();
    qint64 resumeSeconds() const { return m_resumeSeconds; }

Q_SIGNALS:
    void resumeRequested(qint64 seconds);
    void restartRequested();

protected:
    void paintEvent(QPaintEvent* e) override;
    void keyPressEvent(QKeyEvent* e) override;

private:
    void updateLabels();

    QLabel* m_titleLabel {nullptr};
    QLabel* m_messageLabel {nullptr};
    QPushButton* m_resumeButton {nullptr};
    QPushButton* m_restartButton {nullptr};
    QTimer* m_autoResumeTimer {nullptr};
    qint64 m_resumeSeconds {0};
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
