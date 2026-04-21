// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QWidget>

class QLineEdit;
class QRadioButton;

namespace kinema::config {
class PlayerSettings;
}

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
    PlayerSettingsPage(config::PlayerSettings& settings,
        QWidget* parent = nullptr);

    void load();
    void apply();
    void resetToDefaults();

private:
    void updateCustomEnabled();

    config::PlayerSettings& m_settings;
    QRadioButton* m_embeddedRadio {};
    QRadioButton* m_mpvRadio {};
    QRadioButton* m_vlcRadio {};
    QRadioButton* m_customRadio {};
    QLineEdit* m_customCmdEdit {};
};

} // namespace kinema::ui::settings
