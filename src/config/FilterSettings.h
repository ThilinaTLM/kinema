// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>
#include <QStringList>

namespace kinema::config {

/**
 * Torrent-row filters.
 *
 * - Resolution / category exclusions feed into Torrentio's
 *   `qualityfilter` URL param (server-side).
 * - Keyword blocklist is a client-side substring filter.
 *
 * KConfig group: [Filters]
 * Keys:
 *   excludedResolutions  CSV: 4k,1080p,720p,480p,other
 *   excludedCategories   CSV: cam,scr,threed,hdr,hdr10plus,
 *                             dolbyvision,nonen,unknown,brremux
 *   keywordBlocklist     newline-separated free-form substrings
 */
class FilterSettings : public QObject
{
    Q_OBJECT
public:
    explicit FilterSettings(KSharedConfigPtr config, QObject* parent = nullptr);

    QStringList excludedResolutions() const;
    void setExcludedResolutions(QStringList list);

    QStringList excludedCategories() const;
    void setExcludedCategories(QStringList list);

    QStringList keywordBlocklist() const;
    void setKeywordBlocklist(QStringList list);

Q_SIGNALS:
    /// Emitted when either exclusion list changes. Server-side refetch.
    void exclusionsChanged();
    /// Emitted when the client-side keyword list changes.
    void keywordBlocklistChanged(QStringList);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
