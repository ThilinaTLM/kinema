// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/TorrentioSectionViewModel.h"

#include "api/torrentio/TorrentioConfig.h"
#include "config/TorrentioSettings.h"
#include "domain/Indexer.h"
#include "domain/Media.h"

namespace kinema::ui::qml::settings {

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

TorrentioSectionViewModel::TorrentioSectionViewModel(api::IndexerSelector* indexers,
                                                     config::TorrentioSettings& settings,
                                                     QObject* parent)
    : IndexerSectionViewModelBase(indexers, parent), m_settings(settings)
{
    connect(&m_settings,
            &config::TorrentioSettings::defaultSortChanged,
            this,
            &TorrentioSectionViewModel::defaultSortChanged);
    connect(&m_settings,
            &config::TorrentioSettings::baseUrlChanged,
            this,
            &TorrentioSectionViewModel::baseUrlChanged);
}

int TorrentioSectionViewModel::defaultSort() const
{
    return sortModeToIndex(m_settings.defaultSort());
}

QString TorrentioSectionViewModel::baseUrl() const
{
    return m_settings.baseUrl();
}

QString TorrentioSectionViewModel::defaultBaseUrlString() const
{
    return config::TorrentioSettings::defaultBaseUrl();
}

void TorrentioSectionViewModel::setDefaultSort(int sort)
{
    const auto next = indexToSortMode(sort);
    if (m_settings.defaultSort() == next) {
        return;
    }
    m_settings.setDefaultSort(next);
    // settings emits defaultSortChanged → we re-emit via connect.
}

void TorrentioSectionViewModel::setBaseUrl(const QString& url)
{
    m_settings.setBaseUrl(url);
}

void TorrentioSectionViewModel::resetBaseUrl()
{
    m_settings.setBaseUrl(config::TorrentioSettings::defaultBaseUrl());
}

domain::IndexerKind TorrentioSectionViewModel::indexerKind() const
{
    return domain::IndexerKind::Torrentio;
}

QString TorrentioSectionViewModel::providerName() const
{
    return QStringLiteral("Torrentio");
}

} // namespace kinema::ui::qml::settings
