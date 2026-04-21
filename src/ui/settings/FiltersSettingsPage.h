// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QHash>
#include <QString>
#include <QWidget>

class QCheckBox;
class QPlainTextEdit;

namespace kinema::config {
class FilterSettings;
}

namespace kinema::ui::settings {

/**
 * Filter defaults applied to every torrent fetch.
 *
 * - Resolution / variant exclusions go into Torrentio's `qualityfilter`
 *   URL param (server-side, so excluded rows never arrive).
 * - Keyword blocklist is a case-insensitive substring filter applied
 *   client-side after fetch, against releaseName + detailsText.
 */
class FiltersSettingsPage : public QWidget
{
    Q_OBJECT
public:
    FiltersSettingsPage(config::FilterSettings& settings,
        QWidget* parent = nullptr);

    void load();
    void apply();
    void resetToDefaults();

private:
    config::FilterSettings& m_settings;
    /// token ("4k", "1080p", …) → checkbox
    QHash<QString, QCheckBox*> m_resolutionChecks;
    /// token ("cam", "scr", "threed", …) → checkbox
    QHash<QString, QCheckBox*> m_categoryChecks;
    QPlainTextEdit* m_blocklistEdit {};
};

} // namespace kinema::ui::settings
