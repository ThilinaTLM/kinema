// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QList>
#include <QString>
#include <QStringList>

namespace kinema::core::stream_filter {

/**
 * Runtime, client-side filters applied on top of whatever Torrentio has
 * already returned. Server-side filters (resolution/category exclusions)
 * are encoded into the Torrentio URL via core::torrentio::ConfigOptions;
 * these are the knobs we can't push upstream.
 */
struct ClientFilters {
    /// When true, only rows with `Stream::rdCached == true` survive.
    /// Callers are expected to only set this when Real-Debrid is
    /// actually configured.
    bool cachedOnly = false;

    /// Case-insensitive substring blocklist applied to `releaseName`
    /// and `detailsText`. Empty strings are ignored.
    QStringList keywordBlocklist;
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
