// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/util/TorrentioConfig.h"

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
    // Debrid pair is always rendered last so it matches the order
    // emitted by Torrentio's own `configure` page. Both halves must
    // be present — an orphan provider or token is dropped silently
    // rather than producing a malformed `=` segment.
    if (!opts.debridProvider.isEmpty() && !opts.debridToken.isEmpty()) {
        parts.append(opts.debridProvider + QLatin1Char('=') + opts.debridToken);
    }

    return parts.join(QLatin1Char('|'));
}

} // namespace kinema::core::torrentio
