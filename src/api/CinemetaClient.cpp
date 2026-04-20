// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/CinemetaClient.h"

#include "api/CinemetaParse.h"
#include "core/HttpClient.h"

namespace kinema::api {

CinemetaClient::CinemetaClient(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_baseUrl(QStringLiteral("https://v3-cinemeta.strem.io"))
{
}

void CinemetaClient::setBaseUrl(QUrl url)
{
    m_baseUrl = std::move(url);
}

QCoro::Task<QList<MetaSummary>> CinemetaClient::search(MediaKind kind, QString query)
{
    const auto kindStr = mediaKindToPath(kind);
    // Path has to be assembled carefully: the "search=..." token is part of
    // the .json filename, not a real query string.
    const auto encoded = QString::fromUtf8(QUrl::toPercentEncoding(query));
    QUrl url = m_baseUrl;
    url.setPath(QStringLiteral("/catalog/%1/top/search=%2.json").arg(kindStr, encoded));

    const auto doc = co_await m_http->getJson(url);
    co_return cinemeta::parseSearch(doc, kind);
}

QCoro::Task<MetaDetail> CinemetaClient::meta(MediaKind kind, QString imdbId)
{
    const auto kindStr = mediaKindToPath(kind);
    QUrl url = m_baseUrl;
    url.setPath(QStringLiteral("/meta/%1/%2.json").arg(kindStr, imdbId));

    const auto doc = co_await m_http->getJson(url);
    co_return cinemeta::parseMeta(doc, kind);
}

QCoro::Task<SeriesDetail> CinemetaClient::seriesMeta(QString imdbId)
{
    QUrl url = m_baseUrl;
    url.setPath(QStringLiteral("/meta/series/%1.json").arg(imdbId));

    const auto doc = co_await m_http->getJson(url);
    co_return cinemeta::parseSeriesMeta(doc);
}

} // namespace kinema::api
