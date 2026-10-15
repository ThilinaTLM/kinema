// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/qml-bridge/settings/IndexerSectionViewModelBase.h"

namespace kinema::config {
class PeerflixSettings;
}

namespace kinema::ui::qml::settings {

/**
 * Peerflix indexer settings section. Inherits status/busy and the
 * connection test from `IndexerSectionViewModelBase`; owns the
 * Peerflix base-URL editing.
 */
class PeerflixSectionViewModel : public IndexerSectionViewModelBase
{
    Q_OBJECT
    Q_PROPERTY(QString baseUrl READ baseUrl
        WRITE setBaseUrl NOTIFY baseUrlChanged)
    Q_PROPERTY(QString defaultBaseUrl READ defaultBaseUrlString CONSTANT)

public:
    PeerflixSectionViewModel(api::IndexerSelector* indexers,
        config::PeerflixSettings& settings,
        QObject* parent = nullptr);

    QString baseUrl() const;
    QString defaultBaseUrlString() const;

    void setBaseUrl(const QString& url);

public Q_SLOTS:
    void resetBaseUrl();

Q_SIGNALS:
    void baseUrlChanged();

protected:
    domain::IndexerKind indexerKind() const override;
    QString providerName() const override;

private:
    config::PeerflixSettings& m_settings;
};

} // namespace kinema::ui::qml::settings
