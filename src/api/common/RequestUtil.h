// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QJsonValue>
#include <QNetworkRequest>
#include <QPair>
#include <QUrl>
#include <QUrlQuery>

#include <QList>

namespace kinema::api {

/// Append a path segment (e.g. `"/v4/magnet/upload"`) to a base URL,
/// preserving the base's existing scheme/host/path.
inline QUrl appendPath(QUrl base, QString path)
{
    base.setPath(base.path() + std::move(path));
    return base;
}

/// `Authorization: Bearer <token>` header value.
inline QByteArray bearer(const QString& token)
{
    return QByteArrayLiteral("Bearer ") + token.toUtf8();
}

/// Form-encode a list of name/value pairs for an
/// `application/x-www-form-urlencoded` request body.
inline QByteArray urlEncode(const QList<QPair<QString, QString>>& fields)
{
    QUrlQuery q;
    for (const auto& kv : fields) {
        q.addQueryItem(kv.first, kv.second);
    }
    return q.toString(QUrl::FullyEncoded).toUtf8();
}

/// A GET request carrying a bearer token.
inline QNetworkRequest bearerGet(const QUrl& url, const QByteArray& auth)
{
    QNetworkRequest req(url);
    req.setRawHeader("Authorization", auth);
    return req;
}

/// A POST request carrying a bearer token and the conventional
/// `application/x-www-form-urlencoded` content type.
inline QNetworkRequest bearerFormPost(const QUrl& url, const QByteArray& auth)
{
    QNetworkRequest req(url);
    req.setRawHeader("Authorization", auth);
    req.setHeader(QNetworkRequest::ContentTypeHeader,
        QStringLiteral("application/x-www-form-urlencoded"));
    return req;
}

/// Coerce a JSON scalar (`QJsonValue::Double` number, or a numeric
/// string) to a `qint64`. Returns 0 for absent / non-numeric values.
inline qint64 jsonInt64(const QJsonValue& v)
{
    switch (v.type()) {
    case QJsonValue::Double:
        return static_cast<qint64>(v.toDouble());
    case QJsonValue::String:
        return v.toString().toLongLong();
    default:
        return 0;
    }
}

/// Coerce a JSON scalar (`QJsonValue::Bool`, or `"true"` / `"1"` /
/// non-zero number) to a bool. Returns false for absent / unknown
/// values.
inline bool jsonBool(const QJsonValue& v)
{
    switch (v.type()) {
    case QJsonValue::Bool:
        return v.toBool();
    case QJsonValue::Double:
        return v.toDouble() != 0.0;
    case QJsonValue::String: {
        const QString s = v.toString().toLower();
        return s == QStringLiteral("true") || s == QStringLiteral("1");
    }
    default:
        return false;
    }
}

} // namespace kinema::api
