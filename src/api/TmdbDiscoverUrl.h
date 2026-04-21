// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"

#include <QString>
#include <QUrlQuery>

namespace kinema::api::tmdb {

/**
 * Pure helpers that translate a DiscoverQuery into the URL segments
 * TMDB's /discover/{movie|tv} endpoint expects. Isolated in this file
 * so unit tests can exercise the mapping without hitting the network.
 *
 * TmdbClient::discover() composes `discoverPath()` + `discoverQueryToQuery()`
 * with its usual `language=` / bearer-auth layer.
 */

/// Path segment for /discover for a given kind.
/// Returns "/discover/movie" or "/discover/tv".
QString discoverPath(MediaKind kind);

/// Canonical `sort_by=` value for a given (kind, axis). Some axes are
/// kind-dependent (e.g. TitleAsc uses `original_title.asc` for movies
/// but `name.asc` for TV).
QString discoverSortValue(MediaKind kind, DiscoverSort sort);

/**
 * Build the `QUrlQuery` of TMDB-facing parameters for a DiscoverQuery.
 *
 * Does NOT include `language=` — that's layered on by TmdbClient's
 * `buildUrl()` alongside bearer auth.
 *
 * Semantics:
 *  - `with_genres` is a comma-joined AND list, omitted when empty.
 *  - Date params use `primary_release_date.gte/.lte` (movies) or
 *    `first_air_date.gte/.lte` (TV), formatted yyyy-MM-dd.
 *  - When `sort == DiscoverSort::Rating`, `vote_count.gte` is forced
 *    to at least 200 (rating sort without a vote floor is meaningless).
 *  - `page` is omitted when == 1.
 *  - `include_adult=false` is always emitted; `include_video=false`
 *    is emitted for movies only (TMDB ignores it on /discover/tv).
 */
QUrlQuery discoverQueryToQuery(const DiscoverQuery& q);

} // namespace kinema::api::tmdb
