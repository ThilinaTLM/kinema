// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/PeerflixIndexer.h"

#include "api/StremioStreamParse.h"
#include "config/PeerflixSettings.h"
#include "core/io/HttpClient.h"
#include "domain/DebridCredentials.h"
#include "kinema_log_api.h"

#include <QElapsedTimer>
#include <QStringList>

namespace kinema::api {
using namespace kinema::domain;

PeerflixIndexer::PeerflixIndexer(core::HttpClient* http,
    const config::PeerflixSettings& settings,
    const domain::DebridCredentialsProvider* creds, QObject* parent)
    : domain::Indexer(parent)
    , m_http(http)
    , m_settings(settings)
    , m_creds(creds)
{
}

PeerflixIndexer::~PeerflixIndexer() = default;

QCoro::Task<QList<Stream>> PeerflixIndexer::streams(MediaKind kind,
    QString streamId)
{
    const auto kindStr = mediaKindToPath(kind);

    const auto creds
        = m_creds ? m_creds->active() : domain::ActiveDebrid {};
    const bool useDebrid
        = creds.provider != domain::DebridProvider::None
        && !creds.token.isEmpty();

    QUrl url(useDebrid ? m_settings.addonBaseUrl()
                       : m_settings.baseUrl());
    if (useDebrid) {
        // Sensible defaults for the addon's tuning knobs until we
        // expose them as user settings:
        //   debridoptions=torrentlinks  → keep raw torrent fallback
        //                                 rows alongside debrid-cached.
        //   debridoptions=nocatalog     → we never surface the addon's
        //                                 own catalog; suppress it.
        // These live inline (not in PeerflixSettings) per the
        // "per-indexer URL config lives inside each concrete
        // indexer" rule in AGENTS.md.
        QStringList parts;
        parts.reserve(2);
        parts.append(QStringLiteral(
            "debridoptions=torrentlinks,nocatalog"));
        parts.append(domain::providerToUrlToken(creds.provider)
            + QLatin1Char('=') + creds.token);
        url.setPath(QStringLiteral("/%1/stream/%2/%3.json")
                .arg(parts.join(QLatin1Char('|')), kindStr, streamId));
    } else {
        url.setPath(QStringLiteral("/stream/%1/%2.json")
                .arg(kindStr, streamId));
    }

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
