// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QJsonArray>
#include <QJsonDocument>

namespace kinema::api::cinemeta {

/**
 * Parse a Cinemeta `/catalog/.../search=...json` response body.
 *
 * @param doc   Parsed JSON document from the server.
 * @param kind  The kind we requested (used to tag every result — Cinemeta
 *              sometimes mixes types but we filter to the requested kind).
 * @return list of MetaSummary values. Empty list on an empty/missing "metas".
 *
 * Throws HttpError(Kind::Json) if the shape is fundamentally wrong
 * (e.g. not a JSON object at the top level).
 */
QList<MetaSummary> parseSearch(const QJsonDocument& doc, MediaKind kind);

/**
 * Parse a Cinemeta `/meta/{kind}/{id}.json` response body.
 *
 * Throws HttpError(Kind::Json) if the "meta" key is missing or malformed.
 */
MetaDetail parseMeta(const QJsonDocument& doc, MediaKind kind);

/**
 * Parse a series `/meta/series/{id}.json` response — same shape as
 * `parseMeta`, plus the `videos` array rendered into `Episode` rows.
 *
 * Episodes are sorted by (season, number). Rows missing season/number
 * are dropped; malformed rows are skipped tolerantly.
 */
SeriesDetail parseSeriesMeta(const QJsonDocument& doc);

/**
 * Convenience: parse just the videos array. Same tolerant semantics.
 */
QList<Episode> parseEpisodes(const QJsonArray& videos);

/// Unique season numbers present in `episodes`, excluding season 0
/// (specials), in ascending order. Used by the series picker to
/// populate the season combo box.
QList<int> seasonNumbers(const QList<Episode>& episodes);

} // namespace kinema::api::cinemeta
