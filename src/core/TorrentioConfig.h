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
 *   sort=seeders|qualityfilter=4k,cam,threed|providers=yts,eztv|realdebrid=XYZ
 *
 * Torrentio's `qualityfilter` URL param is an **exclusion** list that mixes
 * resolution and variant tags. We keep the two concepts separate in code for
 * clarity and merge them into a single comma-separated value at render time
 * (resolutions first, then categories, preserving input order within each).
 *
 * `realDebridToken`, when non-empty, is always emitted last — Torrentio
 * requires this for the RD addon to pick it up.
 */
struct ConfigOptions {
    SortMode sort = SortMode::Seeders;
    /// Resolution tags to exclude, e.g. {"4k","480p"}. Valid tokens:
    /// `4k` (covers 2160p & 1440p), `1080p`, `720p`, `480p`, `other`.
    QStringList excludedResolutions;
    /// Variant tags to exclude, e.g. {"cam","threed"}. Valid tokens:
    /// `cam`, `scr`, `threed`, `hdr`, `hdr10plus`, `dolbyvision`,
    /// `nonen`, `unknown`, `brremux`.
    QStringList excludedCategories;
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
