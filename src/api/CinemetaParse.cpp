// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/CinemetaParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QRegularExpression>

namespace kinema::api::cinemeta {

namespace {

std::optional<int> parseYear(const QJsonValue& v)
{
    // Cinemeta emits "year" as either a number or a string like "1999" / "1999–2001".
    if (v.isDouble()) {
        return static_cast<int>(v.toDouble());
    }
    const auto s = v.toString();
    if (s.isEmpty()) {
        return std::nullopt;
    }
    static const QRegularExpression re(QStringLiteral("(\\d{4})"));
    const auto m = re.match(s);
    if (!m.hasMatch()) {
        return std::nullopt;
    }
    bool ok = false;
    const int y = m.captured(1).toInt(&ok);
    return ok ? std::optional<int> { y } : std::nullopt;
}

std::optional<double> parseRating(const QJsonValue& v)
{
    if (v.isDouble()) {
        return v.toDouble();
    }
    const auto s = v.toString();
    if (s.isEmpty()) {
        return std::nullopt;
    }
    bool ok = false;
    const double r = s.toDouble(&ok);
    return ok ? std::optional<double> { r } : std::nullopt;
}

QStringList parseStringArray(const QJsonValue& v)
{
    QStringList out;
    if (!v.isArray()) {
        return out;
    }
    const auto arr = v.toArray();
    out.reserve(arr.size());
    for (const auto& item : arr) {
        const auto s = item.toString();
        if (!s.isEmpty()) {
            out.append(s);
        }
    }
    return out;
}

MediaKind kindFromType(const QString& type, MediaKind fallback)
{
    if (type == QLatin1String("movie")) {
        return MediaKind::Movie;
    }
    if (type == QLatin1String("series")) {
        return MediaKind::Series;
    }
    return fallback;
}

MetaSummary summaryFromObject(const QJsonObject& obj, MediaKind requestedKind)
{
    MetaSummary m;
    m.imdbId = obj.value(QStringLiteral("id")).toString();
    m.kind = kindFromType(obj.value(QStringLiteral("type")).toString(), requestedKind);
    m.title = obj.value(QStringLiteral("name")).toString();
    m.year = parseYear(obj.value(QStringLiteral("year")));
    m.description = obj.value(QStringLiteral("description")).toString();
    m.imdbRating = parseRating(obj.value(QStringLiteral("imdbRating")));

    const auto posterStr = obj.value(QStringLiteral("poster")).toString();
    if (!posterStr.isEmpty()) {
        m.poster = QUrl(posterStr);
    }
    return m;
}

} // namespace

QList<MetaSummary> parseSearch(const QJsonDocument& doc, MediaKind kind)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Cinemeta search response was not a JSON object."));
    }
    const auto metas = doc.object().value(QStringLiteral("metas"));
    if (!metas.isArray()) {
        return {}; // absent "metas" == no results, not a hard error
    }

    QList<MetaSummary> out;
    out.reserve(metas.toArray().size());
    for (const auto& v : metas.toArray()) {
        if (!v.isObject()) {
            continue;
        }
        auto m = summaryFromObject(v.toObject(), kind);
        if (m.imdbId.isEmpty() || m.title.isEmpty()) {
            continue;
        }
        // Filter to the requested kind; Cinemeta occasionally mixes.
        if (m.kind != kind) {
            continue;
        }
        out.append(std::move(m));
    }
    return out;
}

