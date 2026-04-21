// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QWidget>

class QCheckBox;
class QComboBox;

namespace kinema::config {
class AppearanceSettings;
class SearchSettings;
class TorrentioSettings;
}

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
    GeneralSettingsPage(config::SearchSettings& search,
        config::TorrentioSettings& torrentio,
        config::AppearanceSettings& appearance,
        QWidget* parent = nullptr);

    /// Populate widgets from the current settings.
    void load();
    /// Write widget state back to settings. Only called on Apply/OK.
    void apply();
    /// Restore widget state to hard-coded defaults (does not touch disk).
    void resetToDefaults();

private:
    config::SearchSettings& m_search;
    config::TorrentioSettings& m_torrentio;
    config::AppearanceSettings& m_appearance;

    QComboBox* m_searchKindCombo {};
    QComboBox* m_sortCombo {};
    QCheckBox* m_closeToTrayCheck {};
};

} // namespace kinema::ui::settings
