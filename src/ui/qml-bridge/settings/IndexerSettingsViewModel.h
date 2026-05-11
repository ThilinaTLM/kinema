// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include "ui/qml-bridge/settings/PeerflixSectionViewModel.h"
#include "ui/qml-bridge/settings/TorrentioSectionViewModel.h"

namespace kinema::api {
class IndexerSelector;
}

namespace kinema::config {
class IndexerSettings;
class PeerflixSettings;
class TorrentioSettings;
}

namespace kinema::ui::qml::settings {

class IndexerSettingsViewModel : public QObject
{
    Q_OBJECT
    /// 1 = Torrentio, 2 = Peerflix. Maps to `domain::IndexerKind`.
    Q_PROPERTY(int activeIndexer READ activeIndexer
        WRITE setActiveIndexer NOTIFY activeIndexerChanged)
    Q_PROPERTY(TorrentioSectionViewModel* torrentio
        READ torrentio CONSTANT)
    Q_PROPERTY(PeerflixSectionViewModel* peerflix
        READ peerflix CONSTANT)

public:
    IndexerSettingsViewModel(api::IndexerSelector* indexers,
        config::IndexerSettings& indexerSettings,
        config::TorrentioSettings& torrentioSettings,
        config::PeerflixSettings& peerflixSettings,
        QObject* parent = nullptr);

    int activeIndexer() const;
    void setActiveIndexer(int kind);

    TorrentioSectionViewModel* torrentio() const { return m_torrentio; }
    PeerflixSectionViewModel* peerflix() const { return m_peerflix; }

Q_SIGNALS:
    void activeIndexerChanged();

private:
    config::IndexerSettings& m_settings;
    TorrentioSectionViewModel* m_torrentio {};
    PeerflixSectionViewModel* m_peerflix {};
};

} // namespace kinema::ui::qml::settings
