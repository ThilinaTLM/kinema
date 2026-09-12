// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

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
 *   sort=seeders|qualityfilter=4k,cam,threed|providers=yts,eztv|realdebrid=TOKEN
 *
 * Torrentio's `qualityfilter` URL param is an **exclusion** list that mixes
 * resolution and variant tags. We keep the two concepts separate in code for
 * clarity and merge them into a single comma-separated value at render time
 * (resolutions first, then categories, preserving input order within each).
 *
 * When a debrid provider is active, Kinema appends the provider's URL
 * token + credential as the last pipe entry so the upstream addon can
 * surface its debrid-cached streams. The picked row is still resolved
 * through the unified downloader at play time.
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
    /// Active debrid provider URL token, e.g. `"realdebrid"` or
    /// `"alldebrid"`. Empty when no debrid is active.
    QString debridProvider;
    /// Debrid token / API key. Rendered alongside `debridProvider`
    /// as `<debridProvider>=<debridToken>`. Both must be non-empty
    /// for the segment to be emitted — a mismatch is silently
    /// dropped rather than producing a malformed URL.
    QString debridToken;
};

/**
 * Render a ConfigOptions into the URL path segment that Torrentio expects.
 * The output is intended to be slotted between `torrentio.strem.fun/` and
 * `/stream/...`.
 */
QString toPathSegment(const ConfigOptions& opts);

} // namespace kinema::core::torrentio
