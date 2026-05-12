// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/PeerflixIndexer.h"

#include "api/StremioStreamParse.h"
#include "config/PeerflixSettings.h"
#include "core/io/HttpClient.h"
#include "kinema_log_api.h"

#include <QElapsedTimer>

namespace kinema::api {
using namespace kinema::domain;

PeerflixIndexer::PeerflixIndexer(core::HttpClient* http,
    const config::PeerflixSettings& settings, QObject* parent)
    : domain::Indexer(parent)
    , m_http(http)
    , m_settings(settings)
{
}

PeerflixIndexer::~PeerflixIndexer() = default;

QCoro::Task<QList<Stream>> PeerflixIndexer::streams(MediaKind kind,
    QString streamId)
{
    const auto kindStr = mediaKindToPath(kind);

    QUrl url(m_settings.baseUrl());
    url.setPath(
        QStringLiteral("/stream/%1/%2.json").arg(kindStr, streamId));

    QElapsedTimer t;
    t.start();
    const auto doc = co_await m_http->getJson(url);
    auto streams = stremio::parseStreams(doc);
    qCInfo(KINEMA_API)
        << "Peerflix:" << kindStr << streamId
        << "\u2192" << streams.size() << "streams in" << t.elapsed() << "ms";
    co_return streams;
}

} // namespace kinema::api
