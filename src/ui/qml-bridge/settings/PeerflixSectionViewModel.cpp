// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/PeerflixSectionViewModel.h"

#include "config/PeerflixSettings.h"
#include "domain/Indexer.h"

namespace kinema::ui::qml::settings {

PeerflixSectionViewModel::PeerflixSectionViewModel(
    api::IndexerSelector* indexers,
    config::PeerflixSettings& settings, QObject* parent)
    : IndexerSectionViewModelBase(indexers, parent)
    , m_settings(settings)
{
    connect(&m_settings, &config::PeerflixSettings::baseUrlChanged,
        this, &PeerflixSectionViewModel::baseUrlChanged);
}

QString PeerflixSectionViewModel::baseUrl() const
{
    return m_settings.baseUrl();
}

QString PeerflixSectionViewModel::defaultBaseUrlString() const
{
    return config::PeerflixSettings::defaultBaseUrl();
}

void PeerflixSectionViewModel::setBaseUrl(const QString& url)
{
    m_settings.setBaseUrl(url);
}

void PeerflixSectionViewModel::resetBaseUrl()
{
    m_settings.setBaseUrl(config::PeerflixSettings::defaultBaseUrl());
}

domain::IndexerKind PeerflixSectionViewModel::indexerKind() const
{
    return domain::IndexerKind::Peerflix;
}

QString PeerflixSectionViewModel::providerName() const
{
    return QStringLiteral("Peerflix");
}

} // namespace kinema::ui::qml::settings
