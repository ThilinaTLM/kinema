// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QList>
#include <QString>
#include <QStringList>

namespace kinema::core::stream_filter {

/**
 * Runtime, client-side filters applied on top of whatever the active
 * indexer returned. Exclusions are *always* applied client-side so
 * the same `FilterSettings` work uniformly across indexers; the
 * Torrentio indexer additionally pushes them into the URL
 * `qualityfilter=` segment as an optimisation, but Peerflix has
 * no equivalent knob.
 */
struct ClientFilters {
    /// Case-insensitive substring blocklist applied to `releaseName`
    /// and `detailsText`. Empty strings are ignored.
    QStringList keywordBlocklist;
    /// Resolution tokens to drop, matched against `Stream::resolution`.
    /// Tokens follow the user-facing taxonomy:
    ///   `4k` covers 2160p & 1440p,
    ///   `1080p`, `720p`, `480p` map literally,
    ///   `other` covers everything below 480p plus unknown.
    QStringList excludedResolutions;
    /// Variant tokens to drop, matched against parsed tokens from the
    /// stream's release name (CAM / Screener / HDR / Dolby Vision /
    /// 3D / non-English / unknown quality / BluRay REMUX).
    QStringList excludedCategories;
};

/**
 * Apply `filters` to `in` and return the survivors in the same order.
 * Pure and cheap to call repeatedly.
 */
QList<api::Stream> apply(const QList<api::Stream>& in,
    const ClientFilters& filters);

/**
 * Return true if any keyword in `blocklist` appears as a case-insensitive
 * substring of the stream's release name or details text. Empty
 * keywords are skipped. An empty blocklist always returns false.
 */
bool matchesBlocklist(const api::Stream& s, const QStringList& blocklist);

} // namespace kinema::core::stream_filter
