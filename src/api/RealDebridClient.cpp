// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/RealDebridClient.h"

#include "api/RealDebridParse.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"

#include <KLocalizedString>

#include <QNetworkRequest>

namespace kinema::api {

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
    if (m_token.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::HttpStatus, 401,
            i18n("No Real-Debrid token set."));
    }

    QUrl url = m_baseUrl;
    url.setPath(m_baseUrl.path() + QStringLiteral("/user"));

    QNetworkRequest req(url);
    req.setRawHeader("Authorization",
        QByteArrayLiteral("Bearer ") + m_token.toUtf8());

    const auto doc = co_await m_http->getJson(std::move(req));
    co_return realdebrid::parseUser(doc);
}

} // namespace kinema::api
