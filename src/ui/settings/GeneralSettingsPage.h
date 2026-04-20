// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QWidget>

class QComboBox;

namespace kinema::ui::settings {

/**
 * General Kinema preferences: default search kind, default Torrentio
 * sort mode. Writes are committed through the owning SettingsDialog's
 * Apply/OK via apply().
 */
class GeneralSettingsPage : public QWidget
{
    Q_OBJECT
public:
    explicit GeneralSettingsPage(QWidget* parent = nullptr);

    /// Populate widgets from the current Config state.
    void load();
    /// Write widget state back into Config. Only called on Apply/OK.
    void apply();
    /// Restore widget state to hard-coded defaults (does not touch disk).
    void resetToDefaults();

private:
    QComboBox* m_searchKindCombo {};
    QComboBox* m_sortCombo {};
};

} // namespace kinema::ui::settings
