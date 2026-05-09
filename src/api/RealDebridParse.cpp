// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/RealDebridParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>

namespace kinema::api::realdebrid {

namespace {

qint64 readInt64(const QJsonValue& v)
{
    if (v.isDouble()) {
        return static_cast<qint64>(v.toDouble());
    }
    if (v.isString()) {
        return v.toString().toLongLong();
    }
    return 0;
}

} // namespace

RealDebridUser parseUser(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid /user response was not a JSON object."));
    }
    const auto obj = doc.object();

    RealDebridUser u;
    u.username = obj.value(QStringLiteral("username")).toString();
    u.email = obj.value(QStringLiteral("email")).toString();
    u.type = obj.value(QStringLiteral("type")).toString();

    const auto exp = obj.value(QStringLiteral("expiration")).toString();
    if (!exp.isEmpty()) {
        const auto dt = QDateTime::fromString(exp, Qt::ISODate);
        if (dt.isValid()) {
            u.premiumUntil = dt;
        }
    }
    return u;
}

RdInstantAvailability parseInstantAvailability(const QJsonDocument& doc,
    const QString& infoHash)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid instant-availability response was not a JSON object."));
    }
    RdInstantAvailability out;
    out.infoHash = infoHash.toLower();

    const auto root = doc.object();
    // RD keys responses by the info-hash lowercased. Try direct match
    // first, then fall back to a case-insensitive lookup.
    QJsonValue entry = root.value(out.infoHash);
    if (entry.isUndefined()) {
        for (auto it = root.begin(); it != root.end(); ++it) {
            if (it.key().compare(out.infoHash, Qt::CaseInsensitive) == 0) {
                entry = it.value();
                break;
            }
        }
    }
    if (!entry.isObject()) {
        return out; // not cached / unknown shape
    }
    const auto rdArr = entry.toObject().value(QStringLiteral("rd")).toArray();
    out.variants.reserve(rdArr.size());
    for (const auto& vRaw : rdArr) {
        if (!vRaw.isObject()) {
            continue;
        }
        const auto variantObj = vRaw.toObject();
        RdInstantVariant variant;
        variant.files.reserve(variantObj.size());
        for (auto it = variantObj.begin(); it != variantObj.end(); ++it) {
            RdInstantVariantFile f;
            f.fileId = it.key().toInt();
            const auto fObj = it.value().toObject();
            f.filename = fObj.value(QStringLiteral("filename")).toString();
            f.sizeBytes = readInt64(fObj.value(QStringLiteral("filesize")));
            variant.files.append(f);
        }
        if (!variant.files.isEmpty()) {
            out.variants.append(std::move(variant));
        }
    }
    return out;
}

RdAddMagnetResult parseAddMagnet(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid addMagnet response was not a JSON object."));
    }
    const auto obj = doc.object();
    RdAddMagnetResult r;
    r.id = obj.value(QStringLiteral("id")).toString();
    const auto uri = obj.value(QStringLiteral("uri")).toString();
    if (!uri.isEmpty()) {
        r.uri = QUrl(uri);
    }
    if (r.id.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid addMagnet response is missing the torrent id."));
    }
    return r;
}

RdTorrentInfo parseTorrentInfo(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid torrent-info response was not a JSON object."));
    }
    const auto obj = doc.object();
    RdTorrentInfo info;
    info.id = obj.value(QStringLiteral("id")).toString();
    info.filename = obj.value(QStringLiteral("filename")).toString();
    info.hash = obj.value(QStringLiteral("hash")).toString().toLower();
    info.bytes = readInt64(obj.value(QStringLiteral("bytes")));
    info.host = obj.value(QStringLiteral("host")).toString();
    info.status = obj.value(QStringLiteral("status")).toString();
    {
        const auto p = obj.value(QStringLiteral("progress"));
        if (p.isDouble()) {
            info.progress = static_cast<int>(p.toDouble());
        }
    }

    const auto filesArr = obj.value(QStringLiteral("files")).toArray();
    info.files.reserve(filesArr.size());
    for (const auto& fv : filesArr) {
        if (!fv.isObject()) {
            continue;
        }
        const auto fObj = fv.toObject();
        RdTorrentFile f;
        f.id = fObj.value(QStringLiteral("id")).toInt();
        f.path = fObj.value(QStringLiteral("path")).toString();
        f.bytes = readInt64(fObj.value(QStringLiteral("bytes")));
        const auto sel = fObj.value(QStringLiteral("selected"));
        if (sel.isDouble()) {
            f.selected = sel.toInt() != 0;
        } else if (sel.isBool()) {
            f.selected = sel.toBool();
        }
        info.files.append(f);
    }

    const auto linksArr = obj.value(QStringLiteral("links")).toArray();
    info.links.reserve(linksArr.size());
    for (const auto& lv : linksArr) {
        const auto s = lv.toString();
        if (!s.isEmpty()) {
            info.links.append(QUrl(s));
        }
    }
    return info;
}

RdUnrestrictedLink parseUnrestrictedLink(const QJsonDocument& doc)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid unrestrict response was not a JSON object."));
    }
    const auto obj = doc.object();
    RdUnrestrictedLink u;
    u.id = obj.value(QStringLiteral("id")).toString();
    u.filename = obj.value(QStringLiteral("filename")).toString();
    u.fileSize = readInt64(obj.value(QStringLiteral("filesize")));
    const auto dl = obj.value(QStringLiteral("download")).toString();
    if (!dl.isEmpty()) {
        u.download = QUrl(dl);
    }
    u.mimeType = obj.value(QStringLiteral("mimeType")).toString();
    u.host = obj.value(QStringLiteral("host")).toString();
    {
        const auto s = obj.value(QStringLiteral("streamable"));
        if (s.isDouble()) {
            u.streamable = s.toInt() != 0;
        } else if (s.isBool()) {
            u.streamable = s.toBool();
        }
    }
    if (u.download.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid unrestrict response is missing the download URL."));
    }
    return u;
}

} // namespace kinema::api::realdebrid
