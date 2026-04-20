// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "core/TorrentioConfig.h"

namespace kinema::core::torrentio {

QString toString(SortMode m)
{
    switch (m) {
    case SortMode::Seeders:
        return QStringLiteral("seeders");
    case SortMode::Size:
        return QStringLiteral("size");
    case SortMode::QualitySize:
        return QStringLiteral("qualitysize");
    }
    return QStringLiteral("seeders");
}

QString toPathSegment(const ConfigOptions& opts)
{
    QStringList parts;
    parts.reserve(4);

    // sort is always present so an empty ConfigOptions still produces a stable output.
    parts.append(QStringLiteral("sort=") + toString(opts.sort));

    if (!opts.qualityFilter.isEmpty()) {
        parts.append(QStringLiteral("qualityfilter=") + opts.qualityFilter.join(QLatin1Char(',')));
    }
    if (!opts.providers.isEmpty()) {
        parts.append(QStringLiteral("providers=") + opts.providers.join(QLatin1Char(',')));
    }
    // MUST be last (Torrentio RD addon requirement).
    if (!opts.realDebridToken.isEmpty()) {
        parts.append(QStringLiteral("realdebrid=") + opts.realDebridToken);
    }

    return parts.join(QLatin1Char('|'));
}

} // namespace kinema::core::torrentio
