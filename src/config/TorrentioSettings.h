// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/TorrentioConfig.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Torrentio query defaults (currently just the wire-format sort
 * mode that becomes the `sort=` URL parameter).
 *
 * KConfig group: [General]
 * Keys:
 *   defaultSort   "seeders" | "size" | "qualitysize"
 */
class TorrentioSettings : public QObject
{
    Q_OBJECT
public:
    explicit TorrentioSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    core::torrentio::SortMode defaultSort() const;
    void setDefaultSort(core::torrentio::SortMode);

Q_SIGNALS:
    void defaultSortChanged(core::torrentio::SortMode);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
