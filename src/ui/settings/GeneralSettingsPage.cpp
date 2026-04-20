// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/settings/GeneralSettingsPage.h"

#include "api/Types.h"
#include "config/Config.h"
#include "core/TorrentioConfig.h"

#include <KLocalizedString>

#include <QComboBox>
#include <QFormLayout>
#include <QLabel>
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

    auto* form = new QFormLayout;
    form->addRow(i18nc("@label:listbox", "Default search kind:"),
        m_searchKindCombo);
    form->addRow(i18nc("@label:listbox", "Default torrent sort:"),
        m_sortCombo);

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
}

void GeneralSettingsPage::apply()
{
    auto& cfg = config::Config::instance();
    cfg.setSearchKind(m_searchKindCombo->currentIndex() == 1
            ? api::MediaKind::Series
            : api::MediaKind::Movie);
    cfg.setDefaultSort(indexToSortMode(m_sortCombo->currentIndex()));
}

void GeneralSettingsPage::resetToDefaults()
{
    m_searchKindCombo->setCurrentIndex(0);
    m_sortCombo->setCurrentIndex(sortModeToIndex(core::torrentio::SortMode::Seeders));
}

} // namespace kinema::ui::settings
