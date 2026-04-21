// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QDate>
#include <QList>
#include <QMetaType>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::api {

/// Lightweight entry returned by TMDB catalog endpoints. Deliberately
/// distinct from MetaSummary: list endpoints don't carry an IMDB id,
/// so we keep a `tmdbId` side-channel. Click-through resolves the IMDB
/// id via an on-demand TMDB detail call and then hands off to the
/// existing Cinemeta + Torrentio pipeline.
struct DiscoverItem {
    int tmdbId = 0;
    MediaKind kind = MediaKind::Movie;
    QString title;
    std::optional<int> year; ///< from release_date or first_air_date
    QUrl poster; ///< full https://image.tmdb.org/t/p/... URL
    QUrl backdrop; ///< unused in M1/M2, parsed for later use
    QString overview;
    std::optional<double> voteAverage;
};

/// Sort axis for TMDB /discover. The actual `sort_by` string depends on
/// the media kind (e.g. `original_title.asc` for movies vs `name.asc`
/// for TV), resolved by `tmdb::discoverSortValue`.
enum class DiscoverSort {
    Popularity,   ///< popularity.desc
    ReleaseDate,  ///< primary_release_date.desc / first_air_date.desc
    Rating,       ///< vote_average.desc (forces vote_count.gte ≥ 200)
    TitleAsc,     ///< original_title.asc / name.asc
};

/// A TMDB genre as returned by /genre/{movie|tv}/list.
struct TmdbGenre {
    int id = 0;
    QString name;
};

/// Parameters for a single TMDB /discover call. Built by the Browse page
/// from its filter-bar state and fed through `tmdb::discoverQueryToQuery`
/// to produce the URL query string.
struct DiscoverQuery {
    MediaKind kind = MediaKind::Movie;
    QList<int> withGenreIds;              ///< AND semantics ("," joined)
    std::optional<QDate> releasedGte;     ///< inclusive lower bound
    std::optional<QDate> releasedLte;     ///< inclusive upper bound
    std::optional<double> voteAverageGte; ///< 0–10
    std::optional<int> voteCountGte;      ///< client-side "hide obscure"
    DiscoverSort sort = DiscoverSort::Popularity;
    int page = 1;                         ///< 1-indexed; TMDB caps at 500
};

/// One page of /discover results plus paging metadata.
struct DiscoverPageResult {
    QList<DiscoverItem> items;
    int page = 1;
    int totalPages = 0;
    int totalResults = 0;
};

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::DiscoverItem)
Q_DECLARE_METATYPE(kinema::api::TmdbGenre)
Q_DECLARE_METATYPE(kinema::api::DiscoverPageResult)
