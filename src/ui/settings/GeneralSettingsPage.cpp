// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/settings/GeneralSettingsPage.h"

#include "api/Types.h"
#include "config/Config.h"
#include "core/TorrentioConfig.h"

#include <KLocalizedString>

#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QLabel>
#include <QSystemTrayIcon>
#include <QVBoxLayout>

namespace kinema::ui::settings {

namespace {

int sortModeToIndex(core::torrentio::SortMode m)
{
    switch (m) {
    case core::torrentio::SortMode::Seeders:
        return 0;
    case core::torrentio::SortMode::Size:
        return 1;
    case core::torrentio::SortMode::QualitySize:
        return 2;
    }
    return 0;
}

core::torrentio::SortMode indexToSortMode(int idx)
{
    switch (idx) {
    case 1:
        return core::torrentio::SortMode::Size;
    case 2:
        return core::torrentio::SortMode::QualitySize;
    default:
        return core::torrentio::SortMode::Seeders;
    }
}

} // namespace

GeneralSettingsPage::GeneralSettingsPage(QWidget* parent)
    : QWidget(parent)
{
    m_searchKindCombo = new QComboBox(this);
    m_searchKindCombo->addItem(i18nc("@item:inlistbox", "Movie"));
    m_searchKindCombo->addItem(i18nc("@item:inlistbox", "Series"));

    m_sortCombo = new QComboBox(this);
    m_sortCombo->addItem(i18nc("@item:inlistbox sort mode", "Seeders"));
    m_sortCombo->addItem(i18nc("@item:inlistbox sort mode", "Size"));
    m_sortCombo->addItem(i18nc("@item:inlistbox sort mode", "Quality & Size"));

    m_closeToTrayCheck = new QCheckBox(
        i18nc("@option:check",
            "Close main window to system tray (keep Kinema running)"),
        this);
    if (QSystemTrayIcon::isSystemTrayAvailable()) {
        m_closeToTrayCheck->setToolTip(i18nc("@info:tooltip",
            "When enabled, closing the main window hides Kinema to the "
            "system tray instead of quitting. The player window keeps "
            "playing. Use the tray icon or Ctrl+Q to quit."));
    } else {
        m_closeToTrayCheck->setEnabled(false);
        m_closeToTrayCheck->setToolTip(i18nc("@info:tooltip",
            "Your desktop does not expose a system tray, so "
            "close-to-tray is unavailable."));
    }

    auto* form = new QFormLayout;
    form->addRow(i18nc("@label:listbox", "Default search kind:"),
        m_searchKindCombo);
    form->addRow(i18nc("@label:listbox", "Default torrent sort:"),
        m_sortCombo);
    form->addRow(m_closeToTrayCheck);

    auto* hint = new QLabel(
        i18nc("@info",
            "The default sort is applied to every Torrentio query. "
            "You can still re-sort any column by clicking its header."),
        this);
    hint->setWordWrap(true);
    hint->setEnabled(false);

    auto* layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(hint);
    layout->addStretch(1);

    load();
}

void GeneralSettingsPage::load()
{
    const auto& cfg = config::Config::instance();
    m_searchKindCombo->setCurrentIndex(
        cfg.searchKind() == api::MediaKind::Series ? 1 : 0);
    m_sortCombo->setCurrentIndex(sortModeToIndex(cfg.defaultSort()));
    m_closeToTrayCheck->setChecked(cfg.closeToTray());
}

void GeneralSettingsPage::apply()
{
    auto& cfg = config::Config::instance();
    cfg.setSearchKind(m_searchKindCombo->currentIndex() == 1
            ? api::MediaKind::Series
            : api::MediaKind::Movie);
    cfg.setDefaultSort(indexToSortMode(m_sortCombo->currentIndex()));
    cfg.setCloseToTray(m_closeToTrayCheck->isChecked());
}

void GeneralSettingsPage::resetToDefaults()
{
    m_searchKindCombo->setCurrentIndex(0);
    m_sortCombo->setCurrentIndex(sortModeToIndex(core::torrentio::SortMode::Seeders));
    m_closeToTrayCheck->setChecked(true);
}

} // namespace kinema::ui::settings
