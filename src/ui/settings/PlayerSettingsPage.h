// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QWidget>

class QLineEdit;
class QRadioButton;

namespace kinema::ui::settings {

/**
 * Player preferences: mpv / VLC / custom command.
 *
 * Availability badges reflect $PATH lookup done up-front (via
 * core::player::isAvailable). Missing players are NOT disabled — the
 * user may install them later — but their label gets a hint.
 */
class PlayerSettingsPage : public QWidget
{
    Q_OBJECT
public:
    explicit PlayerSettingsPage(QWidget* parent = nullptr);

    void load();
    void apply();
    void resetToDefaults();

private:
    void updateCustomEnabled();

    QRadioButton* m_embeddedRadio {};
    QRadioButton* m_mpvRadio {};
    QRadioButton* m_vlcRadio {};
    QRadioButton* m_customRadio {};
    QLineEdit* m_customCmdEdit {};
};

} // namespace kinema::ui::settings
