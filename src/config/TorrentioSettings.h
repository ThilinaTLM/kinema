// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/TorrentioConfig.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Torrentio query defaults (sort mode + RD cached-only toggle).
 *
 * KConfig group: [General]
 * Keys:
 *   defaultSort   "seeders" | "size" | "qualitysize"
 *   cachedOnly    bool (RD cached-only filter)
 */
class TorrentioSettings : public QObject
{
    Q_OBJECT
public:
    explicit TorrentioSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    core::torrentio::SortMode defaultSort() const;
    void setDefaultSort(core::torrentio::SortMode);

    bool cachedOnly() const;
    void setCachedOnly(bool);

Q_SIGNALS:
    void defaultSortChanged(core::torrentio::SortMode);
    void cachedOnlyChanged(bool);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
