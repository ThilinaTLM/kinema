// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Types.h"

#include <QJsonDocument>
#include <QString>
#include <QUrl>

#include <utility>

namespace kinema::api::tmdb {

/// Poster size segment used when composing image URLs. TMDB's image CDN
/// expects `https://image.tmdb.org/t/p/{size}/{path}`. We hardcode a
/// small set of known-good sizes; the `/configuration` endpoint exists
/// if you ever need to audit them.
inline constexpr const char* kPosterSize = "w342";
inline constexpr const char* kBackdropSize = "w780";

/// Compose an image URL from a TMDB relative path (e.g.
/// "/abc.jpg"). Returns an empty URL if `path` is empty or missing.
QUrl composeImageUrl(const QString& size, const QString& path);

/**
 * Parse a TMDB list response body (standard shape: `{ "results": [ ... ] }`).
 *
 * Used by catalog endpoints: /trending, /movie/popular, /tv/popular,
 * /movie/top_rated, /tv/top_rated, /movie/now_playing, /tv/on_the_air,
 * as well as /movie/{id}/recommendations, /tv/{id}/similar, etc.
 *
 * The caller's `kind` is authoritative — we tag every row with it and
 * ignore the per-item `media_type` field that some endpoints emit
 * (since M1/M2 always request a fixed kind).
 *
 * Rows missing `id` or `poster_path` are skipped (a card with no poster
 * looks broken; rows with no id can't be opened).
 *
 * Throws HttpError(Kind::Json) if the document is not a JSON object.
 */
QList<DiscoverItem> parseList(const QJsonDocument& doc, MediaKind kind);

/**
 * Parse a paginated TMDB list response (/discover/{movie|tv}, and any
 * other endpoint that returns the standard `{ results, page,
 * total_pages, total_results }` envelope). Items are filtered the
 * same way `parseList` filters them (missing id / title / poster).
 *
 * Throws HttpError(Kind::Json) if the document is not a JSON object.
 */
DiscoverPageResult parsePagedList(const QJsonDocument& doc, MediaKind kind);

/**
 * Parse a TMDB /genre/{movie|tv}/list response:
 * `{ "genres": [ { "id": 28, "name": "Action" }, … ] }`.
 *
 * Throws HttpError(Kind::Json) if the document is not a JSON object.
 * Rows missing `id` or `name` are skipped.
 */
QList<TmdbGenre> parseGenreList(const QJsonDocument& doc);

/**
 * Parse /movie/{id}?append_to_response=external_ids and extract the
 * IMDB id. Returns empty if absent. Throws HttpError on malformed body.
 */
QString parseMovieExternalIds(const QJsonDocument& doc);

/**
 * Parse /tv/{id}/external_ids and extract the IMDB id. Returns empty
 * if absent. Throws HttpError on malformed body.
 */
QString parseSeriesExternalIds(const QJsonDocument& doc);

/**
 * Parse /find/{imdb_id}?external_source=imdb_id. Returns the first hit
 * preferring movie_results over tv_results (callers pass the
 * media_kind they expect to disambiguate when both are present).
 *
 * Returns {0, MediaKind::Movie} if neither array yields a match.
 * Throws HttpError(Kind::Json) on malformed top-level shape.
 */
std::pair<int, MediaKind> parseFindResult(const QJsonDocument& doc,
    MediaKind preferredKind);

} // namespace kinema::api::tmdb