MetaDetail parseMeta(const QJsonDocument& doc, MediaKind kind)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Cinemeta meta response was not a JSON object."));
    }
    const auto metaV = doc.object().value(QStringLiteral("meta"));
    if (!metaV.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Cinemeta meta response missing 'meta' object."));
    }
    const auto obj = metaV.toObject();

    MetaDetail d;
    d.summary = summaryFromObject(obj, kind);

    const auto bg = obj.value(QStringLiteral("background")).toString();
    if (!bg.isEmpty()) {
        d.background = QUrl(bg);
    }

    d.genres = parseStringArray(obj.value(QStringLiteral("genres")));
    d.cast = parseStringArray(obj.value(QStringLiteral("cast")));

    const auto runtimeV = obj.value(QStringLiteral("runtime"));
    if (runtimeV.isString()) {
        // "116 min" → 116
        static const QRegularExpression re(QStringLiteral("(\\d+)"));
        const auto m = re.match(runtimeV.toString());
        if (m.hasMatch()) {
            bool ok = false;
            int v = m.captured(1).toInt(&ok);
            if (ok) {
                d.runtimeMinutes = v;
            }
        }
    } else if (runtimeV.isDouble()) {
        d.runtimeMinutes = static_cast<int>(runtimeV.toDouble());
    }

    return d;
}

namespace {

std::optional<QDate> parseReleased(const QJsonValue& v)
{
    if (!v.isString()) {
        return std::nullopt;
    }
    const auto s = v.toString();
    if (s.isEmpty()) {
        return std::nullopt;
    }
    // Cinemeta emits either "2008-01-20T00:00:00.000Z" or just "2008-01-20".
    auto dt = QDateTime::fromString(s, Qt::ISODate);
    if (dt.isValid()) {
        return dt.date();
    }
    auto d = QDate::fromString(s, Qt::ISODate);
    if (d.isValid()) {
        return d;
    }
    return std::nullopt;
}

Episode episodeFromObject(const QJsonObject& obj)
{
    Episode e;
    e.season = obj.value(QStringLiteral("season")).toInt(-1);
    // Cinemeta uses "number" and sometimes "episode" for the episode index.
    const auto nv = obj.value(QStringLiteral("number"));
    e.number = nv.isDouble() ? nv.toInt() : obj.value(QStringLiteral("episode")).toInt(-1);

    e.title = obj.value(QStringLiteral("title")).toString();
    // "overview" in some fixtures, "description" in others.
    const auto ov = obj.value(QStringLiteral("overview")).toString();
    e.description = ov.isEmpty()
        ? obj.value(QStringLiteral("description")).toString()
        : ov;

    const auto thumb = obj.value(QStringLiteral("thumbnail")).toString();
    if (!thumb.isEmpty()) {
        e.thumbnail = QUrl(thumb);
    }
    e.released = parseReleased(obj.value(QStringLiteral("released")));
    return e;
}

} // namespace

QList<Episode> parseEpisodes(const QJsonArray& videos)
{
    QList<Episode> out;
    out.reserve(videos.size());
    for (const auto& v : videos) {
        if (!v.isObject()) {
            continue;
        }
        auto e = episodeFromObject(v.toObject());
        if (e.season < 0 || e.number < 0) {
            continue;
        }
        out.append(std::move(e));
    }
    std::sort(out.begin(), out.end(), [](const Episode& a, const Episode& b) {
        if (a.season != b.season) {
            return a.season < b.season;
        }
        return a.number < b.number;
    });
    return out;
}

SeriesDetail parseSeriesMeta(const QJsonDocument& doc)
{
    SeriesDetail sd;
    sd.meta = parseMeta(doc, MediaKind::Series);

    const auto metaV = doc.object().value(QStringLiteral("meta"));
    if (metaV.isObject()) {
        const auto videos = metaV.toObject().value(QStringLiteral("videos"));
        if (videos.isArray()) {
            sd.episodes = parseEpisodes(videos.toArray());
        }
    }
    return sd;
}

QList<int> seasonNumbers(const QList<Episode>& episodes)
{
    QList<int> out;
    for (const auto& e : episodes) {
        if (e.season <= 0) {
            continue;
        }
        if (out.isEmpty() || out.last() != e.season) {
            if (!out.contains(e.season)) {
                out.append(e.season);
            }
        }
    }
    std::sort(out.begin(), out.end());
    return out;
}

} // namespace kinema::api::cinemeta
