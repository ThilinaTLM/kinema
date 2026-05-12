// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/IndexerSettingsViewModel.h"
#include "api/IndexerSelector.h"
#include "config/IndexerSettings.h"
#include "config/PeerflixSettings.h"
#include "config/TorrentioSettings.h"
#include "domain/Indexer.h"

namespace kinema::ui::qml::settings {

// ============================== Indexers: parent VM ======================

IndexerSettingsViewModel::IndexerSettingsViewModel(
    api::IndexerSelector* indexers,
    config::IndexerSettings& indexerSettings,
    config::TorrentioSettings& torrentioSettings,
    config::PeerflixSettings& peerflixSettings,
    QObject* parent)
    : QObject(parent)
    , m_settings(indexerSettings)
    , m_torrentio(new TorrentioSectionViewModel(indexers,
          torrentioSettings, this))
    , m_peerflix(new PeerflixSectionViewModel(indexers,
          peerflixSettings, this))
{
    connect(&m_settings, &config::IndexerSettings::activeIndexerChanged,
        this, [this](domain::IndexerKind) {
            Q_EMIT activeIndexerChanged();
        });
}

int IndexerSettingsViewModel::activeIndexer() const
{
    return static_cast<int>(m_settings.activeIndexer());
}

void IndexerSettingsViewModel::setActiveIndexer(int kind)
{
    auto value = domain::IndexerKind::Torrentio;
    switch (kind) {
    case static_cast<int>(domain::IndexerKind::Peerflix):
        value = domain::IndexerKind::Peerflix;
        break;
    case static_cast<int>(domain::IndexerKind::Torrentio):
    default:
        value = domain::IndexerKind::Torrentio;
        break;
    }
    m_settings.setActiveIndexer(value);
}

} // namespace kinema::ui::qml::settings
