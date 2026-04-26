// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/OpenSubtitlesParse.h"

#include "core/Language.h"

#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QTimeZone>

#include <stdexcept>

namespace kinema::api::opensubtitles {

namespace {

QString stringField(const QJsonObject& obj, const char* key)
{
    const auto v = obj.value(QLatin1String(key));
    if (v.isString()) {
        return v.toString();
    }
    if (v.isDouble()) {
        return QString::number(v.toDouble());
    }
    return {};
}

int intField(const QJsonObject& obj, const char* key)
{
    const auto v = obj.value(QLatin1String(key));
    if (v.isDouble()) {
        return v.toInt();
    }
    if (v.isString()) {
        return v.toString().toInt();
    }
    return 0;
}

double doubleField(const QJsonObject& obj, const char* key)
{
    const auto v = obj.value(QLatin1String(key));
    if (v.isDouble()) {
        return v.toDouble();
    }
    if (v.isString()) {
        return v.toString().toDouble();
    }
    return 0.0;
}

bool boolField(const QJsonObject& obj, const char* key)
{
    const auto v = obj.value(QLatin1String(key));
    if (v.isBool()) {
        return v.toBool();
    }
    if (v.isDouble()) {
        return v.toInt() != 0;
    }
    if (v.isString()) {
        const auto s = v.toString().toLower();
        return s == QLatin1String("true") || s == QLatin1String("1");
    }
    return false;
}

QString inferFormat(const QString& fileName)
{
    const QFileInfo fi(fileName);
    const auto ext = fi.suffix().toLower();
    if (ext == QLatin1String("vtt")) return QStringLiteral("vtt");
    if (ext == QLatin1String("ass")) return QStringLiteral("ass");
    if (ext == QLatin1String("ssa")) return QStringLiteral("ssa");
    if (ext == QLatin1String("sub")) return QStringLiteral("sub");
    if (ext == QLatin1String("srt")) return QStringLiteral("srt");
    return {};
}

} // namespace

QList<SubtitleHit> parseSearch(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw std::runtime_error("OpenSubtitles search: response is not an object");
    }
    const auto obj = doc.object();
    const auto data = obj.value(QStringLiteral("data"));
    if (!data.isArray()) {
        throw std::runtime_error("OpenSubtitles search: missing data array");
    }

    QList<SubtitleHit> out;
    for (const auto& v : data.toArray()) {
        if (!v.isObject()) {
            continue;
        }
        const auto entry = v.toObject();
        const auto attrs = entry.value(QStringLiteral("attributes")).toObject();
        if (attrs.isEmpty()) {
            continue;
        }

        SubtitleHit hit;
        hit.language = stringField(attrs, "language");
        hit.languageName = core::language::displayName(hit.language);
        hit.releaseName = stringField(attrs, "release");
        hit.downloadCount = intField(attrs, "download_count");
        hit.rating = doubleField(attrs, "ratings");
        hit.hearingImpaired = boolField(attrs, "hearing_impaired");
        hit.foreignPartsOnly = boolField(attrs, "foreign_parts_only");
        hit.moviehashMatch = boolField(attrs, "moviehash_match");
        hit.fps = intField(attrs, "fps");

        const auto uploader = attrs.value(QStringLiteral("uploader")).toObject();
        if (!uploader.isEmpty()) {
            hit.uploader = stringField(uploader, "name");
        }

        const auto files = attrs.value(QStringLiteral("files")).toArray();
        if (files.isEmpty()) {
            continue; // no downloadable file → useless
        }
        const auto file = files.first().toObject();
        // file_id is documented as int; cast through QString so we
        // store the canonical string form on disk.
        const auto fileIdVal = file.value(QStringLiteral("file_id"));
        if (fileIdVal.isDouble()) {
            hit.fileId = QString::number(static_cast<qint64>(fileIdVal.toDouble()));
        } else {
            hit.fileId = fileIdVal.toString();
        }
        hit.fileName = stringField(file, "file_name");
        hit.format = inferFormat(hit.fileName);
        if (hit.format.isEmpty()) {
            hit.format = QStringLiteral("srt");
        }

        if (hit.fileId.isEmpty()) {
            continue;
        }
        out.append(hit);
    }
    return out;
}

SubtitleDownloadTicket parseDownload(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw std::runtime_error("OpenSubtitles download: response is not an object");
    }
    const auto obj = doc.object();
    SubtitleDownloadTicket ticket;
    const auto link = stringField(obj, "link");
    if (!link.isEmpty()) {
        ticket.link = QUrl(link);
    }
    ticket.fileName = stringField(obj, "file_name");
    ticket.remaining = intField(obj, "remaining");
    ticket.format = inferFormat(ticket.fileName);
    if (ticket.format.isEmpty()) {
        ticket.format = QStringLiteral("srt");
    }
    const auto reset = stringField(obj, "reset_time_utc");
    if (!reset.isEmpty()) {
        auto dt = QDateTime::fromString(reset, Qt::ISODate);
        if (!dt.isValid()) {
            // OpenSubtitles uses formats like "2026-04-26 10:53:00"
            dt = QDateTime::fromString(reset,
                QStringLiteral("yyyy-MM-dd HH:mm:ss"));
        }
        if (dt.isValid()) {
            dt.setTimeZone(QTimeZone::utc());
            ticket.resetAt = dt;
        }
    }
    return ticket;
}

QString parseLogin(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw std::runtime_error("OpenSubtitles login: response is not an object");
    }
    const auto obj = doc.object();
    const auto token = stringField(obj, "token");
    if (token.isEmpty()) {
        throw std::runtime_error("OpenSubtitles login: missing token");
    }
    return token;
}

} // namespace kinema::api::opensubtitles
