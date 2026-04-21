// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/TmdbParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QDate>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QLatin1StringView>

namespace kinema::api::tmdb {

namespace {

std::optional<int> yearFromDate(const QJsonValue& v)
{
    if (!v.isString()) {
        return std::nullopt;
    }
    const auto s = v.toString();
    if (s.isEmpty()) {
        return std::nullopt;
    }
    const auto d = QDate::fromString(s, Qt::ISODate);
    if (d.isValid()) {
        return d.year();
    }
    // Some fixtures expose just "YYYY".
    if (s.size() >= 4) {
        bool ok = false;
        const int y = QStringView { s }.left(4).toInt(&ok);
        if (ok) {
            return y;
        }
    }
    return std::nullopt;
}

std::optional<double> parseVote(const QJsonValue& v)
{
    if (v.isDouble()) {
        const double d = v.toDouble();
        // TMDB emits 0.0 for unrated items — surface as absent so the
        // UI doesn't print "★ 0.0".
        if (d > 0.0) {
            return d;
        }
    }
    return std::nullopt;
}

DiscoverItem itemFromObject(const QJsonObject& obj, MediaKind kind)
{
    DiscoverItem d;
    d.kind = kind;
    d.tmdbId = obj.value(QStringLiteral("id")).toInt(0);

    // Title fields differ for movies vs series.
    if (kind == MediaKind::Movie) {
        d.title = obj.value(QStringLiteral("title")).toString();
        if (d.title.isEmpty()) {
            d.title = obj.value(QStringLiteral("original_title")).toString();
        }
        d.year = yearFromDate(obj.value(QStringLiteral("release_date")));
    } else {
        d.title = obj.value(QStringLiteral("name")).toString();
        if (d.title.isEmpty()) {
            d.title = obj.value(QStringLiteral("original_name")).toString();
        }
        d.year = yearFromDate(obj.value(QStringLiteral("first_air_date")));
    }

    d.overview = obj.value(QStringLiteral("overview")).toString();
    d.voteAverage = parseVote(obj.value(QStringLiteral("vote_average")));

    d.poster = composeImageUrl(QString::fromLatin1(kPosterSize),
        obj.value(QStringLiteral("poster_path")).toString());
    d.backdrop = composeImageUrl(QString::fromLatin1(kBackdropSize),
        obj.value(QStringLiteral("backdrop_path")).toString());

    return d;
}

} // namespace

QUrl composeImageUrl(const QString& size, const QString& path)
{
    if (path.isEmpty() || size.isEmpty()) {
        return {};
    }
    // TMDB relative paths begin with "/"; handle the absent case just to
    // be safe.
    const QString prefix = path.startsWith(QLatin1Char('/'))
        ? QStringLiteral("https://image.tmdb.org/t/p/%1%2").arg(size, path)
        : QStringLiteral("https://image.tmdb.org/t/p/%1/%2").arg(size, path);
    return QUrl(prefix);
}

QList<DiscoverItem> parseList(const QJsonDocument& doc, MediaKind kind)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("TMDB list response was not a JSON object."));
    }
    const auto results = doc.object().value(QStringLiteral("results"));
    if (!results.isArray()) {
        return {};
    }

    QList<DiscoverItem> out;
    const auto arr = results.toArray();
    out.reserve(arr.size());
    for (const auto& v : arr) {
        if (!v.isObject()) {
            continue;
        }
        auto d = itemFromObject(v.toObject(), kind);
        // Skip items we can't render or can't open.
        if (d.tmdbId <= 0 || d.title.isEmpty() || d.poster.isEmpty()) {
            continue;
        }
        out.append(std::move(d));
    }
    return out;
}

QString parseMovieExternalIds(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("TMDB movie response was not a JSON object."));
    }
    const auto obj = doc.object();
    // Two layouts are possible: top-level (GET /movie/{id} has imdb_id
    // directly), or nested under "external_ids" (append_to_response).
    const auto direct = obj.value(QStringLiteral("imdb_id")).toString();
    if (!direct.isEmpty()) {
        return direct;
    }
    const auto ext = obj.value(QStringLiteral("external_ids"));
    if (ext.isObject()) {
        return ext.toObject().value(QStringLiteral("imdb_id")).toString();
    }
    return {};
}

QString parseSeriesExternalIds(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("TMDB series response was not a JSON object."));
    }
    // /tv/{id}/external_ids returns imdb_id at the top level; /tv/{id}
    // with append_to_response nests it under external_ids.
    const auto obj = doc.object();
    const auto direct = obj.value(QStringLiteral("imdb_id")).toString();
    if (!direct.isEmpty()) {
        return direct;
    }
    const auto ext = obj.value(QStringLiteral("external_ids"));
    if (ext.isObject()) {
        return ext.toObject().value(QStringLiteral("imdb_id")).toString();
    }
    return {};
}

std::pair<int, MediaKind> parseFindResult(const QJsonDocument& doc,
    MediaKind preferredKind)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("TMDB /find response was not a JSON object."));
    }
    const auto obj = doc.object();

    auto firstId = [&obj](const char* key) -> int {
        const auto arr = obj.value(QLatin1StringView(key)).toArray();
        for (const auto& v : arr) {
            const auto id = v.toObject().value(QStringLiteral("id")).toInt(0);
            if (id > 0) {
                return id;
            }
        }
        return 0;
    };

    // Preferred first, fall back to the other.
    if (preferredKind == MediaKind::Movie) {
        if (const auto id = firstId("movie_results"); id > 0) {
            return { id, MediaKind::Movie };
        }
        if (const auto id = firstId("tv_results"); id > 0) {
            return { id, MediaKind::Series };
        }
    } else {
        if (const auto id = firstId("tv_results"); id > 0) {
            return { id, MediaKind::Series };
        }
        if (const auto id = firstId("movie_results"); id > 0) {
            return { id, MediaKind::Movie };
        }
    }
    return { 0, preferredKind };
}

} // namespace kinema::api::tmdb
