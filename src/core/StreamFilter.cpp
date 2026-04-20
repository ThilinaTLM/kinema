// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "core/StreamFilter.h"

namespace kinema::core::stream_filter {

bool matchesBlocklist(const api::Stream& s, const QStringList& blocklist)
{
    for (const auto& keyword : blocklist) {
        if (keyword.isEmpty()) {
            continue;
        }
        if (s.releaseName.contains(keyword, Qt::CaseInsensitive)
            || s.detailsText.contains(keyword, Qt::CaseInsensitive)) {
            return true;
        }
    }
    return false;
}

QList<api::Stream> apply(const QList<api::Stream>& in, const ClientFilters& filters)
{
    QList<api::Stream> out;
    out.reserve(in.size());
    for (const auto& s : in) {
        if (filters.cachedOnly && !s.rdCached) {
            continue;
        }
        if (!filters.keywordBlocklist.isEmpty()
            && matchesBlocklist(s, filters.keywordBlocklist)) {
            continue;
        }
        out.append(s);
    }
    return out;
}

} // namespace kinema::core::stream_filter
