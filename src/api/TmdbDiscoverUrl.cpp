// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TmdbDiscoverUrl.h"

#include <QStringList>

#include <algorithm>

namespace kinema::api::tmdb {

namespace {

// When sort_by=vote_average.desc is in play, TMDB gladly returns
// titles with a single 10/10 vote at the top of page 1. Force a
// reasonable vote floor so rating-sorted pages aren't nonsense.
constexpr int kRatingSortMinVotes = 200;

} // namespace

QString discoverPath(MediaKind kind)
{
    return kind == MediaKind::Movie
        ? QStringLiteral("/discover/movie")
        : QStringLiteral("/discover/tv");
}

QString discoverSortValue(MediaKind kind, DiscoverSort sort)
{
    switch (sort) {
    case DiscoverSort::Popularity:
        return QStringLiteral("popularity.desc");
    case DiscoverSort::ReleaseDate:
        return kind == MediaKind::Movie
            ? QStringLiteral("primary_release_date.desc")
            : QStringLiteral("first_air_date.desc");
    case DiscoverSort::Rating:
        return QStringLiteral("vote_average.desc");
    case DiscoverSort::TitleAsc:
        return kind == MediaKind::Movie
            ? QStringLiteral("original_title.asc")
            : QStringLiteral("name.asc");
    }
    return QStringLiteral("popularity.desc");
}

QUrlQuery discoverQueryToQuery(const DiscoverQuery& q)
{
    QUrlQuery out;

    out.addQueryItem(QStringLiteral("include_adult"),
        QStringLiteral("false"));
    if (q.kind == MediaKind::Movie) {
        out.addQueryItem(QStringLiteral("include_video"),
            QStringLiteral("false"));
    }
    out.addQueryItem(QStringLiteral("sort_by"),
        discoverSortValue(q.kind, q.sort));

    if (!q.withGenreIds.isEmpty()) {
        QStringList ids;
        ids.reserve(q.withGenreIds.size());
        for (int id : q.withGenreIds) {
            ids.append(QString::number(id));
        }
        out.addQueryItem(QStringLiteral("with_genres"),
            ids.join(QLatin1Char(',')));
    }

    const auto gteKey = q.kind == MediaKind::Movie
        ? QStringLiteral("primary_release_date.gte")
        : QStringLiteral("first_air_date.gte");
    const auto lteKey = q.kind == MediaKind::Movie
        ? QStringLiteral("primary_release_date.lte")
        : QStringLiteral("first_air_date.lte");

    if (q.releasedGte) {
        out.addQueryItem(gteKey, q.releasedGte->toString(Qt::ISODate));
    }
    if (q.releasedLte) {
        out.addQueryItem(lteKey, q.releasedLte->toString(Qt::ISODate));
    }

    if (q.voteAverageGte) {
        out.addQueryItem(QStringLiteral("vote_average.gte"),
            QString::number(*q.voteAverageGte, 'f', 1));
    }

    // Rating sort pins a vote-count floor on top of whatever the user
    // asked for, so "Highest rated" stays meaningful.
    int effectiveVoteCount = q.voteCountGte.value_or(0);
    if (q.sort == DiscoverSort::Rating) {
        effectiveVoteCount = std::max(effectiveVoteCount, kRatingSortMinVotes);
    }
    if (effectiveVoteCount > 0) {
        out.addQueryItem(QStringLiteral("vote_count.gte"),
            QString::number(effectiveVoteCount));
    }

    if (q.page > 1) {
        out.addQueryItem(QStringLiteral("page"),
            QString::number(q.page));
    }

    return out;
}

} // namespace kinema::api::tmdb
