// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/AllDebridParse.h"

#include "core/HttpError.h"

#include <KLocalizedString>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>

namespace kinema::api::alldebrid {

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

bool readBool(const QJsonValue& v)
{
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

[[noreturn]] void throwApiError(const QString& code,
    const QString& message,
    const char* endpointLabel)
{
    int status = 400;
    if (code.startsWith(QLatin1String("AUTH_"))) {
        status = 401;
    }
    const auto label = QString::fromUtf8(endpointLabel);
    const QString display = code.isEmpty()
        ? i18n("AllDebrid %1 failed: %2", label,
              message.isEmpty() ? i18n("unknown error") : message)
        : i18n("AllDebrid %1 failed: %2 \u2014 %3",
              label, code,
              message.isEmpty() ? i18n("unknown error") : message);
    throw core::HttpError(core::HttpError::Kind::HttpStatus, status,
        display);
}

/// Throw if the per-row object carries an `error` sub-object (used
/// for `magnet/upload` and `magnet/files` rows).
void throwIfRowError(const QJsonObject& row, const char* endpointLabel)
{
    const auto err = row.value(QStringLiteral("error")).toObject();
    if (err.isEmpty()) {
        return;
    }
    throwApiError(err.value(QStringLiteral("code")).toString(),
        err.value(QStringLiteral("message")).toString(),
        endpointLabel);
}

void flattenFilesTree(const QJsonArray& nodes,
    const QString& prefix,
    QList<AdMagnetFile>& out)
{
    for (const auto& v : nodes) {
        if (!v.isObject()) {
            continue;
        }
        const auto obj = v.toObject();
        const auto name = obj.value(QStringLiteral("n")).toString();
        if (obj.contains(QStringLiteral("e"))) {
            const auto subPrefix = prefix.isEmpty()
                ? name
                : prefix + QLatin1Char('/') + name;
            flattenFilesTree(obj.value(QStringLiteral("e")).toArray(),
                subPrefix, out);
            continue;
        }
        AdMagnetFile f;
        f.path = prefix.isEmpty()
            ? name
            : prefix + QLatin1Char('/') + name;
        f.bytes = readInt64(obj.value(QStringLiteral("s")));
        const auto link = obj.value(QStringLiteral("l")).toString();
        if (link.isEmpty()) {
            // Folder-like node without children, or a file the
            // provider can't link. Skip rather than surface as a
            // bogus candidate.
            continue;
        }
        f.hosterLink = QUrl(link);
        out.append(std::move(f));
    }
}

} // namespace

QJsonObject unwrapEnvelope(const QJsonDocument& doc,
    const char* endpointLabel)
{
    if (!doc.isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid %1: response was not a JSON object.",
                QString::fromUtf8(endpointLabel)));
    }
    const auto root = doc.object();
    const auto status = root.value(QStringLiteral("status")).toString();
    if (status == QLatin1String("error")) {
        const auto err = root.value(QStringLiteral("error")).toObject();
        throwApiError(err.value(QStringLiteral("code")).toString(),
            err.value(QStringLiteral("message")).toString(),
            endpointLabel);
    }
    if (status != QLatin1String("success")) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid %1: missing or invalid `status` field.",
                QString::fromUtf8(endpointLabel)));
    }
    return root.value(QStringLiteral("data")).toObject();
}

AllDebridUser parseUser(const QJsonDocument& doc)
{
    const auto data = unwrapEnvelope(doc, "user");
    const auto user = data.value(QStringLiteral("user")).toObject();
    if (user.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid user response is missing the `user` object."));
    }
    AllDebridUser u;
    u.username = user.value(QStringLiteral("username")).toString();
    u.email = user.value(QStringLiteral("email")).toString();
    u.isPremium = readBool(user.value(QStringLiteral("isPremium")));
    u.isTrial = readBool(user.value(QStringLiteral("isTrial")));
    const auto until = readInt64(user.value(QStringLiteral("premiumUntil")));
    if (until > 0) {
        u.premiumUntil = QDateTime::fromSecsSinceEpoch(until);
    }
    return u;
}

