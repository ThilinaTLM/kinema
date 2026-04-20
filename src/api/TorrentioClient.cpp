// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/TorrentioClient.h"

#include "api/TorrentioParse.h"
#include "core/HttpClient.h"
#include "core/TorrentioConfig.h"

namespace kinema::api {

TorrentioClient::TorrentioClient(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_baseUrl(QStringLiteral("https://torrentio.strem.fun"))
{
}

void TorrentioClient::setBaseUrl(QUrl url)
{
    m_baseUrl = std::move(url);
}

QCoro::Task<QList<Stream>> TorrentioClient::streams(MediaKind kind,
    QString streamId,
    core::torrentio::ConfigOptions opts)
{
    const auto kindStr = mediaKindToPath(kind);
    const auto cfg = core::torrentio::toPathSegment(opts);

    QUrl url = m_baseUrl;
    if (cfg.isEmpty()) {
        url.setPath(QStringLiteral("/stream/%1/%2.json").arg(kindStr, streamId));
    } else {
        url.setPath(QStringLiteral("/%1/stream/%2/%3.json").arg(cfg, kindStr, streamId));
    }

    const auto doc = co_await m_http->getJson(url);
    co_return torrentio::parseStreams(doc);
}

} // namespace kinema::api
