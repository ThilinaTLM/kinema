// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/qml-bridge/settings/IndexerSectionViewModelBase.h"

namespace kinema::config {
class TorrentioSettings;
}

namespace kinema::ui::qml::settings {

/**
 * Torrentio indexer settings section. Inherits status/busy and the
 * connection test from `IndexerSectionViewModelBase`; owns the
 * Torrentio-specific `defaultSort` plus its base URL editing.
 */
class TorrentioSectionViewModel : public IndexerSectionViewModelBase
{
    Q_OBJECT
    /// 0 = Seeders, 1 = Size, 2 = Quality & Size.
    Q_PROPERTY(int defaultSort READ defaultSort
        WRITE setDefaultSort NOTIFY defaultSortChanged)
    Q_PROPERTY(QString baseUrl READ baseUrl
        WRITE setBaseUrl NOTIFY baseUrlChanged)
    Q_PROPERTY(QString defaultBaseUrl READ defaultBaseUrlString CONSTANT)

public:
    TorrentioSectionViewModel(api::IndexerSelector* indexers,
        config::TorrentioSettings& settings,
        QObject* parent = nullptr);

    int defaultSort() const;
    QString baseUrl() const;
    QString defaultBaseUrlString() const;

    void setDefaultSort(int sort);
    void setBaseUrl(const QString& url);

public Q_SLOTS:
    void resetBaseUrl();

Q_SIGNALS:
    void defaultSortChanged();
    void baseUrlChanged();

protected:
    domain::IndexerKind indexerKind() const override;
    QString providerName() const override;

private:
    config::TorrentioSettings& m_settings;
};

} // namespace kinema::ui::qml::settings
