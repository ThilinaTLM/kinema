// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/StremioStreamParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QRegularExpression>

namespace kinema::api::stremio {

namespace {

// The descriptive field these addons return looks like:
//
//   "The Matrix 1999 1080p BluRay x264-NOGRP\n👤 123 💾 2.1 GB ⚙️ ThePirateBay"
//
// The current Stremio Stream spec calls this `description`; the older
// `title` field is deprecated. Torrentio still emits `title`,
// Peerflix correctly uses `description`. We prefer `description`
// when present and fall back to `title`.
//
// Sometimes lines are separated by "\n", sometimes by "\r\n". The extra
// metadata may be on one line or spread across several. Use tolerant
// regexes over the whole blob. Indexers that also surface structured
// fields (`seed`, `sizebytes`, `quality`) get those preferred over
// regex extraction — see `parseOne` below.

static const QRegularExpression kSeedersRe(QStringLiteral("👤\\s*([0-9]+)"));
static const QRegularExpression kSizeRe(
    QStringLiteral("💾\\s*([0-9]+(?:\\.[0-9]+)?)\\s*(TB|GB|MB|KB)"),
    QRegularExpression::CaseInsensitiveOption);
static const QRegularExpression kProviderRe(
    QStringLiteral("⚙️\\s*(\\S+)"));
static const QRegularExpression kResolutionRe(
    QStringLiteral("\\b(2160p|1440p|1080p|720p|480p|360p|4K|SD|HD)\\b"),
    QRegularExpression::CaseInsensitiveOption);

std::optional<qint64> parseSize(const QString& text)
{
    const auto m = kSizeRe.match(text);
    if (!m.hasMatch()) {
        return std::nullopt;
    }
    bool ok = false;
    const double value = m.captured(1).toDouble(&ok);
    if (!ok) {
        return std::nullopt;
    }
    const auto unit = m.captured(2).toUpper();
    qint64 multiplier = 1;
    if (unit == QLatin1String("KB")) {
        multiplier = 1024LL;
    } else if (unit == QLatin1String("MB")) {
        multiplier = 1024LL * 1024;
    } else if (unit == QLatin1String("GB")) {
        multiplier = 1024LL * 1024 * 1024;
    } else if (unit == QLatin1String("TB")) {
        multiplier = 1024LL * 1024 * 1024 * 1024;
    }
    return static_cast<qint64>(value * static_cast<double>(multiplier));
}

std::optional<int> parseSeeders(const QString& text)
{
    const auto m = kSeedersRe.match(text);
    if (!m.hasMatch()) {
        return std::nullopt;
    }
    bool ok = false;
    const int v = m.captured(1).toInt(&ok);
    return ok ? std::optional<int> { v } : std::nullopt;
}

QString parseProvider(const QString& text)
{
    const auto m = kProviderRe.match(text);
    return m.hasMatch() ? m.captured(1) : QString {};
}

QString parseResolution(const QString& qualityLabel, const QString& releaseName)
{
    // Quality label is first; the release name is a useful fallback.
    for (const QString* src : { &qualityLabel, &releaseName }) {
        const auto m = kResolutionRe.match(*src);
        if (m.hasMatch()) {
            return m.captured(1).toLower();
        }
    }
    return QStringLiteral("—");
}

Stream parseOne(const QJsonObject& obj)
{
    Stream s;

    const auto nameRaw = obj.value(QStringLiteral("name")).toString();
    // `name` is sometimes multi-line; first line is the quality label.
    s.qualityLabel = nameRaw.section(QLatin1Char('\n'), 0, 0).trimmed();

    // Prefer `description` (current Stremio spec) and fall back to
    // the deprecated `title` for Torrentio backward compatibility.
    auto descRaw = obj.value(QStringLiteral("description")).toString();
    if (descRaw.isEmpty()) {
        descRaw = obj.value(QStringLiteral("title")).toString();
    }
    if (!descRaw.isEmpty()) {
        const auto lines = descRaw.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
        if (!lines.isEmpty()) {
            s.releaseName = lines.first().trimmed();
            if (lines.size() > 1) {
                s.detailsText = lines.mid(1).join(QStringLiteral(" · ")).trimmed();
            }
        }
    }

    // Prefer structured numeric fields when present (Peerflix), else
    // fall back to emoji-regex extraction (Torrentio).
    if (const auto v = obj.value(QStringLiteral("seed")); v.isDouble()) {
        s.seeders = v.toInt();
    } else {
        s.seeders = parseSeeders(descRaw);
    }
    if (const auto v = obj.value(QStringLiteral("sizebytes")); v.isDouble()) {
        s.sizeBytes = static_cast<qint64>(v.toDouble());
    } else {
        s.sizeBytes = parseSize(descRaw);
    }
    s.provider = parseProvider(descRaw);

    // Lower-cased ISO language code (Peerflix surfaces this for
    // every row; Torrentio leaves it empty). Consumed by the
    // "Non-English" client filter.
    s.language = obj.value(QStringLiteral("language"))
                     .toString()
                     .trimmed()
                     .toLower();

    s.infoHash = obj.value(QStringLiteral("infoHash")).toString();
    const auto bh = obj.value(QStringLiteral("behaviorHints")).toObject();
    // Some Stremio addon responses tuck infoHash inside behaviorHints
    // (notably certain RD/AD-resolved entries). Fall back to that
    // nested location when the top-level field is absent so the
    // history layer can still key resume on a stable identifier.
    if (s.infoHash.isEmpty()) {
        s.infoHash = bh.value(QStringLiteral("infoHash")).toString();
    }

    // `fileIdx` may live at the top level or under behaviorHints.
    // Default to -1 when absent so the backend can choose its own
    // file from the torrent contents.
    {
        const auto top = obj.value(QStringLiteral("fileIdx"));
        const auto nested = bh.value(QStringLiteral("fileIdx"));
        const QJsonValue chosen = top.isUndefined() ? nested : top;
        if (chosen.isDouble()) {
            s.fileIndex = chosen.toInt(-1);
        }
    }
    s.fileNameHint = bh.value(QStringLiteral("filename")).toString();

    // `sources` is an optional list of tracker URIs / DHT hints.
    const auto srcs = obj.value(QStringLiteral("sources")).toArray();
    s.sources.reserve(srcs.size());
    for (const auto& v : srcs) {
        if (v.isString()) {
            s.sources.append(v.toString());
        }
    }

    const auto urlStr = obj.value(QStringLiteral("url")).toString();
    if (!urlStr.isEmpty()) {
        s.directUrl = QUrl(urlStr);
    }

    // Prefer the indexer's structured `quality` field when present
    // (Peerflix); fall back to regex over the quality label /
    // release name (Torrentio).
    const auto qualityStr
        = obj.value(QStringLiteral("quality")).toString().trimmed();
    s.resolution = qualityStr.isEmpty()
        ? parseResolution(s.qualityLabel, s.releaseName)
        : qualityStr.toLower();

    return s;
}

} // namespace

QList<Stream> parseStreams(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Stream addon response was not a JSON object."));
    }
    const auto arr = doc.object().value(QStringLiteral("streams"));
    if (!arr.isArray()) {
        return {};
    }

    QList<Stream> out;
    out.reserve(arr.toArray().size());
    for (const auto& v : arr.toArray()) {
        if (!v.isObject()) {
            continue;
        }
        auto s = parseOne(v.toObject());
        // A row with neither a hash nor a direct URL is not actionable.
        if (s.infoHash.isEmpty() && s.directUrl.isEmpty()) {
            continue;
        }
        out.append(std::move(s));
    }
    return out;
}

} // namespace kinema::api::stremio
