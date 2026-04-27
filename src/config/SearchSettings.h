// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <KSharedConfig>

#include <QObject>
#include <QStringList>

namespace kinema::config {

/**
 * SearchBar defaults + recent-query history.
 *
 * KConfig group: [General]
 * Keys:
 *   searchKind        "Movie" | "Series"
 *
 * KConfig group: [Search]
 * Keys:
 *   recentQueries     comma-separated MRU list (cap 8)
 */
class SearchSettings : public QObject
{
    Q_OBJECT
public:
    explicit SearchSettings(KSharedConfigPtr config, QObject* parent = nullptr);

    api::MediaKind kind() const;
    void setKind(api::MediaKind);

    /// Most-recently-used search queries, newest first. Capped at
    /// `kRecentCap` entries by `addRecentQuery`. Returns an empty
    /// list when no history is recorded.
    QStringList recentQueries() const;

    /// Push `q` (trimmed) to the front of the MRU list, removing
    /// any earlier occurrence (case-sensitive) and dropping the
    /// tail beyond the cap. No-op on empty/whitespace input.
    void addRecentQuery(const QString& q);

    /// Wipe the recent-queries list.
    void clearRecentQueries();

Q_SIGNALS:
    void recentQueriesChanged();

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
