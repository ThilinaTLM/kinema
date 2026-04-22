// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/PlaybackContext.h"

#include <QWidget>

class QLabel;
class QPushButton;

namespace kinema::ui::player::widgets {

class NextEpisodeBanner : public QWidget
{
    Q_OBJECT
public:
    explicit NextEpisodeBanner(QWidget* parent = nullptr);

    void showBanner(const api::PlaybackContext& ctx, int countdownSec);
    void setCountdown(int seconds);
    void hideBanner();
    bool acceptVisibleBanner();
    bool cancelVisibleBanner();

Q_SIGNALS:
    void playNowRequested();
    void cancelRequested();

protected:
    void paintEvent(QPaintEvent* e) override;

private:
    void updateLabels();

    api::PlaybackContext m_ctx;
    int m_countdownSec {0};

    QLabel* m_headingLabel {nullptr};
    QLabel* m_episodeLabel {nullptr};
    QLabel* m_countdownLabel {nullptr};
    QPushButton* m_playNowButton {nullptr};
    QPushButton* m_cancelButton {nullptr};
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
