// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

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

    if (!opts.excludedResolutions.isEmpty() || !opts.excludedCategories.isEmpty()) {
        QStringList merged;
        merged.reserve(opts.excludedResolutions.size() + opts.excludedCategories.size());
        merged.append(opts.excludedResolutions);
        merged.append(opts.excludedCategories);
        parts.append(QStringLiteral("qualityfilter=") + merged.join(QLatin1Char(',')));
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