AdAddMagnetResult parseAddMagnet(const QJsonDocument& doc)
{
    const auto data = unwrapEnvelope(doc, "magnet/upload");
    const auto magnets = data.value(QStringLiteral("magnets")).toArray();
    if (magnets.isEmpty() || !magnets.first().isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/upload response is missing magnets[]."));
    }
    const auto row = magnets.first().toObject();
    throwIfRowError(row, "magnet/upload");
    AdAddMagnetResult r;
    r.id = readInt64(row.value(QStringLiteral("id")));
    r.hash = row.value(QStringLiteral("hash")).toString().toLower();
    r.name = row.value(QStringLiteral("name")).toString();
    r.sizeBytes = readInt64(row.value(QStringLiteral("size")));
    r.ready = readBool(row.value(QStringLiteral("ready")));
    if (r.id <= 0) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/upload response is missing the magnet id."));
    }
    return r;
}

AdMagnetStatus parseMagnetStatus(const QJsonDocument& doc)
{
    const auto data = unwrapEnvelope(doc, "magnet/status");
    const auto magnets = data.value(QStringLiteral("magnets")).toArray();
    QJsonObject row;
    if (magnets.isEmpty()) {
        // The endpoint also accepts being passed a single id and
        // returning the row directly under `magnets`. Some legacy
        // shapes wrap it under `magnet`.
        row = data.value(QStringLiteral("magnet")).toObject();
    } else {
        row = magnets.first().toObject();
    }
    if (row.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/status response is empty."));
    }
    throwIfRowError(row, "magnet/status");
    AdMagnetStatus s;
    s.id = readInt64(row.value(QStringLiteral("id")));
    s.filename = row.value(QStringLiteral("filename")).toString();
    s.size = readInt64(row.value(QStringLiteral("size")));
    s.status = row.value(QStringLiteral("status")).toString();
    s.statusCode = static_cast<int>(
        readInt64(row.value(QStringLiteral("statusCode"))));
    s.downloaded = readInt64(row.value(QStringLiteral("downloaded")));
    s.downloadSpeed = readInt64(row.value(QStringLiteral("downloadSpeed")));
    s.seeders = static_cast<int>(
        readInt64(row.value(QStringLiteral("seeders"))));
    return s;
}

QList<AdMagnetFile> parseMagnetFiles(const QJsonDocument& doc)
{
    const auto data = unwrapEnvelope(doc, "magnet/files");
    const auto magnets = data.value(QStringLiteral("magnets")).toArray();
    if (magnets.isEmpty() || !magnets.first().isObject()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/files response is missing magnets[]."));
    }
    const auto row = magnets.first().toObject();
    throwIfRowError(row, "magnet/files");
    const auto files = row.value(QStringLiteral("files")).toArray();
    QList<AdMagnetFile> out;
    flattenFilesTree(files, QString(), out);
    return out;
}

AdUnlockedLink parseUnlock(const QJsonDocument& doc)
{
    const auto data = unwrapEnvelope(doc, "link/unlock");
    AdUnlockedLink u;
    const auto link = data.value(QStringLiteral("link")).toString();
    if (!link.isEmpty()) {
        u.download = QUrl(link);
    }
    u.filename = data.value(QStringLiteral("filename")).toString();
    u.fileSize = readInt64(data.value(QStringLiteral("filesize")));
    u.host = data.value(QStringLiteral("host")).toString();
    u.delayedId = readInt64(data.value(QStringLiteral("delayed")));
    if (u.download.isEmpty() && u.delayedId == 0) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid link/unlock response has neither a download URL "
                 "nor a delayed id."));
    }
    return u;
}

AdUnlockedLink parseDelayed(const QJsonDocument& doc)
{
    const auto data = unwrapEnvelope(doc, "link/delayed");
    const int status = static_cast<int>(
        readInt64(data.value(QStringLiteral("status"))));
    if (status == 3) {
        throwApiError(QStringLiteral("DELAYED_FAILED"),
            i18n("Could not generate the delayed download link."),
            "link/delayed");
    }
    AdUnlockedLink u;
    if (status == 2) {
        const auto link = data.value(QStringLiteral("link")).toString();
        u.download = QUrl(link);
        if (u.download.isEmpty()) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                i18n("AllDebrid link/delayed reported ready but returned no URL."));
        }
    }
    return u;
}

} // namespace kinema::api::alldebrid
