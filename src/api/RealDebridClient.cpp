// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/RealDebridClient.h"

#include "api/RealDebridParse.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"

#include <KLocalizedString>

#include <QNetworkRequest>
#include <QUrl>
#include <QUrlQuery>

namespace kinema::api {
using namespace kinema::domain;

namespace {

QByteArray bearer(const QString& token)
{
    return QByteArrayLiteral("Bearer ") + token.toUtf8();
}

QUrl appendPath(QUrl base, QString path)
{
    base.setPath(base.path() + std::move(path));
    return base;
}

QByteArray urlEncode(const QList<QPair<QString, QString>>& fields)
{
    QUrlQuery q;
    for (const auto& kv : fields) {
        q.addQueryItem(kv.first, kv.second);
    }
    return q.toString(QUrl::FullyEncoded).toUtf8();
}

void requireToken(const QString& token)
{
    if (token.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::HttpStatus, 401,
            i18n("No Real-Debrid token set."));
    }
}

} // namespace

RealDebridClient::RealDebridClient(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_baseUrl(QStringLiteral("https://api.real-debrid.com/rest/1.0"))
{
}

void RealDebridClient::setBaseUrl(QUrl url)
{
    m_baseUrl = std::move(url);
}

void RealDebridClient::setToken(QString token)
{
    m_token = std::move(token);
}

QCoro::Task<RealDebridUser> RealDebridClient::user()
{
    requireToken(m_token);

    QNetworkRequest req(appendPath(m_baseUrl, QStringLiteral("/user")));
    req.setRawHeader("Authorization", bearer(m_token));

    const auto doc = co_await m_http->getJson(std::move(req));
    co_return realdebrid::parseUser(doc);
}

QCoro::Task<RdAddMagnetResult> RealDebridClient::addMagnet(QString magnet)
{
    requireToken(m_token);
    if (magnet.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid addMagnet: empty magnet URI."));
    }

    QNetworkRequest req(appendPath(m_baseUrl, QStringLiteral("/torrents/addMagnet")));
    req.setRawHeader("Authorization", bearer(m_token));
    req.setHeader(QNetworkRequest::ContentTypeHeader,
        QStringLiteral("application/x-www-form-urlencoded"));

    const auto body = urlEncode({ { QStringLiteral("magnet"), magnet } });
    const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
    co_return realdebrid::parseAddMagnet(doc);
}

QCoro::Task<RdTorrentInfo> RealDebridClient::torrentInfo(QString rdTorrentId)
{
    requireToken(m_token);
    if (rdTorrentId.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid torrentInfo: empty torrent id."));
    }

    QNetworkRequest req(appendPath(m_baseUrl,
        QStringLiteral("/torrents/info/") + rdTorrentId));
    req.setRawHeader("Authorization", bearer(m_token));

    const auto doc = co_await m_http->getJson(std::move(req));
    co_return realdebrid::parseTorrentInfo(doc);
}

QCoro::Task<void> RealDebridClient::selectFiles(QString rdTorrentId,
    QList<int> fileIds)
{
    requireToken(m_token);
    if (rdTorrentId.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid selectFiles: empty torrent id."));
    }

    QNetworkRequest req(appendPath(m_baseUrl,
        QStringLiteral("/torrents/selectFiles/") + rdTorrentId));
    req.setRawHeader("Authorization", bearer(m_token));
    req.setHeader(QNetworkRequest::ContentTypeHeader,
        QStringLiteral("application/x-www-form-urlencoded"));

    QString filesField = QStringLiteral("all");
    if (!fileIds.isEmpty()) {
        QStringList parts;
        parts.reserve(fileIds.size());
        for (const auto id : fileIds) {
            parts.append(QString::number(id));
        }
        filesField = parts.join(QLatin1Char(','));
    }
    const auto body = urlEncode({ { QStringLiteral("files"), filesField } });
    co_await m_http->postJson(std::move(req), body);
    co_return;
}

QCoro::Task<RdUnrestrictedLink> RealDebridClient::unrestrictLink(QUrl link)
{
    requireToken(m_token);
    if (link.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid unrestrict: empty link."));
    }

    QNetworkRequest req(appendPath(m_baseUrl, QStringLiteral("/unrestrict/link")));
    req.setRawHeader("Authorization", bearer(m_token));
    req.setHeader(QNetworkRequest::ContentTypeHeader,
        QStringLiteral("application/x-www-form-urlencoded"));

    const auto body = urlEncode({ { QStringLiteral("link"), link.toString() } });
    const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
    co_return realdebrid::parseUnrestrictedLink(doc);
}

QCoro::Task<void> RealDebridClient::deleteTorrent(QString rdTorrentId)
{
    requireToken(m_token);
    if (rdTorrentId.isEmpty()) {
        co_return;
    }

    QNetworkRequest req(appendPath(m_baseUrl,
        QStringLiteral("/torrents/delete/") + rdTorrentId));
    req.setRawHeader("Authorization", bearer(m_token));

    co_await m_http->del(std::move(req));
    co_return;
}

} // namespace kinema::api
