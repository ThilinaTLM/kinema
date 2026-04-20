// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QString>
#include <QStringList>

namespace kinema::core::torrentio {

/**
 * Supported sort modes. String form matches Torrentio's URL param values.
 */
enum class SortMode {
    Seeders, ///< "seeders" (default)
    Size, ///< "size"
    QualitySize, ///< "qualitysize"
};

QString toString(SortMode);

/**
 * Options that shape a Torrentio stream query. Rendered into a pipe-separated
 * URL path segment like:
 *
 *   sort=seeders|qualityfilter=1080p,2160p|providers=yts,eztv|realdebrid=XYZ
 *
 * `realDebridToken`, when non-empty, is always emitted last — Torrentio
 * requires this for the RD addon to pick it up.
 */
struct ConfigOptions {
    SortMode sort = SortMode::Seeders;
    QStringList qualityFilter; ///< e.g. {"720p","1080p"} — joined with ','
    QStringList providers; ///< e.g. {"yts","eztv"}
    QString realDebridToken; ///< empty = no RD
};

/**
 * Render a ConfigOptions into the URL path segment that Torrentio expects.
 * The output is intended to be slotted between `torrentio.strem.fun/` and
 * `/stream/...`.
 */
QString toPathSegment(const ConfigOptions& opts);

} // namespace kinema::core::torrentio
