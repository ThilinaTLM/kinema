// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/util/TorrentioConfig.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Torrentio-specific settings: the wire-format sort mode that becomes
 * the `sort=` URL parameter, plus the base URL (so users can point
 * at a private Torrentio mirror).
 *
 * KConfig group: [Torrentio]
 * Keys:
 *   defaultSort   "seeders" | "size" | "qualitysize"
 *   baseUrl       URL, default "https://torrentio.strem.fun"
 */
class TorrentioSettings : public QObject
{
    Q_OBJECT
public:
    explicit TorrentioSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    core::torrentio::SortMode defaultSort() const;
    void setDefaultSort(core::torrentio::SortMode);

    QString baseUrl() const;
    void setBaseUrl(const QString&);

    /// Default base URL used when no override has been saved.
    static QString defaultBaseUrl();

Q_SIGNALS:
    void defaultSortChanged(core::torrentio::SortMode);
    void baseUrlChanged(const QString&);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
