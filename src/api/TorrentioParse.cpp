// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/TorrentioParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QRegularExpression>

namespace kinema::api::torrentio {

namespace {

// The `title` field Torrentio returns looks like:
//
//   "The Matrix 1999 1080p BluRay x264-NOGRP\n👤 123 💾 2.1 GB ⚙️ ThePirateBay"
//
// Sometimes lines are separated by "\n", sometimes by "\r\n". The extra
// metadata may be on one line or spread across several. Use tolerant
// regexes over the whole blob.

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
    s.rdCached = nameRaw.contains(QLatin1String("[RD+]"));
    s.rdDownload = nameRaw.contains(QLatin1String("[RD download]"));

    const auto titleRaw = obj.value(QStringLiteral("title")).toString();
    if (!titleRaw.isEmpty()) {
        const auto lines = titleRaw.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
        if (!lines.isEmpty()) {
            s.releaseName = lines.first().trimmed();
            if (lines.size() > 1) {
                s.detailsText = lines.mid(1).join(QStringLiteral(" · ")).trimmed();
            }
        }
    }

    s.seeders = parseSeeders(titleRaw);
    s.sizeBytes = parseSize(titleRaw);
    s.provider = parseProvider(titleRaw);

    s.infoHash = obj.value(QStringLiteral("infoHash")).toString();
    const auto urlStr = obj.value(QStringLiteral("url")).toString();
    if (!urlStr.isEmpty()) {
        s.directUrl = QUrl(urlStr);
    }

    s.resolution = parseResolution(s.qualityLabel, s.releaseName);

    return s;
}

} // namespace

QList<Stream> parseStreams(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Torrentio response was not a JSON object."));
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

} // namespace kinema::api::torrentio
